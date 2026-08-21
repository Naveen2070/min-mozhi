# 10 — Test Map: What Is Covered, What Isn't, and Why

Every test, what it locks in, and what a failure means. Update this page
when tests are added or removed (the count below is asserted nowhere —
this page is the human ledger).

> **Live breakdown:** run **`cargo test-summary --workspace`** instead of
> `cargo test` — it runs the suite, then prints a per-binary table (lib unit,
> each bin, every integration suite, doctests) and a grand total.
> Cross-platform (a standalone dev crate at `tools/test-summary/`, aliased in
> `.cargo/config.toml`); forwards all `cargo test` args (`--release`,
> `--test sim`, …) and honors `REQUIRE_IVERILOG`. Use it to keep the
> hand-maintained counts above honest.
>
> **`--workspace` is required**, not optional: root `Cargo.toml` sets
> `default-members = ["."]` (fast local iteration on the shell crate, kept
> from before the 3-crate workspace split), so a bare `cargo test-summary` /
> `cargo test` only runs the root crate's own tests and silently skips
> `mimz-core` (675 lib unit + 2 crate integration) and `mimz-sim` (172 lib
> unit + 81 crate integration) — **935 tests invisible without the flag**.
> CI (`.github/workflows/ci.yml`) had this exact gap for one day
> (2026-07-10 → 2026-07-11) after the workspace split landed; fixed by
> adding `--workspace` to its clippy/test/doc/build steps.

**1315 tests** as of 2026-08-21 (`cargo test-summary --workspace`):

| Where it lives                                      |    Count | Kind                                                   |
| --------------------------------------------------- | -------: | ------------------------------------------------------ |
| `crates/mimz-core/src/**` (lib unit)                |      675 | in-process, `#[cfg(test)] mod tests`                   |
| `crates/mimz-sim/src/**` (lib unit)                 |      172 | in-process                                             |
| `src/**` (mimz shell crate, lib unit)               |       51 | in-process (`config`, `emulate`, `project`)            |
| `src/main.rs` (mimz bin unit)                       |        7 | in-process (`lsp`)                                     |
| `src/bin/mimz-bench/` (bin unit)                    |        6 | in-process                                             |
| `crates/mimz-wasm` (lib unit)                       |        0 | no unit tests — covered via `wasm_parity`              |
| doctests (×4 crates)                                |        0 | none currently — runnable examples live in `examples/` |
| `crates/mimz-sim/tests/sim_errors.rs`               |       81 | crate integration                                      |
| `crates/mimz-core/tests/width_rules_conformance.rs` |        2 | crate integration                                      |
| `tests/cli.rs`                                      |        6 | workspace integration (runs the binary)                |
| `tests/compile_string.rs`                           |       14 | workspace integration (in-process lib)                 |
| `tests/config.rs`                                   |        7 | workspace integration                                  |
| `tests/differential_fuzz.rs`                        |        6 | workspace integration (generative + Icarus)            |
| `tests/docs_sync.rs`                                |        5 | workspace integration (doc staleness guard)            |
| `tests/errors.rs`                                   |        4 | workspace integration (error fixtures)                 |
| `tests/eval.rs`                                     |       15 | workspace integration                                  |
| `tests/examples.rs`                                 |       13 | workspace integration (golden `.v`)                    |
| `tests/extern.rs`                                   |        5 | workspace integration                                  |
| `tests/fmt.rs`                                      |        9 | workspace integration                                  |
| `tests/grammar.rs`                                  |       16 | workspace integration                                  |
| `tests/grammar_sync.rs`                             |        6 | workspace integration (spec staleness guard)           |
| `tests/icarus.rs`                                   |       16 | differential (needs `iverilog`)                        |
| `tests/lsp.rs`                                      |        1 | workspace integration (smoke)                          |
| `tests/morph.rs`                                    |       20 | workspace integration                                  |
| `tests/packages.rs`                                 |        2 | workspace integration                                  |
| `tests/self_determined_regression.rs`               |      116 | workspace integration (BUG-19/20/23/24)                |
| `tests/showcase.rs`                                 |        6 | workspace integration                                  |
| `tests/sim.rs`                                      |       17 | workspace integration                                  |
| `tests/stdlib.rs`                                   |       11 | workspace integration                                  |
| `tests/test_run.rs`                                 |        9 | workspace integration                                  |
| `tests/translate.rs`                                |       15 | workspace integration                                  |
| `tests/wasm_parity.rs`                              |        2 | workspace integration (CLI vs. WASM)                   |
| **Total**                                           | **1315** |                                                        |

