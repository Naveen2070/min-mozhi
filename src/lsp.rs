//! `mimz lsp` — the diagnostics-only language server (LSP v0, Phase 1,
//! non-gating; hover/go-to-definition land in Phase 4).
//!
//! A module of the BINARY, not the library: tower-lsp drags in tokio,
//! and the lib must stay async-free for the Phase 4 WASM playground.
//!
//! On `didOpen`/`didChange`/`didSave` the server runs the full pipeline
//! (lexer → parser → checker) over the edited document's IN-MEMORY text;
//! `import`ed files are read from disk (documented limitation: an edited
//! but unsaved import is seen as last saved). Every diagnostic carries
//! its stable E-code and the teaching help line.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server, jsonrpc};

use mimz::analysis::{self, CandKind, SymKind};
use mimz::lexer::token::Flavor;
use mimz::project::{LoadError, LoadedFile};
use mimz::{checker, diag, lexer, morph, parser, project};

/// Serve LSP over stdio until the client disconnects.
pub fn run() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let (service, socket) = LspService::new(|client| Backend {
            client,
            published: tokio::sync::Mutex::new(HashSet::new()),
            docs: tokio::sync::Mutex::new(HashMap::new()),
        });
        Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
            .serve(service)
            .await;
    });
}

struct Backend {
    client: Client,
    /// URIs we last published diagnostics for — stale ones get an empty
    /// publish so fixed errors actually disappear from the editor.
    published: tokio::sync::Mutex<HashSet<Url>>,
    /// Last-seen text per open document — hover/def/completion get only a
    /// position, so the current buffer must be cached here.
    docs: tokio::sync::Mutex<HashMap<Url, String>>,
}

