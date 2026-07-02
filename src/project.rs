//! Source loading and project assembly: reading + NFC-normalizing files,
//! running lexer/parser, and resolving `import` declarations
//! (file-relative, `.mimz` extension, cycle-safe — spec/02 section 1.5).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use crate::lexer::token::Flavor;
use crate::{ast, diag, lexer, parser, stdlib};

/// Why loading failed. Carries everything a caller needs to REPORT the
/// failure its own way (the CLI renders carets or JSON; the LSP publishes
/// diagnostics) — this module never prints and never exits.
pub enum LoadError {
    /// Filesystem problem before any parsing (message is ready to show).
    Io(String),
    /// Lexer/parser/import diagnostics, with the source they point into.
    Source {
        /// The file the diagnostics belong to.
        path: PathBuf,
        /// Its NFC-normalized text (spans index into this).
        src: String,
        /// What went wrong (every entry carries an E-code).
        diags: Vec<diag::Diag>,
    },
}

/// One successfully lexed + parsed source file. The original source text is
/// kept alongside the AST because diagnostics render spans against it.
pub struct LoadedFile {
    /// Path as given on the command line / resolved from an `import`.
    pub path: PathBuf,
    /// NFC-normalized source text (what all spans refer to).
    pub src: String,
    /// The parsed file.
    pub ast: ast::File,
}

/// Largest source file the compiler will read, in bytes. Real HDL modules are
/// kilobytes; this 32 MB ceiling bounds the lexer's `Vec<(usize, char)>` (which
/// is several times the file size) so a pathological input cannot exhaust
/// memory. Generous enough that no legitimate file is ever refused.
const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

/// Read + NFC-normalize a source file (spec/02 section 2: lexing is defined over
/// NFC-normalized text so Tamil combining marks compare consistently). Rejects
/// files over `MAX_SOURCE_BYTES` before reading them into memory.
pub fn read_source(path: &Path) -> Result<String, String> {
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > MAX_SOURCE_BYTES
    {
        return Err(format!(
            "`{}` is {} bytes, over the {} MB source-size limit",
            path.display(),
            meta.len(),
            MAX_SOURCE_BYTES / (1024 * 1024)
        ));
    }
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s.nfc().collect()),
        Err(e) => Err(format!("cannot read `{}`: {e}", path.display())),
    }
}

/// Lex + parse one file. Errors come back as values ([`LoadError`]) —
/// rendering them is the caller's job.
pub fn parse_file(path: &Path) -> Result<LoadedFile, LoadError> {
    let src = read_source(path).map_err(LoadError::Io)?;
    let toks = match lexer::lex(&src) {
        Ok(t) => t,
        Err(diags) => {
            return Err(LoadError::Source {
                path: path.to_path_buf(),
                src,
                diags,
            });
        }
    };
    let ast = match parser::parse(toks) {
        Ok(f) => f,
        Err(diags) => {
            return Err(LoadError::Source {
                path: path.to_path_buf(),
                src,
                diags,
            });
        }
    };
    Ok(LoadedFile {
        path: path.to_path_buf(),
        src,
        ast,
    })
}

/// Render project-level diagnostics, each against the file its `file`
/// index names (the entry file when unset). Single-file passes render
/// with `diag::render` directly; this exists for passes that see the
/// whole project (`Project::from_files`, the emitter), whose spans may
/// point into any loaded file.
pub fn render_diags(diags: &[diag::Diag], files: &[LoadedFile]) -> String {
    render_diags_lang(diags, files, Flavor::English)
}

/// Like [`render_diags`], but renders each message in `flavor` where the
/// localized catalog covers its E-code (Phase 1.8, spec/04 section 5); English
/// otherwise. The CLI passes the file's effective error language here.
pub fn render_diags_lang(diags: &[diag::Diag], files: &[LoadedFile], flavor: Flavor) -> String {
    let mut out = String::new();
    for d in diags {
        let f = &files[d.file.unwrap_or(0).min(files.len() - 1)];
        out.push_str(&diag::render_lang(
            std::slice::from_ref(d),
            &f.src,
            &f.path.display().to_string(),
            flavor,
        ));
    }
    out
}

