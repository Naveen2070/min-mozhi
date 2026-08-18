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
> `default-members = ["."]` (fast local iteration on the shell crate,
> `docs/plan/workspace-split.local.md`), so a bare `cargo test-summary` /
> `cargo test` only runs the root crate's own tests and silently skips
> `mimz-core` (607 lib unit + 2 crate integration) and `mimz-sim` (157 lib
> unit + 79 crate integration) — **845 tests invisible without the flag**.
> CI (`.github/workflows/ci.yml`) had this exact gap for one day
> (2026-07-10 → 2026-07-11) after the workspace split landed; fixed by
> adding `--workspace` to its clippy/test/doc/build steps.

**1115 tests** as of 2026-08-02 (`cargo test-summary --workspace`):

| Where it lives                                      |    Count | Kind                                                   |
| --------------------------------------------------- | -------: | ------------------------------------------------------ |
| `crates/mimz-core/src/**` (lib unit)                |      607 | in-process, `#[cfg(test)] mod tests`                   |
| `crates/mimz-sim/src/**` (lib unit)                 |      157 | in-process                                             |
| `src/**` (mimz shell crate, lib unit)               |       51 | in-process (`config`, `emulate`, `project`)            |
| `src/main.rs` (mimz bin unit)                       |        7 | in-process (`lsp`)                                     |
| `src/bin/mimz-bench/` (bin unit)                    |        6 | in-process                                             |
| `crates/mimz-wasm` (lib unit)                       |        0 | no unit tests — covered via `wasm_parity`              |
| doctests (×4 crates)                                |        0 | none currently — runnable examples live in `examples/` |
| `crates/mimz-sim/tests/sim_errors.rs`               |       79 | crate integration                                      |
| `crates/mimz-core/tests/width_rules_conformance.rs` |        2 | crate integration                                      |
| `tests/cli.rs`                                      |        6 | workspace integration (runs the binary)                |
| `tests/compile_string.rs`                           |       14 | workspace integration (in-process lib)                 |
| `tests/config.rs`                                   |        7 | workspace integration                                  |
| `tests/differential_fuzz.rs`                        |        4 | workspace integration (generative + Icarus)            |
| `tests/docs_sync.rs`                                |        4 | workspace integration (doc staleness guard)            |
| `tests/errors.rs`                                   |        4 | workspace integration (error fixtures)                 |
| `tests/eval.rs`                                     |       15 | workspace integration                                  |
| `tests/examples.rs`                                 |       13 | workspace integration (golden `.v`)                    |
| `tests/extern.rs`                                   |        5 | workspace integration                                  |
| `tests/fmt.rs`                                      |        9 | workspace integration                                  |
| `tests/grammar.rs`                                  |       16 | workspace integration                                  |
| `tests/grammar_sync.rs`                             |        6 | workspace integration (spec staleness guard)           |
| `tests/icarus.rs`                                   |       10 | differential (needs `iverilog`)                        |
| `tests/lsp.rs`                                      |        1 | workspace integration (smoke)                          |
| `tests/morph.rs`                                    |       20 | workspace integration                                  |
| `tests/packages.rs`                                 |        2 | workspace integration                                  |
| `tests/self_determined_regression.rs`               |       12 | workspace integration (BUG-19/20/23/24)                |
| `tests/showcase.rs`                                 |        6 | workspace integration                                  |
| `tests/sim.rs`                                      |       17 | workspace integration                                  |
| `tests/stdlib.rs`                                   |       11 | workspace integration                                  |
| `tests/test_run.rs`                                 |        7 | workspace integration                                  |
| `tests/translate.rs`                                |       15 | workspace integration                                  |
| `tests/wasm_parity.rs`                              |        2 | workspace integration (CLI vs. WASM)                   |
| **Total**                                           | **1115** |                                                        |

Fixture counts (current): **117** error fixtures (`tests/fixtures/errors/*.mimz`,
plus a `README.md` and the `e0110_support/` helper folder) · **8** grammar
fixtures · **3** extern fixtures · **3** package fixtures · **70** golden
module `.v` outputs + **17** `_tb.v` testbench goldens + **1** `.vcd` ·
**50** Icarus self-checking testbenches · **43** `BASE_EXAMPLES` × 4
flavors + **16** pure-Tamil twins.

## Legend — how to read this page

New here? These words mean specific things in this repo:

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

Changelog of test-count changes (newest first):

- 2026-08-18 round-7 plan Tasks 10–11 (`docs/plan/v0.2-class-closure-round7.local.md`)
  — no new tests, three widened. **Task 10**: all three `tests/icarus.rs`
  corpus sweeps walked `examples/` only, each with its own copy of the same
  directory walk, so `demo/cpu.mimz`'s emitted testbench sat outside the very
  test that closed BUG-64/65. All three — plus Task 1's sweep in
  `tests/self_determined_regression.rs`, the only one that already covered
  `demo/` — now share one `support::corpus_files()` (`examples/` + `demo/`,
  226 files). Stale floors raised off their placeholders: `>= 48` → `>= 226`
  files, `>= 5` → `>= 50` testbenches, `>= 5` → `>= 50` files / `>= 90`
  modules. **Task 11**: `gen_special_leaves` (`tests/differential_fuzz.rs`)
  now generates a second `fn` callable from the first's body, closing the
  grammar-reachability hole BUG-67 sat in; acceptance criterion re-derived by
  reverting BUG-67's fix (clocked seed `202427811`, inside 400), 5000/5000
  green with it restored. Workspace unchanged at 1291.
  **Note:** the per-section counts below were last reconciled 2026-08-02 at
  1115 and have not been re-reconciled since; treat them as of that date.

- 2026-08-02 documentation audit (no test-behavior change): the master
  count and per-section counts on this page were reconciled against
  `cargo test --workspace --all-features -- --list` — **1034 → 1115**,
  which is the count that was already true, not new tests. Sections were
  added for suites this page had never listed (`crates/mimz-sim/tests/
sim_errors.rs` 79, `tests/self_determined_regression.rs` 12,
  `tests/differential_fuzz.rs` 4, `tests/extern.rs` 5,
  `crates/mimz-core/tests/width_rules_conformance.rs` 2, the wide-integer
  units `bits`/`wide`/`width_rules` 50, the AST lowering units
  `foreach_lower`/`sync_loop_lower`/`sync_prim_lower`/`ast` 21, `pretty` 8,
  `stdlib` 5, `runner` 13, and the 42 `emulate` peripheral units), the
  emitter section was restructured from one file into the
  `emit_verilog/tests/` topic split, and the simulator sections were
  corrected for the `elaborate.rs`/`value.rs`/`harness.rs` → directory
  splits. Fixture counts refreshed (error 106 → 117, `_tb.v` 14 → 17,
  Icarus TBs 45 → 50).

- 2026-07-26 oversized test-file split (branch `oversized-test-file-split`,
  zero test-behavior change — every test moved file, none renamed/added/
  removed): `checker/tests.rs` (3026 lines) → `checker/tests/` (`mod.rs`
  helpers + 11 topic files, 268 tests); `parser/tests.rs` (1594 lines) →
  `parser/tests/` (`mod.rs` helpers + 13 topic files, 91 tests). This page's
  checker/parser sections below were restructured to match, and the master
  count/breakdown above was corrected to the real `cargo test-summary
--workspace` total (1034) — it had drifted independently of this split
  (the two prior entries below already flagged their own staleness).

