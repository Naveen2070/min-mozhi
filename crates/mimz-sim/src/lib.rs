//! Event-driven simulator + in-memory command runner. Depends only on
//! `mimz-core` — no optional dependencies, no filesystem/OS access, no
//! knowledge of hardware-emulation peripherals (that's the shell crate's
//! `EmulationHost` implementation, plugged in through `sim::host`). The
//! pure/impure boundary from the workspace split
//! (`docs/plan/workspace-split.local.md`).
#![forbid(unsafe_code)]

pub mod runner;
pub mod sim;

pub use runner::run_command;

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
