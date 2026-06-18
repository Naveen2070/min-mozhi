// Headless end-to-end check: load the wasm (Node glue from `wasm-bindgen
// --target nodejs`) and compile through `compileToVerilog`. See README.md for
// the two build steps that must run first. Exits non-zero on any failure.
const assert = require("node:assert");
const { compileToVerilog } = require("./pkg/mimz_wasm.js");

// Success path: the canonical counter compiles to Verilog naming the module.
const counter = `module Counter(WIDTH: int = 8) {
  clock clk
  reset rst
  out count: bits[WIDTH]
  reg value: bits[WIDTH] = 0
  on rise(clk) {
    value <- value +% 1
  }
  count = value
}`;
const verilog = compileToVerilog(counter);
assert(verilog.includes("module Counter"), "expected `module Counter` in output");
console.log(`OK: compiled Counter -> Verilog (${verilog.length} bytes)`);

// Error path: a width mismatch throws with the rendered diagnostic (E0401).
try {
  compileToVerilog("module N {\n  in a: bits[4]\n  out y: bits[2]\n  y = a\n}\n");
  throw new Error("expected a compile error, got none");
} catch (e) {
  assert(/E0401/.test(String(e.message)), `expected E0401, got: ${e.message}`);
  console.log("OK: width mismatch throws E0401");
}

console.log("wasm smoke test passed");