- 2026-07-21 `sync.pulse` Icarus differential example (Task 8 of
  `docs/superpowers/plans/2026-07-20-sync-cdc.local.md`, branch
  `phase-2-correctness-consolidation-part2`): 4-flavor `sync_pulse` example
  (`BASE_EXAMPLES` 42 → 43, +1 golden `sync_pulse.v`, 69 → 70). The example
  itself deviates from the task brief's literal snippet: `sync.pulse`'s
  checker (E0704) requires the signal argument to be EXACTLY a register
  owned by `src_clock` (unlike `double_flop`, no domain-free source is
  allowed), so the brief's bare `in src_pulse: bit` fed straight into
  `sync.pulse(...)` fails to compile — fixed by adding an intermediate
  `reg src_reg` sampled by its own `on rise(clk_src)` block first, then
  passing `src_reg` to `sync.pulse`, mirroring
  `sync_pulse_produces_a_one_cycle_dst_pulse_after_toggle`'s own module
  shape in `crates/mimz-sim/src/sim/harness.rs`. **+1 hand-written Icarus
  TB** (`sync_pulse_tb.v`, 44 → 45) and **+1 `#[test]`**
  `sync_pulse_matches_icarus` in `tests/icarus.rs` (Layer 2 style, same
  reasoning as Task 7's `sync_double_flop_matches_icarus` — two clocks,
  can't use `differential()`). Icarus differential 9 → 10. No tamil-pure
  twin added (same vocabulary-invention concern Task 7 raised for
  `sync_double_flop` — inventing a Tamil term for a CDC pulse synchronizer
  without native-speaker review); note this is a minority-precedent call,
  not a hard rule — 16 of the (now) 43 `BASE_EXAMPLES` have a tamil-pure
  twin, including two recently-added ones (`tested_adder`→`tested_kuutti`,
  `foreach_sum`→`kootu`), so "recent examples never get one" would be
  false. Together, `sync_double_flop_matches_icarus` and
  `sync_pulse_matches_icarus` satisfy the spec's §7 ask for "at least one
  kernel-vs-Icarus multi-clock test that exercises an actual crossing
  end-to-end" — both are real multi-clock crossings run against real
  Icarus, so no separate test was added beyond the two per-primitive
  examples. (This changelog entry does not reconcile the master test-count
  line or the `_tb.v`/testbench-count figures above, which were already out
  of sync with `cargo test-summary --workspace`'s actual count before this
  task — out of scope for a single-example addition, same caveat as Task 7's
  own entry below.)

- 2026-07-21 `sync.double_flop` Icarus differential example (Task 7 of
  `docs/superpowers/plans/2026-07-20-sync-cdc.local.md`, branch
  `phase-2-correctness-consolidation-part2`): 4-flavor `sync_double_flop`
  example (`BASE_EXAMPLES` 41 → 42, +1 golden `sync_double_flop.v`, 68 → 69),
  **+1 hand-written Icarus TB** (`sync_double_flop_tb.v`, 43 → 44) and
  **+1 `#[test]`** `sync_double_flop_matches_icarus` in `tests/icarus.rs`
  (Layer 2 style, not Layer 3 — the two-clock design can't use
  `differential()`'s single-clock default stimulus). Icarus differential
  8 → 9 tests. (This changelog entry does not reconcile the master test-count
  line or the `_tb.v`/testbench-count figures above, which were already out
  of sync with `cargo test-summary --workspace`'s actual 955 before this
  task — out of scope for a single-example addition.)

- 2026-07-11 bundle-typed fn arg/return width shape-checking (checker,
  `Ty::Bundle` consolidation) + workspace-split test-visibility fix: true
  count was already 737 post-split (mimz-core 399 + mimz-sim 97 absorbed the
  old single-crate 480 lib-unit figure, plus new tests), but `cargo test` /
  CI only saw 241 without `--workspace` — see the callout above. 663 → 737
  (net of both the new tests and the crate-count reconciliation).
  **+5 unit tests** in `checker::widths::tests` (a pocket inside
  `crates/mimz-core/src/checker/widths/mod.rs`, distinct from the sibling
  `checker::tests` module below):
  `bundle_typed_fn_param_supports_field_access` (a bundle-typed fn param's
  field access resolves via `cx.sigs` instead of false-E0105ing),
  `module_param_field_access_is_rejected` (`W.foo` on an int/bool module
  param now errors instead of silently passing),
  `mem_field_access_reports_exactly_one_diagnostic` (mem/clock/reset field
  access reports pass-3's diagnostic once, not doubled with `field_ty`'s),
  `enum_variant_from_wrong_enum_is_rejected` (assigning a variant from the
  wrong enum into an enum-typed reg/wire is caught — was a silent
  zero-diagnostic regression), `bundle_literal_tail_return_is_shape_checked`
  (a bundle-literal fn-tail return goes through `check_return_expr`, not the
  old `infer_ty`+`check_return_ty` path). **+4 error fixtures**: 102 → 106 —
  `E0901_bundle_fn_arg_missing_field.mimz`,
  `E0907_bundle_fn_arg_type_mismatch.mimz`,
  `E0804_bundle_return_type_mismatch.mimz`,
  `E0901_bundle_return_missing_field.mimz` (all under
  `tests/fixtures/errors/`).
- 2026-07-06 `loop` and `sync loop` features + bundles (branch `phase-2-sync-loop` and `phase-2-interfaces-bundles`):
  Added `loop` unrolling in combinational `fn` bodies and `on` blocks.
  Added `sync loop` grammar, lowering to primitive states, and checker verification.
  Added `bundle` types, parsing, checker validation, and emitter flattening.
  Suite 528 → 663.

- 2026-06-30 `const if` elaboration (branch `phase-2-default-and-const-if`, Tasks 7–10):
  `ModuleItem::ConstIf` AST + parser (one-token lookahead), E0811 checker, all-passes
  winning-branch recursion, 4-flavor `debug_wrapper` example + golden.
  **+1 parser unit** (`const_if_items_parse_correctly`), **+1 checker unit** (`e0811_const_if_not_const`).
  +1 error fixture (`e0811_const_if_not_const.mimz`), +1 golden (`debug_wrapper.v`). `BASE_EXAMPLES` 35 → 36.
  Suite 526 → 528.

- 2026-06-30 `default` assignments — Thamizh-order parser fix (branch `phase-2-default-and-const-if`):
  `seq_stmt_thamizh()` missed `Kw::Default` guard — `default` is word-order neutral, always leads.
  Fixed; all translate tests (idempotency + Verilog-preservation) pass. Suite 523 → 526.

- 2026-06-30 `default` assignments (branch `phase-2-default-and-const-if`, Tasks 2–6):
  Promoted `default` keyword (`Kw::Default`), `SeqStmt::Default` AST + parser, E0809/E0810
  checker passes, two-pass emitter, sim, and surface wiring. 4-flavor `pulse_gen` example.
  **+1 lexer unit** (`kw_default_is_recognized`), **+2 checker unit** (`e0809_default_target_not_reg`,
  `e0810_duplicate_default`). +2 error fixtures, +1 golden (`pulse_gen.v`). `BASE_EXAMPLES` 34 → 35.
  Suite 522 → 523.

- 2026-06-29 OR-arm binding intersection (branch `phase-2-tagged-unions`, Tasks 1–3):
  E0808 algorithm in `crates/mimz-core/src/checker/names.rs` (5-phase intersection). **+6 lib unit**
  (checker/tests.rs: 2 positive — 2-way OR-arm clean, 3-way OR-arm clean; 4 negative
  — name missing, extra name, width mismatch, wildcard-not-binding). Also updated
  stale pre-existing counts (lib unit 349 → 356, checker 112 → 133, sim integration
  10 → 13) to match `cargo test-summary` actuals. Suite 513 → 521.

- 2026-06-28 Tagged-union T7 surface (branch `phase-2-comb-function`): pretty-printer
  (`enum_decl`/`pattern`), critical translit fix (binding names in
  `Pattern::Variant` were not being walked — silent payload-slice miss in
  pure-Tamil), spec v0.2.15, 5 new example files (`tagged_packet.mimz` × 4
  flavors + `sirappu_pothi.mimz` tamil-pure), E0806/E0807 added to
  `ALL_CHECKER_CODES` (41 → 43) + 2 new error fixtures. Fixed
  `compile_string.rs` golden path collision (`tagged_packet.v` →
  `tagged_packet_decoder.v`). **+14 net** over the 499 baseline (T1–T6
  checker/parser/emitter/sim unit tests + T7 fixture/compile_string additions).
  Suite 499 → 513.

- 2026-06-28 Five post-review bug fixes (branch `phase-2-comb-function`): (1) CtInt locals
  no longer leave `inferred_width` at None (emitter panic fixed); (2) `render_fn_decl` uses
  `mem::replace` with file-level env instead of `mem::take` (file const folding in fn bodies);
  (3) sim elaborate + comb now collect functions from ALL project files (D3 cross-file fns);
  (4) E0805 help text corrected (reports back-edge fn, not every fn in cycle); (5) dev log
  E-code labels corrected. Two new four-flavor examples (`fn_const_local`, `fn_with_const`)
  - two new goldens. **+1 lib unit** (`fn_with_const_local_compiles_clean` — checker/tests.rs).
    Suite 498 → 499.

- 2026-06-28 Combinational functions wrap-up (branch `phase-2-comb-function`, Task 12).
  Removed all stale `// ponytail: temporary arm` comments from 6 src/ files (the arms
  were already correct; the comments were scaffolding from Task 2). Spec/02 bumped to
  v0.2.14 (fnDecl + fnCall EBNF, missing v0.2.13 clog2 changelog entry added); spec/03
  bumped to v0.2.12 (fn promotion from reserved to active). **+1 lib unit**
  (`fn_decl_parses_in_thamizh_order` — parser/tests.rs) **+1 translate integration**
  (`fn_keyword_translates_across_all_flavors` — tests/translate.rs). Suite 496 → 498.

- 2026-06-26 CLI subcommands and DX (branch `cli-and-code-improvements`). New subcommands: `init`, `doctor`, `completions`, `repl`, `lint`. `check --watch` for continuous rechecking. Colorized diagnostics + test output via `owo-colors`. Global `-q`/`--quiet` and `-d`/`--debug` flags. `--lang` restructured to Clap `ValueEnum` with aliases. `crates/mimz-core/src/lint.rs`: style/hygiene lint passes (W0002 snake_case, W0003 PascalCase). `tests/cli.rs`: 6 smoke + integration tests for doctor, init, watch, completions. **+5 lib unit** (lint: snake_case ×2, PascalCase ×2, empty-file clean) **+6 cli integration** (new `tests/cli.rs`). Suite 465 → 476.

- 2026-06-25 LSP DX (branch `phase-4-lsp-dx`). `mimz lsp` serves hover (type + doc-on-type), go-to-definition (cross-file, `test` blocks, `import` targets), and completion (scope identifiers + flavor keywords). `crates/mimz-core/src/analysis.rs`: symbol index, `resolve_at` offset-to-definition resolver, completions — scope idents + flavor keywords. `src/lsp.rs`: LSP server wired through Tower LSP. `KeywordTable::canonical_spellings` for flavor-aware keyword completion. **+12 lib unit** (analysis.rs: symbol index, resolve_at, completions; lsp.rs: handlers; tests for each) **+1 LSP unit (bin)** (`lsp.rs` smoke). Suite 456 → 469.

- 2026-06-24 Importable `std.*` library (branch `stdlib-importable-path`). `import std.fifo` (and `serkka nuulagam.varisai` / `சேர்க்க நூலகம்.வரிசை`) now resolve to an **embedded** standard library — `crates/mimz-core/src/stdlib.rs` `include_str!`s the already-tested `examples/english/std/*.mimz` + `examples/tamil-pure/*.mimz` (zero duplication), so resolution needs no install path and works in WASM. Routing keys on the written alias: English stem → canonical module, twin name/romanization → pure-Tamil twin. `src/project.rs` gained a `std` branch (`load_project_with_lib`) that parses the embedded `&str` into a synthetic in-memory file, or loads `<dir>/<m>.mimz` when `mimz.toml [lib] std` overrides; `src/config.rs` gained the `[lib]` section + `resolve_with_path`; `mimz eject std` (`src/commands/eject.rs`, `stdlib::eject_to`) vendors the library all-or-nothing. New loader code **E1202** (bad std import) added to `crates/mimz-core/src/explain.rs` + `06-diagnostics.md`. **+8 lib unit** (5 in `crates/mimz-core/src/stdlib.rs`: aliases, canonical/twin routing, unknown-module, no-transitive-imports invariant; 3 in `src/config.rs`: `[lib]` parse, unknown-key reject, `resolve_with_path` location) **+11 stdlib integration** (new `tests/stdlib.rs`: embedded resolve + entry-stays-`files[0]` ordering, Tamil twin routing, unknown/arity E1202, relative-import regression, `[lib]` override wins + twin-spelling override matches eject, 3 eject + all-or-nothing partial-conflict). Spec/02 §1.5 gained the `std.*` clause. A post-review fix corrected two bugs the green suite missed: embedded std modules were pushed ahead of the entry (breaking the `files[0] == entry` invariant `sim`/`test` rely on), and the `[lib]` override keyed the filename on the raw written alias instead of the resolved variant (so a Tamil-twin-name import missed the ejected `varisai.mimz`). Suite 455 → 456.

- 2026-06-23 BUG-6 (left-shift truncation) fixed in `crates/mimz-sim/src/sim/value.rs`. +1 lib unit (`shl_does_not_truncate_to_left_operand_width`). The shift example (`examples/english/shift.mimz`) was rewritten to follow the template (header + inline tests), mixed flavor added, and a real pure-Tamil twin `tamil-pure/nakartthi.mimz` created (replacing the old `shift.mimz` which had English identifiers). Both registered: `BASE_EXAMPLES` 28 → 29, `PURE_TAMIL` 12 → 13 (`tests/examples.rs`); `nakartthi` added to the `tests/icarus.rs` differential. The FIFO workaround (explicit `DEPTH` param) was reverted — all 4 flavors + `varisai` now use `1 << AW`. The FIFO doc page was updated accordingly (removed `DEPTH` parameter row). **No new test functions** beyond the shl unit test — the example and the revert ride the existing parametrized loops. Suite count 436 → 437.

- 2026-06-23 stdlib modules `seg7`, `pwm`, `fifo`, `uart_tx` shipped (after `debouncer`), each in all four flavors + a pure-Tamil twin (`ennkaatti`, `minukki`, `varisai`, `anuppi`), with inline `test` blocks, module + emitted-testbench goldens, and a hand-written self-checking Icarus testbench. **No new test functions** — the modules ride the existing parametrized loops, so `BASE_EXAMPLES` 24 → 28, `PURE_TAMIL` 8 → 12 (`tests/examples.rs`) and `TESTBENCHES` 17 → 21, `PURE_TESTBENCHES` 7 → 11 (`tests/icarus.rs`) auto-extend coverage. Suite count unchanged at 436.

- 2026-06-22 Parser AST error recovery (`phase-4-parser-ast-error-recovery` branch; Phase 4 LSP prerequisite, `architectural_ideas.md` idea 1). New `Error(Span)` variant on `TopItem`/`ModuleItem`/`SeqStmt`/`TestStmt` + a non-discarding `parser::parse_recover` entry point that leaves an `Error` placeholder at each recovery boundary instead of dropping the broken construct (the strict `parse` is unchanged — any error still discards the tree, so codegen never sees an `Error` node). Every consumer handles the variant (checker skips, codegen treats as unreachable). +4 lib unit (`parser`: `parse_recover_keeps_good_items_around_a_bad_one`, `parse_recover_top_level_error_keeps_following_module`, `parse_recover_seq_and_test_blocks_emit_error_nodes`, `strict_parse_still_errs_on_bad_input`). Suite 432 → 436.
- 2026-06-22 Fuzz crash fix: `is_word_byte` was missing `?`, so `push_guarded` in `translate::reskin` didn't insert a separating space when a `MaskedInt` ending with `?` (e.g. `0b1?`) abutted a romanized identifier, causing the re-lexer to consume `0b1?rrrram` as a single invalid number. +2 lib unit (`masked_int_q_does_not_glue_onto_romanized_identifier`, `masked_int_q_does_not_glue_onto_english_keyword`). Also: rebuilt `crates/mimz-wasm/pkg/` with `--target nodejs` + fixed `pkg/package.json` `"type": "commonjs"` — `wasm_parity` now passes locally on Node 24 (was a pre-existing ESM/CJS interop failure). Site `npm run build` auto-runs `build:wasm` to regenerate the web glue. Suite 430 → 432.

- 2026-06-22 Reserved `extern` (external-Verilog / black-box-IP module; `docs/Ideas/architectural_ideas.md` idea 3) ahead of the v0.1.0 freeze (R11): added to `lang/keywords.toml` `reserved` + spec/03 v0.2.11 + the grammar invalid pattern + a lexer test. The three separate reserved-word keyword-table tests (`fn_and_function_are_reserved`, `the_v03_backlog_keywords_are_reserved`, `the_section8_keywords_are_reserved`) were merged into one data-driven `future_keywords_are_reserved_not_usable` that also covers `extern`. Net −2 lib unit (3 removed, 1 added). Suite 432 → 430.

- 2026-06-22 WASM↔CLI Verilog parity + testbench golden/Icarus coverage. New `tests/wasm_parity.rs` asserts the `mimz-wasm` `compileToVerilog` binding emits byte-identical Verilog to the CLI's `compile` — the CLI writes to a temp `-o` path the test reads then deletes (cleaned up even if the assertion fails), so the comparison is file-content vs binding output, not status-line vs Verilog; skips with a note when `crates/mimz-wasm/pkg/` isn't built. The `--emit-testbench` work also landed `emitted_testbench_matches_the_goldens` + `emit_testbench_without_test_blocks_notes_and_writes_only_v` (`tests/examples.rs`) and `every_emitted_testbench_passes_iverilog` (`tests/icarus.rs`). +2 example integration, +1 Icarus differential, +1 wasm_parity integration. Suite 428 → 432.
- 2026-06-21 Testbench emitter (`crates/mimz-core/src/emit_verilog/testbench.rs`) `--emit-testbench` fixes: `test_env` now merges the DUT's module-parameter defaults for any arg a test doesn't override (mirrors `sim::elaborate::elaborate_module`'s override-or-default order), and args chain left-to-right so a later arg can reference an earlier one (mirrors `sim::harness::params`) — without this, a defaulted param omitted by a test, or `M(W: 8, DEPTH: W * 2)`-style chaining, failed to resolve width expressions. Also: two tests whose names sanitize to the same Verilog module identifier (e.g. `"edge case"` and `"edge_case"` both → `edge_case_tb`) are now rejected with a diagnostic instead of silently emitting two same-named modules. +3 lib unit (`test_env_falls_back_to_module_param_defaults`, `test_env_chains_earlier_args`, `colliding_sanitized_test_names_are_rejected`). Suite 425 → 428.
- 2026-06-21 Testbench emitter (`crates/mimz-core/src/emit_verilog/testbench.rs`) security and logic hardening — added `sanitize_verilog_ident` helper, bounded loop iteration counts, properly recursed into nested conditionals within inline tests, and pushed `consteval` errors gracefully. +1 lib unit (`sanitize_verilog_ident_replaces_invalid_chars`). Suite 424 → 425.
- 2026-06-20 Re-audit `crates/mimz-sim/src/sim/value.rs`: Finding A — `BinOp::Shl` used bare `r.bits as u32` to cast the shift amount, silently truncating when bit ≥ 32 was set (e.g. `1 << (1 << 32)` became `1 << 0` = 1 instead of 0). Also corrected `BinOp::Shr`'s `.min(127)` guard which avoided the truncation panic but produced wrong results (shift-by-128 became shift-by-127 instead of 0). Both fixed with `if r.bits >= 128 { 0 } else { … as u32 }`. +7 lib unit in `sim::comb::tests` (all new, section below). Suite 417 → 424.
- 2026-06-19 Two new pure-Tamil showcase examples so the playground's six curated examples (counter, adder, comparator, mux4, blinker, traffic*light) exist in **every** flavor — `examples/tamil-pure/kuutti.mimz` (adder twin) and `saalaivilakku.mimz` (traffic-light FSM twin), both Tamil keywords AND identifiers. `PURE_TAMIL` (in `tests/examples.rs` and `tests/translate.rs`) grew 4 → 6, so the equivalence, golden, and round-trip checks now cover them (new goldens `tests/golden/tamil_pure*{kuutti,saalaivilakku}.v`); the Icarus suite gained matching self-checking testbenches (`tests/icarus/{kuutti,saalaivilakku}\_tb.v`) + bit-for-bit differentials. **No new `#[test]` functions\*\* (these ride existing loop-driven tests), so the count is unchanged at 417.
- 2026-06-19 Website Phase 4 — the interactive playground waveform. The runner (`crates/mimz-sim/src/runner.rs`) gained a `ports` command (emits the module interface as JSON — `{module, clocked, inputs[], outputs[]}` — so the browser can build input controls without re-parsing) and a `sim --steps "a=3,b=5;a=7,b=1"` flag (explicit per-step input vectors, fed straight into the existing `comb_run`; rejected for clocked designs). The `/playground` got a stimulus panel — an editable step table for combinational designs (the fix for "an adder with a fixed input draws flat") and held-inputs + cycles for clocked ones — that re-simulates live, plus a hover cursor on the canvas reading each signal's value at a time point. +4 lib unit (`runner`: ports×2, sim_steps×2). Suite 413 → 417.
- 2026-06-18 Website Phase 4 step 5 — the playground waveform viewer. The runner's `sim` gained a `--vcd` flag (returns the 2-state VCD from `sim::vcd::to_vcd` instead of a console trace), so the in-browser **Simulate** button gets a waveform via the existing `runCommand` (no new wasm binding). New `site/src/components/WaveformViewer.tsx` — a self-contained canvas renderer behind the stable `vcd` prop (parses the VCD; square waves for 1-bit, value-labelled buses for wider signals; Surfer is the documented future drop-in). +1 lib unit (`runner::sim_vcd_emits_a_vcd_document`). Suite 412 → 413.
- 2026-06-18 Website Phase 4 step 4 — the in-browser playground console. New `crates/mimz-sim/src/runner.rs` (private lib module, re-exported): a filesystem-free `run_command(source, command, argv)` that runs `check`/`compile`/`eval`/`sim`/`test` against a source string and returns the text a user would see, composing the existing lib pipeline (`comb::eval_outputs`, `elaborate`, `run`/`comb_run`, `trace::render`). The `--in`/`--param`/`--sweep`/trace-scope parsers were **lifted from the CLI's `commands/helpers.rs` into the lib** (single source; the CLI now re-exports them), and `compile_string` is now a thin wrapper over `run_command`. The wasm crate gained `runCommand`; the site got a `/playground` page (textarea editor + console, a `client:only` React island over the web wasm). +5 lib unit (`runner`: sweep×2, check, eval, sim), −2 command unit (the moved `sweep_vectors` tests). Suite 409 → 412.
- 2026-06-18 Website Phase 2 (WASM groundwork) — `mimz::compile_string` (`src/lib.rs`): the filesystem-free `lex→parse→check→transliterate→emit` entry point behind the browser playground (single-file; `import` rejected with a plain message). New `crates/mimz-wasm` (wasm-bindgen `compileToVerilog`) + a Cargo workspace; the CLI-only deps (`tokio`/`tower-lsp`/`memory-stats`) were made optional and feature-gated (`default = ["lsp", "bench"]`) so the lib builds for `wasm32` under `default-features = false`. +5 compile_string integration (`tests/compile_string.rs`: valid compile names the module, trilingual byte-identical output, E0401 width mismatch, syntax error reported, `import` rejected). Verified: full native gate green, `cargo build -p mimz-wasm --target wasm32-unknown-unknown`, and a headless Node smoke test (`crates/mimz-wasm/smoke-test.cjs`) compiling the counter through wasm. Suite 404 → 409.
- 2026-06-17 Workstream B versioning + language edition — new `crates/mimz-core/src/version.rs`: the compiler-version vs language-edition axes, `EDITION_HISTORY` (first edition **Wingless Butterfly** `wingless-butterfly-2026-1`), `version_block()` (uname-style `mimz --version`), and `KEYWORD_SET_VERSION` cross-checked against `lang/keywords.toml`'s `version` (now parsed + exposed via `KeywordTable::version`). The Verilog header carries both axes. +3 lib unit (`version`: `current_is_the_last_history_row`, `keyword_set_version_matches_keywords_toml`, `version_block_shows_both_axes`). Crate stays `0.1.0-dev` (drops `-dev` at the v0.1.0 tag, Workstream D). Suite 401 → 404.
- 2026-06-17 A5 asynchronous reset `async reset` (pre-v0.1.0 RTL-parity batch) — `async` promoted from reserved to an active keyword KW_ASYNC (Tanglish/Tamil `otthisaivatra`/`ஒத்திசைவற்ற` PROVISIONAL, pending native review). `ModuleItem::Reset` became `{ name, is_async }`; the emitter widens the sensitivity list to `@(posedge clk or posedge rst)` for an async reset. Active-high only (active-low polarity deferred). The cycle-based kernel is unchanged — async and sync reset are observationally identical at per-cycle sample points, so it's an emitter-only distinction. +5 lib unit (lexer `async_is_an_active_keyword`; parser `async_reset_parses_with_the_async_flag`, `a_plain_reset_is_synchronous`; emitter `async_reset_widens_the_sensitivity_list`, `a_sync_reset_stays_clock_only`). New four-flavor `async_reset` example (`BASE_EXAMPLES` 21 → 22, golden + the Icarus three-way differential). Spec `02` → v0.2.12, `03` → v0.2.10. Suite 396 → 401.
- 2026-06-17 A4 memories `mem` (pre-v0.1.0 RTL-parity batch) — `mem` promoted from reserved to an active keyword KW_MEM (Tanglish/Tamil `ninaivagam`/`நினைவகம்` PROVISIONAL, pending native review). New `ModuleItem::Mem`; checker `Ty::Memory` (indexed read/write yields the element type, address range-checked against `depth`); emitter `reg [W-1:0] m [0:DEPTH-1]` + an `initial` power-on seed; the sim kernel gained a sparse cell store (`is_mem`/`mem_read` on the `Resolver`, indexed write into `next_mems`). +10 lib unit (lexer `mem_is_an_active_keyword`; parser `mem_declaration_parses_to_a_mem_item`, `a_mem_without_an_init_value_is_e1104`; checker `register_file_passes`, `a_non_constant_memory_depth_is_e0201`, `a_zero_memory_depth_is_e0410`, `a_memory_init_that_overflows_the_element_is_e0405`, `a_constant_address_past_the_depth_is_e0406`, `a_memory_inside_repeat_is_e0303`; kernel `memory_write_then_read_round_trips_a_cell`). New four-flavor `regfile` example (`BASE_EXAMPLES` 20 → 21, golden + the Icarus three-way differential; the `regfile` cells are internal-only — not dumped to VCD, like the tamil-pure exemption note). Spec `02` → v0.2.11, `03` → v0.2.9. Suite 386 → 396.
- 2026-06-17 A3 falling-edge `on fall(clk)` (pre-v0.1.0 RTL-parity batch) — `fall` promoted from reserved to an active keyword KW_FALL (Tanglish/Tamil `irakkam`/`இறக்கம்` PROVISIONAL, pending native review); `OnBlock`/`Reg`/`Process` gained an `edge`; emitter lowers `posedge`/`negedge`; the sim kernel is now edge-aware (rise → sample → fall per period) so mixed-edge designs match Icarus bit-for-bit. +4 lib unit (parser `on_fall_parses_with_the_fall_edge`, `thamizh_order_on_fall_parses_to_the_fall_edge`; emitter `on_fall_emits_negedge`; kernel `dual_edge_negedge_reg_captures_posedge_within_a_period`); 2 lexer tests renamed (`fall_is_an_active_keyword`, `a_reserved_word_is_an_error`). New four-flavor `dual_edge` example (`BASE_EXAMPLES` 19 → 20, golden + the Icarus three-way differential). Spec `02` → v0.2.10, `03` → v0.2.8. Suite 382 → 386.
- 2026-06-17 A2 don't-care `match` patterns `0b1??` (pre-v0.1.0 RTL-parity batch) — new `TokKind::MaskedInt` / `Pattern::IntMask` (binary `?` don't-care), mirroring the literal-pattern path; additive, no new keyword. +6 lib unit (lexer `dont_care_binary_literal_lexes_to_masked_int`; parser `dont_care_pattern_parses_to_intmask`; checker `dont_care_pattern_must_match_the_scrutinee_width`, `a_dont_care_match_still_needs_a_wildcard`, `a_dont_care_pattern_on_an_enum_is_e0409`; sim `dont_care_match_picks_the_masked_arm`). New four-flavor example `priority` (`BASE_EXAMPLES` 18 → 19, golden + the Icarus three-way differential) — no new test functions. Exact-width reuses E0409, still-needs-`_` is E0601 (no new code). Spec `02` → v0.2.9. Suite 376 → 382.
- 2026-06-17 A1 replication `{N{x}}` (pre-v0.1.0 RTL-parity batch) — new `ExprKind::Replicate` mirroring concat through the whole pipeline; purely additive, no new keyword. +7 lib unit (parser `replication_parses_to_replicate`, `braces_without_an_inner_group_stay_concat`; checker `replication_width_is_count_times_inner`, `replication_width_mismatch_is_e0401`, `a_non_constant_replication_count_is_e0201`, `a_zero_replication_count_is_e0410`; sim `replication_repeats_the_group`). New four-flavor example `replicate` (`BASE_EXAMPLES` 17 → 18, golden + the Icarus three-way differential) — no new test functions (existing parametrized iterators). Width reuses E0410, non-const count reuses E0201 (no new code). Spec `02` → v0.2.8. Suite 369 → 376.
- 2026-06-17 SEC-6 hardening audit — C2–C4 elaboration-time DoS bounds: `mimz sim`/`mimz test` skip the checker, so the structural elaborator (`crates/mimz-sim/src/sim/elaborate.rs`) gained `MAX_INSTANCE_DEPTH = 16` (recursive/cyclic instantiation → clean error, not a stack-overflow abort), `checked_sub` on the `repeat` span (extreme `hi - lo` → over-budget error, not an overflow panic), a `0..128` bound on bit-index drives (no silent `as u32` truncation), and a flatten name-collision error (no silent overwrite). A same-day follow-up pass added a 5th finding (SIM-5): `int_expr`, which lowers each flattened child const to a literal, built a negative value via a raw `i128` negation that overflow-panicked on `i128::MIN` (reachable via `(-i128::MAX) - 1`) — now non-recursive and `unsigned_abs`-based. +5 lib unit (`recursive_instantiation_errors_not_overflows`, `extreme_repeat_bounds_error_not_overflow`, `an_out_of_range_bit_index_errors`, `a_flatten_name_collision_errors`, `an_i128_min_const_elaborates_without_overflow` — `crates/mimz-sim/src/sim/elaborate.rs`). See SEC-6/HARD-6 in `docs/audit/`.
- 2026-06-16 Phase 1.5 C3 + C4 — full simulator parity: the sim elaborator now unrolls `repeat` (array instances `fa__i`, bit-indexed drives assembled into a Concat — ripple\*adder) and encodes enum-typed signals by variant index with width `clog2(variants)` (variant reads/patterns → index — traffic_light), via a unified `Rw` elaborate-time rewriter (`crates/mimz-sim/src/sim/elaborate.rs`). The Layer-3 differential now covers the **entire single-file corpus, 18 → 21 examples** (added ripple_adder, traffic_light, vilakku) — every example the emitter compiles also simulates bit-for-bit vs Icarus. +2 lib unit (`unrolls_repeat_with_instance_array_and_bit_drives`, `elaborates_an_enum_signal_and_match`). Phase 1.5 full-parity simulator complete (C1–C4).
- 2026-06-16 Phase 1.5 C2 — module-instance flattening in the sim elaborator: `elaborate_project` (`crates/mimz-sim/src/sim/elaborate.rs`) flattens `let` instances (incl. across `import`s) by inlining each child with signals name-prefixed `{inst}*{name}`, so `inst.port`reads resolve to the wire`inst*port`the emitter auto-declares — the flattened`Design`matches the emitted Verilog bit-for-bit.`mimz sim`/`mimz test`now`load_project`; the Layer-3 differential gained **alu** (`Top`instantiating the imported`Adder`) and **chained** (two chained `FullAdder`s), 16 → **18 examples**. +2 lib unit (`flattens_a_same_file_instance`, `rejects_unknown_instance_module`, replacing `rejects_instances_for_now`); the differential is one `#[test]`so the new examples add no separate count. Remaining sim parity: C3`repeat`(ripple_adder), C4 enum FSM (traffic_light).
- 2026-06-16 security/bug audit (SEC-5) — bound the simulator's unbounded count inputs: a critical→medium audit (core pipeline clean) found the new sim skipped the "bound every count" doctrine. Caps`MAX_SIM_CYCLES`/`MAX_SWEEP_VECTORS` (`crates/mimz-sim/src/sim/run.rs`) now bound `tick(clk, n)`(untrusted-input hang/OOM via`mimz test`), the `--sweep`cartesian product (unchecked`usize`mul), and`--cycles`; plus a `translate`no-panic fix and a`mimz.toml` walk-up cap. +2 command unit (`sweep_vectors`cap —`src/commands/helpers.rs`), +1 sim integration (`cycles_over_the_limit_is_rejected_by_the_cli`), +1 test integration (`a_tick_count_over_the_cycle_limit_errors_fast_not_hangs`). The auditor's `cycle * PERIOD`overflow "highs" are unreachable once the loops are bounded — recorded checked-safe, see`docs/audit/`.
- 2026-06-16 C1 carry-forward closed — the Layer-3 Icarus differential (`our*simulator_matches_icarus_bit_for_bit`) now also covers the four pure-Tamil examples (kanakki/cimitti/oppidi/thervi), so its list equals the emitter's single-module list, **12 english + 4 tamil-pure = 16**. The testbench romanizes interface names via the emitter's own `transliterate` (`interface_name_map`in`tests/icarus.rs`) to match the compiled Verilog while the kernel keeps source names; no new test function, so the count is unchanged.
- 2026-06-16 Phase 1.5 C1 — combinational `mimz sim`+ signed-aware differential:`comb_run` (`crates/mimz-sim/src/sim/run.rs`) settles a clockless design one frame per input vector, so `mimz sim`now runs combinational modules too —`--in`is one settled frame,`--sweep a=0|1|2` a frame each — emitting the same VCD/trace. The Layer-3 Icarus differential (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) was broadened to **12 ASCII-named english examples** (clocked AND combinational, incl. SIGNED `bitops`/`signed_math`), auto-routing on whether the design is clocked, comparing via Verilog `%b`(binary ⇒ signedness-agnostic) with per-example param overrides. It caught a real bug: the shared evaluator's lossless signed`+`/`*` (`crates/mimz-sim/src/sim/value.rs`) added raw bits without sign-extending a negative operand — fixed to use `as_i128`(matching Verilog), which also corrects`mimz eval`. +5 lib unit (4 `comb_run` + 1 signed regression) + 2 net sim integration (−1 clockless-reject removed, +3 combinational). Romanized tamil-pure + instance/`repeat`/enum designs are deferred (C2–C4).
- 2026-06-16 Phase 1.5 B8 — differential vs Icarus + perf baseline + golden VCD: a Layer-3 Icarus test (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) runs each design through OUR event-driven kernel in-process AND reconstructs the values from the VCD our writer emits, comparing both against `iverilog`/`vvp` under the SAME stimulus — three views (kernel == VCD waveform == Icarus) must agree bit-for-bit per cycle (counter + shift register + edge detector). A byte-for-byte golden lock (`tests/sim.rs::the_counter_vcd_matches_the_golden_byte_for_byte`vs`tests/golden/counter.vcd`, `MIMZ_UPDATE_GOLDENS=1` to regenerate) pins the writer's exact output format. A perf test (`tests/sim.rs::the_counter_kernel_clears_the_perf_baseline`) gates the kernel at ≥1M cycle-events/sec on the counter in release (best of 5 to reject load-induced dips; measured ~2.3M; debug uses a low sanity floor). +1 Icarus differential + 2 sim integration. Phase 1.5 (simulator) is now feature-complete: B1 elaborate, B2 kernel, B3 comb propagation, B4 stimulus, B5 VCD+trace+`mimz sim`, B6 `mimz test`, B7 test-header flip, B8 differential+perf+golden.
- 2026-06-16 Phase 1.5 B7 — test-header thamizh-order flip: `M(args) kaaga "…" sodhanai { }`parses to the SAME`TestDecl`as the code-order`test "…" for M(args) { }` (`crates/mimz-core/src/parser/items/test.rs::test_decl_thamizh`, dispatched from the file loop when `syntax thamizh`is active and a bare identifier leads), and`crates/mimz-core/src/pretty.rs`flips it for`mimz translate --order thamizh`— completing all five clause flips of the word-order engine. Execution is the oracle: a passing thamizh-order test re-parsing to the same tree replaces the same-Verilog check`test` blocks can't provide. +3 parser lib unit + 1 test integration (`a_thamizh_order_test_header_runs_like_its_code_order_twin`) + 1 translate integration (`pretty_print_thamizh_flips_the_test_header_and_reparses`).
- 2026-06-16 Phase 1.5 B6 — `mimz test`: the `test`-block runner in `crates/mimz-sim/src/sim/harness.rs` runs each block (`drive`/`tick`/`expect`/`if`) on the kernel, halts a failing `expect`with a teaching message (expression source + cycle + each comparison side's value), and exits non-zero on any failure;`--filter`/`--trace`/`--verbose`/`--signals`supported, the trace-scope logic shared with`mimz sim`via`commands/helpers.rs::trace_scope`. `async`was reserved alongside`await` (spec/03 v0.2.7, R11/R13) so the v0.3 backlog list is now 9 words. +6 lib unit (`crates/mimz-sim/src/sim/harness.rs`) + 5 test integration (`tests/test_run.rs`).
- 2026-06-16 Phase 1.5 B4+B5 — `mimz sim`: default stimulus + a hand-written 2-state VCD writer + the `--trace`/`--trace=changes`console table (scope via`--verbose`/`--signals`), all riding one per-cycle snapshot from the kernel. +9 lib unit (`crates/mimz-sim/src/sim/{run,vcd,trace}.rs`) + 5 sim integration (`tests/sim.rs`).
- 2026-06-16 Phase 1.5 B1 — simulator elaboration: +5 lib unit in `crates/mimz-sim/src/sim/elaborate.rs`, the `Design`flattener (signals/regs/comb/processes, widths + reset folded) the event-driven kernel will interpret.
- 2026-06-16 Phase 1.5 B2 — event-driven two-phase kernel: +7 lib unit in`crates/mimz-sim/src/sim/kernel.rs` (counting/reset, width-wrap, the two-phase register swap, statement-`if`, the per-cycle snapshot seam, leaf validation). The shared 2-state value model + expression evaluator were extracted to `crates/mimz-sim/src/sim/value.rs`behind a`Resolver`trait that both`comb`and`kernel`implement —`comb`'s 7 tests are unchanged and verify the extraction.
- 2026-06-16 Phase 1.5 B3 — combinational propagation: +2 kernel lib unit locking multi-level `wire → wire → output`settling order and the kernel's comb-cycle guard; B3 needed no new code — the kernel's memoized resolver already settles drivers in dependency order.
- 2026-06-16 close Phase 1.8 + pre-freeze keyword reservation: Phase 1.8 closed by bumping`spec/04`DRAFT → stable (docs only, no test change); and`fn`/`function`reserved for a future combinational-function construct ahead of the v0.1.0 freeze (R11/R13) — +1 keyword-table lib unit`fn_and_function_are_reserved`. Also listed `the_section8_keywords_are_reserved` in the keyword-table section below, present since 2026-06-13 but previously unlisted.
- 2026-06-16 native-authored error catalog + audit/coverage follow-up: the Tamil/Tanglish catalog (`lang/messages.toml`, decision C3 ratified) grew from a one-shape stub to **33 of 36** localized codes with structured-arg interpolation; an audit of PRs #14–#17 found no bug/overflow/security/perf issue, so the work was test-coverage + prevention guards only. +2 morph lib unit (`arg_code_without_args_falls_back_to_english`, `fill_with_empty_name_leaves_no_stray_fragment`), +4 morph integration (`e0402`/`e0408`/`e0601`interpolation tests +`message_catalog_placeholders_are_known_tokens`— a guard that every active`{token}`in`lang/messages.toml`is one`morph::fill` fills, so a typo'd placeholder can't silently fall back to English forever), +1 grammar-sync (`keywords_toml_has_no_superseded_spelling` — a superseded v1 spelling may not return as a keyword/alias). The remaining +9 morph integration vs. the prior count are #16's newly-localized codes (`e0502`/`e0505`/`e0202`/`e0401`), the `message_catalog_keys_are_real_checker_codes` guard, and the W0001 mixed-flavor lint tests.
- 2026-06-15 fuzz/security audit of the since-2026-06-14 changes: a deterministic stress harness over adversarial Tamil/keyword/ASCII input found that reskinning a numeric literal directly abutting a Tamil keyword/identifier (`42தொகுதி`) glued it into an unlexable lexeme — fixed by a boundary-space guard in `reskin`; and that `--names-map`accepted any`NameMap.version`— fixed by a version check in`load_name_map`. +1 translate integration (boundary guard regression), +1 config integration (unknown-version rejected). No overflow/unsafe/crash found. A `translate_roundtrip`cargo-fuzz target was added to close the coverage gap, CI-only, outside this count.
- 2026-06-15`mimz.toml`config + name-map auto-discovery: a new`config`module reads per-project flag defaults from`mimz.toml`(discovered by walking up from the input file; precedence CLI › config › default), and reverse`translate`auto-loads the`<input>.names.json` sidecar with no flag (`--no-names-map` opts out). +4 lib unit (`config`: parse, defaults, unknown-key reject, walk-up discovery), +4 config integration (auto-restore, --no-names-map, config precedence, malformed-config error).
- 2026-06-15 reversible romanization: `--romanize-names`now writes a per-file sidecar`<out>.names.json` (`NameMap`, romanized→Tamil) beside `-o`, and `mimz translate --names-map <file>`restores the exact Tamil names — so`Tamil → Latin → Tamil`is lossless. New`romanize_with_map`/`restore_with_map`share a factored`reskin` helper. +3 lib unit (`translate`: inverse map, restore inverts romanize, NameMap serde), +2 translate integration (lib round-trip via map, CLI forward+reverse).
- 2026-06-15 pure-Tamil showcase + opt-in `translate --romanize-names`: a new `examples/tamil-pure/`folder holds fully-Tamil programs — Tamil keywords AND identifiers — exempt from the four-flavor byte-identity rule (R9) and instead proven equivalent to their English counterparts by canonical identifier renaming.`mimz translate --romanize-names`reuses the emitter's`romanize` to rewrite Tamil identifiers to Latin (opt-in, one-way; lossless default unchanged). +2 lib unit (`translate`), +2 example integration (pure-Tamil golden + equivalence), +1 Icarus (pure-Tamil testbenches), +3 translate integration.
- 2026-06-15 mixed-flavor lint: a non-fatal warning **W0001** fires when a file mixes Tamil keywords with English/Tanglish — `Diag`gained a`Severity`(Error/Warning),`check`/`compile`/`eval`print it and still succeed, and the LSP shows it as a WARNING. +2`morph`lib unit, +1 LSP unit, +3`morph`integration.
- 2026-06-15 robustness follow-up to the 2026-06-14 batch audit: +9 lib unit — 2`morph`(tie-break + empty-stem inflection), 5 checker (two-literal`min`E0407,`nand`of a bare`bit`, nested `abs(min)`/`min(abs)`, `abs`at the width boundary), 1 parser (a long flat binary chain parses without tripping the E1113 depth guard), 1 emitter (a built-in lowers parenthesized inside a larger expression) — and +2`fmt` integration (`-o`onto the input path round-trips via the new atomic write; an unknown`--to`is a clean error). A`pretty_roundtrip`cargo-fuzz target was added (CI-only, outside this count).
- A QA pass for the new built-ins added the`bitops`example in all four flavors — golden + a self-checking Icarus testbench incl. the abs(MIN) width-growth case — plus edge tests: parser arity E1110, checker literal-adapt + abs-of-literal, fmt keyword-free/non-lexing, and`compile --lang`localization.
- Arithmetic built-ins`min`/`max`/`abs`/`nand`/`nor`/`xnor`added 6 checker unit tests + 1`eval`integration test.
- Phase 1.8 error-language plumbing added 8`morph`lib unit tests + 7`tests/morph.rs`integration tests for selection, inflection, and the additive English-fallback path.
- 2026-06-14, after merging the security-hardening and Phase 1.8 grammar branches: the security audit added 2 parser unit tests + 3`eval`integration tests for overflow/recursion guards; the Phase 1.8 thamizh-order flips — conditional / if-expression / match — added 10 grammar integration tests incl. the profile-boundary and depth-guard regressions. Then`mimz translate --order`(the`pretty`AST printer) added 4 translate integration tests + 1 grammar test for the Tamil thamizh-order traffic light.
- The error-fixture tests are data-driven over ~70 broken`.mimz`fixtures; one locks`ALL_CHECKER_CODES`— now`pub`in`crates/mimz-core/src/diag.rs`— to the 11-checker.md catalog, one locks the`--json`wire format.
- The 2026-06-13 quick-wins block added the tooling tests below:`explain`(+3),`translate`(+3 unit, +3 integration),`sim::comb`(+7 unit, +6`eval` integration).

## Unit: keyword table (`crates/mimz-core/src/lexer/keywords.rs`, 13 tests)

| Test                                                  | Locks in                                                                                                                                                                             | If it fails…                                                |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| `all_three_flavors_resolve_to_same_keyword`           | EN/Tanglish/Tamil spellings → one `Kw` token                                                                                                                                         | `lang/keywords.toml` edit broke a mapping                   |
| `flavors_are_recorded`                                | the lexer remembers which column a spelling came from                                                                                                                                | flavor tracking broke (P1.8 depends on it)                  |
| `include_is_an_alias_for_import`                      | `include` lexes to the exact same token as `import`                                                                                                                                  | the alias mechanism or table entry broke                    |
| `fall_is_an_active_keyword`                           | `fall` lexes as KW_FALL in all three flavors (A3 promoted it from reserved)                                                                                                          | someone changed `fall`'s keyword status without a decision  |
| `future_keywords_are_reserved_not_usable`             | every reserved future keyword (`fn`/`function`, the v0.3 backlog `secret`…`await`, section-8 `fixed`/`requires`/`ensures`, and `extern`) stays reserved and is not an active keyword | a future keyword was claimed without a decision (R11)       |
| `canonical_spellings_lists_every_keyword_in_a_flavor` | `canonical_spellings(flavor)` returns one spelling per `Kw` (42) in the asked column — the LSP's flavor-matched keyword completion list                                              | the reverse-lookup table or completion keyword source broke |
| `extern_lexes_in_all_three_flavors`                   | `extern`/`anniya`/`அன்னிய` lex as KW_EXTERN                                                                                                                                          | the Verilog-FFI keyword lost a column                       |
| `sim_bind_speed_lex_in_all_flavors`                   | the three hardware-emulation keywords (`sim`/`bind`/`speed`) lex in every flavor                                                                                                     | a `sim {}` block stopped parsing in one flavor              |
| `kw_bundle_is_recognized`                             | `bundle` is an active keyword, not an identifier                                                                                                                                     | the bundle feature's keyword row was dropped                |
| `kw_default_is_recognized`                            | `default` likewise                                                                                                                                                                   | —                                                           |
| `kw_loop_is_recognized`                               | `loop` likewise                                                                                                                                                                      | —                                                           |
| `kw_return_is_recognized`                             | `return` likewise                                                                                                                                                                    | —                                                           |
| `kw_sync_is_recognized`                               | `sync` likewise (promoted from reserved in A6)                                                                                                                                       | —                                                           |

Note: the table's structural rules (disjoint columns, known keys, valid
TOML) need no dedicated test — the `LazyLock` panics at startup, so
**every** test fails if the table is broken. That's by design.

## Unit: lexer (`crates/mimz-core/src/lexer/tests.rs`, 15 tests)

| Test                                                     | Locks in                                                                                                                                                                          |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lexes_mixed_flavors`                                    | mixing three flavors in ONE line works — the migration path                                                                                                                       |
| `tamil_identifiers_work`                                 | Tamil-script identifiers lex as identifiers (XID rules)                                                                                                                           |
| `numbers`                                                | decimal / `0b` / `0x` parse, `_` separators, correct values                                                                                                                       |
| `wrapping_operators`                                     | `+%` / `-%` are single tokens                                                                                                                                                     |
| `larrow_vs_comparison`                                   | `<-` vs `<=` vs `<<` disambiguation — longest match                                                                                                                               |
| `newline_continuation_after_operator`                    | the Go-style newline policy, both directions (kept AND dropped)                                                                                                                   |
| `division_is_rejected_with_teaching_error`               | `/` errors AND the help text teaches the alternative                                                                                                                              |
| `a_reserved_word_is_an_error`                            | a still-reserved word (`inout`) is a clean E1005, not a silent identifier — `fall`/`mem`/`sync` were each promoted to active, so the check moved to a word that is still reserved |
| `mem_is_an_active_keyword`                               | `mem`/`ninaivagam`/`நினைவகம்` lex as KW_MEM in all three flavors (A4 promoted it from reserved)                                                                                   |
| `async_is_an_active_keyword`                             | `async`/`otthisaivatra`/`ஒத்திசைவற்ற` lex as KW_ASYNC in all three flavors (A5 promoted it from reserved)                                                                         |
| `dont_care_binary_literal_lexes_to_masked_int`           | `0b1??` lexes to `MaskedInt` (value/mask/width); plain `0b101` stays `Int` (A2)                                                                                                   |
| `fn_keyword_lexes_in_all_flavors`                        | `fn`/`function`/`saarbu`/`சார்பு` lex as KW_FN (Phase 2)                                                                                                                          |
| `rarrow_token_lexes`                                     | `->` lexes as RArrow (for fn returns and sync loops)                                                                                                                              |
| `lexes_question_and_question_question`                   | `?` and `??` are distinct tokens — the optional-type suffix vs the coalesce operator                                                                                              |
| `a_literal_wider_than_128_bits_lexes_without_a_size_cap` | a literal past the fast-path boundary lexes intact instead of being clamped                                                                                                       |

## Unit: parser (`crates/mimz-core/src/parser/tests/`, 91 tests)

Split 2026-07-26 (`oversized-test-file-split`) from a single 1594-line
`tests.rs` into 13 topic files under `tests/`; `mod.rs` keeps only the
shared `parse_ok`/`parse_err`/`parse_expr_ok` helpers. Zero test-behavior
change — every row below is the same test that existed before, just
organized by file.

### parser/tests/bundles.rs (5 tests)

| Test                                 | Locks in                                                                                 |
| ------------------------------------ | ---------------------------------------------------------------------------------------- |
| `parse_bundle_decl`                  | `bundle` struct declarations parse with fields                                           |
| `parse_bundle_as_port_type`          | a bundle type used as a port type (bare or with args, e.g. `Hs(X: 1)`) parses            |
| `parse_bundle_literal`               | bundle literals `Bundle { f: x }` parse                                                  |
| `parse_bundle_destructure`           | bundle destructuring `let { f } = b` parses                                              |
| `parse_bundle_field_rename_is_error` | `let { valid: v } = bus` (renaming a destructured field) is E0904, not silently accepted |

### parser/tests/calls_and_modules.rs (6 tests)

| Test                                    | Locks in                                                                                            |
| --------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `builtin_with_wrong_arity_is_e1110`     | a built-in called with the wrong argument count (e.g. `min(a)`) is E1110                            |
| `non_builtin_call_parses_as_fncall`     | a call to a non-builtin name (`mac(x, y)`) parses as `ExprKind::FnCall`, not a builtin `Call`       |
| `builtin_call_still_parses_as_builtin`  | a call to a builtin name (`extend(x, 8)`) still parses as `ExprKind::Call`, not swept into `FnCall` |
| `zero_arg_call_parses_as_fncall`        | a zero-argument call (`foo()`) parses as `FnCall` with an empty arg list                            |
| `parses_counter`                        | the canonical example parses; module has the expected 6 items                                       |
| `parses_tanglish_counter_to_same_shape` | Tanglish source → structurally identical AST (the thesis, AST level)                                |

### parser/tests/enums_and_tagged_unions.rs (6 tests)

| Test                                                        | Locks in                                                                                                |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `tagged_enum_parses`                                        | enum with payload fields parses correctly (Phase 2)                                                     |
| `mixed_tag_only_and_tagged_parses`                          | an enum mixing tag-only and payload-bearing variants in one declaration parses                          |
| `match_with_payload_bindings_parses`                        | `match` arms with payload bindings `Variant(x, y)` parse                                                |
| `enum_construct_parses_with_payload_args`                   | `Packet.Ctrl(k)` parses to `ExprKind::EnumConstruct` with the variant name and args                     |
| `enum_construct_parses_with_zero_args_for_tag_only_variant` | `State.Idle()` (explicit empty parens on a tag-only variant) parses to `EnumConstruct` with zero args   |
| `bare_enum_variant_reference_still_parses_as_field`         | `State.Idle` with no trailing `()` stays `ExprKind::Field`, not swept into `EnumConstruct` (regression) |

### parser/tests/extern_module_and_sync_builtins.rs (6 tests)

| Test                                                   | Locks in                                                                                                   |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `extern_module_parses_with_params_doc_and_ports`       | `extern module Pll(MULT: int = 2) { doc: "..." ... }` parses to `ExternModule` with params, doc, and ports |
| `extern_module_parses_with_alias_and_no_params_or_doc` | `extern module Pll = "PLL_HARD_IP_v2" { }` parses with the Verilog alias name, no params, no doc           |
| `extern_module_body_rejects_wire_declarations`         | an `extern module` body containing a `wire` declaration is a parse error — only ports are allowed          |
| `sync_double_flop_call_parses_as_a_builtin_call`       | `sync.double_flop(fast_bit, clk_src, clk_dst)` parses as `Builtin::SyncDoubleFlop` with 3 args             |
| `sync_pulse_call_parses_as_a_builtin_call`             | `sync.pulse(src_pulse, clk_src, clk_dst)` parses as `Builtin::SyncPulse` with 3 args                       |
| `sync_dot_with_unknown_method_is_a_clean_parse_error`  | `sync.nonsense(...)` (an unknown `sync.*` method) is a clean E1116, never a panic                          |

### parser/tests/fn_decl_thamizh_and_stmts.rs (3 tests)

| Test                              | Locks in                                                                                                   |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `fn_decl_parses_in_thamizh_order` | `fn` declarations are code-order-only (no SOV flip) — a `syntax thamizh` file still accepts a leading `fn` |
| `parse_default_stmt`              | `default` assignment statements parse inside `on`                                                          |
| `parse_const_if_block`            | `const if` elaboration blocks parse                                                                        |

### parser/tests/fn_decls.rs (5 tests)

| Test                                                  | Locks in                                                                                                                                                       |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `parses_fn_with_local_let_and_body`                   | Phase 2 `fn` with `let` locals and a final block return parses                                                                                                 |
| `parses_fn_with_guard_clause_return`                  | `fn` with `return` statement guard clause parses                                                                                                               |
| `parses_fn_with_thamizh_order_guard_clause_return`    | thamizh word-order guard clause (`<cond> enil { thirumbu ... }`) parses to the same shape as code order — `return`/`thirumbu` stays prefix-only in both orders |
| `parses_fn_with_if_else_stmt`                         | a statement-level `if`/`else` inside a `fn` body parses with both branches populated                                                                           |
| `parses_fn_with_only_locals_and_tail_backward_compat` | the pre-Phase-2 `fn` shape (locals + tail expr, no `if`/`return`) still parses — the backward-compat contract                                                  |

### parser/tests/item_grammar.rs (5 tests)

| Test                                         | Locks in                                                                                                                      |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `on_fall_parses_with_the_fall_edge`          | `on fall(clk)` parses to `OnBlock` with `Edge::Fall` (A3)                                                                     |
| `mem_declaration_parses_to_a_mem_item`       | `mem m: bits[8][4] = 0` parses to `ModuleItem::Mem` (name/ty/depth/init) (A4)                                                 |
| `a_mem_without_an_init_value_is_e1104`       | a `mem` missing its `= init` is E1104 (no uninitialized state), like a reg (A4)                                               |
| `array_type_parses_in_a_fn_param`            | an array-typed `fn` parameter (`vals: bits[8][4]`) parses to `Type::Array`                                                    |
| `nested_array_type_parses_two_brackets_deep` | a doubly-bracketed array type (`bits[8][4][2]`) parses without ambiguity — the CHECKER, not the parser, rejects nested arrays |

### parser/tests/module_refs_and_arrays.rs (5 tests)

| Test                                                 | Locks in                                                                                                 |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `qualified_module_reference_parses`                  | `a.b.Foo() { }` parses to an `Inst` with a 2-segment qualified path                                      |
| `bare_module_reference_still_parses_with_empty_path` | `Foo() { }` with no qualifying path parses with `inst.module.is_bare()` true                             |
| `array_literal_parses`                               | `[1, 2, 3, 4]` parses to `ExprKind::ArrayLit` with 4 elements                                            |
| `empty_array_literal_parses`                         | `[]` parses to an empty `ArrayLit` — the parser accepts it; the checker later rejects zero-length arrays |
| `array_literal_as_fn_call_argument_parses`           | an array literal passed as a `fn` call argument (`f([1, 2, 3, 4])`) parses                               |

### parser/tests/repeat_loop_foreach.rs (9 tests)

| Test                                                  | Locks in                                                                        |
| ----------------------------------------------------- | ------------------------------------------------------------------------------- |
| `parses_repeat_and_const`                             | `repeat i: 0..8` and file-level `const` parse                                   |
| `parses_loop_inside_on_block`                         | `loop i in 0..4` inside `on` block parses                                       |
| `sync_loop_parses`                                    | `sync loop` constructs parse with variables, boundaries, and result types       |
| `parses_loop_inside_fn_body`                          | `loop` inside combinational `fn` block parses                                   |
| `foreach_range_form_parses_as_module_item`            | `foreach i in 0..4 { }` at module-item level parses to `ForEach` (range form)   |
| `foreach_elements_form_parses_as_module_item`         | `foreach v in arr { }` at module-item level parses to `ForEach` (elements form) |
| `foreach_parses_inside_on_block`                      | `foreach` inside a clocked `on` block parses                                    |
| `foreach_parses_inside_fn_body`                       | `foreach` inside a combinational `fn` block parses                              |
| `foreach_elements_form_rejects_non_identifier_source` | `foreach v in <non-identifier expr>` is a parse error, not a silent misparse    |

### parser/tests/reset_and_thamizh_order.rs (10 tests)

| Test                                                        | Locks in                                                                             |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `async_reset_parses_with_the_async_flag`                    | `async reset rst` sets `Reset.is_async` (A5)                                         |
| `a_plain_reset_is_synchronous`                              | a bare `reset rst` leaves `is_async` false — sync is the default (A5)                |
| `thamizh_order_on_fall_parses_to_the_fall_edge`             | `irakkam(clk) pothu { }` → the same fall block (thamizh order) (A3)                  |
| `thamizh_order_on_block_parses_to_the_same_shape`           | `syntax thamizh` + `yetram(clk) pothu { }` → the same module (spec/04)               |
| `english_syntax_thamizh_directive_also_selects_the_profile` | flavor and word-order profile are orthogonal (`syntax thamizh` in English)           |
| `unknown_syntax_profile_is_e1112`                           | `syntax wibble` → E1112, not silently ignored                                        |
| `flipped_on_block_needs_the_directive`                      | a leading `rise(...)` is a parse error without the directive (gated flip)            |
| `thamizh_order_test_header_parses_to_the_same_shape`        | `M(args) kaaga "…" sodhanai { }` → the SAME `TestDecl` as the code-order header (B7) |
| `thamizh_test_header_with_no_params_parses`                 | the flipped test header with no params (`Counter kaaga "…" sodhanai`) parses         |
| `the_test_header_flip_needs_the_directive`                  | a leading identifier test header without `syntax thamizh` is E1102 (gated flip)      |

### parser/tests/safety_and_precedence.rs (17 tests)

| Test                                                               | Locks in                                                                                |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| `deeply_nested_expression_errors_not_overflows`                    | `(((…)))` past the depth cap → clean E1113, not a stack overflow (SEC-1)                |
| `deeply_nested_unary_errors_not_overflows`                         | `!!!!…x` prefix chain → E1113 via the `unary` guard, not a crash                        |
| `a_long_flat_binary_chain_parses_without_tripping_the_depth_guard` | a 5000-term `a + a + …` chain parses — LENGTH is unbounded, distinct from nesting DEPTH |
| `stray_top_level_brace_does_not_hang`                              | a stray top-level `}` errors and terminates — `file()` cannot spin (OOM)                |
| `rust_precedence_defuses_the_c_trap`                               | `x & 1 == 0` parses as `(x & 1) == 0` — **never** change this                           |
| `monotonic_chained_comparison_desugars_to_and`                     | `0 <= x <= 7` desugars to `(0<=x) && (x<=7)` — the safe Python form (8.9)               |
| `qq_parses_as_lowest_precedence_left_associative`                  | `a \|\| b ?? c` parses as `(a \|\| b) ?? c` — `??` binds LOOSER than `\|\|`             |
| `qq_chain_is_left_associative`                                     | `a ?? b ?? c` reads `(a ?? b) ?? c` — left-associative chaining                         |
| `replication_parses_to_replicate`                                  | `{2{a}}` parses as `Replicate` (count + inner parts), not concatenation (A1)            |
| `braces_without_an_inner_group_stay_concat`                        | `{a, a}` still parses as `Concat` — the replication path is no regression               |
| `dont_care_pattern_parses_to_intmask`                              | `0b1??` in a match arm parses as `Pattern::IntMask` (value/mask/width) (A2)             |
| `mixed_direction_chain_is_an_error`                                | `a < b > c` stays E1109 (the confusing form)                                            |
| `equality_cannot_be_chained`                                       | `a == b == c` stays E1109                                                               |
| `wire_if_without_else_teaches_about_latches`                       | mandatory `else` on if-expressions + the latch help text                                |
| `reg_without_reset_value_is_an_error`                              | mandatory reg reset (safety rule)                                                       |
| `assign_arrow_confusion_teaches`                                   | `=` inside `on` → help text pointing to `<-`                                            |
| `every_parse_error_carries_a_code`                                 | the E11xx retrofit, locked from outside: no parse error is codeless                     |

### parser/tests/test_blocks_sim_and_recovery.rs (9 tests)

| Test                                                   | Locks in                                                                                                                      |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `parses_test_block`                                    | `test "..." for M(...) { tick/expect }` parses                                                                                |
| `empty_parens_variant_is_a_parse_error`                | `enum Foo { A() }` (empty-parens tag-only variant) is E1113 with a "tag-only" hint — parens must be omitted, not empty        |
| `parse_recover_keeps_good_items_around_a_bad_one`      | `parse_recover` leaves one `ModuleItem::Error` for a bad line; both ports survive                                             |
| `parse_recover_top_level_error_keeps_following_module` | file-level garbage becomes `TopItem::Error`; the next module still parses                                                     |
| `parse_recover_seq_and_test_blocks_emit_error_nodes`   | a bad stmt in `on`/`test` yields `Seq`/`TestStmt::Error`; good stmts survive                                                  |
| `sim_block_parses`                                     | a `test` block's `sim { speed mhz(50) bind playing -> led(color: green) }` parses — speed clause + one bind with an ident arg |
| `sim_block_bind_arg_accepts_a_bare_integer`            | a `sim` bind arg accepts a bare integer (`bind tx -> uart_tx(baud: 9600)`), not just an identifier                            |
| `sim_block_bad_syntax_recovers`                        | bad syntax inside a `sim { }` block recovers to a `TestStmt::Error`; the surrounding `tick` still parses                      |
| `strict_parse_still_errs_on_bad_input`                 | the strict `parse` contract is unchanged — any error discards the tree                                                        |

### parser/tests/valid_bundle_sugar.rs (5 tests)

| Test                                                                            | Locks in                                                                                                                                              |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bit_question_desugars_to_builtin_valid_bundle`                                 | `bit?` desugars to the built-in `__Valid` bundle type with `N=1`                                                                                      |
| `bits_n_question_desugars_with_the_width_expr`                                  | `bits[8]?` desugars to `__Valid` with `N` bound to the width expression (8)                                                                           |
| `signed_n_question_desugars_to_valid_signed`                                    | `signed[8]?` desugars to the built-in `__ValidSigned` bundle type with `N=8`                                                                          |
| `double_question_on_a_type_is_rejected`                                         | `bits[8]??` (double `?`) is E1115, not silently accepted                                                                                              |
| `mem_declaration_still_parses_to_the_same_shape_after_array_type_grammar_lands` | `mem`'s own declaration grammar is unaffected by the `T?`/array-type grammar work — `ty` stays a scalar, `depth` a separate `Expr` (regression guard) |

The error-path tests assert on message/help **substrings** (loose, so
wording can be polished) AND on the stable E-code (tight — the
contract). Lexer error tests do the same with E10xx.

## Unit: checker (`crates/mimz-core/src/checker/tests/`, 268 tests)

One test per error code plus clean-pass cases — the codes are the
stable contract, so each test asserts the CODE and a message substring
(loose on wording). The full catalog with meanings lives in
[`11-checker.md`](11-checker.md); the test names map one-to-one
(`unknown_name_is_e0101_with_teaching_help`, `assignment_width_mismatch_is_e0401`, …).

Split 2026-07-26 (`oversized-test-file-split`) from a single 3026-line
`tests.rs` into 11 topic files under `tests/`; `mod.rs` keeps only the
shared `check_one`/`first_err`/`first_err_multi`/`any_code` helpers. Zero
test-behavior change — every row below is the same test that existed
before, just organized by file and given a row if it lacked one.

### checker/tests/names_and_consts.rs (35 tests)

| Test                                                                   | Locks in                                                                                          |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `clean_module_passes`                                                  | clean code produces ZERO diagnostics — the checker must never cry wolf                            |
| `clog2_in_a_width_position_is_clean`                                   | `clog2(N)` used in a width position checks clean                                                  |
| `clog2_of_a_module_const_is_clean`                                     | `clog2` of a module-level `const` checks clean                                                    |
| `clog2_of_zero_is_e0202`                                               | `clog2(0)` is E0202 (undefined — no width represents zero values)                                 |
| `clog2_in_a_runtime_value_position_is_e0407`                           | `clog2` used in a runtime (non-width) value position is E0407                                     |
| `same_name_module_in_different_files_is_not_an_error_until_referenced` | packages/namespacing: cross-file name collisions are legal until referenced (spec/02 §1.5b)       |
| `ambiguous_bare_module_reference_is_e0110`                             | a bare module reference that's ambiguous across imports is E0110                                  |
| `qualified_module_reference_resolves_unambiguously`                    | a qualified (`a.b.Foo`) reference resolves unambiguously even when a bare name would be ambiguous |
| `qualified_reference_actually_resolves_via_a_real_import_path`         | a qualified reference resolves through a REAL import path end to end, not just a synthetic one    |
| `qualified_reference_with_unmatched_path_is_e0111`                     | a qualified reference whose path segments don't match any actual import is E0111                  |
| `qualified_reference_to_a_file_that_doesnt_declare_the_name_is_e0111`  | a qualified reference to a real file that doesn't declare the named module is E0111               |
| `same_name_module_in_the_same_file_is_still_e0001`                     | two same-named modules in the SAME file is still E0001 — only cross-file collisions are legal     |
| `duplicate_signal_in_module_is_e0003`                                  | a duplicate signal name within one module is E0003                                                |
| `duplicate_file_const_is_e0004`                                        | a duplicate file-level `const` name is E0004                                                      |
| `unknown_name_is_e0101_with_teaching_help`                             | an unknown name is E0101 with teaching help text                                                  |
| `array_param_length_referencing_an_unbound_name_is_e0101`              | an array param's length expression referencing an unbound name is E0101                           |
| `unknown_module_in_inst_is_e0102_and_mentions_import`                  | instantiating an unknown module is E0102, mentioning `import`                                     |
| `unknown_enum_variant_is_e0103_and_lists_variants`                     | referencing an unknown enum variant is E0103, listing the real variants                           |
| `reading_an_input_of_an_instance_is_e0104`                             | reading an instance's OWN input port (not an output) is E0104                                     |
| `field_on_a_wire_is_e0105`                                             | field access on a plain (non-bundle) wire is E0105                                                |
| `unknown_param_in_inst_is_e0106_and_lists_params`                      | an unknown parameter name in an instantiation is E0106, listing the real params                   |
| `connecting_an_output_is_e0107`                                        | connecting to an instance's output port (outputs aren't connectable) is E0107                     |
| `assigning_an_input_is_e0108`                                          | assigning a module's own `in` port is E0108                                                       |
| `on_rise_of_a_non_clock_is_e0109`                                      | `on rise(x)` where `x` isn't declared `clock` is E0109                                            |
| `const_arithmetic_and_repeat_bounds_evaluate`                          | compile-time const arithmetic and `repeat` bounds evaluate correctly with zero diagnostics        |
| `non_constant_repeat_bound_is_e0201`                                   | a non-constant `repeat` bound is E0201                                                            |
| `foreach_elements_form_on_scalar_is_e0417`                             | `foreach v in <scalar>` (not array/mem-typed) is E0417                                            |
| `foreach_range_form_checks_clean`                                      | `foreach i in 0..N { }` (range form) checks clean, lowering to `repeat`/`loop` as expected        |
| `foreach_elements_form_checks_clean_over_mem`                          | `foreach v in <mem>` (elements form over a `mem`) checks clean                                    |
| `foreach_elements_form_variable_resolves_inside_on_block`              | the bound element variable resolves correctly inside a clocked `on` block                         |
| `foreach_elements_form_at_module_level_checks_clean`                   | `foreach` as a bare module item (not inside `on`/`fn`) checks clean                               |
| `foreach_elements_form_in_fn_body_resolves_via_own_param`              | inside a `fn` body, the elements-form source resolves against the `fn`'s own parameter list       |
| `const_using_a_later_const_is_e0201`                                   | a `const` referencing another `const` declared LATER in the file is E0201 (no forward ref)        |
| `const_overflow_is_e0202`                                              | a `const` expression that overflows is E0202                                                      |
| `reg_without_reset_declaration_is_e0301`                               | a `reg` with no `reset` line is E0301                                                             |

