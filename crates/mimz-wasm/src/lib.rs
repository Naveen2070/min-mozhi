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
    mimz::compile_string(source).map_err(|diagnostics| JsError::new(&diagnostics))
}