Fixture counts (current): **119** error fixtures (`tests/fixtures/errors/*.mimz`,
plus a `README.md` and the `e0110_support/` helper folder) · **8** grammar
fixtures · **3** extern fixtures · **3** package fixtures · **70** golden
module `.v` outputs + **17** `_tb.v` testbench goldens (**87** `.v` files
total in `tests/golden/`) + **1** `.vcd` ·
**50** Icarus self-checking testbenches · **43** `BASE_EXAMPLES` × 4
flavors + **16** pure-Tamil twins.

---

## Legend — how to read this page

| Term                   | Means                                                                                                                                                                                                                                  |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **lib unit test**      | A `#[cfg(test)] mod tests` block INSIDE the source file it tests. Fast, in-process, sees private items. Most of the suite.                                                                                                             |
| **integration test**   | A file under `tests/`. Compiled as its own crate, so it only sees the PUBLIC API — or shells out to the real `mimz` binary.                                                                                                            |
| **golden file**        | A committed expected output (`tests/golden/*.v`). The test regenerates and byte-compares. Regenerate on purpose with `MIMZ_UPDATE_GOLDENS=1`.                                                                                          |
| **fixture**            | A small input file the test loads (`tests/fixtures/`). Error fixtures declare their expected code in a header comment (`// expect: E0401`).                                                                                            |
| **differential test**  | Runs the SAME design two ways and demands identical results — usually our simulator vs. real `iverilog`/`vvp`. Catches bugs asserts cannot.                                                                                            |
| **completeness guard** | A test that fails when a list and its documentation drift apart (e.g. every error code must own a fixture). It is how this page stays honest.                                                                                          |
| **parametrized loop**  | One `#[test]` that iterates a table (`BASE_EXAMPLES`, `TESTBENCHES`). Adding a table row adds coverage WITHOUT changing the test count.                                                                                                |
| **flavor**             | One of the keyword spellings: `english`, `tanglish`, `tamil`, `mixed`, plus `tamil-pure` (Tamil keywords AND Tamil identifiers).                                                                                                       |
| **E-code / W-code**    | Compile-time diagnostic: `E` fails the build, `W` warns. Catalogs in [`11-checker.md`](11-checker.md) and [`06-diagnostics.md`](06-diagnostics.md).                                                                                    |
| **S-code**             | Run-time diagnostic from `mimz-sim` (after the checker already accepted the program). Catalog in [`13-tooling.md`](13-tooling.md).                                                                                                     |
| **Layer 1/1.5/2/3**    | Icarus depth (`tests/icarus.rs`'s own terms): 1 = every emitted `.v` is valid Verilog, 1.5 = every auto-generated `_tb.v` is too, 2 = a hand-written self-checking testbench passes, 3 = OUR simulator matches Icarus cycle-for-cycle. |

---

## Detailed Breakdown by Category

### Lib Unit Tests

- [`lib-unit/keyword-table.md`](lib-unit/keyword-table.md) — Keyword table (15 tests)
- [`lib-unit/lexer.md`](lib-unit/lexer.md) — Lexer (15 tests)
- [`lib-unit/parser.md`](lib-unit/parser.md) — Parser (102 tests across 13 files)
- [`lib-unit/checker.md`](lib-unit/checker.md) — Checker (301 tests across 11 files)
- [`lib-unit/widths-pass.md`](lib-unit/widths-pass.md) — Widths pass internals (5 tests)
- [`lib-unit/transliteration.md`](lib-unit/transliteration.md) — Transliteration (6 tests)
- [`lib-unit/emitter.md`](lib-unit/emitter.md) — Emitter (105 tests)
- [`lib-unit/testbench-emitter.md`](lib-unit/testbench-emitter.md) — Testbench emitter (5 tests)
- [`lib-unit/lint.md`](lib-unit/lint.md) — Lint (5 tests)
- [`lib-unit/explain.md`](lib-unit/explain.md) — Explain (3 tests)
- [`lib-unit/translate.md`](lib-unit/translate.md) — Translate (10 tests)
- [`lib-unit/config.md`](lib-unit/config.md) — Config (8 tests)
- [`lib-unit/version.md`](lib-unit/version.md) — Version (3 tests)
- [`lib-unit/morph.md`](lib-unit/morph.md) — Morph (14 tests)
- [`lib-unit/pretty.md`](lib-unit/pretty.md) — Pretty-printer (11 tests)
- [`lib-unit/stdlib.md`](lib-unit/stdlib.md) — Standard-library routing (5 tests)
- [`lib-unit/hardware-emulation.md`](lib-unit/hardware-emulation.md) — Hardware-emulation peripherals (42 tests)
- [`lib-unit/source-normalization.md`](lib-unit/source-normalization.md) — Source normalization (1 test)
- [`lib-unit/ast-lowering.md`](lib-unit/ast-lowering.md) — AST lowering passes (21 tests)
- [`lib-unit/checker-internals.md`](lib-unit/checker-internals.md) — Checker internals (consteval 6, drivers 2, names 3)
- [`lib-unit/wide-integers.md`](lib-unit/wide-integers.md) — Wide integers and width rules (bits 17, wide 18, width_rules 19)

### Crate Integration Tests

- [`crate-integration/sim-errors.md`](crate-integration/sim-errors.md) — Sim runtime errors (81 tests)
- [`crate-integration/width-rules-conformance.md`](crate-integration/width-rules-conformance.md) — Width rules conformance (2 tests)

### Workspace Integration Tests

- [`workspace-integration/cli.md`](workspace-integration/cli.md) — CLI (6 tests)
- [`workspace-integration/compile-string.md`](workspace-integration/compile-string.md) — Compile string (14 tests)
- [`workspace-integration/config.md`](workspace-integration/config.md) — Config (7 tests)
- [`workspace-integration/differential-fuzz.md`](workspace-integration/differential-fuzz.md) — Differential fuzzing (6 tests)
- [`workspace-integration/docs-sync.md`](workspace-integration/docs-sync.md) — Docs sync (5 tests)
- [`workspace-integration/errors.md`](workspace-integration/errors.md) — Error fixtures (4 tests)
- [`workspace-integration/eval.md`](workspace-integration/eval.md) — Eval (15 tests)
- [`workspace-integration/examples.md`](workspace-integration/examples.md) — Examples (13 tests)
- [`workspace-integration/extern.md`](workspace-integration/extern.md) — Extern module (5 tests)
- [`workspace-integration/fmt.md`](workspace-integration/fmt.md) — Fmt (9 tests)
- [`workspace-integration/grammar.md`](workspace-integration/grammar.md) — Grammar engine (16 tests)
- [`workspace-integration/grammar-sync.md`](workspace-integration/grammar-sync.md) — Grammar sync (6 tests)
- [`workspace-integration/icarus.md`](workspace-integration/icarus.md) — Icarus differential (16 tests)
- [`workspace-integration/lsp.md`](workspace-integration/lsp.md) — LSP (1 test)
- [`workspace-integration/morph.md`](workspace-integration/morph.md) — Morph (20 tests)
- [`workspace-integration/packages.md`](workspace-integration/packages.md) — Packages (2 tests)
- [`workspace-integration/self-determined-regression.md`](workspace-integration/self-determined-regression.md) — Self-determined regression (116 tests)
- [`workspace-integration/showcase.md`](workspace-integration/showcase.md) — Showcase (6 tests)
- [`workspace-integration/sim.md`](workspace-integration/sim.md) — Sim (17 tests)
- [`workspace-integration/stdlib.md`](workspace-integration/stdlib.md) — Stdlib (11 tests)
- [`workspace-integration/test-run.md`](workspace-integration/test-run.md) — Test run (9 tests)
- [`workspace-integration/translate.md`](workspace-integration/translate.md) — Translate (15 tests)
- [`workspace-integration/wasm-parity.md`](workspace-integration/wasm-parity.md) — WASM parity (2 tests)

### Simulator Tests

- [`simulator/combinational.md`](simulator/combinational.md) — Combinational evaluator (22 tests)
- [`simulator/value-model.md`](simulator/value-model.md) — Value model + fn-body interpreter (38 tests)
- [`simulator/elaboration.md`](simulator/elaboration.md) — Elaboration (25 tests)
- [`simulator/kernel.md`](simulator/kernel.md) — Kernel (30 tests)
- [`simulator/run-vcd-trace.md`](simulator/run-vcd-trace.md) — Sim runner / VCD / console trace (18 tests)
- [`simulator/playground-runner.md`](simulator/playground-runner.md) — Playground runner (14 tests)
- [`simulator/test-harness.md`](simulator/test-harness.md) — Test harness (27 tests)
- [`simulator/sim-integration.md`](simulator/sim-integration.md) — Sim integration (17 tests)

---

## Changelog of Test-Count Changes

See [`test-map-changelog.md`](test-map-changelog.md) for the full history of test count changes.
