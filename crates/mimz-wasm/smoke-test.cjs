// Headless end-to-end check: load the wasm (Node glue from `wasm-bindgen
// --target nodejs`) and compile through `compileToVerilog`. See README.md for
// the two build steps that must run first. Exits non-zero on any failure.
const assert = require("node:assert");
const { compileToVerilog, runCommand } = require("./pkg/mimz_wasm.js");

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

// runCommand: the in-browser console engine. Exercise sim (clocked, traced),
// eval (combinational), and the error path through the same wasm entry point.
const trace = runCommand(counter, "sim", ["--cycles", "3", "--trace"]);
assert(trace.includes("count"), `sim --trace should name the output, got:\n${trace}`);
console.log("OK: runCommand sim --trace produced a trace");

const and2 = "module And2 {\n in a: bit\n in b: bit\n out y: bit\n y = a and b\n}\n";
const ev = runCommand(and2, "eval", ["--in", "a=1,b=1"]);
assert(/y = 1/.test(ev), `eval should report y = 1, got: ${ev}`);
console.log("OK: runCommand eval --in computed y = 1");

try {
  runCommand("module N {\n in a: bits[4]\n out y: bits[2]\n y = a\n}\n", "check", []);
  throw new Error("expected a check error, got none");
} catch (e) {
  assert(/E0401/.test(String(e.message)), `expected E0401, got: ${e.message}`);
  console.log("OK: runCommand check surfaces E0401");
}

console.log("wasm smoke test passed");