impl Backend {
    /// Re-analyze one document (its current text) and publish per-file
    /// diagnostics for it and everything it imports.
    async fn recheck(&self, uri: Url, text: String) {
        let Ok(path) = uri.to_file_path() else {
            return; // untitled buffer — nothing to resolve imports against
        };
        let reports = analyze(&path, &text);

        let mut current: HashSet<Url> = HashSet::new();
        for r in &reports {
            let Ok(file_uri) = Url::from_file_path(&r.path) else {
                continue;
            };
            // Localize messages to the file's predominant flavor, exactly as
            // `check`/`compile` do (additive English-fallback via `morph`).
            let toks = lexer::lex(&r.src).ok();
            let flavor = toks
                .as_ref()
                .map(|t| morph::majority_flavor(t))
                .unwrap_or(Flavor::English);
            let mut diags: Vec<Diagnostic> =
                r.diags.iter().map(|d| to_lsp(d, &r.src, flavor)).collect();
            // Surface the non-fatal mixed-flavor warning (W0001) as a WARNING.
            if let Some(w) = toks.as_ref().and_then(|t| morph::flavor_mix_warning(t)) {
                diags.push(to_lsp(&w, &r.src, flavor));
            }
            current.insert(file_uri.clone());
            self.client.publish_diagnostics(file_uri, diags, None).await;
        }

        let mut published = self.published.lock().await;
        for stale in published.difference(&current) {
            self.client
                .publish_diagnostics(stale.clone(), Vec::new(), None)
                .await;
        }
        *published = current;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions::default()),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "mimz".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "mimz lsp ready (diagnostics-only v0)")
            .await;
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        self.docs
            .lock()
            .await
            .insert(p.text_document.uri.clone(), p.text_document.text.clone());
        self.recheck(p.text_document.uri, p.text_document.text)
            .await;
    }

    async fn did_change(&self, mut p: DidChangeTextDocumentParams) {
        // FULL sync: the single change IS the whole document.
        if let Some(change) = p.content_changes.pop() {
            self.docs
                .lock()
                .await
                .insert(p.text_document.uri.clone(), change.text.clone());
            self.recheck(p.text_document.uri, change.text).await;
        }
    }

    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        let text = match p.text {
            Some(t) => t,
            // Client didn't include text on save — read it back from disk.
            None => match p.text_document.uri.to_file_path() {
                Ok(path) => match project::read_source(&path) {
                    Ok(s) => s,
                    Err(_) => return,
                },
                Err(_) => return,
            },
        };
        self.docs
            .lock()
            .await
            .insert(p.text_document.uri.clone(), text.clone());
        self.recheck(p.text_document.uri, text).await;
    }

    async fn hover(&self, p: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        let tdp = p.text_document_position_params;
        let Ok(path) = tdp.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let Some(text) = self.docs.lock().await.get(&tdp.text_document.uri).cloned() else {
            return Ok(None);
        };
        let files = load_for_features(&path, &text);
        if files.is_empty() {
            return Ok(None);
        }
        let index = analysis::build_index(&files);
        let off = offset(&text, tdp.position);
        let Some(sym) = analysis::resolve_at(&index, &files, 0, off) else {
            return Ok(None);
        };
        let render = &index.symbols[sym].render;
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```mimz\n{render}\n```"),
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<GotoDefinitionResponse>> {
        let tdp = p.text_document_position_params;
        let Ok(path) = tdp.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let Some(text) = self.docs.lock().await.get(&tdp.text_document.uri).cloned() else {
            return Ok(None);
        };
        let files = load_for_features(&path, &text);
        if files.is_empty() {
            return Ok(None);
        }
        let index = analysis::build_index(&files);
        let off = offset(&text, tdp.position);
        let Some(sym) = analysis::resolve_at(&index, &files, 0, off) else {
            return Ok(None);
        };
        let s = &index.symbols[sym];
        let (def_path, def_src) = &index.files[s.file_idx];
        // Embedded std (virtual `std:` path) has no real file URI — no jump.
        let Ok(uri) = Url::from_file_path(def_path) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: Range {
                start: position(def_src, s.span.start),
                end: position(def_src, s.span.end.max(s.span.start + 1)),
            },
        })))
    }

    async fn completion(&self, p: CompletionParams) -> jsonrpc::Result<Option<CompletionResponse>> {
        let tdp = p.text_document_position;
        let Ok(path) = tdp.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let Some(text) = self.docs.lock().await.get(&tdp.text_document.uri).cloned() else {
            return Ok(None);
        };
        let files = load_for_features(&path, &text);
        if files.is_empty() {
            return Ok(None);
        }
        let index = analysis::build_index(&files);
        let off = offset(&text, tdp.position);
        let items: Vec<CompletionItem> = analysis::completions(&index, &files, 0, off)
            .into_iter()
            .map(|c| {
                let kind = match c.kind {
                    CandKind::Keyword => CompletionItemKind::KEYWORD,
                    CandKind::Symbol(SymKind::Module) => CompletionItemKind::CLASS,
                    CandKind::Symbol(SymKind::Const | SymKind::Param) => {
                        CompletionItemKind::CONSTANT
                    }
                    CandKind::Symbol(SymKind::Enum) => CompletionItemKind::ENUM,
                    CandKind::Symbol(SymKind::EnumVariant) => CompletionItemKind::ENUM_MEMBER,
                    CandKind::Symbol(_) => CompletionItemKind::VARIABLE,
                };
                CompletionItem {
                    label: c.label,
                    kind: Some(kind),
                    ..Default::default()
                }
            })
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }
}

/// Diagnostics for one file, with the source the spans index into.
struct FileReport {
    path: PathBuf,
    src: String,
    diags: Vec<diag::Diag>,
}

