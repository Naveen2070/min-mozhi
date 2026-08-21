# Integration: wasm_parity (`tests/wasm_parity.rs`, 2 tests — CLI vs. WASM)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Drives every `.mimz` file under `examples/` and `showcase/` through both the
native CLI and the built WASM package (via a Node.js script) and asserts
`compile`/`check` output is byte-identical. **Skips with a printed note if
`crates/mimz-wasm/pkg/` isn't built** — run `wasm-pack build crates/mimz-wasm
--target web --release` first. Catches drift where a language feature works
on one target but not the other (see `docs/log/2026-07-14.md`'s `foreach`
WASM-pkg-staleness fix).

| Test                        | Locks in                                                           |
| --------------------------- | ------------------------------------------------------------------ |
| `all_examples_work_in_wasm` | every `examples/` file compiles/checks identically on CLI and WASM |
| `all_showcase_work_in_wasm` | every `showcase/` file compiles/checks identically on CLI and WASM |