### checker/tests/widths.rs (55 tests)

The width slice (E0401–E0410) added error paths for every code (several
codes get two angles, e.g. `extend`-narrowing AND `trunc`-widening for
E0407) plus clean passes.

| Test                                                       | Locks in                                                                                             |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `assignment_width_mismatch_is_e0401`                       | an assignment with mismatched widths is E0401                                                        |
| `plus_into_same_width_target_teaches_wrap_in_e0401`        | the dropped-carry moment teaches `+%` — the spec/02 section 1.2 promise, executable                  |
| `connection_width_mismatch_is_e0401_naming_the_port`       | an instance port connection at the wrong width is E0401, naming the port                             |
| `replication_width_is_count_times_inner`                   | `{2{bits[4]}}` is `bits[8]`, `{3{bits[4]}}` is `bits[12]` (A1)                                       |
| `replication_width_mismatch_is_e0401`                      | `{2{a}}` (bits[8]) into a `bits[4]` is the usual assignment width error                              |
| `a_non_constant_replication_count_is_e0201`                | `{n{a}}` with a signal count is "not a compile-time constant" (reused code)                          |
| `a_zero_replication_count_is_e0410`                        | `{0{a}}` has zero width — reuses the "not a valid width" code                                        |
| `dont_care_pattern_must_match_the_scrutinee_width`         | `0b1??` is fine on `bits[3]`, a width error (E0409) on `bits[4]` (A2)                                |
| `a_dont_care_match_still_needs_a_wildcard`                 | masked patterns earn no coverage — `0b1??`+`0b0??` without `_` is E0601 (A2)                         |
| `a_dont_care_pattern_on_an_enum_is_e0409`                  | a masked pattern on an enum scrutinee is rejected (match variants by name) (A2)                      |
| `min_max_take_two_same_width_operands`                     | `min`/`max` require both operands at the same width                                                  |
| `min_of_mismatched_widths_is_e0402`                        | `min` of two mismatched-width operands is E0402                                                      |
| `abs_of_signed_grows_one_bit`                              | `abs` of a signed value grows the result by one bit (sign-removal headroom)                          |
| `abs_of_unsigned_is_e0407`                                 | `abs` of an already-unsigned value is E0407 (nothing to make absolute)                               |
| `nand_reduces_to_a_bit`                                    | `nand` reduces its operand to a single bit                                                           |
| `nor_of_signed_is_e0403`                                   | `nor` of a signed operand is E0403 (bitwise built-ins reject signed)                                 |
| `max_with_a_literal_operand_adapts`                        | `max` with one literal operand adapts the literal to the other operand's width                       |
| `abs_of_a_literal_is_e0407`                                | `abs` of a bare literal (no signed context) is E0407                                                 |
| `min_of_two_literals_is_e0407`                             | `min` of two bare literals is E0407 (needs a signal to establish width context)                      |
| `nand_of_a_bare_bit_is_a_bit`                              | `nand` of a bare `bit` operand stays `bit`-typed                                                     |
| `nested_abs_of_min_type_checks`                            | `abs(min(a, b))` (nested built-ins) type-checks through both layers                                  |
| `min_of_two_abs_type_checks`                               | `min(abs(a), abs(b))` type-checks, each `abs` growing independently before `min` compares            |
| `abs_grows_at_the_width_boundary`                          | `abs` at the width boundary (widest representable signed value) grows correctly, no overflow         |
| `bitwise_operand_mismatch_is_e0402`                        | mismatched-width operands to a bitwise op (`&`/`\|`/`^`) is E0402                                    |
| `wrapping_add_operand_mismatch_is_e0402`                   | mismatched-width operands to `+%` is E0402                                                           |
| `signed_bits_mixing_is_e0403`                              | mixing `signed[N]` and unsigned `bits[N]` in one expression is E0403                                 |
| `clock_in_a_data_expression_is_e0403`                      | using a `clock` signal inside a data expression is E0403                                             |
| `logical_and_on_a_bus_is_e0404`                            | using `&&` (logical) on a multi-bit bus is E0404 (logical ops are bit-only)                          |
| `literal_that_does_not_fit_is_e0405`                       | a literal that doesn't fit its declared/target width is E0405                                        |
| `negative_literal_in_unsigned_context_is_e0405`            | a negative literal used in an unsigned context is E0405                                              |
| `a_wide_literal_fits_a_wide_declared_width`                | a wide literal (past 128 bits) fits cleanly when the declared width is wide enough                   |
| `index_out_of_range_is_e0406`                              | a bit index past the signal's width is E0406                                                         |
| `reversed_slice_is_e0406`                                  | a slice with `hi < lo` (reversed bounds) is E0406                                                    |
| `huge_slice_bound_that_would_wrap_u32_is_still_e0406`      | a slice bound large enough to wrap a `u32` cast is still a clean E0406, not a silently-wrapped index |
| `extend_to_a_smaller_width_is_e0407`                       | `extend` to a SMALLER width is E0407 (that's narrowing — use `trunc`)                                |
| `trunc_to_a_larger_width_is_e0407`                         | `trunc` to a LARGER width is E0407 (that's widening — use `extend`)                                  |
| `negating_bits_is_e0407`                                   | unary `-` on an unsigned `bits[N]` is E0407 (negation needs `signed`)                                |
| `if_arms_that_disagree_are_e0408`                          | an if-expression whose arms disagree in width is E0408                                               |
| `match_pattern_wider_than_scrutinee_is_e0409`              | a match pattern literal wider than the scrutinee is E0409                                            |
| `match_on_signed_is_e0409`                                 | matching on a `signed[N]` scrutinee is E0409 (match patterns are unsigned-only)                      |
| `zero_width_is_e0410`                                      | a zero-width declaration (`bits[0]`) is E0410                                                        |
| `zero_width_output_with_indexed_drivers_does_not_panic`    | a zero-width output with indexed per-bit drivers is a clean E0410-family error, not a panic          |
| `adder_growth_passes`                                      | the adder-growth idiom (`bits[W] + bits[W] -> bits[W+1]`) checks clean                               |
| `alu_match_arms_pass`                                      | an ALU's `match`-selected arithmetic arms all check clean together                                   |
| `enum_state_machine_passes`                                | an enum-driven FSM module checks clean end to end                                                    |
| `register_file_passes`                                     | a `mem` with a clocked indexed write + combinational indexed read checks clean (A4)                  |
| `a_non_constant_memory_depth_is_e0201`                     | a memory `DEPTH` that is not a compile-time constant is E0201 (A4)                                   |
| `a_zero_memory_depth_is_e0410`                             | a memory `DEPTH` of 0 is E0410 — a memory needs at least one cell (A4)                               |
| `a_memory_init_that_overflows_the_element_is_e0405`        | a `mem` init value too wide for the element type is E0405 (A4)                                       |
| `a_constant_address_past_the_depth_is_e0406`               | a compile-time address `≥ DEPTH` is E0406 (out of range) (A4)                                        |
| `a_memory_inside_repeat_is_e0303`                          | declaring a `mem` inside `repeat` is E0303 (declare once, outside) (A4)                              |
| `extend_of_a_bit_into_bitwise_passes`                      | the fixed shift-register shape — explicit `extend` where widths differ                               |
| `comparison_with_a_const_passes`                           | comparing a signal against a compile-time `const` checks clean                                       |
| `defaultless_param_module_is_checked_per_instantiation`    | a module with no param defaults is checked under each instantiation's concrete binding               |
| `repeat_index_out_of_range_at_the_last_iteration_is_e0406` | `repeat` bodies are width-checked per iteration value, not just once                                 |

### checker/tests/drivers.rs (17 tests)

The driver slice (E0501–E0505) covers every code's error paths (both
halves where a code covers two mistakes, e.g. zero AND multiple `on`
blocks for E0503) plus clean passes guarding against false positives.

| Test                                                        | Locks in                                                                                  |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `driving_a_signal_twice_is_e0501`                           | driving the same signal twice is E0501                                                    |
| `driving_a_wire_after_its_declaration_is_e0501`             | a second drive of a wire after its own declaration-with-init is E0501                     |
| `overlapping_slice_drives_are_e0501`                        | two overlapping bit-slice drives of the same signal are E0501                             |
| `an_undriven_output_is_e0502`                               | an undriven output is E0502                                                               |
| `a_partially_driven_output_is_e0502_naming_the_bit`         | a partially-driven output (some bits undriven) is E0502, naming the specific bit          |
| `a_reg_assigned_in_two_on_blocks_is_e0503`                  | a reg assigned in two different `on` blocks is E0503                                      |
| `a_reg_never_assigned_is_e0503`                             | a reg with no assignment at all is E0503                                                  |
| `a_self_referential_wire_is_e0504`                          | a wire that references itself is E0504                                                    |
| `a_two_wire_cycle_is_e0504_showing_the_path`                | a two-wire combinational cycle is E0504, showing the cycle path                           |
| `a_cycle_through_instances_is_e0504`                        | combinational loops THROUGH child modules are caught via the comb summaries               |
| `forward_reference_to_unknown_output_field_is_e0104`        | a forward reference to an unknown instance output field is E0104                          |
| `arrow_assignment_to_a_wire_is_e0505`                       | using `<-` (clocked assign) on a wire is E0505                                            |
| `combinational_drive_of_a_reg_is_e0505`                     | using `=` (combinational drive) on a reg is E0505                                         |
| `disjoint_per_bit_drives_via_repeat_pass`                   | the Chaser idiom: eight `led[i] = ...` drives are eight drivers for eight bits — legal    |
| `feedback_through_a_register_is_not_a_cycle`                | a reg breaks the loop — the normal shape of hardware never false-positives                |
| `repeat_instance_array_ripple_carry_is_not_a_cycle`         | per-index instance-output nodes: `fa[1] -> fa[0]` is a chain, not a loop                  |
| `defaultless_module_with_param_indexed_drives_is_not_e0501` | a defaultless-param module whose per-index drives depend on the param isn't falsely E0501 |

### checker/tests/clocks.rs (14 tests)

The clock-domain matrix (E0701–E0705): independent domains clean,
direct read, through-a-wire, domain-mixing wire, unused-second-clock
clean, plus the `sync.*` arg-shape and domain/placement rules.

| Test                                                            | Locks in                                                                                              |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `two_clocks_with_separate_logic_pass`                           | two independent clock domains with no cross-talk pass clean                                           |
| `reading_another_domains_reg_is_e0701`                          | reading a register owned by another clock domain directly is E0701                                    |
| `cross_domain_through_a_wire_is_e0701`                          | crossing clock domains through an intermediate wire is still caught as E0701                          |
| `a_wire_mixing_two_domains_is_e0701`                            | a wire that mixes signals from two clock domains is E0701                                             |
| `same_domain_logic_under_two_declared_clocks_passes`            | E0701 colors by USE, not by declaration count — an unused clock changes nothing                       |
| `sync_double_flop_with_non_clock_second_arg_is_e0702`           | `sync.double_flop`'s second argument not being a clock is E0702                                       |
| `sync_double_flop_with_matching_src_and_dst_clock_is_e0702`     | `sync.double_flop` with identical src/dst clock arguments is E0702 (needs two distinct clocks)        |
| `sync_double_flop_with_a_2_bit_signal_is_e0703`                 | `sync.double_flop` on a signal wider than 1 bit is E0703 (single-bit-only crossing primitive)         |
| `sync_double_flop_signal_from_a_third_unrelated_clock_is_e0704` | `sync.double_flop`'s source signal belonging to neither declared clock is E0704                       |
| `sync_pulse_signal_that_is_domain_free_is_e0704`                | `sync.pulse`'s source must be exactly a register owned by `src_clock` — a domain-free source is E0704 |
| `sync_double_flop_used_outside_its_own_on_block_clock_is_e0705` | `sync.double_flop`'s result used outside the `on`-block clock it was assigned in is E0705             |
| `sync_pulse_used_as_a_reg_source_is_e0705`                      | `sync.pulse`'s result feeding a register clocked wrong is E0705                                       |
| `sync_double_flop_hidden_in_a_reg_reset_value_is_e0705`         | a `sync.double_flop` call hidden inside a reg's reset-value expression is still caught, E0705         |
| `sync_double_flop_hidden_in_a_sync_loop_body_is_e0705`          | a `sync.double_flop` call hidden inside a `sync loop` body is still caught, E0705                     |

### checker/tests/insts.rs (4 tests)

| Test                                                 | Locks in                                                                  |
| ---------------------------------------------------- | ------------------------------------------------------------------------- |
| `unconnected_input_is_e0302_naming_it`               | an unconnected instance input is E0302, naming it                         |
| `several_unconnected_inputs_are_listed_in_one_error` | multiple unconnected inputs are listed together in one E0302              |
| `clock_and_reset_ports_may_be_omitted`               | E0302 exempts clock/reset — implicit-by-name stays the emitter's contract |
| `connecting_an_input_twice_is_e0302`                 | connecting the same instance input twice is E0302                         |

### checker/tests/enums.rs (35 tests)

Instantiation/exhaustiveness completeness (E0302/E0601/E0602/E0701,
2026-06-12) plus tagged-union/enum-variant construction and OR-arm
binding intersection (E0808, algorithm in `checker/names.rs`).

| Test                                                                                | Locks in                                                                                                         |
| ----------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `tagged_enum_unknown_payload_type_is_e0103`                                         | a tagged-enum variant field with an unknown type is E0103                                                        |
| `tagged_enum_toplevel_unknown_payload_type_is_e0103`                                | a top-level tagged-enum declaration with an unknown payload type is E0103                                        |
| `tagged_pattern_arity_mismatch_is_e0806`                                            | a match pattern binding the wrong number of payload fields is E0806                                              |
| `tag_only_pattern_with_bindings_is_e0806`                                           | a tag-only variant's match pattern supplying bindings (it has no fields) is E0806                                |
| `valid_tagged_pattern_compiles_clean`                                               | a correctly-arity-matched tagged pattern checks clean                                                            |
| `enum_construct_unknown_enum_name`                                                  | `Enum.Variant(...)` construction with an unknown enum name is rejected                                           |
| `enum_construct_unknown_variant_name`                                               | `Enum.Variant(...)` construction with an unknown variant name is rejected                                        |
| `enum_construct_arity_mismatch_is_e0806`                                            | enum construction with the wrong argument count is E0806                                                         |
| `enum_construct_tag_only_with_extra_args_is_e0806`                                  | constructing a tag-only variant with extra args is E0806                                                         |
| `enum_construct_recurses_into_args_for_name_resolution`                             | enum-construct arguments are name-resolved recursively                                                           |
| `match_arm_binding_field_width_resolves_against_enum_declaring_file_not_match_site` | a match-arm binding's field width resolves against the enum's DECLARING file, not the matching file (cross-file) |
| `tagged_enum_total_width_is_tag_plus_max_payload`                                   | a tagged enum's total width is the tag width plus the widest variant's payload                                   |
| `pattern_binding_types_match_payload_fields`                                        | match-pattern bindings take on their corresponding payload field's type                                          |
| `enum_payload_enum_type_is_e0807`                                                   | an enum variant field typed as another enum is E0807 (no nested enum payloads)                                   |
| `enum_payload_array_type_is_e0807`                                                  | an enum variant field typed as an array is E0807                                                                 |
| `enum_construct_wrong_arg_width_is_e0401`                                           | an enum-construct argument at the wrong width is E0401                                                           |
| `enum_construct_valid_use_checks_clean_and_infers_enum_ty`                          | a correct enum-construct checks clean and infers the enum type for the expression                                |
| `enum_construct_literal_arg_adapts_to_field_width`                                  | a literal argument to enum-construct adapts to the target field's width                                          |
| `enum_construct_emits_tag_and_payload_concat`                                       | enum-construct emits a tag+payload concat in the lowered representation                                          |
| `enum_construct_literal_arg_is_sized_to_field_width_in_concat`                      | a literal enum-construct argument is explicitly sized to the field width in the emitted concat                   |
| `enum_construct_negative_literal_arg_is_masked_and_sized_not_left_bare`             | a negative literal enum-construct argument is masked and sized, not left bare                                    |
| `enum_construct_tag_only_zero_args_emits_bare_tag`                                  | constructing a tag-only variant with zero args emits just the bare tag (no payload concat)                       |
| `or_arm_same_names_same_widths_is_clean`                                            | a 2-way OR-arm match pattern with identical binding names and widths is clean                                    |
| `or_arm_three_alts_same_bindings_is_clean`                                          | a 3-way OR-arm match pattern with identical bindings across all alternatives is clean                            |
| `or_arm_different_names_is_e0808`                                                   | an OR-arm pattern whose alternatives bind different names is E0808                                               |
| `or_arm_tag_only_alt_is_e0808`                                                      | an OR-arm mixing a tag-only alternative with a binding alternative is E0808                                      |
| `or_arm_subset_binding_is_e0808`                                                    | an OR-arm alternative binding a subset of the other alternatives' names is E0808                                 |
| `or_arm_width_mismatch_is_e0808`                                                    | an OR-arm alternative binding the same name at a different width is E0808                                        |
| `e0809_default_target_not_reg`                                                      | `default` keyword must target a `reg`                                                                            |
| `e0810_duplicate_default`                                                           | a duplicate `default` assignment to the same reg is E0810                                                        |
| `e0811_const_if_condition_not_const`                                                | `const if` conditions must be compile-time constants                                                             |
| `e0813_fn_let_shadow_width_mismatch`                                                | BUG-9: a `fn`-body `let` re-binding a name (earlier `let` or param) at a different width is E0813                |
| `fn_let_shadow_same_width_stays_clean`                                              | the fold/accumulator idiom (same-width shadow, e.g. `foreach_sum.mimz`'s `acc`) stays legal                      |
| `fn_let_shadowing_a_param_at_a_different_width_is_e0813`                            | shadowing a PARAM (not just an earlier `let`) at a different width is the same E0813 conflict                    |
| `or_arm_wildcard_not_binding_e0808`                                                 | an OR-arm alternative that's a bare wildcard `_` mixed with a binding alternative is E0808                       |

### checker/tests/funcs_and_loops.rs (24 tests)

`repeat`/`fn`/`loop`/`sync loop` declaration restrictions, return-width
checking, and per-scope name/width leakage guards.

| Test                                                                             | Locks in                                                                                                    |
| -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `wire_inside_repeat_is_e0303`                                                    | declaring a `wire` inside `repeat` is E0303                                                                 |
| `reg_inside_repeat_is_e0303`                                                     | declaring a `reg` inside `repeat` is E0303                                                                  |
| `on_block_inside_repeat_is_e0303`                                                | an `on` block inside `repeat` is E0303                                                                      |
| `const_inside_repeat_is_e0303`                                                   | a `const` declared inside `repeat` is E0303                                                                 |
| `repeat_with_only_drives_and_nested_repeat_is_clean`                             | a `repeat` containing only drives and a nested `repeat` checks clean                                        |
| `fn_body_width_mismatch_is_e0804`                                                | `fn` return values must match the declared return width                                                     |
| `return_width_mismatch_is_e0804`                                                 | a `return` statement's value at the wrong width is E0804                                                    |
| `return_width_match_is_accepted`                                                 | a `return` statement's value at the matching width is accepted                                              |
| `mac_function_type_checks_clean`                                                 | the canonical `mac` combinational function type-checks clean                                                |
| `fn_with_const_local_compiles_clean`                                             | a `fn` body with a local `const` compiles clean                                                             |
| `unbound_name_inside_fn_return_is_rejected`                                      | an unbound name inside a `fn`'s return expression is rejected                                               |
| `fn_if_branch_names_are_resolved`                                                | names declared inside a `fn`'s `if` branch resolve correctly within that branch                             |
| `let_bound_only_inside_an_if_branch_does_not_leak_outside`                       | a `let` bound only inside an `if` branch does not leak outside the branch                                   |
| `let_bound_only_inside_one_if_branch_is_not_visible_in_the_sibling_branch`       | a `let` bound in one `if` branch is not visible in the sibling (`else`) branch                              |
| `let_bound_only_inside_an_if_branch_is_not_visible_to_width_checking_outside_it` | a branch-local `let` is invisible to width-checking outside the branch, not just name resolution            |
| `fn_loop_variable_resolves_inside_its_own_body`                                  | a `loop` variable resolves correctly inside its own body                                                    |
| `seq_loop_variable_resolves_inside_on_block`                                     | a `sync loop` variable resolves correctly inside its `on`-block body                                        |
| `fn_loop_variable_does_not_leak_outside_the_loop`                                | `loop` variables are scoped strictly to the loop body                                                       |
| `seq_loop_variable_does_not_leak_outside_the_loop`                               | a `sync loop` variable does not leak outside the loop, mirroring the `fn`-loop scoping rule                 |
| `fn_loop_local_let_does_not_leak_outside_the_loop`                               | a `let` declared inside a `fn`-body `loop` does not leak outside the loop                                   |
| `non_constant_seq_loop_bound_is_e0201`                                           | loop boundary conditions must be compile-time constants                                                     |
| `non_constant_fn_loop_bound_is_e0201`                                            | a non-constant `fn`-body `loop` boundary is E0201, mirroring the `sync loop` rule                           |
| `fn_loop_body_width_mismatch_is_checked`                                         | width mismatches inside a `fn`-body `loop`'s body are still checked per iteration                           |
| `fn_loop_width_bug_independent_of_loop_var_reports_once`                         | a width bug inside a `fn` loop independent of the loop variable is reported exactly once, not per-iteration |

### checker/tests/patterns.rs (12 tests)

| Test                                                          | Locks in                                                                                                 |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `enum_match_covering_every_variant_needs_no_wildcard`         | the v0.2.3 ruling, executable: full coverage IS exhaustive, no `_` ceremony                              |
| `enum_match_missing_a_variant_is_e0601_naming_it`             | an enum `match` missing a variant is E0601, naming the missing variant                                   |
| `wildcard_after_full_enum_coverage_is_allowed`                | the defensive `_` (bit-flip recovery) is never flagged unreachable                                       |
| `duplicate_variant_pattern_is_e0602`                          | matching the same enum variant twice is E0602                                                            |
| `arm_after_wildcard_is_e0602`                                 | a match arm after the `_` wildcard is E0602 (unreachable)                                                |
| `bits2_match_covering_all_four_values_passes`                 | a `bits[2]` match covering all 4 values passes without a wildcard                                        |
| `bits2_match_missing_a_value_is_e0601_naming_it`              | a `bits[2]` match missing one value is E0601, naming it                                                  |
| `bit_match_missing_one_is_e0601`                              | a `bit` match missing one of its two values (0/1) is E0601                                               |
| `wide_match_without_wildcard_is_e0601`                        | a match on a wide (`bits[N]`, N large) scrutinee with no wildcard is E0601 (can't enumerate every value) |
| `wide_match_with_a_past_128_bit_pattern_is_e0601_not_a_panic` | a match with a pattern literal past 128 bits is E0601, not a panic                                       |
| `multi_pattern_arms_count_toward_coverage`                    | a single arm matching multiple patterns (`0 \| 1 => ...`) counts all toward exhaustiveness coverage      |
| `duplicate_value_in_multi_pattern_arm_is_e0602`               | a duplicate value within one multi-pattern arm's own pattern list is E0602                               |

### checker/tests/regressions.rs (4 tests)

Kept as one file, not split further — a named historical batch (Task 15
sweep) tied to `.superpowers/sdd/progress.md`'s notes.

| Test                                                                                 | Locks in                                                                                         |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| `overlapping_import_prefixes_disambiguate_correctly`                                 | two imports whose paths share a prefix still disambiguate to the correct file                    |
| `no_default_param_module_only_discovered_via_instantiation_still_gets_width_checked` | a defaultless-param module only ever discovered via instantiation still gets its width pass run  |
| `two_same_named_modules_each_get_their_own_clock_check_reversed_order`               | the two-same-named-modules clock-check isolation holds regardless of file load order             |
| `recursive_call_inside_return_is_e0805`                                              | a `fn` recursively calling itself from inside a `return` expression is E0805 (cyclic-call guard) |

### checker/tests/bundles.rs (31 tests)

| Test                                                                 | Locks in                                                                                                |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `bundle_duplicate_name_is_e0909`                                     | duplicate bundle declarations error out                                                                 |
| `bundle_clean_declaration_passes`                                    | a well-formed bundle declaration checks clean                                                           |
| `bundle_named_field_as_module_port_passes`                           | a bundle field used as a module port name checks clean                                                  |
| `bundle_unknown_parametric_type_in_field_is_e0906`                   | a bundle field with an unknown parametric type is E0906                                                 |
| `bundle_nested_bundle_field_is_e0807`                                | nested bundles (bundles inside bundles) are caught as an error                                          |
| `bundle_array_field_is_e0807`                                        | an array-typed bundle field is E0807 (same nesting restriction as nested bundles)                       |
| `builtin_valid_bundle_resolves_by_name`                              | the built-in `__Valid` bundle type resolves by name                                                     |
| `builtin_valid_signed_bundle_resolves_by_name`                       | the built-in `__ValidSigned` bundle type resolves by name                                               |
| `qq_unwrap_form_types_as_the_data_field_type`                        | `a??` (unwrap) types as the `data` field's own type                                                     |
| `qq_or_mux_form_types_as_still_optional`                             | `a ?? b` (or-mux) types as still-optional when both sides are optional                                  |
| `qq_lhs_not_optional_is_e0911`                                       | `??`'s LHS not being an optional (valid-bundle) type is E0911                                           |
| `qq_rhs_wrong_width_is_e0912`                                        | `??`'s RHS at the wrong width vs the LHS's data field is E0912                                          |
| `builtin_valid_bundle_shows_as_surface_syntax_in_diagnostics`        | a diagnostic naming `__Valid` renders it as the surface `bit?`/`bits[N]?` syntax, not the internal name |
| `builtin_valid_bundle_bit_question_collapses_to_bit_in_diagnostics`  | `bit?`'s diagnostic rendering collapses `bits[1]?` to `bit?`                                            |
| `builtin_valid_signed_bundle_shows_as_surface_syntax_in_diagnostics` | a diagnostic naming `__ValidSigned` renders it as `signed[N]?`                                          |
| `qq_same_shaped_user_bundle_satisfies_a_valid_bundle_slot`           | a user bundle with the same shape as `__Valid` structurally satisfies a `T?` slot                       |
| `qq_lhs_missing_valid_field_is_e0911`                                | a user bundle missing the `valid` field used as `??`'s LHS is E0911                                     |
| `qq_or_mux_rhs_with_extra_field_is_e0912`                            | `??`'s RHS with an extra field beyond the LHS's data type is E0912                                      |
| `bundle_field_typed_as_valid_bundle_sugar_is_rejected_e0807`         | a bundle field typed with the `T?` sugar (`bit?`) is rejected as nested-bundle-like, E0807              |
| `bundle_literal_missing_field`                                       | bundle literal missing a declared field is an error                                                     |
| `bundle_literal_unknown_field`                                       | a bundle literal naming an unknown field is an error                                                    |
| `bundle_type_mismatch`                                               | bundle type mismatch is caught correctly                                                                |
| `structurally_compatible_bundles_check_clean_in_a_drive`             | a drive between structurally compatible (not nominally identical) bundles checks clean                  |
| `structurally_compatible_bundle_with_extra_fields_checks_clean`      | a bundle with extra fields beyond the target's requirement still checks clean structurally              |
| `drive_bundle_missing_required_field_is_e0910`                       | a drive whose bundle source is missing a required field is E0910                                        |
| `drive_bundle_shared_field_wrong_width_is_e0907`                     | a drive between bundles with a shared field at mismatched width is E0907                                |
| `drive_bundle_same_name_regression_still_checks_clean`               | two same-named-and-shaped bundle types in a drive still check clean (no false E0907 from name alone)    |
| `bundle_destructure_duplicate_binding`                               | a bundle destructure with a duplicate binding name is an error                                          |
| `two_same_named_modules_each_get_their_own_driver_check`             | two same-named modules from different files each get their OWN driver-pass check, not a merged one      |
| `two_same_named_modules_each_get_their_own_width_check`              | two same-named modules from different files each get their own width-pass check                         |
| `two_same_named_modules_each_get_their_own_clock_check`              | two same-named modules from different files each get their own clock-domain check                       |

### checker/tests/arrays.rs (37 tests)

Array-typed params/literals/indices, `extern module`, and structural
(shape-based, not nominal) bundle compatibility across drives/fn
args-returns/port connections.

| Test                                                                  | Locks in                                                                                                |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `unreachable_code_after_return_is_e0812`                              | a statement after `return` (not the tail) is unreachable — E0812                                        |
| `return_as_last_statement_before_tail_is_not_e0812`                   | `return` immediately before the tail expression is NOT flagged unreachable (no code follows it)         |
| `fn_loop_body_return_followed_by_more_code_is_unreachable`            | code after `return` inside a `loop` body is still E0812                                                 |
| `fn_loop_after_return_in_sibling_branch_is_not_flagged`               | code after an `if` branch's `return` in a SIBLING branch is not falsely flagged                         |
| `array_param_with_bundle_element_type_is_e0411`                       | an array param whose element type is a bundle is E0411                                                  |
| `array_param_with_zero_length_is_e0412`                               | a zero-length array param is E0412                                                                      |
| `array_literal_infers_its_own_type`                                   | an array literal infers its own element type and length                                                 |
| `array_literal_with_mismatched_element_widths_is_e0414`               | an array literal with mismatched element widths is E0414                                                |
| `array_literal_argument_length_mismatch_is_e0413`                     | an array literal argument with the wrong length is E0413                                                |
| `array_param_forwarded_by_name_with_matching_type_is_accepted`        | forwarding an array param by name with a matching type is accepted                                      |
| `array_param_forwarded_by_name_with_mismatched_length_is_rejected`    | forwarding an array param by name with a mismatched length is rejected                                  |
| `constant_array_index_out_of_range_is_e0415`                          | a compile-time-constant array index past the length is E0415                                            |
| `runtime_array_index_is_accepted`                                     | a runtime (non-constant) array index is accepted — only constant indices are range-checked              |
| `array_typed_module_port_is_e0416`                                    | an array-typed module port is E0416 (arrays aren't a port type)                                         |
| `array_typed_wire_is_e0416`                                           | an array-typed `wire` is E0416                                                                          |
| `array_typed_output_with_constant_indexed_drive_is_e0416_not_a_panic` | an array-typed output driven by a constant index is E0416, not a panic (regression)                     |
| `extern_module_duplicate_in_same_file_is_e1301`                       | two `extern module`s with the same name in one file is E1301                                            |
| `extern_module_bundle_typed_port_is_e1302`                            | a bundle-typed port on an `extern module` is E1302 (extern ports must be scalar)                        |
| `extern_module_array_typed_port_is_e1302`                             | an array-typed port on an `extern module` is E1302                                                      |
| `extern_module_scalar_ports_check_clean`                              | an `extern module` with only scalar ports checks clean                                                  |
| `extern_instantiation_checks_clean_with_correct_connections`          | instantiating an `extern module` with correct port connections checks clean                             |
| `extern_instantiation_missing_input_connection_is_reported`           | instantiating an `extern module` with a missing input connection is reported (E0302)                    |
| `extern_instantiation_unknown_port_is_reported`                       | connecting an unknown port on an `extern module` instance is reported                                   |
| `extern_instantiation_wrong_width_connection_is_e0401`                | connecting an `extern module` port at the wrong width is E0401                                          |
| `structurally_compatible_bundle_wire_binding_checks_clean`            | a wire bound to a structurally (not nominally) compatible bundle checks clean                           |
| `structurally_compatible_fn_arg_checks_clean`                         | a `fn` argument passed a structurally compatible bundle checks clean                                    |
| `wire_binding_bundle_missing_field_is_e0910`                          | a wire's bundle initializer missing a required field is E0910                                           |
| `structurally_compatible_fn_return_checks_clean`                      | a `fn` returning a structurally compatible bundle checks clean                                          |
| `fn_return_bundle_missing_field_is_e0910`                             | a `fn` bundle-tail return missing a required field is E0910                                             |
| `fn_return_same_name_bundle_regression_still_e0804`                   | two same-named-but-different bundle types on a `fn` return stays E0804, not silently structural-matched |
| `fn_return_bundle_shared_field_wrong_width_is_e0804`                  | a `fn` bundle return with a shared field at the wrong width is E0804                                    |
| `structurally_compatible_bundle_port_connection_checks_clean`         | a module port connection with a structurally compatible bundle checks clean                             |
| `port_connection_bundle_missing_field_is_e0910`                       | a port connection's bundle argument missing a required field is E0910                                   |
| `port_connection_bundle_shared_field_wrong_width_is_e0401`            | a port connection's bundle argument with a shared field at the wrong width is E0401                     |
| `structural_match_composes_across_fn_return_and_port_connection`      | structural bundle matching composes across a `fn` return feeding a port connection                      |
| `drive_bundle_zero_required_fields_always_compatible`                 | a bundle type with zero required fields is always structurally compatible (trivial case)                |
| `matched_ty_same_shaped_bundle_equality_passes`                       | two identically-shaped bundle types compare as matched via structural equality                          |

## Unit: widths pass internals (`crates/mimz-core/src/checker/widths/mod.rs`, `checker::widths::tests`, 7 tests)

A second, smaller unit-test pocket living inside the width pass itself
(distinct from the sibling `checker::tests` module above) — these pin
`Ty`/`Wcx` internals and dispatch paths too fine-grained for the
one-test-per-error-code table, added 2026-07-11 alongside the `Ty::Bundle`
model (see this file's changelog entry for that date).

| Test                                                              | Locks in                                                                                                        |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `sync_loop_result_init_width_checked`                             | a `sync loop`'s `-> result: ty = init` initializer is width-checked against `ty`                                |
| `sync_loop_var_width_is_clog2_hi_not_clog2_range_when_lo_nonzero` | the loop variable's width is `clog2(hi)`, not `clog2(hi - lo)`, when `lo != 0`                                  |
| `bundle_typed_fn_param_supports_field_access`                     | a bundle-typed fn parameter's field access resolves via `cx.sigs`, not a false E0105                            |
| `module_param_field_access_is_rejected`                           | `W.foo` on an int/bool module parameter errors instead of silently passing                                      |
| `mem_field_access_reports_exactly_one_diagnostic`                 | mem/clock/reset field access reports pass-3's diagnostic once, not doubled with `field_ty`'s                    |
| `enum_variant_from_wrong_enum_is_rejected`                        | assigning a variant from the wrong enum into an enum-typed reg/wire is caught (was a silent miss)               |
| `bundle_literal_tail_return_is_shape_checked`                     | a bundle-literal fn-tail return goes through `check_return_expr`, not the old `infer_ty`+`check_return_ty` path |

## Unit: transliteration (`crates/mimz-core/src/emit_verilog/translit.rs`, 7 tests)

| Test                                              | Locks in                                                                               |
| ------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `pure_tamil_words_romanize_readably`              | விளக்கு → `villakku`, நிலை → `nilai` — the readable-output promise                     |
| `ascii_and_mixed_names_keep_their_ascii`          | ASCII passes through untouched, even mixed into a Tamil name                           |
| `non_tamil_unicode_falls_back_to_hex`             | other scripts → `_uXXXX`, never dropped                                                |
| `results_always_start_like_an_identifier`         | output is always a valid Verilog identifier start                                      |
| `the_two_n_letters_romanize_identically`          | ந/ன → `n` is a DOCUMENTED collision; the suffix counter disambiguates                  |
| `enum_construct_romanizes_enum_and_variant_names` | `Enum.Variant(payload)` construction sites romanize BOTH the enum and the variant name |
| `translate_preserves_fn_return_and_if_semantics`  | romanizing a `fn` with `return`/`if` does not change what the function computes        |

## Unit: emitter (`crates/mimz-core/src/emit_verilog/`, 68 tests)

The emitter's units live in five places. `mod.rs`'s own single-file test
module was split into `emit_verilog/tests/` (topic files, 50 tests) on the
`oversized-test-file-split` branch; the remaining four are small pockets
inside the file they test.

| Location                          | Tests | Covers                                                         |
| --------------------------------- | ----: | -------------------------------------------------------------- |
| `emit_verilog/tests/` (10 files)  |    50 | end-to-end emission behavior, by topic (tables below)          |
| `emit_verilog/module/tests.rs`    |     5 | `build_decls` internals + `sync.*`/`sync loop` lowering shape  |
| `emit_verilog/kinds.rs`           |     5 | `infer_kind` — mimz's own width/signedness for an expression   |
| `emit_verilog/self_determined.rs` |     3 | what real Verilog would self-determine for the same expression |
| `emit_verilog/expr.rs`            |     1 | the `is_plain_identifier` hoist predicate                      |

### emit_verilog/tests/builtin_and_loops.rs (9 tests)

| Test                                                        | Locks in                                                                                      |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `repeat_unrolls_drives_with_folded_indices`                 | `repeat i: 0..4 { y[i] = … }` emits `assign y[0..3]`; the half-open range stops at 3          |
| `repeat_var_folds_in_index_arithmetic`                      | `y[i + 1]` folds to `y[1]`/`y[3]` — index arithmetic over the loop var collapses to a literal |
| `empty_and_reversed_ranges_emit_nothing`                    | `0..0` and `4..0` generate no hardware (no crash, no partial output)                          |
| `repeat_over_budget_errors_cleanly`                         | a range past `REPEAT_BUDGET` (4096) is a clean error, not a runaway unroll                    |
| `nested_repeat_folds_both_variables`                        | nested loops bind both `i` and `j` per iteration                                              |
| `repeat_instance_array_gets_flat_names`                     | `let u[i] = …` → `u__<i>` with outputs `u__<i>_<port>`                                        |
| `on_block_loop_unrolls_to_n_copies`                         | `loop` inside an `on` block emits one statement copy per iteration                            |
| `on_block_loop_over_budget_is_rejected`                     | the same budget cap applies to `loop`, not just `repeat`                                      |
| `a_builtin_lowers_parenthesized_inside_a_larger_expression` | `abs(x) + 1` parenthesizes the builtin so precedence cannot shift                             |

### emit_verilog/tests/bundle_flatten.rs (6 tests)

| Test                                                                    | Locks in                                                              |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `bundle_typed_port_flattens_at_instantiation`                           | a bundle-typed port becomes one flat Verilog wire per field           |
| `bundle_typed_fn_param_flattens_to_per_field_inputs`                    | same flattening for a bundle-typed `fn` parameter                     |
| `bundle_port_forwarding_a_module_parameter_stays_symbolic`              | a parametric bundle keeps its parameter expression in the declaration |
| `bundle_port_forwarding_a_module_parameter_resolves_per_instance`       | …and resolves to the concrete width at each instantiation site        |
| `bare_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog`       | returning a bundle from a `fn` errors cleanly (no bogus Verilog)      |
| `parametric_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog` | same for the parametric case                                          |

### emit_verilog/tests/valid_bundle_sugar.rs (10 tests)

The `T?` / `??` sugar — a valid-bundle is `{ valid: bit, data: T }`, and
`??` either unwraps it or OR-muxes two of them.

| Test                                                               | Locks in                                                          |
| ------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `qq_unwrap_form_emits_a_ternary_on_validity`                       | `a ?? fallback` becomes `a_valid ? a_data : fallback`             |
| `qq_unwrap_form_emits_a_ternary_via_drive`                         | same through a `=` drive                                          |
| `qq_or_mux_form_emits_per_field_ternaries_at_wire_init`            | `a ?? b` between two valid-bundles muxes each field separately    |
| `qq_or_mux_form_emits_per_field_ternaries_via_drive`               | same through a drive                                              |
| `qq_or_mux_form_emits_per_field_ternaries_at_port_connection`      | same at an instance port connection                               |
| `qq_or_mux_form_expands_at_fn_call_site`                           | same as a `fn` call argument                                      |
| `qq_or_mux_chain_emits_valid_nested_verilog`                       | `a ?? b ?? c` nests without producing invalid Verilog             |
| `qq_or_mux_chain_expands_correctly_at_fn_call_site`                | …and the chain still expands correctly as an argument             |
| `user_bundle_shaped_like_valid_bundle_emits_same_as_builtin_sugar` | a hand-written `{valid, data}` bundle behaves identically to `T?` |
| `a_decimal_literal_past_128_bits_emits_correctly`                  | wide literals survive emission (the `bits`/`wide` path)           |

### emit_verilog/tests/clocking.rs (3 tests)

| Test                                      | Locks in                                                            |
| ----------------------------------------- | ------------------------------------------------------------------- |
| `on_fall_emits_negedge`                   | `on fall(clk)` lowers to `always @(negedge clk)` (A3)               |
| `async_reset_widens_the_sensitivity_list` | `async reset` lowers to `always @(posedge clk or posedge rst)` (A5) |
| `a_sync_reset_stays_clock_only`           | a plain `reset` keeps `always @(posedge clk)` — no widening (A5)    |

### emit_verilog/tests/clog2.rs (4 tests)

| Test                                                               | Locks in                                                          |
| ------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `clog2_of_a_const_derives_the_width`                               | `clog2(CONST)` folds to a literal width at compile time           |
| `clog2_folds_into_the_port_width`                                  | …including in a port declaration                                  |
| `clog2_of_a_parameter_in_a_body_width_emits_the_constant_function` | over a PARAMETER, the emitter writes a Verilog `function` instead |
| `clog2_of_a_parameter_in_a_port_is_an_emit_error`                  | but a port width cannot use it — a clean error, not bad Verilog   |

### emit_verilog/tests/consts_and_translit.rs (3 tests)

| Test                                                            | Locks in                                                                                 |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `module_const_folds_in_widths_and_emits_no_hardware`            | a `const` folds to a literal in widths and declares no Verilog of its own                |
| `tamil_identifiers_emit_as_romanized_verilog`                   | the transliterated pipeline end to end; no non-ASCII outside the banner comment          |
| `colliding_romanizations_get_suffixes_and_ascii_names_are_safe` | ந/ன clash + an existing ASCII `nii`: user names are never stolen; clashes get `_2`, `_3` |

### emit_verilog/tests/consts_scoping.rs (4 tests)

| Test                                               | Locks in                                                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `child_consts_fold_into_parent_auto_wires`         | the CHILD's const sizes the auto-wire (regression: `wire [(W)-1:0]` leaked and iverilog rejected) |
| `parent_const_never_substitutes_into_child_widths` | same const NAME in parent and child: the child's value wins                                       |
| `two_same_named_modules_emit_their_own_bodies`     | cross-file name reuse emits two distinct module bodies, not one shared                            |
| `diags_carry_the_file_index`                       | project-level diagnostics record WHICH file they point into, so multi-file errors render right    |

### emit_verilog/tests/extern_and_arrays.rs (3 tests)

| Test                                                                | Locks in                                                     |
| ------------------------------------------------------------------- | ------------------------------------------------------------ |
| `extern_instantiation_emits_only_the_instance_line_no_definition`   | an `extern module` is instantiated but never defined by us   |
| `extern_instantiation_uses_the_alias_when_set`                      | the `verilog "RealName"` alias is what appears in the output |
| `zero_length_array_param_runtime_index_is_a_clean_diag_not_a_panic` | a degenerate array parameter errors instead of panicking     |

### emit_verilog/tests/fn_loop.rs (4 tests)

| Test                                                 | Locks in                                                                  |
| ---------------------------------------------------- | ------------------------------------------------------------------------- |
| `fn_loop_with_return_finds_first_match`              | a `loop` + `return` inside a `fn` short-circuits at the first hit         |
| `fn_loop_with_return_first_match_wins_on_duplicate`  | with duplicates, the FIRST match wins (priority, not last-write)          |
| `emitter_injects_function_called_only_from_a_return` | a `fn` reachable only through a `return` is still inlined                 |
| `flattened_loop_shape_fails_the_nesting_assertion`   | a malformed lowered shape trips an internal assertion instead of emitting |

### emit_verilog/tests/structural_match.rs (4 tests)

Bundle compatibility is STRUCTURAL (same field names and types), not
nominal (same declared bundle name) — these prove the emitted Verilog is
byte-identical either way.

| Test                                                                   | Locks in                                   |
| ---------------------------------------------------------------------- | ------------------------------------------ |
| `structurally_matched_port_connection_emits_same_as_nominal_match`     | at an instance port connection             |
| `structurally_matched_drive_emits_same_as_nominal_match`               | at a `=` drive                             |
| `structurally_matched_fn_arg_emits_same_as_nominal_match`              | as a `fn` argument                         |
| `structurally_matched_fn_return_is_a_diagnostic_same_as_nominal_match` | and the return case errors identically too |

### emit_verilog/module/tests.rs (5 tests)

| Test                                                     | Locks in                                                               |
| -------------------------------------------------------- | ---------------------------------------------------------------------- |
| `build_decls_maps_names_to_kinds`                        | the declaration table records each signal's kind (wire/reg/mem/…)      |
| `build_decls_resolves_port_and_wire_widths`              | …and its folded concrete width                                         |
| `sync_loop_emits_fsm_and_ports`                          | a `sync loop` lowers to a real index reg + `start`/`done` handshake    |
| `sync_double_flop_emits_a_plain_reg_chain`               | `sync.double_flop` becomes two ordinary registers — no special Verilog |
| `sync_pulse_emits_a_toggle_reg_and_a_src_clock_on_block` | `sync.pulse` becomes a toggle plus an `on` block on the SOURCE clock   |

### emit_verilog/kinds.rs (5 tests)

`infer_kind` is the emitter-local counterpart to the checker's `Ty` — it
answers "how wide, and signed or not, is this expression?" straight from
the AST.

| Test                                                                | Locks in                                                   |
| ------------------------------------------------------------------- | ---------------------------------------------------------- |
| `literal_gets_its_minimal_width`                                    | a bare literal is as narrow as it can be                   |
| `ident_looks_up_declared_kind`                                      | an identifier takes the kind of its declaration            |
| `lossless_add_grows_by_one_bit`                                     | `+` grows — the exact-widths promise                       |
| `concat_sums_part_widths`                                           | `{a, b}` is `width(a) + width(b)`                          |
| `wrap_add_with_a_narrower_bare_literal_adapts_to_the_sized_operand` | `x +% 1` sizes the literal to `x`, not the other way round |

### emit_verilog/self_determined.rs (3 tests)

The mirror of `kinds.rs`: what real Verilog's own self-determined-width
rule computes for the same expression. Where the two disagree, the
emitter hoists the subexpression into an explicitly-sized wire (BUG-19/20).

| Test                                                           | Locks in                                                                       |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `lossless_sub_self_determines_to_max_operand_width_not_growth` | Verilog does NOT grow `-`; this is exactly the disagreement that needs a hoist |
| `comparison_has_no_verilog_specific_rule`                      | comparisons agree — no hoist needed                                            |
| `plain_identifier_has_no_verilog_specific_rule`                | a bare identifier agrees — no hoist needed                                     |

### emit_verilog/expr.rs (1 test)

| Test                                                | Locks in                                                                   |
| --------------------------------------------------- | -------------------------------------------------------------------------- |
| `is_plain_identifier_accepts_and_rejects_correctly` | the predicate that decides an expression is simple enough to skip hoisting |

## Unit: testbench emitter (`crates/mimz-core/src/emit_verilog/testbench.rs`, 4 tests)

| Test                                            | Locks in                                                                                                                                                         |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sanitize_verilog_ident_replaces_invalid_chars` | spaces/symbols/leading digits/empty string all sanitize to a valid Verilog identifier                                                                            |
| `test_env_falls_back_to_module_param_defaults`  | `--emit-testbench` resolves a width expression for a module parameter the test never overrides, from its `default` (BUG-3)                                       |
| `test_env_chains_earlier_args`                  | a test's later `(NAME: expr, …)` arg may reference an earlier one in the same list, e.g. `DOUBLE: WIDTH * 2`                                                     |
| `colliding_sanitized_test_names_are_rejected`   | two tests whose names sanitize to the same Verilog module id (`"edge case"`/`"edge_case"` -> `edge_case_tb`) error instead of emitting duplicate modules (BUG-4) |

## Integration (`tests/examples.rs`, 13 tests — run the real binary)

`examples/` holds four flavor folders — `english/`, `tanglish/`, `tamil/`,
`mixed/` — each with the SAME 23 base designs + 1 `lib/` helper + 5 `std/`
modules (29 `.mimz` files total; identical identifiers, only keywords differ;
`lib/` and `std/` subfolders hold dotted-import targets and the standard-library
modules). The base-example list lives in the
`BASE_EXAMPLES` const in the test file. (`bitops` — the arithmetic / reduction
built-ins — and `datapath` — `*`/`*%`, `>>`, concat, slice, `trunc` — were added
2026-06-14; the five `std/` modules — `debouncer`, `seg7`, `pwm`, `fifo`,
`uart_tx` — over 2026-06-13…23. The FIFO originally used an explicit `DEPTH`
parameter to work around BUG-6; after the fix it was reverted to `1 << AW`.)

A fifth folder, `examples/tamil-pure/`, holds the **pure-Tamil showcase** —
fully-Tamil programs (Tamil keywords AND identifiers; the `PURE_TAMIL` const
pairs each with the English base example it mirrors). Being language-pure, they
are NOT byte-identical to any other flavor, so they sit OUTSIDE the four-flavor
identity rule (R9) and are validated by equivalence-to-counterpart + their own
goldens (`tests/golden/tamil_pure_*.v`) + their own testbenches.

| Test                                                         | Locks in                                                                                                                                                                                                                                                                                                                     |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_example_checks_clean`                                 | every `.mimz` under `examples/` (recursive) passes `mimz check` — which now runs the CHECKER over the file and its imports, so this is also a zero-false-positives test for every checker rule. At least 4 × 28 base files (plus `lib/` helpers, the `std/` modules, and the pure-Tamil showcase) — RULES R6 made executable |
| `every_example_compiles`                                     | every example **compiles to Verilog**, including the `lib/` helpers. A new example that doesn't compile fails CI by name                                                                                                                                                                                                     |
| `all_four_flavors_compile_to_identical_verilog`              | each base example → **byte-identical** Verilog from all four flavors. The project's thesis. Never break it                                                                                                                                                                                                                   |
| `counter_compiles_to_verilog`                                | end-to-end compile; asserts the parameter, the always-block, the **generated reset**, the assign                                                                                                                                                                                                                             |
| `alu_with_import_compiles`                                   | `import` resolution end-to-end; instances with params; auto-wired child outputs (`add_sum`)                                                                                                                                                                                                                                  |
| `include_alias_compiles_with_dotted_path`                    | `include lib.full_adder` works through the whole pipeline — the alias AND dotted-path resolution, in one example (`english/chained.mimz`)                                                                                                                                                                                    |
| `ripple_adder_unrolls_repeat`                                | `repeat` end-to-end: four `FullAdder fa__0..3` with the carry chained, folded indices, `const WIDTH` folded into widths — compile-time generation proven through the real binary                                                                                                                                             |
| `traffic_light_fsm_compiles`                                 | enums → localparams (`STATE_RED` …)                                                                                                                                                                                                                                                                                          |
| `emitted_verilog_matches_the_goldens`                        | every base example's FULL output equals `tests/golden/<base>.v` byte for byte (banner stripped). On an INTENDED emitter change: `MIMZ_UPDATE_GOLDENS=1 cargo test --test examples`, then review the golden diff like code. Failure names the first differing line                                                            |
| `emitted_testbench_matches_the_goldens`                      | every base example with inline `test` blocks generates a `_tb.v` byte-identical to `tests/golden/<base>_tb.v` (banner stripped); `MIMZ_UPDATE_GOLDENS=1` regenerates — pins the auto-generated `--emit-testbench` output                                                                                                     |
| `emit_testbench_without_test_blocks_notes_and_writes_only_v` | `mimz compile --emit-testbench` on a source with NO `test` blocks still succeeds, writes only the `.v` (no stray `_tb.v`), and prints a no-effect note on stderr — the flag never silently produces nothing                                                                                                                  |
| `pure_tamil_examples_match_goldens`                          | each `examples/tamil-pure/<x>.mimz` output equals `tests/golden/tamil_pure_<x>.v` (banner stripped) — pins the transliterated Verilog so a romanization regression can't slip through                                                                                                                                        |
| `pure_tamil_examples_are_equivalent_to_their_counterparts`   | each pure-Tamil example is the SAME circuit as its English twin, proven by `canonicalize_verilog` (alpha-equivalence: identifiers renamed to `id<N>` by first appearance). Equal canonical forms ⇒ same hardware, just named in Tamil                                                                                        |

## Icarus differential (`tests/icarus.rs`, 10 tests — run a REAL Verilog tool)

The independent judge: our substring asserts check OUR expectations of
the output; these check a real tool's. **Skips with a printed note when
`iverilog` is not installed** (probe order: `MIMZ_IVERILOG` env →
PATH → the Windows installer default `C:\iverilog\bin`); in CI
`REQUIRE_IVERILOG=1` makes a missing install a hard failure, so CI can
never skip silently. Local install: the Windows installer
(bleyer.org/icarus) or `apt-get install iverilog`.

| Test                                                          | Locks in                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_emitted_verilog_passes_iverilog`                       | all 226 corpus files' emitted `.v` (`examples/` + `demo/`, via `support::corpus_files()`) pass `iverilog -t null` — syntax AND elaboration, by Icarus's judgment (incl. the transliterated Tamil-identifier `vilakku`, the pure-Tamil showcase, and `wire signed` `signed_math`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `every_emitted_testbench_passes_iverilog`                     | every corpus file with a `test` block (50: 49 under `examples/`, plus `demo/cpu.mimz`) has an auto-generated `_tb.v` (from `--emit-testbench`) that passes `iverilog -t null` — the generated testbenches are themselves valid, elaborable Verilog by Icarus's judgment, not just our goldens                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `every_emitted_testbench_reports_pass_under_vvp`              | Layer 2 for the whole corpus: each of the 90 emitted testbench MODULES (one per `test` block, built and run separately with `-s <module>`) prints PASS and no FAIL under real `vvp`. The test that closed BUG-64/65; floors at 50 files / 90 modules so a sweep that silently stops covering the corpus fails instead of passing on five files                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `self_checking_testbenches_pass`                              | one hand-written TB per base example (`tests/icarus/*_tb.v`, 45 files) encodes Min-Mozhi's documented semantics (`+%` wraps, sync reset, non-blocking `<-`, FSM timing, SIGNED extension/comparison, `bitops` min/max/abs(MIN)/nand/nor/xnor, `datapath` lossless `*` vs wrapping `*%`/`>>`/concat/slice/`trunc`) and must print PASS under `vvp` — the differential                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `self_checking_pure_tamil_testbenches_pass`                   | the four pure-Tamil showcase circuits (`kanakki`/`cimitti`/`oppidi`/`thervi`), driven through their **romanized** ports (clk=`katikai`, rst=`miill`, …) — proves the transliterated Verilog SIMULATES, not just elaborates. Shares the `run_self_checking` helper with the English layer                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `self_checking_showcase_testbenches_pass`                     | the `showcase/english` self-checking testbenches (`SHOWCASE_TESTBENCHES`) pass under `vvp`; skips on Icarus < v13 (needs `(expr)[(n)-1:0]` truncation syntax)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `fn_array_search_duplicate_match_lower_index_wins_via_icarus` | `fn_array_search.mimz`'s duplicate-match case (target present at index 0 AND 2): our own kernel AND real `iverilog`/`vvp` on the compiled example both return the LOWER index — the loop-unroll's continuation-threading in the emitter didn't regress relative to the kernel                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `sync_loop_search_timing_matches_icarus`                      | `sync_loop_search.mimz`'s `start`→`done` FSM timing and result-latching against real `iverilog`/`vvp` (hand-written, self-checking TB, not the generated-`diff_tb` Layer 3 style)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `sync_double_flop_matches_icarus`                             | `sync_double_flop.mimz`'s two-stage CDC synchronizer crossing latency (2 `clk_dst` edges after the `clk_src` edge that latches the value) against real `iverilog`/`vvp` (hand-written, self-checking TB — the two-clock design can't use the Layer 3 `differential()` helper below, whose default stimulus only drives one clock)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `sync_pulse_matches_icarus`                                   | `sync_pulse.mimz`'s toggle-based CDC pulse synchronizer: a single-cycle `src_pulse` assertion produces exactly one single-cycle `dst_pulse` two `clk_dst` edges after the toggle flips, against real `iverilog`/`vvp` (hand-written, self-checking TB — same two-clock reasoning as `sync_double_flop_matches_icarus` above)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `our_simulator_matches_icarus_bit_for_bit`                    | **Layer 3 (B8 + C1–C4):** three views must agree bit-for-bit per step — our kernel (in-process), the VCD waveform our writer emits, and Icarus on the emitted Verilog under the same stimulus. Auto-routes per design: **clocked** (counter, shift register, edge detector, blinker @ `LIMIT=3`) and **combinational** over generated input vectors (adder, comparator, mux4, datapath, window, full_adder + SIGNED `bitops`/`signed_math`) — 12 ASCII-named english examples — plus the 6 pure-Tamil showcases (kanakki/cimitti/oppidi/thervi/kuutti/saalaivilakku, driven through romanized port names) and the full-parity additions: **alu** (cross-file instance, C2), **chained** (chained instances, C2), **ripple_adder** (`repeat`, C3), **traffic_light** (enum FSM, C4), and **vilakku** (Tamil identifiers). **21 examples** in all — the entire single-file corpus the emitter compiles. Compared via Verilog `%b` (binary ⇒ signedness-agnostic). Where Layer 2 checks Icarus against hand-written asserts, this pits our simulator (engine AND waveform) directly against Icarus |

House rule for the testbenches: each prints `PASS` exactly once or
`FAIL: reason` and stops — the Rust side asserts on those markers, so a
broken TB fails loudly, never silently. The Blinker TB overrides the
`LIMIT` parameter (`#(.LIMIT(3))`) instead of simulating 50M cycles.

## Error fixtures (`tests/errors.rs`, 4 tests — run the real binary on broken code)

End-to-end **failure** validation, the mirror of the checker unit tests: those
prove the checker _function_ rejects bad code; these prove the _CLI_ surfaces it.
`tests/fixtures/errors/*.mimz` holds ~72 intentionally-broken files (kept OUT of
`examples/`, which is asserted valid), each declaring its expected code in a
`// expect: Exxxx` header. Source bodies are lifted from `crates/mimz-core/src/checker/tests.rs`.

| Test                                           | Locks in                                                                                                                                                             |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_error_fixture_reports_its_code`         | each fixture, run through `mimz check`, exits non-zero AND prints `error[<code>]` to stderr — the rendered code is the stable user-facing contract, checked for real |
| `error_corpus_covers_every_checker_code`       | completeness guard: every code in `ALL_CHECKER_CODES` (the 36 stable checker codes) has at least one fixture — a new E-code can't ship without an end-to-end fixture |
| `checker_code_list_matches_the_catalog`        | `ALL_CHECKER_CODES` must equal the 11-checker.md catalog table (reserved rows exempt) — the corpus, the docs, and the code can't drift apart                         |
| `json_flag_emits_machine_readable_diagnostics` | the `--json` wire format (docs/code/06): one JSON array on stdout with code/path/line/help; lexer errors included; `[]` + exit 0 on success                          |

`every_error_fixture_reports_its_code` also asserts a `help:` line per
fixture — the teaching contract, proven at the CLI surface.

Coverage is **every distinct edge case**, not one per code: E0302 missing-input
AND duplicate-conn; E0407 extend-narrowing AND `-` on bits; E0303 all eight
forbidden declaration kinds; E0601 enum/`bits[N]`/`bit`; E0701's three crossings;
etc. The assertion is "stderr _contains_ the code", tolerant of a fixture that
incidentally trips a second rule. Convention + how-to: `tests/fixtures/errors/README.md`.

## Docs-sync (`tests/docs_sync.rs`, 4 tests)

The mechanical staleness guard for `docs/code/` — these verify the
structural facts the docs state, so doc drift fails CI. When one fails,
**fix the named doc page, don't weaken the test.**

| Test                                                | Locks in                                                                                             |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crate_map_lists_every_module`                      | both crate-map copies (`src/main.rs` `//!` table, `docs/code/README.md`) name every top-level module |
| `module_pages_list_every_source_file`               | each module page's file-layout table lists every `.rs` file actually in that `src/` directory        |
| `every_module_is_documented_somewhere_in_docs_code` | a new pipeline stage (e.g. `crates/mimz-core/src/checker/`) cannot land without a docs mention       |
| `code_docs_have_a_sync_stamp`                       | the "Last synced" tripwire line survives                                                             |

## Grammar-sync (`tests/grammar_sync.rs`, 6 tests)

Same philosophy as docs-sync, for the keyword data: the keyword table is
data, so the TextMate grammar and the human-readable spec mirror can silently
drift. Whole-member matching throughout, because `in` is a substring of
`include` — a plain `contains` would pass vacuously. When one fails: fix the
grammar / the spec, don't weaken the test.

| Test                                           | Locks in                                                                                                                                              |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_keyword_spelling_is_in_the_grammar`     | every spelling (canonical + aliases) appears as a whole alternation member in the VS Code grammar                                                     |
| `every_reserved_word_is_marked_invalid`        | every reserved word appears in the grammar's `invalid.illegal` rule                                                                                   |
| `spec_03_keyword_table_matches_keywords_toml`  | every spelling appears in `spec/03` as a backtick word — the spec mirror can't drift after the v1 lock                                                |
| `spec_04_uses_no_superseded_keyword_spellings` | `spec/04`'s worked examples contain none of the 14 superseded v1 spellings (whole-word, Tamil-aware)                                                  |
| `keywords_toml_has_no_superseded_spelling`     | a superseded v1 spelling may never return in `lang/keywords.toml` as a canonical spelling or any alias — guards the reintroduction risk at the source |
| `grammar_and_extension_manifest_agree`         | `package.json` registers `.mimz` and its scope name matches the grammar                                                                               |

## Integration: packages (`tests/packages.rs`, 2 tests — run the real binary)

Proves qualified references (`a.b.Name`) disambiguate two different files'
same-named module through the real `project.rs` loader, not a hand-wired
`resolved_file` like the unit tests use.

| Test                                                         | Locks in                                                                                    |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| `qualified_references_check_clean_with_zero_diagnostics`     | `mimz check` on a qualified-reference fixture reports zero diagnostics                      |
| `qualified_instances_compile_with_their_own_distinct_bodies` | two same-named modules from different files each keep their own body in the emitted Verilog |

## Integration: showcase (`tests/showcase.rs`, 6 tests — run the real binary)

Mirrors `tests/examples.rs` for `showcase/`, the demo set behind the web
playground and documentation site: same flavor-identity and golden-file
rules, plus the pure-Tamil equivalence check.

| Test                                       | Locks in                                                                      |
| ------------------------------------------ | ----------------------------------------------------------------------------- |
| `showcase_every_example_checks_clean`      | every showcase file passes `mimz check` with zero diagnostics                 |
| `showcase_every_example_compiles`          | every showcase file emits Verilog without error                               |
| `showcase_all_four_flavors_identical`      | english/tanglish/tamil/mixed showcase folders emit byte-identical Verilog     |
| `showcase_emitted_verilog_matches_goldens` | showcase output matches `tests/golden/showcase_*.v`                           |
| `showcase_pure_tamil_equivalent`           | the pure-Tamil showcase circuits emit Verilog equivalent to their base flavor |
| `showcase_pure_tamil_match_goldens`        | pure-Tamil showcase output matches its own golden files                       |

## Integration: compile_string (`tests/compile_string.rs`, 14 tests)

Tests the in-memory `mimz::compile_string` entry point — the embedding API
behind the WASM playground — asserting the same pipeline behavior a browser
sees, with no filesystem access. Covers valid compilation, flavor identity,
rendered diagnostics on error (width mismatch, syntax error, rejected
import), bundle port flattening/literals, tagged-packet golden output,
guard-clause return ordering, and array-parameter/array-index expansion
(literal call args, `let` bindings, ports, constant vs. runtime indexing).

## Integration: wasm_parity (`tests/wasm_parity.rs`, 2 tests — CLI vs. WASM)

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

## Editor analysis (`crates/mimz-core/src/analysis.rs`, 7 lib unit tests)

The pure, async-free symbol index and resolution behind the LSP's hover /
go-to-definition / completion (the `src/lsp.rs` handlers are a thin adapter).

| Test                                                     | Locks in                                                                                                                                                  |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `index_collects_each_definition_kind`                    | `build_index` emits a `Symbol` for every def kind (module, param, port, clock, reg, const, enum + variant, inst) with the right `SymKind` + hover render  |
| `resolve_at_use_returns_definition`                      | a use site resolves to its **declaration** span, not the use                                                                                              |
| `resolve_at_works_on_partial_tree`                       | `parse_recover` `Error` node between good ports — names around it still resolve                                                                           |
| `resolve_at_inside_test_block`                           | inside `test "…" for M { … }`: the module-under-test name + driven inputs + `expect` signals resolve to M's ports (cross-file via `same_module_any_file`) |
| `resolve_at_cross_file_instance`                         | an instantiated imported module name resolves into the imported file (`file_idx` differs)                                                                 |
| `completions_include_scope_idents_and_majority_keywords` | in-scope module members + majority-flavor keywords offered, with the right `CandKind`                                                                     |
| `completions_exclude_other_flavor_keywords`              | a Tamil-flavored file offers Tamil keywords, never the English spellings (no cross-flavor leak)                                                           |

## LSP (`src/lsp.rs` unit + `tests/lsp.rs` smoke, 8 tests)

| Test                                                        | Locks in                                                                                                                                     |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `positions_are_utf16_lines_and_columns`                     | byte span → LSP Position math (0-based lines)                                                                                                |
| `offset_inverts_position_utf16`                             | `offset` is the exact inverse of `position` (UTF-16 units, incl. a Tamil line) — the cursor→byte mapping the feature handlers depend on      |
| `tamil_text_counts_utf16_units_not_bytes`                   | LSP columns are UTF-16 code units — a Tamil identifier before the error must not skew the squiggle                                           |
| `analyze_reports_checker_errors_with_codes`                 | the in-memory pipeline (didOpen text, never on disk) produces coded checker diagnostics                                                      |
| `diagnostics_localize_to_the_chosen_flavor`                 | the LSP renders E0501 in Tamil (`y-க்கு` via `morph`) and English verbatim — same plumbing as `check`/`compile`                              |
| `uncovered_code_is_not_localized_in_lsp`                    | an uncovered code (E0401) is byte-identical across flavors in the LSP (the English-fallback invariant)                                       |
| `mixed_flavor_lint_publishes_as_a_warning`                  | W0001 reaches the editor as a WARNING (yellow squiggle), not an error — a mixed-flavor file still builds                                     |
| `opening_a_broken_file_publishes_coded_diagnostics` (smoke) | the REAL binary over the real wire protocol: framed JSON-RPC initialize → didOpen → publishDiagnostics with code, source, help, and position |

## Unit: lint (`crates/mimz-core/src/lint.rs`, 5 tests)

Style and hygiene warnings — the `mimz lint` passes (W0002 snake_case,
W0003 PascalCase, W0004 unused signal). Additive and always warning-only;
no spec or grammar change. Note the unused-signal rule (W0004) has no
dedicated unit test here — it is exercised through `mimz lint`'s own
surface rather than in this pocket.

| Test                                   | Locks in                                                  |
| -------------------------------------- | --------------------------------------------------------- |
| `snake_case_rejects_bad_names`         | a port/wire/reg named `BadStyle` or `UPPER_CASE` is W0002 |
| `snake_case_accepts_valid_names`       | `my_signal`, `data_bus_0` pass with no warning            |
| `pascal_case_rejects_bad_names`        | a module named `bad_style` or `UPPER_MODULE` is W0003     |
| `pascal_case_accepts_valid_names`      | `MyModule`, `TrafficLight` pass with no warning           |
| `lint_empty_file_produces_no_warnings` | no lints fire on a file with zero items                   |

## Benchmark harness (`src/bin/mimz-bench/`, 6 unit tests)

The harness itself (docs in [`12-benchmark.md`](12-benchmark.md))
re-measures what this suite asserts — rates and timings instead of
pass/fail — so its own logic is unit-tested here:

| Test                                       | Locks in                                                       |
| ------------------------------------------ | -------------------------------------------------------------- |
| `rate_percent_handles_zero_and_partial`    | rate math (0/0 reads as 100%, never NaN)                       |
| `expect_header_parses_only_the_convention` | the `// expect: Exxxx` fixture-header parse, same as errors.rs |
| `banner_strip_matches_the_golden_rule`     | banner stripping byte-matches the golden test's rule           |
| `median_is_the_middle_run`                 | timing aggregation (median, robust to one cold run)            |
| `report_renders_a_complete_page` (html)    | the HTML report renders whole: charts, tables, embedded JSON   |
| `failures_flip_the_verdict_and_are_listed` | a failing validation turns the verdict red and is named        |

The `criterion` micro-benchmark harness (`benches/compile.rs`, run with
`cargo bench`) carries **no `#[test]`s** — `criterion` benchmarks aren't
test functions, so it doesn't affect the count above. It's a separate
performance tool, not part of the assertion suite.

## Unit: explain (`crates/mimz-core/src/explain.rs`, 3 tests)

The 8.1 long-form diagnostic catalog behind `mimz explain <CODE>`.

| Test                                       | Locks in                                                                                       |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| `every_checker_code_has_an_explanation`    | every `ALL_CHECKER_CODES` entry has long-form text — a new checker code can't ship without one |
| `table_is_sorted_unique_and_self_labelled` | the `EXPLANATIONS` table is ordered, duplicate-free, and each entry opens with its own code    |
| `lookup_is_case_insensitive_and_trims`     | `explain("e0501")` / `" E0501 "` resolve; unknown codes return `None`                          |

## Unit: translate (`crates/mimz-core/src/translate.rs`, 10 tests)

The keyword-flavor reskin behind `mimz translate --to`, plus the opt-in
`--romanize-names` identifier rewrite (reuses the emitter's `romanize`) and the
reversible sidecar name-map (`romanize_with_map` / `restore_with_map`).

| Test                                                             | Locks in                                                                                       |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `parse_flavor_accepts_the_three_columns`                         | `english`/`tanglish`/`tamil` (case-insensitive) parse; junk → `None`                           |
| `reskins_keywords_keeps_everything_else`                         | keywords swap; comments, layout, identifiers, numbers stay verbatim                            |
| `translating_to_the_same_flavor_is_identity_for_canonical_input` | canonical English → English is a no-op                                                         |
| `romanize_names_rewrites_tamil_identifiers_only_when_asked`      | `--romanize-names` turns `கணக்கு` → `kannakku`; the default leaves the Tamil name              |
| `romanize_names_uniques_against_an_existing_ascii_name`          | a romanization clashing with an ASCII name gets `_2` — names never silently merge              |
| `romanize_with_map_returns_the_inverse_map`                      | the sidecar map is keyed by the Latin spelling → original Tamil (`kannakku` → `கணக்கு`)        |
| `restore_with_map_inverts_romanize`                              | `restore(romanize(src), map)` reproduces the canonical Tamil source — the round-trip identity  |
| `name_map_json_round_trips`                                      | `NameMap` serializes and deserializes through `serde_json` unchanged                           |
| `masked_int_q_does_not_glue_onto_romanized_identifier`           | fuzz regression: a `MaskedInt` ending in `?` abutting a romanized identifier keeps a separator |
| `masked_int_q_does_not_glue_onto_english_keyword`                | …and the same when it abuts an English keyword                                                 |

## Integration: translate (`tests/translate.rs`, 15 tests — the four-flavor oracle + the `--order` pretty-printer + `--romanize-names` + the sidecar name-map)

The `examples/{english,tanglish,tamil}/` folders are byte-identical
keyword-swaps (R9), so they validate the reskin against committed truth. Four
cover `--order` (the `pretty` AST printer): it reformats and drops comments, so
its oracle is semantic (same Verilog) + idempotency, not bytes. The final three
cover `--romanize-names` over the pure-Tamil showcase (Tamil identifiers → Latin,
opt-in and one-way; the default stays lossless).

| Test                                                               | Locks in                                                                                                                                                                       |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `round_trip_to_every_flavor_is_byte_identical`                     | translate-and-back reproduces the canonical source byte-for-byte (lossless; anchored past alias normalize)                                                                     |
| `translating_english_matches_the_committed_flavor_token_for_token` | translating english `X` to flavor `T` lexes identically to the committed `T/X` (comments excluded)                                                                             |
| `every_keyword_token_is_in_the_target_flavor`                      | the reskin actually fires — English `module` is gone, Tamil `தொகுதி` present                                                                                                   |
| `pretty_print_preserves_verilog_across_flavor_and_order`           | every import-free example × flavor × order pretty-prints to byte-identical Verilog (meaning preserved)                                                                         |
| `pretty_print_is_idempotent`                                       | the pretty-printer is a stable canonical form (re-printing its own output is a fixed point), all examples                                                                      |
| `thamizh_order_emits_the_directive`                                | thamizh output starts with `syntax thamizh` / `இலக்கணம் தமிழ்`; code order emits none                                                                                          |
| `cli_translate_order_thamizh_compiles`                             | `--order thamizh --to tamil` on the traffic light yields compilable, same-Verilog Tamil SOV source                                                                             |
| `romanize_names_converts_tamil_identifiers_to_latin`               | `--romanize-names` rewrites Tamil identifiers to Latin in the CODE (comments keep the original); no Tamil-script char survives outside comments                                |
| `romanized_translation_compiles_to_the_same_verilog`               | romanizing then compiling a pure-Tamil file is byte-identical to compiling the original — the romanization matches the emitter's, so meaning is preserved                      |
| `pure_tamil_round_trips_losslessly`                                | the DEFAULT (no flag) still round-trips Tamil → English → Tamil byte-for-byte — the lossless contract holds for Tamil-named files too                                          |
| `romanized_round_trips_losslessly_via_the_name_map`                | romanize (capturing the `NameMap`) then `restore_with_map` reproduces the canonical Tamil source — the one-way romanization made lossless by the sidecar                       |
| `cli_romanize_then_restore_round_trips`                            | end-to-end through the binary: `--romanize-names -o` writes a parseable `<out>.names.json`; a reverse run with `--names-map` restores the exact Tamil source                   |
| `number_abutting_tamil_keeps_a_separator_when_reskinned`           | fuzz-audit regression: `42தொகுதி`/`42கணக்கி` (number + Tamil token, script change as the only separator) stays lexable + token-equivalent after reskin (guard inserts a space) |
| `fn_keyword_translates_across_all_flavors`                         | the `fn` keyword reskins correctly in every flavor (it was the newest keyword when added)                                                                                      |
| `pretty_print_thamizh_flips_the_test_header_and_reparses`          | `--order thamizh` flips a `test "…" for M(args)` header into `M(args) kaaga "…" sodhanai` and the result re-parses to the SAME tree (the B7 oracle)                            |

## Unit: config (`src/config.rs`, 8 tests)

`mimz.toml` parsing + discovery (the precedence merge lives in `main.rs` and is
exercised by the integration tests below).

| Test                                                           | Locks in                                                                                 |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `empty_config_is_all_defaults`                                 | an empty/missing config is all `None` — pure built-in defaults                           |
| `parses_every_section`                                         | `lang` + `[translate]` + `[fmt]` keys deserialize to the right fields                    |
| `unknown_key_is_rejected`                                      | a typo'd key (`too`, `flavour`) errors via `deny_unknown_fields`, never silently dropped |
| `discover_walks_up_to_the_nearest_config`                      | discovery climbs from a nested file to the ancestor `mimz.toml`                          |
| `parses_lib_std_section`                                       | the `[lib] std = "…"` override (vendored standard library) parses                        |
| `unknown_lib_key_is_rejected`                                  | …and a typo inside `[lib]` is rejected the same way                                      |
| `resolve_with_path_returns_config_location`                    | resolution reports WHICH `mimz.toml` won, so a `std` override resolves relative to it    |
| `config_parses_top_level_extern_sim_and_compile_verilog_files` | the top-level `extern_sim` mode and `verilog_files` list parse                           |

## Unit: version (`crates/mimz-core/src/version.rs`, 3 tests)

The two version axes — compiler (crate) vs language edition — and the
`EDITION_HISTORY` source of truth (Workstream B).

| Test                                        | Locks in                                                                                             |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `current_is_the_last_history_row`           | `current()` is the tail of `EDITION_HISTORY`, which stays ordered oldest-first by (year, code)       |
| `keyword_set_version_matches_keywords_toml` | `KEYWORD_SET_VERSION` == `lang/keywords.toml`'s `version` == the current edition's `code` (no drift) |
| `version_block_shows_both_axes`             | `mimz --version` block has the variant on top + the compiler and edition (language) lines            |

## Integration: config (`tests/config.rs`, 7 tests — run the real binary)

The CLI merge (CLI › config › default) and name-map auto-discovery, end to end.

| Test                                               | Locks in                                                                                   |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `auto_name_map_restores_without_a_flag`            | reverse translate auto-loads `<input>.names.json` and restores Tamil — no `--names-map`    |
| `no_names_map_keeps_latin_names`                   | `--no-names-map` opts out of auto-discovery; the romanized Latin decl stays                |
| `config_default_flavor_is_overridden_by_the_cli`   | `[translate] to` supplies the default; an explicit `--to` overrides it                     |
| `malformed_config_is_a_clean_error`                | a broken `mimz.toml` fails with `invalid config`, not a panic                              |
| `name_map_with_unknown_version_is_rejected`        | a `--names-map` with an unknown `version` fails closed (`version 999`), never mis-restores |
| `std_override_inside_workspace_root_is_allowed`    | a `[lib] std` path inside the project is honored (vendored stdlib via `mimz eject std`)    |
| `std_override_escaping_workspace_root_is_rejected` | SEC: a `[lib] std` path escaping the project root is refused — no arbitrary-path read      |

## Integration: stdlib (`tests/stdlib.rs`, 11 tests — drive the lib in-process)

The importable `std.*` library: embedded resolution, trilingual alias routing,
the `[lib]` override, the `mimz eject std` core, and the regression that plain
file-relative imports still work. The 5 catalog-level unit tests
(`crates/mimz-core/src/stdlib.rs`: namespace aliases, canonical-vs-twin routing, unknown-module,
the no-transitive-imports invariant) and the 3 config-level unit tests
(`src/config.rs`: `[lib]` parse, unknown-key reject, `resolve_with_path`) back
these.

| Test                                                         | Locks in                                                                                                                                                                      |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `embedded_std_import_resolves_without_filesystem`            | `import std.fifo` loads the compiler-embedded module (synthetic `std:fifo.mimz`) AND the entry stays `files[0]` (std appended last) — `sim`/`test` elaborate `files[0]`       |
| `tamil_twin_routes_to_twin_source`                           | `சேர்க்க நூலகம்.வரிசை` routes to the **pure-Tamil twin** source (`தொகுதி வரிசை`), not the English canonical                                                                   |
| `unknown_std_module_errors_with_available_list`              | `import std.nope` is **E1202** and the message lists the available stems (`fifo`, …)                                                                                          |
| `wrong_std_arity_errors`                                     | `import std.fifo.extra` (three segments) is rejected — a std import is exactly `std.<module>` (E1202)                                                                         |
| `plain_relative_import_still_works`                          | a non-std `import helper` still resolves file-relative — the std branch is no regression                                                                                      |
| `lib_std_override_wins_over_embedded`                        | `[lib] std = "<dir>"` makes `import std.fifo` load `<dir>/fifo.mimz` (a sentinel), not the embedded `Fifo`                                                                    |
| `lib_std_override_filename_matches_eject_for_twin_spellings` | with an ejected Tamil dir, both `import std.வரிசை` and `import std.varisai` resolve to `varisai.mimz` — the override filename keys on the resolved variant, not the raw alias |
| `eject_writes_english_modules`                               | `eject_to(dir, false, false)` writes all 5 English canonical modules; `fifo.mimz` contains `module Fifo`                                                                      |
| `eject_tamil_writes_twins`                                   | `eject_to(dir, true, false)` writes the pure-Tamil twins (`varisai.mimz` contains `தொகுதி வரிசை`)                                                                             |
| `eject_refuses_overwrite_without_force`                      | a second eject over existing files fails; `force = true` overwrites                                                                                                           |
| `eject_is_all_or_nothing_on_partial_conflict`                | one pre-existing target aborts the whole eject before any other file is written — no half-vendored directory                                                                  |

## Unit: morph (`crates/mimz-core/src/morph.rs`, 14 tests)

Error-language selection + Tamil case-suffix inflection (Phase 1.8, spec/04 section 5),
the W0001 mixed-flavor lint, and the structured-arg / English-fallback guards.

| Test                                                      | Locks in                                                                                                                                     |
| --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `majority_picks_the_dominant_keyword_flavor`              | all-English vs all-Tamil keyword files resolve to English / Tamil                                                                            |
| `majority_falls_back_to_english_with_no_keywords`         | a keyword-free token stream defaults to English                                                                                              |
| `majority_breaks_ties_toward_the_earliest_keyword_column` | a flavor tie resolves deterministically to the earliest keyword column                                                                       |
| `effective_lang_override_beats_majority`                  | `--lang` wins over the file majority; absence uses the majority                                                                              |
| `parse_lang_matches_translate_flavor`                     | `--lang` parsing reuses `translate::parse_flavor` (spellings never drift)                                                                    |
| `inflect_attaches_each_case_suffix`                       | each case attaches its spec suffix; Latin stems hyphenate, Tamil joins, English none                                                         |
| `inflect_of_an_empty_stem_is_empty_not_a_bare_suffix`     | inflecting an empty stem yields empty — never a dangling case suffix                                                                         |
| `suffix_table_has_every_case`                             | `lang/case_suffixes.toml` parses and defines all four cases (startup validation)                                                             |
| `localized_is_none_for_uncovered_codes_and_for_english`   | the catalog returns `None` for English and for codes it does not localize                                                                    |
| `fill_inflects_the_stub_template`                         | the template's `{name.dat}` slot renders the inflected identifier                                                                            |
| `arg_code_without_args_falls_back_to_english`             | a code whose template has `{expected}/{found}` but no args attached leaves a leftover `{`, so `localized_msg` returns `None` — the fail-safe |
| `fill_with_empty_name_leaves_no_stray_fragment`           | `fill` with an empty `name` renders cleanly — no orphaned bracket or suffix                                                                  |
| `flavor_mix_warns_only_when_tamil_meets_the_others`       | W0001 fires only when Tamil mixes with English/Tanglish (the SVO pair mixes freely)                                                          |
| `flavor_mix_warning_is_a_nonfatal_w0001`                  | the mixed-flavor diagnostic is a non-fatal W0001 warning, not an error                                                                       |

## Integration: morph (`tests/morph.rs`, 20 tests — run the real binary)

The end-to-end `--lang` path through `mimz check`/`compile`. The catalog is now
the native-authored one (33 of 36 codes, decision C3); these assert the
MECHANISM, the structured-arg interpolation, the W0001 mixed-flavor lint, and —
crucially — the **English-fallback invariant**: codes the catalog does not cover
(E0405) render byte-identically across every flavor.

| Test                                                 | Locks in                                                                                                                                                                          |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `majority_and_effective_lang_track_the_keywords`     | selection: majority + override, via the public lib API                                                                                                                            |
| `inflect_attaches_the_spec_case_suffixes`            | inflection: the four suffixes across Tamil / Tanglish / English                                                                                                                   |
| `covered_code_renders_tamil_with_the_inflected_name` | E0501 under `--lang ta` shows the localized Tamil line with `y-க்கு`                                                                                                              |
| `covered_code_auto_selects_tamil_from_the_file`      | a Tamil-keyword file with no `--lang` auto-renders E0501 in Tamil                                                                                                                 |
| `covered_code_stays_english_with_lang_en`            | `--lang en` keeps the original English wording                                                                                                                                    |
| `uncovered_code_is_identical_across_languages`       | **the fallback invariant** — E0405 is byte-identical under en / ta / tanglish                                                                                                     |
| `compile_also_localizes_diagnostics`                 | the localization path is shared — `compile --lang ta` shows Tamil E0501 too                                                                                                       |
| `unknown_lang_is_a_clean_error`                      | `--lang klingon` fails with a clear "unknown language" message                                                                                                                    |
| `e0502_renders_tamil`                                | an undriven output (E0502, a `{name}`-only template) localizes in Tamil                                                                                                           |
| `e0505_renders_tamil`                                | `=` on a reg (E0505) localizes under `--lang ta`                                                                                                                                  |
| `e0202_renders_tanglish_nameless`                    | a name-less template (E0202 const overflow) localizes with no `{name}` slot                                                                                                       |
| `e0401_interpolates_expected_and_found`              | E0401's `{expected}`/`{found}` widths interpolate; no `{token}` leaks                                                                                                             |
| `e0402_interpolates_op_lhs_rhs`                      | E0402's `{op}`/`{lhs}`/`{rhs}` (operator + both operand widths) interpolate                                                                                                       |
| `e0408_interpolates_first_and_second`                | E0408's `{first}`/`{second}` arm types interpolate (width-inferred position)                                                                                                      |
| `e0601_interpolates_type`                            | E0601's `{type}` scrutinee type interpolates on a non-exhaustive `match`                                                                                                          |
| `message_catalog_keys_are_real_checker_codes`        | every `[message.Exxxx]` key in `lang/messages.toml` is a real `ALL_CHECKER_CODES` code — a typo'd key (dead localization) fails naming it                                         |
| `message_catalog_placeholders_are_known_tokens`      | every active `{token}` in `lang/messages.toml` is one `morph::fill` fills — a typo'd placeholder / unsupplied arg would silently fall back to English forever; this fails instead |
| `mixing_tamil_with_english_warns_but_check_succeeds` | a Tamil+English file emits W0001 yet `check` still succeeds (non-fatal lint)                                                                                                      |
| `a_single_flavor_file_has_no_mix_warning`            | a clean single-flavor file does not warn                                                                                                                                          |
| `json_check_carries_the_warning_and_still_succeeds`  | `--json` includes the W0001 entry with `"severity":"warning"`, exit 0                                                                                                             |

## Integration: fmt (`tests/fmt.rs`, 9 tests — run the real binary)

`mimz fmt` — the in-place keyword-flavor normalizer (the lossless `translate`
token reskin, not the comment-dropping `--order` printer).

| Test                                              | Locks in                                                                                 |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `normalizes_to_majority_and_is_idempotent`        | a mixed file normalizes to its majority flavor; comments survive; re-run no-ops          |
| `to_flag_forces_the_target_flavor`                | `--to tamil` overrides the majority; comment preserved                                   |
| `strict_warns_and_fails_on_mixed_but_still_fixes` | `--strict` warns + exits non-zero on a mixed file, still writing the fix                 |
| `strict_is_clean_on_a_single_flavor_file`         | a single-flavor file passes `--strict` (no warning, exit 0)                              |
| `a_keyword_free_file_is_left_intact`              | a comment-only file (no keywords) normalizes to a no-op                                  |
| `a_non_lexing_file_is_a_clean_error`              | a lex error (e.g. `/`) is reported, exits non-zero, and does not clobber input           |
| `output_flag_leaves_the_input_untouched`          | `-o <dest>` writes the result elsewhere; the input is unchanged                          |
| `output_to_the_input_path_round_trips`            | `-o <input>` writes atomically to a temp file then renames — input is never half-written |
| `unknown_to_flavor_is_a_clean_error`              | `--to wibble` fails with a clear "unknown flavor" message, never a panic                 |

## Integration: CLI (`tests/cli.rs`, 6 tests — run the real binary)

The new `init`, `doctor`, `completions`, and `check --watch` subcommands.
See `docs/code/04-cli.md` for the full command reference.

| Test                                                | Locks in                                                                                      |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `init_scaffolds_a_project_that_passes_its_own_test` | `mimz init myproject` creates a documented `mimz.toml` + a counter module with a passing test |
| `init_refuses_to_clobber_a_non_empty_dir`           | re-running `mimz init myproject` on an existing dir fails with a clean message                |
| `doctor_reports_sections_and_pipeline_ok`           | `mimz doctor` prints version/edition, platform, and an in-memory compile smoke test           |
| `doctor_dev_adds_developer_section`                 | `--dev` adds the Rust/WASM/test toolchain section                                             |
| `env_is_an_alias_for_doctor`                        | `mimz env` produces identical output to `mimz doctor`                                         |
| `watch_starts_and_enters_watch_mode`                | `mimz check --watch` starts the watcher and shows the "watching N dir(s)" banner              |

## Unit: combinational evaluator (`crates/mimz-sim/src/sim/comb.rs`, 20 tests)

The Phase 1.5 simulator's combinational slice behind `mimz eval`.

| Test                                                       | Locks in                                                                          |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `adder_grows_losslessly`                                   | `+` grows `bits[W]` → `bits[W+1]`; 200+100 carries into the 9th bit (no wrap)     |
| `wrapping_add_keeps_width`                                 | `+%` keeps width and wraps (300 → 44 in `bits[8]`)                                |
| `comparator_if_and_compares`                               | `==`, `>`, and a value `if/else` evaluate together                                |
| `mux_match_selects`                                        | `match` on `bits[2]` picks the right arm                                          |
| `chained_comparison_window`                                | `lo <= value <= hi` (desugared) incl. the inclusive boundary                      |
| `rejects_sequential_logic`                                 | a module with `reg`/`on` is rejected with a clear message (out of the comb slice) |
| `reports_missing_input`                                    | a missing `--in` value names the input                                            |
| `replication_repeats_the_group`                            | `{2{a}}`/`{3{a}}` repeat the group (a=0b1010 → 0xAA / 0xAAA) (A1)                 |
| `dont_care_match_picks_the_masked_arm`                     | `0b1??`/`0b01?`/`_` priority decoder picks the right arm per input (A2)           |
| `shift_left_zero_amt`                                      | `a << 0` is identity                                                              |
| `shift_right_zero_amt`                                     | `a >> 0` is identity                                                              |
| `shift_left_max_width`                                     | `1 << 127` yields `2¹²⁷` (max valid shift)                                        |
| `shift_left_exceeding_width_is_zero`                       | `1 << 128`, `1 << 200`, `1 << u128::MAX` → 0 (regression for the `as u32` bug)    |
| `shift_right_exceeding_width_is_zero`                      | `2 >> 128`, `2 >> 200`, `2 >> u128::MAX` → 0                                      |
| `shift_left_bit_32_set_in_amt`                             | `1 << (1 << 32)` → 0 (the specific `as u32` truncation trigger)                   |
| `shift_right_bit_32_set_in_amt`                            | `(1 << 63) >> (1 << 32)` → 0                                                      |
| `eval_outputs_handles_a_wide_input`                        | an input wider than 128 bits evaluates through the wide (`bits`/`wide`) path      |
| `sim_fn_call_mac_basic`                                    | a user `fn` call (multiply-accumulate) evaluates inside `mimz eval`               |
| `sim_fn_call_mac_wrap_truncation`                          | the same call truncates exactly like the emitted Verilog would                    |
| `zero_length_array_param_index_is_a_clean_err_not_a_panic` | indexing a zero-length array parameter is an `Err`, never a panic                 |

## Unit: value model + fn-body interpreter (`crates/mimz-sim/src/sim/value/`, 34 tests)

The shared value model and expression evaluator behind BOTH `comb.rs` and
the kernel. A `Val` is a 2-state bit-vector carrying a width and a
signedness — small values stay on a `u128` fast path and only promote to
the multi-limb `wide` representation past 128 bits. This pocket also
covers `fn`-body statement evaluation: `fn` bodies are interpreted
directly (no elaborate-time lowering pass exists for them, unlike module
items and `on` blocks), so `loop`/`foreach` are lowered on the spot
inside the evaluator itself.

Split across `value/mod.rs` (the `Val` type + statement evaluation),
`value/binary.rs` (binary operators), `value/fn_eval.rs` (`fn` calls),
and `value/tests.rs`.

**Width and representation**

| Test                                                     | Locks in                                                  |
| -------------------------------------------------------- | --------------------------------------------------------- |
| `val_new_stays_on_the_small_fast_path`                   | a narrow value never allocates the wide representation    |
| `val_new_wide_auto_narrows_to_small_at_128_bits_or_less` | a wide value at ≤128 bits collapses back to the fast path |
| `val_new_wide_masks_to_the_declared_width`               | bits above the declared width are dropped, never kept     |
| `checked_width_accepts_up_to_the_shared_max_width`       | the width ceiling matches `mimz_core::width_rules`'s own  |
| `concat_can_exceed_128_bits`                             | `{a, b}` may cross the fast-path boundary                 |

**Wide (>128-bit) arithmetic**

| Test                                                    | Locks in                                              |
| ------------------------------------------------------- | ----------------------------------------------------- |
| `wide_unsigned_add_carries_past_128_bits`               | carry propagates across limbs                         |
| `wide_neg_of_a_512_bit_value`                           | two's-complement negation at 512 bits                 |
| `wide_bitand_of_two_512_bit_values`                     | bitwise ops are element-wise across limbs             |
| `wide_eq_compares_two_equal_512_bit_values`             | equality over multi-limb values                       |
| `wide_lt_compares_signed_512_bit_values`                | signed ordering over multi-limb values                |
| `wide_shl_crosses_a_limb_boundary_in_a_512_bit_context` | a shift that straddles two limbs                      |
| `wide_extend_builtin_widens_past_128_bits`              | `extend` into a wide target                           |
| `builtin_abs_wide_negative`                             | `abs` of a wide negative value                        |
| `builtin_trunc_wide_limb_count`                         | `trunc` drops whole limbs correctly                   |
| `pattern_matches_handles_wide_value_no_saturation`      | a `match` pattern over a wide value does not saturate |

**Operators and Verilog agreement**

| Test                                               | Locks in                                                                   |
| -------------------------------------------------- | -------------------------------------------------------------------------- |
| `shl_widens_to_context_like_verilog`               | `<<` takes the context width, matching Verilog                             |
| `shl_self_determined_preserves_left_operand_width` | …but in a self-determined position it keeps the left operand's width       |
| `shl_chain_stays_at_shared_context_width`          | a chain of shifts does not drift in width                                  |
| `shl_rejects_a_signed_shift_amount`                | a signed shift amount is an error (`S0221`), not a silent reinterpretation |
| `bitand_widens_a_narrower_literal_operand`         | a bare literal adapts to the sized operand                                 |
| `cmp_eq_signed_different_widths`                   | signed comparison across widths sign-extends first                         |
| `sub_of_two_signed_values_is_signed`               | signedness propagates through `-`                                          |
| `sub_of_two_unsigned_values_is_unsigned`           | …and unsignedness does too                                                 |

**Unknown-value (`x`) taint — extern modules in warn mode**

| Test                            | Locks in                                             |
| ------------------------------- | ---------------------------------------------------- |
| `known_vals_are_never_tainted`  | an ordinary value never carries the unknown flag     |
| `unknown_val_taints_binary_ops` | any binary op with an unknown operand yields unknown |
| `unknown_val_taints_unary_ops`  | …and so does any unary op                            |

**`fn` bodies**

| Test                                                         | Locks in                                                                                          |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `fn_call_arity_mismatch_is_err_not_panic`                    | calling a `fn` with the wrong argument count is a clean `Err`, not a panic                        |
| `fn_call_sign_extends_narrower_signed_arg_to_wider_param`    | BUG-7: a narrower signed argument sign-extends (not zero-masks) when bound to a wider `fn` param  |
| `fn_loop_with_return_finds_first_match_in_sim`               | a `loop` + `return` inside a `fn` body finds the first match when interpreted                     |
| `fn_loop_with_return_first_match_wins_on_duplicate_in_sim`   | on a duplicate match, `loop` + `return` returns the FIRST (lowest-index) match                    |
| `fn_loop_over_budget_errors_in_sim`                          | a `loop` past the unroll budget errors instead of hanging                                         |
| `fn_foreach_range_form_with_return_finds_first_match_in_sim` | `foreach i in 0..N` + `return` lowers via `ast::lower_foreach_fn` and finds the first match       |
| `fn_foreach_elements_form_with_return_finds_match_in_sim`    | elements-form `foreach v in vals` + `return v` on match propagates as an early return             |
| `fn_foreach_elements_form_no_match_falls_through_in_sim`     | elements-form `foreach` with no match falls through to the tail expression, not a spurious return |

## Unit: elaboration (`crates/mimz-sim/src/sim/elaborate/`, 24 tests)

Phase 1.5 steps B1 + C2–C4: flatten an AST module (and its instances) into a
`Design` (signals with folded widths, regs with folded reset + clock, comb
drivers, sequential processes), via the `elaborate_project` flattener and the
`Rw` elaborate-time rewriter (enum→index, `repeat` unroll, instance flattening,
array instances, bit-indexed drives). The event-driven kernel interprets a
`Design`.

Split into `mod.rs` (entry + `Design`), `module.rs` (one module's items),
`instance.rs` (instance flattening), `registry.rs` (module/bundle/enum
lookup), `rewrite.rs` (the `Rw` rewriter), `bundle.rs` (bundle field
expansion), and `tests.rs`.

| Test                                                                     | Locks in                                                                                                                                                     |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `elaborates_the_counter`                                                 | the canonical counter flattens correctly: one reg (`value`, reset 0, clock `clk`), the `count` comb driver, `clk`/`rst` recorded, one process                |
| `param_override_folds_widths`                                            | passing `WIDTH=4` folds the reg and output widths to 4                                                                                                       |
| `elaborates_a_combinational_module`                                      | a clockless module has empty regs/procs/clocks/resets, only comb drivers                                                                                     |
| `reg_takes_a_nonzero_folded_reset_value`                                 | `reg r: bits[8] = 5` folds the reset to 5 and binds the reg to its `on`-block clock                                                                          |
| `flattens_a_same_file_instance`                                          | C2: `Top`'s `let u = Add()` inlines the child's signals prefixed `u_*`; the `u.s` field-read resolves to the flattened `u_s` wire                            |
| `rejects_unknown_instance_module`                                        | C2: a `let` instance of a module that doesn't exist is a clean `S0105` error                                                                                 |
| `two_same_named_modules_flatten_their_own_instance`                      | cross-file name reuse: each instance flattens the module from ITS OWN file                                                                                   |
| `ambiguous_bare_module_reference_errors_instead_of_silently_picking_one` | a bare name matching two files is `S0102`, never an arbitrary pick                                                                                           |
| `qualified_instance_reference_resolves_via_a_real_import_path`           | `a.b.Mod()` resolves through the written `import`, not a guess                                                                                               |
| `unrolls_repeat_with_instance_array_and_bit_drives`                      | C3: `repeat` inlines one child per bit (`fa__<i>`); the per-bit `s[i] = …` drives assemble into a whole-signal Concat                                        |
| `elaborates_an_enum_signal_and_match`                                    | C4: an enum reg gets width `clog2(variants)`, its reset folds to the variant index, and a `match` over the enum elaborates (patterns → indices)              |
| `unrolls_foreach_range_form_same_as_repeat`                              | `foreach i in 0..2` elaborates identically to the equivalent `repeat i: 0..2` (pure sugar)                                                                   |
| `foreach_elements_form_substitutes_var_with_mem_index`                   | elements-form `foreach v in values` over a `mem` lowers to a `Repeat` substituting `v` with `values[idx]` throughout the body                                |
| `foreach_nested_inside_if_in_on_block_lowers_via_recursion`              | a `foreach` nested inside an `if` inside `on rise(clk)` still lowers — the seq-lowering pass recurses into `If`'s `then` body, not just top-level statements |
| `bundle_typed_instance_input_port_connection_flattens_per_field`         | BUG-15: a bundle-typed instance input expands to one connection per field                                                                                    |
| `bundle_typed_fn_call_argument_expands_to_one_arg_per_field`             | BUG-15: the same expansion for a bundle passed to a `fn`                                                                                                     |
| `sync_loop_timing_and_no_mid_run_retrigger`                              | a `sync loop` runs its full iteration count and ignores a `start` pulse mid-run                                                                              |
| `sync_loop_nested_in_const_if_elaborates_and_ticks`                      | a `sync loop` inside the WINNING `const if` branch elaborates and runs                                                                                       |
| `sync_loop_in_const_if_losing_branch_is_not_lowered`                     | …and one inside the losing branch generates no hardware at all                                                                                               |
| `recursive_instantiation_errors_not_overflows`                           | SEC-6: a self-instantiating module hits `MAX_INSTANCE_DEPTH` (`S0119`) instead of overflowing the stack                                                      |
| `extreme_repeat_bounds_error_not_overflow`                               | SEC-6: a `repeat` span past `i128::MAX` is an over-budget error (`checked_sub`), not an overflow panic                                                       |
| `an_out_of_range_bit_index_errors`                                       | SEC-6: a bit-index drive ≥ 128 errors before the `as u32` cast (no silent truncation)                                                                        |
| `a_flatten_name_collision_errors`                                        | SEC-6: a parent signal colliding with a flattened `inst_port` wire errors (`S0128`) instead of silently overwriting                                          |
| `an_i128_min_const_elaborates_without_overflow`                          | SEC-6 (SIM-5): a flattened child const evaluating to `i128::MIN` lowers via `unsigned_abs` instead of overflow-panicking the negation in `int_expr`          |

## Unit: kernel (`crates/mimz-sim/src/sim/kernel.rs`, 25 tests)

Phase 1.5 step B2: the event-driven, two-phase simulation kernel that interprets
a `Design` over clock cycles (regs init to reset; each rising edge settles
combinational signals, computes next reg values, then commits all at once).
Shares the value model + expression evaluator with `comb` via
`crates/mimz-sim/src/sim/value/`.

| Test                                                                  | Locks in                                                                                                  |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `counter_counts_and_resets`                                           | the counter counts 0→1→2→3 on rising edges; asserting `rst` forces it back to 0 (synchronous reset)       |
| `dual_edge_negedge_reg_captures_posedge_within_a_period`              | a `posedge` reg feeds a `negedge` reg; the rise→fall tick lets `b` see the new `a` same period (A3)       |
| `memory_write_then_read_round_trips_a_cell`                           | a `mem` cell reads init until written, then holds the clocked value; another cell still reads init (A4)   |
| `regs_init_to_their_reset_value`                                      | before any tick a reg holds its (non-zero) folded reset value                                             |
| `bit_indexed_register_write_sets_one_bit`                             | BUG-8: `shift[i] <- v` on a plain register sets that bit, leaving the rest untouched                      |
| `slice_indexed_register_write_sets_a_range`                           | BUG-8: `r[hi:lo] <- v` replaces that bit range, keeping bits outside it from the prior value              |
| `disjoint_bit_indexed_writes_in_one_on_block_combine`                 | BUG-8: two `reg[i] <- v` writes to disjoint bits of the same register in one `on` block both take effect  |
| `wraps_at_declared_width`                                             | `+%` on a `bits[2]` reg wraps 3→0 — width masking on the next value                                       |
| `two_phase_commit_swaps_registers`                                    | `a <- b; b <- a` SWAPS (non-blocking): each reads the OLD value, proving the two-phase commit             |
| `statement_if_picks_the_next_value`                                   | a statement-level `if` in the `on` block selects the reg's next value from the current state              |
| `snapshot_covers_every_signal`                                        | `snapshot()` lists leaves (clk/rst/inputs), regs, and combinational outputs — the VCD/trace seam          |
| `set_rejects_a_non_leaf`                                              | driving an output or an unknown name is a clean `S0239` error (only inputs/clocks/resets are drivable)    |
| `combinational_chain_propagates_in_order`                             | a multi-level `wire → wire → output` chain (plus a reg input) settles in dependency order each cycle (B3) |
| `combinational_cycle_is_reported`                                     | a pure comb loop (`a = b; b = a`) is caught at settle time and reports `S0238` (BUG-27), not spun on      |
| `on_block_loop_unrolls_at_runtime`                                    | `loop` inside an `on` block unrolls in the kernel, matching the emitter                                   |
| `on_block_loop_over_budget_errors_at_runtime`                         | …and the same budget cap applies at runtime (`S0227`)                                                     |
| `a_wide_register_resets_to_a_nonzero_literal_past_128_bits`           | a >128-bit reset literal survives into the register                                                       |
| `regs_init_to_a_wide_reset_value_and_wide_comparisons_still_work`     | …and wide comparisons against it behave                                                                   |
| `bitwise_not_of_a_wide_register_reset_to_zero_flips_every_bit`        | `~0` over a wide register sets every declared bit, no more                                                |
| `set_and_peek_round_trip_a_wide_value`                                | driving and reading back a >128-bit input is lossless                                                     |
| `bit_indexed_write_above_bit_127_on_a_wide_register_does_not_panic`   | BUG-13: a bit write past the fast-path boundary is handled, not panicked                                  |
| `slice_indexed_write_above_bit_127_on_a_wide_register_does_not_panic` | …and the slice form likewise                                                                              |
| `extern_instance_is_a_hard_error_in_strict_mode`                      | an `extern module` with no simulation model is `S0113` under `--extern-sim strict`                        |
| `extern_instance_output_is_unknown_tainted_in_warn_mode`              | …and in warn mode its outputs become unknown-tainted instead                                              |
| `extern_taint_survives_one_level_of_real_module_nesting`              | that taint propagates out through an enclosing real module                                                |

## Unit: sim runner / VCD / console trace (`crates/mimz-sim/src/sim/{run,vcd,trace}.rs`, 16 tests)

Phase 1.5 step B4/B5 (+ C1): the default stimulus + clocked timeline capture
(`run.rs::run`), the combinational `comb_run` (one settled frame per input
vector), the hand-written 2-state VCD writer (`vcd.rs`), and the console trace
renderer (`trace.rs`) — all over one per-cycle snapshot from the kernel.

| Test (module)                                      | Locks in                                                                            |
| -------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `counter_timeline_counts_after_reset` (run)        | the default stimulus resets cycle 0 then counts; the clock renders as a square wave |
| `inputs_are_held_for_the_run` (run)                | `--in` values hold across the whole run (`r +% x` accumulates)                      |
| `a_clockless_module_is_rejected` (run)             | the CLOCKED `run` rejects a clockless module (callers route it to `comb_run`)       |
| `an_unknown_input_is_rejected` (run)               | an unknown `--in` name is a clean error                                             |
| `comb_run_settles_one_frame_per_vector` (run)      | a combinational design settles its outputs for one input vector (lossless add)      |
| `comb_run_sweeps_a_frame_per_vector` (run)         | N input vectors → N frames, one per settle, on the clocked period                   |
| `comb_run_with_no_vectors_is_one_zero_frame` (run) | no vectors → a single all-zero-input frame                                          |
| `comb_run_rejects_a_clocked_design` (run)          | `comb_run` refuses a clocked/registered design                                      |
| `signed_lossless_add_sign_extends` (run)           | C1 regression: lossless signed `+` sign-extends a negative operand (`-2+7=5`)       |
| `header_scope_and_vars_present` (vcd)              | the VCD has `$timescale`/`$scope`/`$var`/`$enddefinitions`                          |
| `has_initial_dump_and_timestamps` (vcd)            | `$dumpvars` + `#<time>` blocks + a multi-bit `b…` vector line                       |
| `id_codes_are_unique` (vcd)                        | the base-94 signal id codes never collide                                           |
| `dumps_a_wide_signal_as_a_binary_vector` (vcd)     | a >128-bit signal dumps as a full binary vector, not a truncated one                |
| `table_has_a_row_per_cycle` (trace)                | `--trace` renders one table row per cycle with the right count                      |
| `changes_style_omits_unchanged_frames` (trace)     | `--trace=changes` only prints when a watched signal changes (`$monitor`-style)      |
| `table_renders_a_wide_signal_in_decimal` (trace)   | a >128-bit signal prints as a decimal number, not limbs                             |

## Unit: playground runner (`crates/mimz-sim/src/runner.rs`, 13 tests)

The filesystem-free command engine behind the browser playground and the
WASM binding: `run_command(source, command, argv)` runs `check`/`compile`/
`eval`/`sim`/`test`/`ports` against a SOURCE STRING and returns the text a
CLI user would see. The CLI's own `--in`/`--param`/`--sweep` parsers live
here too (single source; `src/commands/helpers.rs` re-exports them).

| Test                                                    | Locks in                                                                      |
| ------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `check_reports_ok_and_errors`                           | `check` returns the same ok/error text as the CLI                             |
| `eval_runs_a_combinational_module`                      | `eval` settles a clockless design from a source string                        |
| `sim_traces_a_clocked_module`                           | `sim` produces the console trace                                              |
| `sim_vcd_emits_a_vcd_document`                          | `sim --vcd` returns a VCD instead of a trace (the waveform viewer feed)       |
| `sim_steps_drives_explicit_vectors`                     | `--steps "a=3,b=5;a=7,b=1"` drives one frame per vector                       |
| `sim_steps_is_rejected_for_a_clocked_design`            | …and is refused for a clocked design                                          |
| `ports_describes_a_combinational_interface`             | `ports` emits the module interface as JSON so the browser can build inputs    |
| `ports_reports_a_clocked_design`                        | …and flags `clocked` when there is a clock                                    |
| `sweep_vectors_allows_a_normal_product`                 | the `--sweep` cartesian product works for ordinary sizes                      |
| `sweep_vectors_rejects_an_oversized_product`            | SEC: an oversized product is rejected before allocating (`MAX_SWEEP_VECTORS`) |
| `parse_bits_stays_on_the_small_path_for_a_narrow_width` | a narrow `--in` literal never allocates the wide representation               |
| `parse_bits_produces_a_wide_value_for_a_wide_width`     | a wide `--in` literal parses into the multi-limb value                        |
| `parse_bits_rejects_an_empty_literal_at_a_wide_width`   | an empty literal is an error, not a zero                                      |

## Integration: sim (`tests/sim.rs`, 17 tests — run the real binary + lib in-process)

End-to-end `mimz sim` over a counter (clocked) and an adder (combinational): the
stimulus, the VCD, the console trace, the `--sweep`; plus the B8 kernel perf
baseline and the golden VCD byte-lock (both run the lib in-process).

| Test                                                                    | Locks in                                                                                                |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `trace_table_shows_a_row_per_cycle`                                     | `--trace` prints the per-cycle table (header + separator + N rows)                                      |
| `cycles_over_the_limit_is_rejected_by_the_cli`                          | SEC: `--cycles` past `MAX_SIM_CYCLES` (1_000_000) is rejected at clap parse time — no unbounded loop    |
| `changes_trace_is_monitor_style`                                        | `--trace=changes` prints `$monitor`-style lines (reaches `count=3`)                                     |
| `writes_a_gtkwave_vcd`                                                  | `-o` writes a VCD with `$timescale`/`$enddefinitions`/`$dumpvars`/`count`                               |
| `signals_flag_limits_the_trace`                                         | `--signals count` shows only `count`, excluding `value`                                                 |
| `a_combinational_module_settles_one_frame`                              | C1: a clockless module simulates — `--in a=200,b=100` → one settled frame, `sum=300`                    |
| `sweep_emits_a_frame_per_combination`                                   | C1: `--sweep a=1\|2\|3` (held `--in b=10`) → 3 frames, sums 11/12/13                                    |
| `a_combinational_module_writes_a_vcd`                                   | C1: a clockless module writes a VCD with the settled output (`sum=12`)                                  |
| `the_counter_kernel_clears_the_perf_baseline`                           | the kernel sustains ≥1M cycle-events/sec on the counter in release (B8; debug uses a low sanity floor)  |
| `the_counter_vcd_matches_the_golden_byte_for_byte`                      | the VCD writer's exact bytes match `tests/golden/counter.vcd` (B8; `MIMZ_UPDATE_GOLDENS=1` regenerates) |
| `a_wide_const_folds_through_the_full_pipeline_and_matches_a_wide_reset` | a >128-bit const survives lex→check→elaborate→run and matches the register it resets                    |
| `sim_bundle_wire`                                                       | a bundle-typed wire simulates through the flattened per-field signals                                   |
| `sim_enum_tag_only_match_works`                                         | a plain (payload-free) enum `match` simulates                                                           |
| `sim_tagged_enum_payload_extracted`                                     | a tagged-union arm binds its payload fields correctly                                                   |
| `sim_tagged_enum_write_arm_payload_extracted`                           | …including when the arm writes a register                                                               |
| `sim_enum_construct_round_trips_through_match`                          | `Enum.Variant(x)` constructed then matched returns the same payload                                     |
| `sim_enum_construct_literal_arg_is_sized_to_field_width_not_its_own`    | a bare literal argument takes the FIELD's width, not its own minimal one                                |

## Unit: test harness (`crates/mimz-sim/src/sim/harness/`, 25 tests)

Phase 1.5 step B6: the `test`-block runner behind `mimz test`. Runs each block
(`drive`/`tick`/`expect`/`if`) on the kernel and reports pass/fail. Also owns
the `sim { speed … bind … }` block: peripheral binding, frame pacing, and the
`EmulationHost` call-out used by `mimz test --emulate` (see
[`14-hardware-emulation.md`](14-hardware-emulation.md)).

**Running a test block**

| Test                                                   | Locks in                                                                        |
| ------------------------------------------------------ | ------------------------------------------------------------------------------- |
| `a_passing_test_counts_its_checks`                     | drive/tick/expect runs in order; the `expect` count is reported                 |
| `a_failing_expect_halts_with_a_teaching_message`       | a false `expect` halts the test and shows the expression + each operand's value |
| `drive_then_tick_feeds_an_input`                       | a driven input is held and accumulates across ticks                             |
| `a_test_if_branches_on_state`                          | `if`/`else` takes the live-state branch; the other branch never runs            |
| `an_unknown_clock_is_an_error`                         | `tick(<not-a-clock>)` is a setup error (`S0301`), not a test failure            |
| `the_timeline_has_a_frame_per_tick`                    | one trace frame per tick (+ the initial frame); default scope = interface+state |
| `trace_false_skips_every_capture`                      | with tracing off, no frames are captured at all (the fast path)                 |
| `show_renders_a_wide_unsigned_value_in_decimal`        | a >128-bit value prints as a decimal number in the report                       |
| `show_renders_a_wide_negative_signed_value_in_decimal` | …including a negative signed one                                                |

**`sim` blocks and peripheral binding**

| Test                                                        | Locks in                                                           |
| ----------------------------------------------------------- | ------------------------------------------------------------------ |
| `has_sim_block_only_true_when_a_sim_block_is_present`       | the `sim`-block detector does not fire on ordinary tests           |
| `sim_block_with_unknown_peripheral_errors`                  | an unknown peripheral kind is `S0401`                              |
| `sim_block_with_unknown_port_errors`                        | binding a port that does not exist is `S0403`                      |
| `sim_block_binding_an_input_to_an_output_peripheral_errors` | direction mismatch is `S0402`…                                     |
| `sim_block_binding_an_output_to_an_input_peripheral_errors` | …in both directions                                                |
| `sim_block_with_speaker_bound_runs_fine_without_emulate`    | a bound peripheral is inert without `--emulate` — tests still pass |
| `live_true_without_a_dashboard_still_passes`                | live mode with no dashboard attached degrades gracefully           |
| `cycles_per_frame_floors_to_one`                            | the frame pacer never computes zero cycles per frame               |
| `batch_sizes_splits_evenly`                                 | cycle batching splits without dropping or duplicating cycles       |
| `tick_without_sim_block_is_unaffected`                      | a plain test's timing is untouched by the `sim`-block machinery    |

**Clock-domain crossing and `??` in a test block**

| Test                                                     | Locks in                                                                |
| -------------------------------------------------------- | ----------------------------------------------------------------------- |
| `sync_double_flop_settles_after_two_dst_clock_cycles`    | `sync.double_flop` takes exactly two destination-clock cycles to settle |
| `sync_pulse_produces_a_one_cycle_dst_pulse_after_toggle` | `sync.pulse` emits a single destination-clock pulse per source toggle   |
| `qq_unwrap_form_evaluates_in_a_test_block`               | `a ?? fallback` evaluates at simulation time as the emitter would       |
| `qq_or_mux_form_evaluates_via_drive`                     | the OR-mux form through a drive                                         |
| `qq_or_mux_form_evaluates_at_wire_init`                  | …and at a wire initializer                                              |
| `qq_or_mux_chain_evaluates_correctly`                    | a chained `a ?? b ?? c` picks the first valid one                       |

## Integration: sim runtime errors (`crates/mimz-sim/tests/sim_errors.rs`, 79 tests)

The contract test for the `S0xxx` runtime catalog
([`13-tooling.md`](13-tooling.md#s0xxx--runtime-diagnostic-codes-r2-docsauditreview-2026-07-17md)).
**One test per live code, plus one completeness guard** —
`every_sim_code_has_a_fixture_above` fails if `ALL_SIM_CODES` gains an
entry with no firing fixture, so a new runtime code cannot ship uncovered.

Each test is named for the code it fires (`s0102_ambiguous_bare_reference`,
`s0238_combinational_cycle_fires_with_its_own_code`, …) and asserts BOTH
that the operation fails and that the failure carries exactly that code —
a code silently downgraded at a trait boundary (BUG-27) fails the test.

Unlike `tests/errors.rs`, these call straight into `mimz-sim`'s public API
rather than shelling out to the binary: most `S0xxx` conditions are ALSO
rejected by the checker, so a fixture routed through the real CLI would
stop at the checker gate and never reach the runtime code it exists to
exercise. That trade-off is recorded in the test file's own module doc.

| Group                                | Tests | Covers                                                                    |
| ------------------------------------ | ----: | ------------------------------------------------------------------------- |
| `s0102`–`s0136`                      |    25 | elaboration and wiring: reference resolution, ports, `repeat`, bit drives |
| `s0137`–`s0139`                      |     3 | in-memory `import` resolution (the playground's single-source path)       |
| `s0201`–`s0229`                      |    28 | expression evaluation: widths, indexes, `fn` calls, builtins              |
| `s0230`–`s0239`                      |    10 | the combinational-only evaluator (`mimz eval`) and `Sim::set`             |
| `s0301`–`s0305`                      |     5 | test-harness control flow (`tick`, `sim { speed … }`)                     |
| `s0401`–`s0404`                      |     4 | peripheral bind errors                                                    |
| `every_sim_code_has_a_fixture_above` |     1 | the completeness guard itself                                             |

## Integration: test (`tests/test_run.rs`, 7 tests — run the real binary)

End-to-end `mimz test`: exit codes, the teaching message, `--filter`, `--trace`,
the cycle-limit guard, and the thamizh-order test header (B7).

| Test                                                        | Locks in                                                                                                                         |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `a_passing_test_exits_zero`                                 | a passing block prints `ok` + the summary and exits 0                                                                            |
| `a_tick_count_over_the_cycle_limit_errors_fast_not_hangs`   | SEC: `tick(clk, n)` past `MAX_SIM_CYCLES` (1_000_000) fails fast with a clean error — no untrusted-input frame-push DoS          |
| `a_failing_expect_exits_nonzero_with_a_teaching_message`    | a failing block prints `FAIL` + the expression/operands and exits 1                                                              |
| `the_filter_selects_tests_by_name`                          | `--filter` runs only the matching test (skips the failing other one)                                                             |
| `trace_shows_a_per_cycle_table`                             | `--trace` prints the per-cycle table for a test                                                                                  |
| `a_file_with_no_tests_is_reported`                          | a file with no `test` blocks reports cleanly and exits 0                                                                         |
| `a_thamizh_order_test_header_runs_like_its_code_order_twin` | a fully thamizh-order, all-tanglish program (`yetram(clk) pothu` + `M(args) kaaga "…" sodhanai`) runs and passes (the B7 oracle) |

## Integration: eval (`tests/eval.rs`, 15 tests — run the real binary)

End-to-end `mimz eval` over corpus examples — proves the lib evaluator AND the
`--in`/`--module` plumbing. The security cases matter because the `eval` path
skips the checker, so `comb.rs` is the only overflow guard (audit SEC-2).

| Test                                                                            | Locks in                                                                           |
| ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `adder_carries`                                                                 | `mimz eval adder --in a=200,b=100` prints `sum = 300`                              |
| `mux4_selects_with_hex_and_binary_inputs`                                       | `--in sel=0b10,...` parses bases; selects the right input                          |
| `comparator_reports_all_three_outputs`                                          | all three outputs print with correct values                                        |
| `window_chained_comparison_boundaries`                                          | inclusive boundary in / below out                                                  |
| `arithmetic_builtins_compute_min_max_abs_and_negated_reductions`                | `min`/`max`/`abs`/`nand`/`nor`/`xnor` evaluate correctly                           |
| `fn_call_guard_clause_return_short_circuits`                                    | an early `return` in a `fn` guard clause wins                                      |
| `fn_call_guard_clause_falls_through_to_tail`                                    | …and falls through to the tail expression when it does not fire                    |
| `fn_call_with_array_literal_argument_indexes_by_constant`                       | an array literal argument indexes at a constant position                           |
| `fn_call_with_array_argument_indexes_by_runtime_value`                          | …and at a runtime-computed position                                                |
| `fn_call_with_array_argument_out_of_range_runtime_index_clamps_to_last_element` | an out-of-range RUNTIME index clamps (matching the emitted mux), it does not error |
| `multi_module_file_needs_module_flag`                                           | a 2-module file asks for `--module`, then accepts it                               |
| `instances_are_rejected_clearly`                                                | a file with sub-module instances is rejected with a clear message                  |
| `oversized_shift_const_does_not_panic`                                          | `a[1 << 200]` → clean overflow error, no panic/wrap (debug+release)                |
| `overflowing_multiply_const_does_not_panic`                                     | a const product past i128::MAX → overflow error, not a panic                       |
| `out_of_range_index_is_rejected_cleanly`                                        | a literal index past the width → clean error, not a truncating cast                |

## Fuzzing: `fuzz/fuzz_targets/` (CI-only, not `cargo test` units)

Four `cargo-fuzz` harnesses over the untrusted-input path, asserting the audit's
core guarantee (any byte string yields a value/Verilog or a clean `Diag`/`Err`,
never a panic / abort / hang):

- `lex_parse_eval` — NFC → `lex` → `parse` → `sim::comb::eval_outputs`, run twice
  (empty inputs for the const path, then AST-derived per-port values for the
  runtime datapath). After the random pass, 8 fixed edge-case evaluation passes
  (0, 1, u128::MAX, 1<<32, 1<<63, 1<<127, (1<<126)-1, (1<<64)-1 as all-port
  values) ensure truncation-prone boundaries are always exercised regardless of
  the random byte stream.
- `lex_parse_compile` — NFC → `lex` → `parse` → `checker::check` →
  `transliterate` → `Project::from_files` → `emit` (the Verilog backend).
- `pretty_roundtrip` — NFC → `lex` → `parse` → `pretty::pretty_print` → re-`lex`
  → re-`parse` (the printed source MUST re-parse), and for an emittable program
  the re-parsed AST must lower to byte-identical Verilog. Exercises the
  `translate --order` printer on arbitrary input (the unit suite only covers the
  fixed example corpus).
- `translate_roundtrip` — NFC → `lex` → `parse` → `translate` (keyword reskin,
  `--romanize-names`, and name-map restore): every reskin/romanize output must
  re-lex, and `romanize → restore` must be token-equivalent to the plain reskin.
  Added 2026-06-15 after a deterministic stress audit found the numeric-literal
  abutment bug (`42தொகுதி`, fixed by the `push_guarded` boundary guard).

**Not** part of the test count above: they need a nightly toolchain + libFuzzer
(Linux/macOS), live in a standalone `fuzz/` crate the root gate never builds, and
run as the CI `fuzz` job (60 s smoke per target on push/PR, corpus seeded from
`examples/`) plus a weekly `fuzz-nightly` job (10 min per target). Run locally
under WSL2/Linux with `cargo +nightly fuzz run <target>`. See
[`../audit/hardening.md`](../audit/hardening.md) "Ongoing assurance".

## Integration: grammar engine (`tests/grammar.rs`, 16 tests — run the real binary)

The `syntax thamizh` word-order profile (spec/04, Phase 1.8). Oracle = the
profile-blind backend: a thamizh-order file and its code-order twin must emit
byte-identical Verilog, so equal Verilog proves the same AST. Fixtures live in
`tests/fixtures/grammar/` (not `examples/`, which stays byte-identical
four-flavor per R9).

| Test                                                  | Locks in                                                                                               |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `thamizh_order_counter_matches_code_order_twin`       | Tanglish `rise(clk) on { }` → same Verilog as code-order twin                                          |
| `thamizh_order_tamil_counter_matches_code_order_twin` | pure Tamil script + SOV order → same Verilog as the Tamil twin                                         |
| `thamizh_order_agrees_with_english_golden`            | profile and keyword skin are fully orthogonal                                                          |
| `thamizh_order_blinker_matches_code_order_twin`       | seq conditional `<cond> enil { } illaiyenil { }` → same Verilog                                        |
| `thamizh_order_blinker_tamil_matches_code_order_twin` | the conditional flip in pure Tamil script → same Verilog                                               |
| `thamizh_order_blinker_agrees_with_english_golden`    | conditional flip is invisible to the backend (English golden)                                          |
| `thamizh_order_comparator_matches_code_order_twin`    | if-expression `c enil { } illaiyenil { }` → same Verilog                                               |
| `thamizh_order_match_matches_code_order_twin`         | match `<expr> thernthedu { }` → same Verilog (self-contained pair)                                     |
| `traffic_light_tamil_thamizh_matches_code_order_twin` | Tamil thamizh-order FSM (all four flips at once) → same Verilog; the committed `pretty`-built artifact |
| `unknown_syntax_profile_is_an_error`                  | `syntax wibble` fails to compile with E1112                                                            |
| `flipped_on_block_is_rejected_in_code_order`          | the clocked-block flip is gated on the profile                                                         |
| `flipped_conditional_is_rejected_in_code_order`       | `<cond> enil { }` rejected without the directive                                                       |
| `flipped_if_expr_is_rejected_in_code_order`           | `a > b enil { } illaiyenil { }` rejected without the directive                                         |
| `flipped_match_is_rejected_in_code_order`             | `op thernthedu { }` rejected without the directive                                                     |
| `code_order_if_is_rejected_in_thamizh`                | leading `enil` (code order) in a thamizh file errors — symmetric profile boundary                      |
| `deeply_nested_thamizh_else_if_errors_not_overflows`  | deep thamizh `illaiyenil … enil` chain → clean E1113, no stack overflow (SEC-1 guard on the flip path) |

## Unit: source normalization (`crates/mimz-core/src/lib.rs`, 1 test)

| Test                                      | Locks in                                                                                                                                                                          |
| ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nfc_normalize_composes_decomposed_forms` | every source string is NFC-normalized on the way in, so `e`+◌́ and `é` are ONE identifier — the precondition every span, keyword lookup, and Tamil comparison downstream relies on |

## Unit: AST lowering passes (`crates/mimz-core/src/ast/`, 21 tests)

Four sugar constructs never reach the emitter or the simulator as
themselves — a shared lowering function rewrites each into primitives that
both back ends already understand. These tests pin the SHAPE of that
rewrite, which is what makes "the emitter and the simulator agree" cheap
(there is only one implementation to agree with).

### ast/foreach_lower.rs (10 tests)

`foreach` has two forms: the RANGE form (`foreach i in 0..8`) is pure
sugar for `repeat`, and the ELEMENTS form (`foreach v in values`) also
substitutes the loop variable with `values[i]` throughout the body.

| Test                                                               | Locks in                                                                             |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| `range_form_lowers_to_repeat_unchanged`                            | the range form is byte-for-byte the `repeat` it desugars to                          |
| `elements_form_resolves_array_port_length`                         | the elements form reads its iteration count from the array's declared length         |
| `elements_form_on_undeclared_name_returns_none`                    | an unresolvable source returns `None` (the checker's E0417 already failed the build) |
| `seq_elements_form_substitutes_var_with_index_expr`                | inside an `on` block, `v` is replaced by `values[i]` everywhere it appears           |
| `fn_elements_form_resolves_via_own_param_and_binds_with_let`       | inside a `fn`, the array parameter supplies the length and the body binds via `let`  |
| `fn_elements_form_on_undeclared_param_returns_none`                | …and an unknown parameter returns `None` rather than guessing                        |
| `loop_var_shadowing_outer_foreach_var_is_not_substituted`          | an inner `loop` reusing the name SHADOWS it — no substitution leaks in               |
| `nested_repeat_var_shadowing_outer_foreach_var_is_not_substituted` | same for a nested `repeat`                                                           |
| `nested_sync_loop_body_substitutes_outer_foreach_var`              | but a nested `sync loop` (different variable) still gets the outer substitution      |
| `subst_expr_match_arm_binding_shadows_target`                      | a `match` arm binding of the same name shadows too — scoping is respected            |

### ast/sync_loop_lower.rs (3 tests)

| Test                                                        | Locks in                                                                                                                            |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `lower_produces_twelve_items_in_order`                      | one `sync loop` expands to exactly twelve primitive items (index reg, `start`/`done` handshake, ports, `on` block) in a fixed order |
| `counter_width_is_clog2_hi_not_clog2_range_when_lo_nonzero` | the index register is `clog2(hi)` wide, not `clog2(hi - lo)` — the counter counts to `hi`, so a nonzero `lo` does not shrink it     |
| `rename_expr_match_arm_binding_shadows_accumulator_name`    | a `match` arm binding named like the accumulator shadows it instead of being rewritten                                              |

### ast/sync_prim_lower.rs (4 tests)

`sync.double_flop` and `sync.pulse` are CDC (clock-domain-crossing)
primitives. They lower to ordinary registers with hidden names derived
from the call's own target, so the emitted Verilog has no special
construct at all.

| Test                                                                       | Locks in                                                                |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `double_flop_call_lowers_to_one_hidden_reg_and_a_rewritten_assign`         | the two-flop synchronizer is one hidden reg plus a rewritten assignment |
| `pulse_call_lowers_to_four_hidden_regs_two_on_blocks_and_a_rewritten_wire` | the pulse synchronizer is four regs across BOTH clock domains           |
| `two_double_flop_calls_in_the_same_on_block_both_get_lowered`              | two calls in one block both lower (no first-one-wins bug)               |
| `two_sync_prim_calls_in_one_module_get_distinct_hidden_names`              | the derived hidden names never collide                                  |

### ast/mod.rs (4 tests)

Constructor smoke tests — cheap guards that a node type still builds after
a field is added or reordered.

| Test                          | Locks in               |
| ----------------------------- | ---------------------- |
| `array_type_constructs`       | `Type::Array`          |
| `bundle_decl_node_constructs` | `BundleDecl`           |
| `func_decl_node_constructs`   | `FuncDecl`             |
| `sync_loop_node_constructs`   | `ModuleItem::SyncLoop` |

## Unit: checker internals (`consteval` 6, `drivers` 2, `names` 3 tests)

Small test pockets living inside individual checker passes, separate from
the by-error-code `checker/tests/` tables above.

### checker/consteval.rs (6 tests)

| Test                                                    | Locks in                                                                |
| ------------------------------------------------------- | ----------------------------------------------------------------------- |
| `clog2_bits_matches_spec_table`                         | `clog2` agrees with the table in the spec, value for value              |
| `small_arithmetic_still_works_exactly_as_before`        | ordinary constant folding is unchanged by the wide-integer work         |
| `a_literal_past_the_old_i128_ceiling_folds_cleanly`     | a literal past 128 bits folds instead of overflowing                    |
| `addition_past_128_bits_folds_to_a_wide_constval`       | …and arithmetic promotes to the wide `ConstVal` representation          |
| `negation_round_trips_through_shrink`                   | negating and re-shrinking a wide value returns the same number (BUG-13) |
| `a_constant_exceeding_max_width_is_a_clean_e0202_error` | past the ceiling it is `E0202`, never a wrap or a panic                 |

### checker/drivers.rs (2 tests)

| Test                                                            | Locks in                                                                   |
| --------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `separate_on_block_writing_the_same_name_is_still_multi_driver` | splitting a register across two `on` blocks is still E0503                 |
| `sync_loop_body_is_one_driver_block`                            | a `sync loop`'s generated body counts as ONE driver, not one per iteration |

### checker/names/tests.rs (3 tests)

The lowering passes generate hidden signal names. These prove a user name
that collides with one is caught as an ordinary duplicate (`E0003`) rather
than silently overwritten.

| Test                                                 | Locks in                            |
| ---------------------------------------------------- | ----------------------------------- |
| `sync_loop_generated_name_collision_is_e0003`        | for `sync loop`'s generated names   |
| `sync_double_flop_generated_name_collision_is_e0003` | for `sync.double_flop`'s hidden reg |
| `sync_pulse_generated_name_collision_is_e0003`       | for `sync.pulse`'s hidden regs      |

## Unit: wide integers and width rules (`bits` 17, `wide` 18, `width_rules` 15 tests)

Min-Mozhi allows bit-vectors wider than a machine word, so values live on
two representations: a `u128` fast path for the common case, and a
multi-limb `wide` representation past it. `bits.rs` owns the boundary
between them, `wide.rs` owns the multi-limb arithmetic, and
`width_rules.rs` owns the ONE table both the checker and the simulator
consult for "how wide is the result of this operator?".

### bits.rs (17 tests) — the small/wide boundary

| Test                                                                   | Locks in                                                    |
| ---------------------------------------------------------------------- | ----------------------------------------------------------- |
| `from_u128_impl_matches_bits_small`                                    | the fast-path constructor and the general one agree         |
| `from_limbs_auto_narrows_at_128_bits_or_less`                          | a wide value that fits collapses back to the fast path      |
| `from_limbs_stays_wide_past_128_bits`                                  | …and stays wide when it does not                            |
| `to_limbs_promotes_a_small_value`                                      | promotion in the other direction is lossless                |
| `retag_trims_a_padded_wide_vector_to_the_new_widths_limb_count`        | re-tagging a width drops the now-unused limbs               |
| `mask_of_128_or_more_is_all_ones`                                      | the width mask saturates instead of shifting out of range   |
| `natural_width_of_zero_is_one`                                         | zero is one bit wide, not zero                              |
| `natural_width_of_a_small_value_is_tight`                              | a value's natural width is minimal                          |
| `natural_width_of_a_wide_value_scans_limbs`                            | …including across limbs                                     |
| `top_bit_set_reads_the_correct_position`                               | sign detection reads the declared top bit, not the limb's   |
| `leading_ones_counts_from_the_top_bit_down`                            | leading-ones counting is width-relative                     |
| `leading_ones_of_all_ones_is_the_full_width`                           | …and saturates correctly                                    |
| `shrink_of_a_nonnegative_value_finds_the_tight_unsigned_width`         | shrinking a positive value finds the minimal unsigned width |
| `shrink_of_negative_one_round_trips`                                   | shrinking `-1` and re-expanding gives `-1`                  |
| `shrink_of_negative_four_reproduces_the_same_value_at_a_smaller_width` | …and the same for other negatives                           |
| `shrink_of_zero_is_never_reported_negative`                            | zero never comes back signed (BUG-13's original symptom)    |
| `bits_to_decimal_string_renders_a_small_negative_value`                | decimal rendering handles the signed small path             |

### wide.rs (18 tests) — multi-limb arithmetic

| Test                                                              | Locks in                          |
| ----------------------------------------------------------------- | --------------------------------- |
| `from_u128_round_trips_through_bit_at`                            | bit addressing across limbs       |
| `add_carries_across_a_limb_boundary`                              | addition carry                    |
| `sub_borrows_across_a_limb_boundary`                              | subtraction borrow                |
| `mul_of_two_wide_values_carries_correctly`                        | multiplication carry              |
| `neg_of_one_is_all_ones`                                          | two's-complement negation         |
| `shl_crosses_a_limb_boundary`                                     | left shift across limbs           |
| `shl_masks_bits_that_overflow_result_width`                       | …and masks what falls off the top |
| `shr_crosses_a_limb_boundary`                                     | right shift across limbs          |
| `bitwise_ops_are_elementwise`                                     | `&`/`\|`/`^` are per-limb         |
| `is_zero_and_count_ones`                                          | population count and zero test    |
| `cmp_unsigned_orders_by_magnitude`                                | unsigned ordering                 |
| `cmp_signed_a_negative_value_is_less_than_a_positive_one`         | signed ordering                   |
| `extend_zero_fills_an_unsigned_value`                             | zero extension                    |
| `extend_sign_fills_a_negative_signed_value`                       | sign extension                    |
| `to_binary_string_has_no_leading_zeros_except_for_the_value_zero` | binary rendering                  |
| `to_decimal_string_renders_zero`                                  | decimal rendering of zero         |
| `to_decimal_string_matches_a_known_large_unsigned_value`          | …of a large unsigned value        |
| `to_decimal_string_renders_a_negative_signed_value`               | …and of a negative one            |

### width_rules.rs (15 tests) — the shared operator table

| Test                                                      | Locks in                                                               |
| --------------------------------------------------------- | ---------------------------------------------------------------------- |
| `lossless_result_add_grows_by_one_bit`                    | `+` grows by one bit                                                   |
| `lossless_result_mul_sums_widths`                         | `*` sums the operand widths                                            |
| `lossless_result_preserves_signed_when_both_operands_are` | signedness survives when both sides agree                              |
| `lossless_result_rejects_mixed_signedness`                | …and mixing is rejected, never silently coerced                        |
| `matched_result_returns_the_shared_kind`                  | the width-matching family (`+%`, bitwise, comparisons) keeps the shape |
| `matched_result_rejects_different_widths`                 | …and rejects a width mismatch                                          |
| `matched_result_rejects_different_signedness`             | …and a signedness mismatch                                             |
| `shift_result_preserves_lhs_kind`                         | a shift takes the left operand's width                                 |
| `shift_result_preserves_signed_lhs`                       | …and its signedness                                                    |
| `shift_result_rejects_signed_amount`                      | but a signed shift AMOUNT is rejected                                  |
| `slice_result_single_bit`                                 | `x[i]` is one bit                                                      |
| `slice_result_computes_width_and_is_always_unsigned`      | `x[hi:lo]` is `hi-lo+1` bits and always unsigned                       |
| `slice_result_rejects_out_of_range_hi`                    | an out-of-range bound is rejected                                      |
| `slice_result_rejects_reversed_bounds`                    | …and so are reversed bounds                                            |
| `max_width_matches_the_checkers_own_ceiling`              | the module's ceiling equals the checker's, so neither can drift        |

### Conformance: checker vs simulator (`crates/mimz-core/tests/width_rules_conformance.rs`, 2 tests)

The reason `width_rules.rs` exists: two independent implementations used
to compute widths (the checker's `widths` pass and the simulator's
evaluator), and they drifted. These two tests replay a shared table
through BOTH and demand the same answer.

| Test                                         | Locks in                                                                    |
| -------------------------------------------- | --------------------------------------------------------------------------- |
| `checker_and_simulator_agree_with_the_table` | for every operator row, the checker's width and the simulator's width match |
| `shift_result_matches_the_table`             | the same for shifts, whose Verilog rule is the easiest to get subtly wrong  |

## Unit: pretty-printer (`crates/mimz-core/src/pretty/`, 8 tests)

The AST → Min-Mozhi source printer behind `mimz translate --order
code|thamizh` and `mimz fmt`. Every test is a ROUND TRIP: print the AST,
re-parse the output, and demand the same tree — the oracle that proves the
printer is not quietly losing information.

| Test                                                                | Locks in                              |
| ------------------------------------------------------------------- | ------------------------------------- |
| `sync_loop_round_trips_through_pretty_print`                        | `sync loop` headers and bodies        |
| `sync_double_flop_call_round_trips_through_pretty_print`            | `sync.double_flop(...)` call sites    |
| `foreach_round_trips_through_pretty_print`                          | both `foreach` forms                  |
| `enum_construct_pretty_prints_with_args`                            | `Enum.Variant(payload)` construction  |
| `extern_module_round_trips_through_pretty_print`                    | `extern module` declarations          |
| `extern_module_with_verilog_alias_round_trips_through_pretty_print` | …including the `verilog "Name"` alias |
| `qualified_reference_round_trips_through_pretty_print`              | `a.b.Name` qualified references       |
| `sim_speed_clause_round_trips_through_pretty_print`                 | a `sim { speed mhz(50) }` clause      |

## Unit: standard-library routing (`crates/mimz-core/src/stdlib.rs`, 5 tests)

`import std.fifo` resolves to a module EMBEDDED in the binary
(`include_str!` over the already-tested `examples/` sources), so there is
no install path and it works in WASM. Routing keys on the written alias:
an English stem picks the canonical module, a Tamil twin name or its
romanization picks the pure-Tamil twin.

| Test                                               | Locks in                                                                       |
| -------------------------------------------------- | ------------------------------------------------------------------------------ |
| `english_stem_selects_canonical`                   | `std.fifo` → the canonical module                                              |
| `twin_name_and_roman_select_twin`                  | `nuulagam.varisai` and its romanization → the pure-Tamil twin                  |
| `namespace_aliases_match_all_three_flavors`        | the `std`/`nuulagam`/`நூலகம்` namespace spellings all resolve                  |
| `unknown_module_is_none_and_available_lists_stems` | an unknown module is `None` and the error can list what IS available           |
| `every_embedded_module_has_no_imports`             | INVARIANT: no embedded module imports another, so resolution can never recurse |

## Unit: hardware-emulation peripherals (`src/emulate/`, 42 tests)

The native LED / speaker / UART peripherals bound in `sim { bind … }`
blocks and driven through `mimz-sim`'s `EmulationHost` trait
(`mimz test --emulate`). Feature-gated behind `hw-emulation` and never
compiled for `wasm32`. Design notes in
[`14-hardware-emulation.md`](14-hardware-emulation.md).

| Module               | Tests | Covers                                                                                                                  |
| -------------------- | ----: | ----------------------------------------------------------------------------------------------------------------------- |
| `emulate/mod.rs`     |     5 | the registry: which peripheral names exist and in which direction (`led`/`speaker`/`uart_tx` out, `uart_rx` in)         |
| `emulate/host.rs`    |     1 | `drive` dispatches by PORT name, not peripheral name (two LEDs on different ports stay separate)                        |
| `emulate/led.rs`     |     7 | config validation (`color:`), single-bit-only signals, on/off change tracking                                           |
| `emulate/speaker.rs` |     6 | single-bit-only, no config args, sample recording, and silencing a held-high bit on drop                                |
| `emulate/uart_rx.rs` |    10 | 8N1 framing of a literal source, socket transport, port validation, idle-high when the queue drains                     |
| `emulate/uart_tx.rs` |    13 | baud/speed validation, byte decoding, framing-error logging, socket streaming, non-blocking writes when the peer stalls |

The UART tests are the fussiest on purpose: a socket peripheral that
blocks would hang the whole simulation, so
`socket_write_does_not_block_when_peer_stops_reading` and
`socket_target_with_no_client_falls_back_to_log_without_repeated_stalls`
exist specifically to keep that from regressing.

## Integration: extern module / Verilog FFI (`tests/extern.rs`, 5 tests)

`extern module` declares a Verilog module we do NOT compile — a black box
(vendor IP, a hand-written primitive). The compiler still type-checks the
port list and instantiates it; the simulator needs to be told what to do
with it.

| Test                                                      | Locks in                                                                                |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `extern_module_checks_clean`                              | a well-formed `extern module` passes `mimz check`                                       |
| `extern_module_compiles_to_instantiation_only`            | the output instantiates it and never emits a definition                                 |
| `extern_module_alias_uses_real_verilog_name`              | `verilog "RealName"` puts the vendor's own name in the output                           |
| `extern_sim_strict_flag_makes_mimz_test_fail_fast`        | `--extern-sim strict` refuses to simulate a black box (`S0113`) instead of guessing     |
| `extern_src_cli_flag_unions_with_mimz_toml_verilog_files` | `--extern-src` on the CLI ADDS to `mimz.toml`'s `verilog_files`, it does not replace it |

## Integration: self-determined-width regressions (`tests/self_determined_regression.rs`, 12 tests)

Verilog computes some subexpression widths by its OWN rule ("self-determined
positions": concat members, comparison operands, `$signed`/`$unsigned`
arguments), which can differ from the width mimz checked. Where they differ
the emitter hoists the subexpression into an explicitly-sized wire. Each
test here is a named bug that this hoist exists to prevent, and most run the
result through real Icarus rather than asserting on text.

| Test                                                           | Locks in                                                                   |
| -------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `bug_19_lossless_sub_in_a_concat_hoists_exactly_one_wire`      | BUG-19: a lossless `-` inside `{…}` hoists — exactly ONE wire, not two     |
| `bug_19_lossless_sub_in_a_concat_matches_icarus`               | …and the hoisted result matches Icarus                                     |
| `bug_19_wrapping_sub_in_a_bitand_matches_icarus`               | the wrapping form inside `&` likewise                                      |
| `bug_20_slice_of_a_composite_expression_matches_icarus`        | BUG-20: slicing a composite expression needs a sized intermediate          |
| `bug_23_top_level_wrap_needs_no_hoist`                         | BUG-23: a top-level `+%` is already context-determined — no wasted wire    |
| `bug_23_wrap_directly_inside_a_concat_matches_icarus`          | …but inside a concat it does need one                                      |
| `bug_23_wrap_under_sibling_add_matches_icarus`                 | …and under a sibling `+`                                                   |
| `bug_23_wrap_under_sibling_add_inside_a_concat_matches_icarus` | …and in both at once                                                       |
| `bug_23_signed_wrap_operand_hoist_preserves_sign_extension`    | the hoisted wire keeps signedness, so sign extension still happens         |
| `bug_24_shl_under_sibling_add_matches_icarus`                  | BUG-24: a shift under a sibling `+` hoists correctly                       |
| `bug_24_regression_shift_in_if_branch_stays_unhoisted`         | …but a shift in an `if` branch must NOT hoist (over-hoisting is a bug too) |
| `bug_24_regression_nested_shift_lhs_of_shift_stays_unhoisted`  | …nor a shift on the left of another shift                                  |

## Integration: differential fuzzing (`tests/differential_fuzz.rs`, 4 tests)

A generative differential harness: it BUILDS random-but-valid Min-Mozhi
programs, compiles them, and runs the result through both our simulator and
real Icarus, demanding identical waveforms. Deterministic (seeded), so a
failure is reproducible. This is the test that found BUG-23.

| Test                                                         | Locks in                                                                        |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| `differential_fuzz_generates_checker_valid_programs`         | the generator only emits programs the checker accepts (else the run is vacuous) |
| `differential_fuzz_matches_icarus`                           | every generated combinational program simulates identically to Icarus           |
| `differential_fuzz_clocked_generates_checker_valid_programs` | the same guarantee for the clocked generator                                    |
| `differential_fuzz_clocked_matches_icarus`                   | …and clocked designs match Icarus cycle for cycle                               |

`gen_special_leaves` is what decides which SHAPES the generator can reach at
all — a `fn` call, a nested `fn` call (`inner{w}(x)` offered to the outer
`fn`'s body as a `special` leaf, round-7 Task 11, so [BUG-67](../audit/bugs.md)'s
shape is reachable), a `const`-bounded slice, and — clocked only — a plain
instance-port read, an array-instance-port read and a `mem` read. Depth
(`MIMZ_DIFF_FUZZ_N`) cannot compensate for a shape the vocabulary does not
contain: see [`gaps.md`](../audit/gaps.md) GAP-13 direction 2.

## Deliberately NOT covered (and what would close each gap)

| Gap                                                     | Why it's open                                                                                                                                                                                                                                                                                                                                                                        | Closes when                                                 |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| Cross-INSTANCE clock-domain tracking                    | pass 6 is module-local (instance outputs carry no domain)                                                                                                                                                                                                                                                                                                                            | with the Phase 2 `sync`/multi-clock design                  |
| Diagnostic rendering format (`render`'s caret layout)   | low risk, changes are cosmetic                                                                                                                                                                                                                                                                                                                                                       | worth a snapshot test if/when output stabilizes for E-codes |
| CLI surface (`--tokens`, exit codes, `-o` default path) | thin wrappers; breakage is loud in manual use                                                                                                                                                                                                                                                                                                                                        | cheap `assert_cmd`-style tests if the CLI grows             |
| `mimz-bench` end-to-end (a full run as a test)          | it is a measuring tool over this very suite — running it under `cargo test` would re-run everything for no new assertion                                                                                                                                                                                                                                                             | if its orchestration grows logic worth locking              |
| `fmt`, grammar engine, full simulator                   | built: all five word-order flips ship (`syntax thamizh` + clocked-block, conditional, if-expression, match, test header — `tests/grammar.rs`, `tests/test_run.rs`); `translate --order` and the full event-driven simulator (`mimz sim` / `mimz test`, B1–B8) ship too, validated by the Icarus differential + the ≥1M cycle-events/sec perf baseline. Phase 1.5 is feature-complete | with their phases (1.8 / 1.5)                               |

## House rules for new tests

- New parser/emitter behavior ships with a test **in the same commit**;
  safety-rule behaviors also test the error path (message + help).
- Prefer the existing layers: table-driven facts → keyword tests; token
  shapes → lexer tests; tree shapes & teaching errors → parser tests;
  output text → integration tests on a real example.
- A new example goes into ALL FOUR flavor folders with identical
  identifiers (only keywords change — take spellings from
  `lang/keywords.toml`, never invent), plus a row in `BASE_EXAMPLES` in
  `tests/examples.rs`. `every_example_compiles` and the
  flavor-identity test then enforce it automatically.
- Update THIS page in the same session (it is the "what does a failing
  test mean" ledger — see also `tests/docs_sync.rs`, which mechanically
  guards the structural facts in these docs).
