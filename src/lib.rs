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
//! | [`sim`]         | Combinational evaluator (`mimz eval`) — Phase 1.5 slice    |
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
pub mod diag;
pub mod emit_verilog;
pub mod explain;
pub mod lexer;
pub mod parser;
pub mod project;
pub mod sim;
pub mod span;
pub mod translate;