/// Resolve `import` declarations transitively from the entry file (embedded
/// `std.*` resolution active; no on-disk std override). See
/// [`load_project_with_lib`].
pub fn load_project(entry: &Path) -> Result<Vec<LoadedFile>, LoadError> {
    load_project_with_lib(entry, None)
}

/// Like [`load_project`], but `lib_std` (when `Some`) overrides the embedded
/// standard library: `import std.<m>` loads `<lib_std>/<m>.mimz` from disk.
///
/// Dots become path separators for plain imports (`import lib.adder` →
/// `lib/adder.mimz`, relative to the importing file); duplicates and cycles
/// are handled by the canonicalized visited set. The entry file is always
/// `files[0]`. An import whose first segment is a standard-library namespace
/// alias (`std` / `nuulagam` / `நூலகம்`) is routed to [`stdlib::resolve`]
/// instead of the filesystem (see `resolve_std_import`).
pub fn load_project_with_lib(
    entry: &Path,
    lib_std: Option<&Path>,
) -> Result<Vec<LoadedFile>, LoadError> {
    let mut files: Vec<LoadedFile> = Vec::new();
    // Embedded std modules are collected separately and appended after every
    // user file, so the entry file stays `files[0]` (sim/test rely on this).
    let mut std_files: Vec<LoadedFile> = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: Vec<PathBuf> = vec![entry.to_path_buf()];
    // (importing file's eventual index in `files`, that import's index within
    // `loaded.ast.imports`, target path) — resolved to real indices once
    // `files` (+ `std_files`, appended after) is final. The importer's index
    // is `files.len()` at the time it's pushed: files are only ever pushed,
    // never reordered, so that value is stable as the file's final slot.
    let mut pending: Vec<(usize, usize, PathBuf)> = Vec::new();

    while let Some(path) = queue.pop() {
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !visited.insert(canon) {
            continue;
        }
        let loaded = parse_file(&path)?;
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let importer_slot = files.len();
        for (import_idx, import) in loaded.ast.imports.iter().enumerate() {
            // Standard-library import? First segment is a std namespace alias.
            if let Some(first) = import.path.first()
                && stdlib::is_std_namespace(&first.name)
            {
                let (std_file, target) =
                    resolve_std_import(import, lib_std, &mut visited, &mut queue, &loaded)?;
                if let Some(std_file) = std_file {
                    std_files.push(std_file);
                }
                pending.push((importer_slot, import_idx, target));
                continue;
            }
            // Plain file-relative import (unchanged).
            let mut p = dir.clone();
            for seg in &import.path {
                p.push(&seg.name);
            }
            p.set_extension("mimz");
            if !p.exists() {
                return Err(missing_import(&loaded, import, &p));
            }
            queue.push(p.clone());
            pending.push((importer_slot, import_idx, p));
        }
        files.push(loaded);
    }
    files.extend(std_files);

    let canon_index: std::collections::HashMap<PathBuf, usize> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.canonicalize().unwrap_or_else(|_| f.path.clone()), i))
        .collect();
    for (importer_slot, import_idx, target) in pending {
        let target_canon = target.canonicalize().unwrap_or(target);
        if let Some(&idx) = canon_index.get(&target_canon) {
            files[importer_slot].ast.imports[import_idx]
                .resolved_file
                .set(Some(idx));
        } else {
            // Every path in `pending` was either pushed onto `queue` above (so
            // it's loaded into `files` by the time we get here) or is a std
            // override's target (loaded via `resolve_std_import`), so this
            // lookup should never miss. A miss here means an import silently
            // stays unresolved instead of failing loudly — debug_assert so a
            // future regression trips a test instead of vanishing.
            debug_assert!(
                false,
                "canon_index has no entry for pending import target {target_canon:?}; \
                 every pending target should already be a loaded file"
            );
        }
    }

    Ok(files)
}

/// The `import name does not exist` diagnostic, factored out so both the
/// relative and the `[lib] std` override paths share it.
fn missing_import(loaded: &LoadedFile, import: &ast::Import, p: &Path) -> LoadError {
    LoadError::Source {
        path: loaded.path.clone(),
        src: loaded.src.clone(),
        diags: vec![
            diag::Diag::new(
                import.span,
                format!("imported file `{}` does not exist", p.display()),
            )
            .with_code("E1201")
            .with_help(
                "`import name` loads `name.mimz` relative to the importing file (spec/02 section 1.5)",
            ),
        ],
    }
}