/// The pipeline over an in-memory entry document: lex + parse the given
/// text, pull imports from DISK (the `load_project` walk, minus the
/// entry read), then run the checker across the lot. Load errors win:
/// the checker only runs when every file parsed.
fn analyze(entry: &Path, text: &str) -> Vec<FileReport> {
    let report = |diags| {
        vec![FileReport {
            path: entry.to_path_buf(),
            src: text.to_string(),
            diags,
        }]
    };
    let toks = match lexer::lex(text) {
        Ok(t) => t,
        Err(diags) => return report(diags),
    };
    let ast = match parser::parse(toks) {
        Ok(f) => f,
        Err(diags) => return report(diags),
    };

    let mut files = vec![LoadedFile {
        path: entry.to_path_buf(),
        src: text.to_string(),
        ast,
    }];
    let mut visited: HashSet<PathBuf> = HashSet::new();
    visited.insert(entry.canonicalize().unwrap_or_else(|_| entry.to_path_buf()));

    // Same import walk as `project::load_project`, with errors attributed
    // to the file that contains the bad `import`.
    let mut i = 0;
    while i < files.len() {
        let dir = files[i]
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let imports = files[i].ast.imports.clone();
        for import in &imports {
            let mut p = dir.clone();
            for seg in &import.path {
                p.push(&seg.name);
            }
            p.set_extension("mimz");
            if !p.exists() {
                let d = diag::Diag::new(
                    import.span,
                    format!("imported file `{}` does not exist", p.display()),
                )
                .with_code("E1201")
                .with_help(
                    "`import name` loads `name.mimz` relative to the importing file (spec/02 section 1.5)",
                );
                return vec![FileReport {
                    path: files[i].path.clone(),
                    src: files[i].src.clone(),
                    diags: vec![d],
                }];
            }
            let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
            if !visited.insert(canon) {
                continue;
            }
            match project::parse_file(&p) {
                Ok(f) => files.push(f),
                Err(LoadError::Source { path, src, diags }) => {
                    return vec![FileReport { path, src, diags }];
                }
                Err(LoadError::Io(_)) => return Vec::new(), // raced a delete
            }
        }
        i += 1;
    }

    let asts: Vec<mimz::ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    let mut per_file: HashMap<usize, Vec<diag::Diag>> = HashMap::new();
    if let Err(diags) = checker::check(&asts) {
        for d in diags {
            per_file.entry(d.file.unwrap_or(0)).or_default().push(d);
        }
    }
    files
        .into_iter()
        .enumerate()
        .map(|(idx, f)| FileReport {
            path: f.path,
            src: f.src,
            diags: per_file.remove(&idx).unwrap_or_default(),
        })
        .collect()
}

/// One [`diag::Diag`] → one LSP diagnostic. The help line travels in the
/// message (below the WHAT line) — v0 keeps the teaching content visible
/// without related-information plumbing.
fn to_lsp(d: &diag::Diag, src: &str, flavor: Flavor) -> Diagnostic {
    // The WHAT line localizes where the catalog covers the code (else English);
    // the help line stays English for now (the catalog is message-only).
    let what = morph::localized_msg(d, src, flavor).unwrap_or_else(|| d.msg.clone());
    let message = match &d.help {
        Some(h) => format!("{what}\nhelp: {h}"),
        None => what,
    };
    Diagnostic {
        range: Range {
            start: position(src, d.span.start),
            end: position(src, d.span.end.max(d.span.start + 1)),
        },
        severity: Some(match d.severity {
            diag::Severity::Error => DiagnosticSeverity::ERROR,
            diag::Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: d.code.map(|c| NumberOrString::String(c.to_string())),
        source: Some("mimz".into()),
        message,
        ..Default::default()
    }
}

/// Byte offset → LSP `Position`. LSP characters count **UTF-16 code
/// units** (the protocol default), NOT bytes and NOT chars — a Tamil
/// identifier before the error would skew a char-based column.
fn position(src: &str, offset: usize) -> Position {
    let offset = offset.min(src.len());
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, b) in src.bytes().enumerate() {
        if i >= offset {
            break;
        }
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let character = src[line_start..offset]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum();
    Position { line, character }
}

/// Inverse of [`position`]: an LSP `Position` (UTF-16 line/character) → byte
/// offset into `src`. Clamps past-the-end positions to `src.len()`.
fn offset(src: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut idx = 0usize;
    // Advance to the start of the target line.
    if pos.line > 0 {
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line += 1;
                if line == pos.line {
                    idx = i + 1;
                    break;
                }
            }
        }
        if line < pos.line {
            return src.len();
        }
    }
    // Walk UTF-16 units within the line up to `pos.character`.
    let mut units = 0u32;
    for ch in src[idx..].chars() {
        if ch == '\n' || units >= pos.character {
            break;
        }
        units += ch.len_utf16() as u32;
        idx += ch.len_utf8();
    }
    idx
}

