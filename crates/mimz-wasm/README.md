# mimz-wasm

WebAssembly bindings for the Min-Mozhi (மின்மொழி) compiler — compile `.mimz` to
Verilog in the browser, no server. Wraps [`mimz_sim::compile_string`] (single-file,
in-memory: no filesystem, no `import` resolution).

## API

- **`compileToVerilog(source: string): string`** — returns the generated
  Verilog, or throws an `Error` whose `message` is the rendered, caret-annotated
  diagnostics (the same text `mimz compile` prints).

## Build for the web

Loaded by the site as an Astro island (ES module + wasm):

```sh
wasm-pack build crates/mimz-wasm --target web --release
```

Output lands in `crates/mimz-wasm/pkg/` (`mimz_wasm.js`, `mimz_wasm_bg.wasm`,
`*.d.ts`). `wasm-pack` runs `wasm-opt`, which strips the dead code this entry
point never reaches (e.g. the CLI's `clap`).

## Headless smoke test (no browser)

Proves load + compile through wasm-bindgen on Node:

```sh
cargo build -p mimz-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target nodejs --out-dir crates/mimz-wasm/pkg \
  target/wasm32-unknown-unknown/release/mimz_wasm.wasm
node crates/mimz-wasm/smoke-test.cjs
```

## Browser demo

After a `--target web` build, serve this folder over HTTP and open `test.html`.

[`mimz_sim::compile_string`]: https://github.com/Naveen2070/min-mozhi/blob/master/crates/mimz-sim/src/lib.rs
