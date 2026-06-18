//! mimz — the Min-Mozhi (மின்மொழி) compiler, as a library.
//!
//! Phase 1 pipeline (docs/architecture.md):
//! lexer → parser → AST → checker (six passes) → Verilog emitter.
//! The `mimz` binary (`main.rs`) is a thin CLI over this crate; the
//! LSP server and future tooling (`translate`, the simulator, the
//! npm/PyPI wrappers) consume the same API — the lib/bin split exists
//! BECAUSE a second consumer arrived (architecture section 5's trigger).
//!
//! Crate map (one module per pipeline stage):
//!
//! | Module          | Role                                                       |
//! | --------------- | ---------------------------------------------------------- |
//! | [`span`]        | Byte-offset source spans carried by every token/AST node   |
//! | [`diag`]        | Teaching diagnostics (stable E-codes) + caret renderer     |
//! | [`lexer`]       | Source text → tokens (trilingual keyword table)            |
//! | [`parser`]      | Tokens → AST (recursive descent, multi-error recovery)     |
//! | [`ast`]         | The one shared AST — flavor- and word-order-blind          |
//! | [`checker`]     | Names, consts, widths, drivers, exhaustiveness, clocks     |
//! | [`emit_verilog`]| AST → Verilog-2005 text (+ Tamil→ASCII transliteration)    |
//! | [`project`]     | File loading, NFC normalization, `import` resolution       |
//!
//! Tooling modules consume the pipeline above (they are not stages in it):
//!
//! | Module          | Role                                                       |
//! | --------------- | ---------------------------------------------------------- |
//! | [`explain`]     | Long-form teaching text per E-code (`mimz explain`)        |
//! | [`translate`]   | Keyword-flavor reskin (`mimz translate --to`)              |
//! | [`pretty`]      | AST → source pretty-printer (`mimz translate --order`)     |
//! | [`morph`]       | Error-language selection + Tamil case-suffix inflection    |
//! | [`sim`]         | Combinational evaluator (`mimz eval`) — Phase 1.5 slice    |
//! | [`config`]      | `mimz.toml` project defaults for CLI flags (CLI overrides)  |
//! | [`version`]     | The compiler-version vs language-edition axes + history    |
//!
//! This table is mechanically checked against the `mod` list by
//! `tests/docs_sync.rs` — add a module, add a row (and a docs/code/ page).
//!
//! Generate the API reference with `cargo doc --open`.

// Memory safety is a hard guarantee for this compiler: there is no `unsafe`
// anywhere, and this makes any future `unsafe` a compile error. A buffer
// overflow / out-of-bounds write is therefore impossible by construction.
#![forbid(unsafe_code)]

pub mod ast;
pub mod checker;
pub mod config;
pub mod diag;
pub mod emit_verilog;
pub mod explain;
pub mod lexer;
pub mod morph;
pub mod parser;
pub mod pretty;
pub mod project;
pub mod sim;
pub mod span;
pub mod translate;
pub mod version;

/// Compile a single Min-Mozhi source string straight to Verilog, entirely in
/// memory — no filesystem, no `import` resolution. This is the embedding entry
/// point used by the in-browser playground (`crates/mimz-wasm`) and any tool
/// that already holds the source as a string.
///
/// The full Phase 1 pipeline runs: NFC-normalize → lex → parse → check →
/// transliterate → emit (the same stages as `mimz compile`, minus file I/O).
/// `import` is **not** supported here — there is no file to resolve against — so
/// a source containing one is rejected with a plain message.
///
/// Returns the generated Verilog on success. On any failure returns the
/// rendered, caret-annotated diagnostics (English) as one string — the same
/// text `mimz compile` prints to stderr — suitable for showing to the user.
pub fn compile_string(source: &str) -> Result<String, String> {
    use unicode_normalization::UnicodeNormalization;

    // Spans index into the NFC-normalized text (spec/02 section 2), so render
    // diagnostics against the same normalized `src`. The name is cosmetic — it
    // is only the path shown in the caret header.
    const NAME: &str = "input.mimz";
    let src: String = source.nfc().collect();
    let render = |diags: &[diag::Diag]| diag::render(diags, &src, NAME);

    let toks = lexer::lex(&src).map_err(|d| render(&d))?;
    let ast = parser::parse(toks).map_err(|d| render(&d))?;

    if !ast.imports.is_empty() {
        return Err(
            "`import` is not supported when compiling a single in-memory source — \
             the in-browser compiler resolves no files. Paste the imported modules \
             into this source."
                .to_string(),
        );
    }

    let mut asts = vec![ast];
    if let Err(d) = checker::check(&asts) {
        return Err(render(&d));
    }
    // Tamil identifiers become readable ASCII before emission (విளக்கு → villakku).
    emit_verilog::transliterate(&mut asts);
    let project = emit_verilog::Project::from_files(&asts).map_err(|d| render(&d))?;
    emit_verilog::emit(&project, &asts).map_err(|d| render(&d))
}
