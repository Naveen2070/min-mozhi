//! WebAssembly bindings for the Min-Mozhi (மின்மொழி) compiler.
//!
//! Exposes the in-memory [`mimz::compile_string`] pipeline to JavaScript so the
//! browser playground compiles `.mimz` to Verilog with no server and no
//! filesystem — Phase 4 web presence, build step 2
//! (`docs/plan/phase-4-web-presence.md`).
//!
//! Build for the web with `wasm-pack build crates/mimz-wasm --target web`
//! (emits the `.wasm` + JS glue the Astro site loads as an island).

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;

/// Compile a single Min-Mozhi source string to Verilog.
///
/// Resolves entirely in the browser — no filesystem, no `import`. On success the
/// returned string is the generated Verilog. On failure a JS `Error` is thrown
/// whose message is the rendered, caret-annotated diagnostics (the same text
/// `mimz compile` prints).
#[wasm_bindgen(js_name = compileToVerilog)]
pub fn compile_to_verilog(source: &str) -> Result<String, JsError> {
    mimz_sim::compile_string(source).map_err(|diagnostics| JsError::new(&diagnostics))
}

/// Run any `mimz` subcommand against the source in memory — the engine behind the
/// in-browser console.
///
/// - `command` is one of `check`, `compile`, `eval`, `sim`, `test`.
/// - `args` is the flag list after the command, e.g.
///   `["--in", "a=1", "--cycles", "8", "--trace"]`.
///
/// On success returns the command's output text (Verilog, an eval result, a sim
/// trace, …). On failure a JS `Error` is thrown whose message is the rendered
/// diagnostics or error text — both are ready to print into the console log.
#[wasm_bindgen(js_name = runCommand)]
pub fn run_command(source: &str, command: &str, args: Vec<String>) -> Result<String, JsError> {
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    mimz_sim::run_command(source, command, &argv).map_err(|e| JsError::new(&e))
}
