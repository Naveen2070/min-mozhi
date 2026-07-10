//! mimz — the Min-Mozhi (மின்மொழி) compiler, as a library.
//!
//! Phase 1 pipeline (docs/architecture.md):
//! lexer → parser → AST → checker (six passes) → Verilog emitter.
//! The `mimz` binary (`main.rs`) is a thin CLI over this crate; the
//! LSP server and future tooling (`translate`, the simulator, the
//! npm/PyPI wrappers) consume the same API — the lib/bin split exists
//! BECAUSE a second consumer arrived (architecture section 5's trigger).
//!
//! This crate is now a thin shell: the pure pipeline lives in `mimz-core`,
//! the simulator lives in `mimz-sim`, and this crate re-exports both under
//! the same `mimz::…` paths that existed before the split, plus the shell's
//! own filesystem-touching modules (`project`, `config`, `emulate`).
//!
//! Crate map (one module per pipeline stage):
//!
//! | Module          | Crate      | Role                                                       |
//! | --------------- | ---------- | ---------------------------------------------------------- |
//! | [`span`]        | mimz-core  | Byte-offset source spans carried by every token/AST node   |
//! | [`diag`]        | mimz-core  | Teaching diagnostics (stable E-codes) + caret renderer     |
//! | [`lexer`]       | mimz-core  | Source text → tokens (trilingual keyword table)            |
//! | [`parser`]      | mimz-core  | Tokens → AST (recursive descent, multi-error recovery)     |
//! | [`ast`]         | mimz-core  | The one shared AST — flavor- and word-order-blind          |
//! | [`checker`]     | mimz-core  | Names, consts, widths, drivers, exhaustiveness, clocks     |
//! | [`emit_verilog`]| mimz-core  | AST → Verilog-2005 text (+ Tamil→ASCII transliteration + testbenches) |
//! | [`project`]     | mimz (shell) | File loading, NFC normalization, `import` resolution      |
//!
//! Tooling modules consume the pipeline above (they are not stages in it):
//!
//! | Module          | Crate      | Role                                                       |
//! | --------------- | ---------- | ---------------------------------------------------------- |
//! | [`explain`]     | mimz-core  | Long-form teaching text per E/W-code (`mimz explain`)      |
//! | [`lint`]        | mimz-core  | Style and hygiene warnings (`mimz lint`)                   |
//! | [`translate`]   | mimz-core  | Keyword-flavor reskin (`mimz translate --to`)              |
//! | [`pretty`]      | mimz-core  | AST → source pretty-printer (`mimz translate --order`)     |
//! | [`morph`]       | mimz-core  | Error-language selection + Tamil case-suffix inflection    |
//! | [`analysis`]    | mimz-core  | Editor symbol index + offset→definition / completion (LSP) |
//! | [`sim`]         | mimz-sim   | Combinational evaluator (`mimz eval`) — Phase 1.5 slice    |
//! | [`config`]      | mimz (shell) | `mimz.toml` project defaults for CLI flags (CLI overrides) |
//! | [`stdlib`]      | mimz-core  | Embedded standard library (`import std.*`) — catalog + eject |
//! | [`version`]     | mimz-core  | The compiler-version vs language-edition axes + history    |
//! | [`emulate`]     | mimz (shell) | Native hardware-emulation peripherals (LED/speaker/UART) bound in `sim{}` blocks (`mimz test --emulate`), feature-gated behind `hw-emulation` |
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
pub use mimz_core::REPEAT_BUDGET;

// Shell-native modules: these touch the filesystem or are otherwise specific
// to this crate (not pure enough to live in mimz-core/mimz-sim).
pub mod config;
pub mod project;

#[cfg(feature = "hw-emulation")]
pub mod emulate;

// mimz-core (pure): pipeline stages + tooling that never touch a filesystem.
pub use mimz_core::{
    analysis, ast, checker, diag, emit_verilog, explain, lexer, lint, morph, parser, pretty, span,
    stdlib, translate, version,
};

// mimz-sim (pure): the simulator module tree, plus its in-memory command
// runner's argument parsers (the single source the CLI command handlers
// reuse) and the embedding entry point. `parse_steps` is intentionally NOT
// re-exported here — it was never part of the root facade before the split
// either (pre-existing gap, not introduced by this refactor).
pub use mimz_sim::runner::{parse_bindings, parse_sweep, parse_u128, sweep_vectors, trace_scope};
pub use mimz_sim::{compile_string, run_command, sim};