/// Load the entry (via `parse_recover`, so partial trees work) plus its
/// on-disk imports, for the editor-feature handlers. Returns the loaded files;
/// the entry is always index 0. Best-effort: a missing/broken import is simply
/// not loaded (features degrade, they don't error).
fn load_for_features(entry: &Path, text: &str) -> Vec<LoadedFile> {
    let toks = match lexer::lex(text) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let (ast, _diags) = parser::parse_recover(toks);
    let mut files = vec![LoadedFile {
        path: entry.to_path_buf(),
        src: text.to_string(),
        ast,
    }];
    let mut visited: HashSet<PathBuf> = HashSet::new();
    visited.insert(entry.canonicalize().unwrap_or_else(|_| entry.to_path_buf()));
    let mut i = 0;
    while i < files.len() {
        let dir = files[i]
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let imports = files[i].ast.imports.clone();
        for import in &imports {
            // Skip std.* virtual imports — no on-disk file to resolve.
            if import
                .path
                .first()
                .is_some_and(|s| matches!(s.name.as_str(), "std" | "nuulagam" | "நூலகம்"))
            {
                continue;
            }
            let mut p = dir.clone();
            for seg in &import.path {
                p.push(&seg.name);
            }
            p.set_extension("mimz");
            let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
            if !visited.insert(canon) {
                continue;
            }
            if let Ok(f) = project::parse_file(&p) {
                files.push(f);
            }
        }
        i += 1;
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_inverts_position_utf16() {
        let src = "abc\ndef";
        for off in [0usize, 2, 4, 6] {
            let pos = position(src, off);
            assert_eq!(offset(src, pos), off, "round-trip at {off}");
        }
        // Tamil line: offset of `x` after `மணி `.
        let src = "மணி x";
        let off = "மணி ".len();
        assert_eq!(offset(src, position(src, off)), off);
    }

    #[test]
    fn positions_are_utf16_lines_and_columns() {
        let src = "abc\ndef";
        assert_eq!(position(src, 0), Position::new(0, 0));
        assert_eq!(position(src, 2), Position::new(0, 2));
        assert_eq!(position(src, 4), Position::new(1, 0));
        assert_eq!(position(src, 6), Position::new(1, 2));
    }

    #[test]
    fn tamil_text_counts_utf16_units_not_bytes() {
        // மணி = 3 chars = 9 UTF-8 bytes = 3 UTF-16 units; the error
        // sits right after it.
        let src = "மணி x";
        let offset = "மணி ".len(); // byte offset of `x`
        assert_eq!(position(src, offset), Position::new(0, 4));
    }

    #[test]
    fn analyze_reports_checker_errors_with_codes() {
        let dir = std::env::temp_dir().join("mimz_lsp_unit");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("m.mimz");
        // The text is IN-MEMORY; the path only anchors import resolution.
        let reports = analyze(&entry, "module M {\n  out y: bit\n  y = nope\n}\n");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].diags[0].code, Some("E0101"));
    }

    #[test]
    fn diagnostics_localize_to_the_chosen_flavor() {
        let entry = std::env::temp_dir().join("mimz_lsp_unit/dd.mimz");
        // Double-driven `y` → E0501, the one shape the stub catalog covers.
        let src = "module M {\n  in a: bit\n  out y: bit\n  y = a\n  y = a\n}\n";
        let reports = analyze(&entry, src);
        let d = &reports[0].diags[0];
        assert_eq!(d.code, Some("E0501"));
        // Tamil render uses the localized template with the inflected name.
        let ta = to_lsp(d, src, Flavor::Tamil);
        assert!(ta.message.starts_with("`y-க்கு`"), "got {:?}", ta.message);
        // English is the original wording (the verbatim fallback).
        let en = to_lsp(d, src, Flavor::English);
        assert!(en.message.starts_with("`y` has more than one driver"));
    }

    #[test]
    fn uncovered_code_is_not_localized_in_lsp() {
        let entry = std::env::temp_dir().join("mimz_lsp_unit/wm.mimz");
        // Literal too large → E0405, a multi-shape code that is NOT localized.
        let src = "module M {\n  out y: bits[2]\n  y = 9\n}\n";
        let reports = analyze(&entry, src);
        let d = &reports[0].diags[0];
        assert_eq!(d.code, Some("E0405"));
        // Same message under every flavor (additive plumbing leaves it English).
        assert_eq!(
            to_lsp(d, src, Flavor::Tamil).message,
            to_lsp(d, src, Flavor::English).message
        );
    }

    #[test]
    fn mixed_flavor_lint_publishes_as_a_warning() {
        // Tamil `module` + English `in`/`out`: a valid program whose only
        // diagnostic is the non-fatal mixed-flavor lint (W0001).
        let src = "தொகுதி M {\n  in a: bit\n  out y: bit\n  y = a\n}\n";
        let toks = lexer::lex(src).expect("lexes");
        let w = morph::flavor_mix_warning(&toks).expect("mix warns");
        let d = to_lsp(&w, src, Flavor::English);
        assert_eq!(d.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(d.code, Some(NumberOrString::String("W0001".to_string())));
    }
}
