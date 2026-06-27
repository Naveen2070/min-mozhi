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
//! | [`emit_verilog`]| AST → Verilog-2005 text (+ Tamil→ASCII transliteration + testbenches) |
//! | [`project`]     | File loading, NFC normalization, `import` resolution       |
//!
//! Tooling modules consume the pipeline above (they are not stages in it):
//!
//! | Module          | Role                                                       |
//! | --------------- | ---------------------------------------------------------- |
//! | [`explain`]     | Long-form teaching text per E/W-code (`mimz explain`)      |
//! | [`lint`]        | Style and hygiene warnings (`mimz lint`)                   |
//! | [`translate`]   | Keyword-flavor reskin (`mimz translate --to`)              |
//! | [`pretty`]      | AST → source pretty-printer (`mimz translate --order`)     |
//! | [`morph`]       | Error-language selection + Tamil case-suffix inflection    |
//! | [`analysis`]    | Editor symbol index + offset→definition / completion (LSP) |
//! | [`sim`]         | Combinational evaluator (`mimz eval`) — Phase 1.5 slice    |
//! | [`config`]      | `mimz.toml` project defaults for CLI flags (CLI overrides)  |
//! | [`stdlib`]      | Embedded standard library (`import std.*`) — catalog + eject |
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

/// Largest number of `repeat` iterations expanded before erroring — shared by
/// the Verilog emitter (which unrolls at compile time) and the simulator's
/// elaborator. The two MUST agree: the simulator is the emitter's differential
/// oracle, so any design that compiles must also elaborate. 4096 is far past any
/// real datapath while still catching a typo'd bound. (The checker's driver pass
/// keeps its OWN, independent walk budget — a precision/perf knob that degrades
/// gracefully rather than erroring, so it is deliberately not this constant.)
pub const REPEAT_BUDGET: i128 = 4096;

pub mod analysis;
pub mod ast;
pub mod checker;
pub mod config;
pub mod diag;
pub mod emit_verilog;
pub mod explain;
pub mod lexer;
pub mod lint;
pub mod morph;
pub mod parser;
pub mod pretty;
pub mod project;
pub mod sim;
pub mod span;
pub mod stdlib;
pub mod translate;
pub mod version;

mod runner;

// The in-memory command runner (compile / check / eval / sim / test against a
// source STRING) powers the browser playground and any embedder; its argument
// parsers are the single source the CLI command handlers reuse.
pub use runner::{
    parse_bindings, parse_sweep, parse_u128, run_command, sweep_vectors, trace_scope,
};

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
    run_command(source, "compile", &[])
}