/// Resolve one `import std.<module>` (namespace already matched). Returns the
/// embedded source as a synthetic [`LoadedFile`] for the caller to append after
/// the user files (or `None` when the on-disk override is used — queued like
/// any file — or the module was already loaded), alongside the target path
/// this import resolved to (real on-disk path for the override, or the
/// synthetic `std:<module>.mimz` path used to dedup/tag the embedded
/// `LoadedFile`) so the caller can later map the import to its file index.
/// Std modules are self-contained (no transitive imports — guarded by a unit
/// test in `stdlib`), so the embedded variant is parsed directly.
fn resolve_std_import(
    import: &ast::Import,
    lib_std: Option<&Path>,
    visited: &mut HashSet<PathBuf>,
    queue: &mut Vec<PathBuf>,
    loaded: &LoadedFile,
) -> Result<(Option<LoadedFile>, PathBuf), LoadError> {
    let std_err = |msg: String| LoadError::Source {
        path: loaded.path.clone(),
        src: loaded.src.clone(),
        diags: vec![diag::Diag::new(import.span, msg).with_code("E1202").with_help(
            "standard-library imports are `import std.<module>` — one namespace, one module",
        )],
    };

    if import.path.len() != 2 {
        return Err(std_err(
            "a standard-library import must be `std.<module>` (exactly two segments)".into(),
        ));
    }
    let ns = &import.path[0].name;
    let module = &import.path[1].name;

    let Some((m, variant)) = stdlib::resolve(ns, module) else {
        return Err(std_err(format!(
            "no standard-library module `{module}`; available: {}",
            stdlib::available()
        )));
    };

    // On-disk override: load <lib_std>/<file>.mimz with the normal machinery.
    // The filename keys on the resolved variant (matching what `mimz eject std`
    // writes), not the raw written alias — so `import std.வரிசை` and
    // `import std.varisai` both find the ejected `varisai.mimz`.
    if let Some(dir) = lib_std {
        let fname = match variant {
            stdlib::StdVariant::Twin => m.twin_roman,
            stdlib::StdVariant::Canonical => m.stem,
        };
        let mut p = dir.to_path_buf();
        p.push(fname);
        p.set_extension("mimz");
        if !p.exists() {
            return Err(missing_import(loaded, import, &p));
        }
        queue.push(p.clone());
        return Ok((None, p));
    }

    // Embedded: parse the &str into a synthetic in-memory file.
    let vpath = PathBuf::from(format!("std:{}.mimz", m.stem));
    if !visited.insert(vpath.clone()) {
        return Ok((None, vpath)); // already loaded this std module
    }
    let src = m.source(variant).to_string();
    let toks = lexer::lex(&src).map_err(|diags| LoadError::Source {
        path: vpath.clone(),
        src: src.clone(),
        diags,
    })?;
    let ast = parser::parse(toks).map_err(|diags| LoadError::Source {
        path: vpath.clone(),
        src: src.clone(),
        diags,
    })?;
    Ok((
        Some(LoadedFile {
            path: vpath.clone(),
            src,
            ast,
        }),
        vpath,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_resolved_file_points_at_the_right_index() {
        let dir = std::env::temp_dir().join(format!("mimz_import_resolve_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("adder.mimz"),
            "module Adder {\n  out y: bit\n  y = 0\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("top.mimz"),
            "import adder\nmodule Top {\n  let x = adder.Adder() { }\n}\n",
        )
        .unwrap();
        let files = match load_project(&dir.join("top.mimz")) {
            Ok(f) => f,
            Err(_) => panic!("loads"),
        };
        assert_eq!(files[0].ast.imports.len(), 1);
        let resolved = files[0].ast.imports[0].resolved_file.get();
        assert_eq!(resolved, Some(1), "adder.mimz should be files[1]");
        std::fs::remove_dir_all(&dir).ok();
    }
}
