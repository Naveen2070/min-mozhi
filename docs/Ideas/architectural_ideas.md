# Architectural Ideas & Future Challenges

While the foundation of Min-Mozhi is incredibly solid, here are a few architectural challenges and improvements to consider as the language and tooling mature.

## 1. AST Error Recovery for the LSP

Language Servers (LSPs) need to provide diagnostics even when the code is broken (e.g., while the user is actively typing). If the parser halts on the first syntax error, the LSP experience degrades.

**Idea:** Upgrade the parser to support "Error Recovery" (generating an incomplete AST with `Error` nodes). This ensures that hover states, semantic highlighting, and autocomplete still work on half-written lines or files with syntax errors.

## 2. Fuzzing and Differential Testing

Because Min-Mozhi ships its own simulator _and_ a Verilog emitter, the project inherently has two sources of truth.

**Idea:** Build a Differential Testing suite in CI. This would involve a fuzzer that:

1. Generates random, valid Min-Mozhi code.
2. Runs it through the built-in Min-Mozhi simulator.
3. Compiles the same code to Verilog.
4. Runs the Verilog through a trusted, industry-standard simulator (like Verilator or Icarus Verilog).
5. Asserts that the VCD waveforms from both simulators are byte-for-byte identical.

## 3. Black-box / External IP Integration

Eventually, hardware engineers will need to instantiate primitives that Min-Mozhi can't express natively (e.g., FPGA-specific DSP slices, PLLs, or hardened PCIe IP).

**Idea:** Design a clean `extern module` system. This would allow users to instantiate raw Verilog black-boxes securely, mapping Verilog ports to Min-Mozhi types without breaking the safety checks and width-inference of the core compiler.

## 4. Keeping the Core Wasm-Friendly

Compiling to WebAssembly for the playground (via `crates/mimz-wasm`) is the absolute best way to teach the language without installation friction.

**Idea:** To keep the Wasm build viable and lightweight, ensure the core compiler architecture strictly isolates OS-level operations (like File I/O, multithreading, or environment variables) from the parsing, checking, and emitting logic. The core compiler library should remain perfectly pure: it should only ever take strings as input and return strings/ASTs as output.
