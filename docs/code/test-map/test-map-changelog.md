# Test Map Changelog

> Back to [Test Map Index](index.md) · [Overview](../10-test-map.md)

Changelog of test-count changes (newest first):

- 2026-08-21 documentation sync (no test-behavior change): the master
  count and per-section counts on this page were reconciled against
  `cargo test-summary --workspace` - **1115 → 1315**, reflecting all tests
  added since 2026-08-02 (primarily `self_determined_regression` 12→116,
  `differential_fuzz` 4→6, `sim_errors` 79→81, `icarus` 10→16, `test_run` 7→9,
  plus lib unit growth in `mimz-core` 607→675 and `mimz-sim` 157→172, plus
  the new `test_count_matches_docs_and_badge` test in `docs_sync.rs`).
  Fixture counts refreshed (error 117 → 119, golden `.v` steady at 70 module
  outputs + 17 `_tb.v` testbench goldens, 87 `.v` files total in
  `tests/golden/` - a same-day follow-up corrected an earlier miscount that
  read the 87-file total as the module-only count).

- 2026-08-18 the v0.2 class-closure round-7 plan's Tasks 10–11
  - no new tests, three widened. **Task 10**: all three `tests/icarus.rs`
    corpus sweeps walked `examples/` only, each with its own copy of the same
    directory walk, so `demo/cpu.mimz`'s emitted testbench sat outside the very
    test that closed BUG-64/65. All three - plus Task 1's sweep in
    `tests/self_determined_regression.rs`, the only one that already covered
    `demo/` - now share one `support::corpus_files()` (`examples/` + `demo/`,
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
  `cargo test --workspace --all-features -- --list` - **1034 → 1115**,
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
  zero test-behavior change - every test moved file, none renamed/added/
  removed): `checker/tests.rs` (3026 lines) → `checker/tests/` (`mod.rs`
  helpers + 11 topic files, 268 tests); `parser/tests.rs` (1594 lines) →
  `parser/tests/` (`mod.rs` helpers + 13 topic files, 91 tests). This page's
  checker/parser sections below were restructured to match, and the master
  count/breakdown above was corrected to the real `cargo test-summary
--workspace` total (1034) - it had drifted independently of this split
  (the two prior entries below already flagged their own staleness).

- 2026-07-21 `sync.pulse` Icarus differential example (Task 8 of the
  `sync.*` CDC-primitives implementation plan, branch
  `phase-2-correctness-consolidation-part2`): 4-flavor `sync_pulse` example
  (`BASE_EXAMPLES` 42 → 43, +1 golden `sync_pulse.v`, 69 → 70). The example
  itself deviates from the task brief's literal snippet: `sync.pulse`'s
  checker (E0704) requires the signal argument to be EXACTLY a register
  owned by `src_clock` (unlike `double_flop`, no domain-free source is
  allowed), so the brief's bare `in src_pulse: bit` fed straight into
  `sync.pulse(...)` fails to compile - fixed by adding an intermediate
  `reg src_reg` sampled by its own `on rise(clk_src)` block first, then
  passing `src_reg` to `sync.pulse`, mirroring
  `sync_pulse_produces_a_one_cycle_dst_pulse_after_toggle`'s own module
  shape in `crates/mimz-sim/src/sim/harness.rs`. **+1 hand-written Icarus
  TB** (`sync_pulse_tb.v`, 44 → 45) and **+1 `#[test]`**
  `sync_pulse_matches_icarus` in `tests/icarus.rs` (Layer 2 style, same
  reasoning as Task 7's `sync_double_flop_matches_icarus` - two clocks,
  can't use `differential()`). Icarus differential 9 → 10. No tamil-pure
  twin added (same vocabulary-invention concern Task 7 raised for
  `sync_double_flop` - inventing a Tamil term for a CDC pulse synchronizer
  without native-speaker review); note this is a minority-precedent call,
  not a hard rule - 16 of the (now) 43 `BASE_EXAMPLES` have a tamil-pure
  twin, including two recently-added ones (`tested_adder`→`tested_kuutti`,
  `foreach_sum`→`kootu`), so "recent examples never get one" would be
  false. Together, `sync_double_flop_matches_icarus` and
  `sync_pulse_matches_icarus` satisfy the spec's section 7 ask for "at least one
  kernel-vs-Icarus multi-clock test that exercises an actual crossing
  end-to-end" - both are real multi-clock crossings run against real
  Icarus, so no separate test was added beyond the two per-primitive
  examples. (This changelog entry does not reconcile the master test-count
  line or the `_tb.v`/testbench-count figures above, which were already out
  of sync with `cargo test-summary --workspace`'s actual count before this
  task - out of scope for a single-example addition, same caveat as Task 7's
  own entry below.)

- 2026-07-21 `sync.double_flop` Icarus differential example (Task 7 of the
  `sync.*` CDC-primitives implementation plan, branch
  `phase-2-correctness-consolidation-part2`): 4-flavor `sync_double_flop`
  example (`BASE_EXAMPLES` 41 → 42, +1 golden `sync_double_flop.v`, 68 → 69),
  **+1 hand-written Icarus TB** (`sync_double_flop_tb.v`, 43 → 44) and
  **+1 `#[test]`** `sync_double_flop_matches_icarus` in `tests/icarus.rs`
  (Layer 2 style, not Layer 3 - the two-clock design can't use
  `differential()`'s single-clock default stimulus). Icarus differential
  8 → 9 tests. (This changelog entry does not reconcile the master test-count
  line or the `_tb.v`/testbench-count figures above, which were already out
  of sync with `cargo test-summary --workspace`'s actual 955 before this
  task - out of scope for a single-example addition.)

- 2026-07-11 bundle-typed fn arg/return width shape-checking (checker,
  `Ty::Bundle` consolidation) + workspace-split test-visibility fix: true
  count was already 737 post-split (mimz-core 399 + mimz-sim 97 absorbed the
  old single-crate 480 lib-unit figure, plus new tests), but `cargo test` /
  CI only saw 241 without `--workspace` - see the callout above. 663 → 737
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
  wrong enum into an enum-typed reg/wire is caught - was a silent
  zero-diagnostic regression), `bundle_literal_tail_return_is_shape_checked`
  (a bundle-literal fn-tail return goes through `check_return_expr`, not the
  old `infer_ty`+`check_return_ty` path). **+4 error fixtures**: 102 → 106 -
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

- 2026-06-30 `default` assignments - Thamizh-order parser fix (branch `phase-2-default-and-const-if`):
  `seq_stmt_thamizh()` missed `Kw::Default` guard - `default` is word-order neutral, always leads.
  Fixed; all translate tests (idempotency + Verilog-preservation) pass. Suite 523 → 526.

- 2026-06-30 `default` assignments (branch `phase-2-default-and-const-if`, Tasks 2–6):
  Promoted `default` keyword (`Kw::Default`), `SeqStmt::Default` AST + parser, E0809/E0810
  checker passes, two-pass emitter, sim, and surface wiring. 4-flavor `pulse_gen` example.
  **+1 lexer unit** (`kw_default_is_recognized`), **+2 checker unit** (`e0809_default_target_not_reg`,
  `e0810_duplicate_default`). +2 error fixtures, +1 golden (`pulse_gen.v`). `BASE_EXAMPLES` 34 → 35.
  Suite 522 → 523.

- 2026-06-29 OR-arm binding intersection (branch `phase-2-tagged-unions`, Tasks 1–3):
  E0808 algorithm in `crates/mimz-core/src/checker/names.rs` (5-phase intersection). **+6 lib unit**
  (checker/tests.rs: 2 positive - 2-way OR-arm clean, 3-way OR-arm clean; 4 negative
  - name missing, extra name, width mismatch, wildcard-not-binding). Also updated
    stale pre-existing counts (lib unit 349 → 356, checker 112 → 133, sim integration
    10 → 13) to match `cargo test-summary` actuals. Suite 513 → 521.

- 2026-06-28 Tagged-union T7 surface (branch `phase-2-comb-function`): pretty-printer
  (`enum_decl`/`pattern`), critical translit fix (binding names in
  `Pattern::Variant` were not being walked - silent payload-slice miss in
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
  - two new goldens. **+1 lib unit** (`fn_with_const_local_compiles_clean` - checker/tests.rs).
    Suite 498 → 499.

- 2026-06-28 Combinational functions wrap-up (branch `phase-2-comb-function`, Task 12).
  Removed all stale `// ponytail: temporary arm` comments from 6 src/ files (the arms
  were already correct; the comments were scaffolding from Task 2). Spec/02 bumped to
  v0.2.14 (fnDecl + fnCall EBNF, missing v0.2.13 clog2 changelog entry added); spec/03
  bumped to v0.2.12 (fn promotion from reserved to active). **+1 lib unit**
  (`fn_decl_parses_in_thamizh_order` - parser/tests.rs) **+1 translate integration**
  (`fn_keyword_translates_across_all_flavors` - tests/translate.rs). Suite 496 → 498.

- 2026-06-26 CLI subcommands and DX (branch `cli-and-code-improvements`). New subcommands: `init`, `doctor`, `completions`, `repl`, `lint`. `check --watch` for continuous rechecking. Colorized diagnostics + test output via `owo-colors`. Global `-q`/`--quiet` and `-d`/`--debug` flags. `--lang` restructured to Clap `ValueEnum` with aliases. `crates/mimz-core/src/lint.rs`: style/hygiene lint passes (W0002 snake_case, W0003 PascalCase). `tests/cli.rs`: 6 smoke + integration tests for doctor, init, watch, completions. **+5 lib unit** (lint: snake_case ×2, PascalCase ×2, empty-file clean) **+6 cli integration** (new `tests/cli.rs`). Suite 465 → 476.

- 2026-06-25 LSP DX (branch `phase-4-lsp-dx`). `mimz lsp` serves hover (type + doc-on-type), go-to-definition (cross-file, `test` blocks, `import` targets), and completion (scope identifiers + flavor keywords). `crates/mimz-core/src/analysis.rs`: symbol index, `resolve_at` offset-to-definition resolver, completions - scope idents + flavor keywords. `src/lsp.rs`: LSP server wired through Tower LSP. `KeywordTable::canonical_spellings` for flavor-aware keyword completion. **+12 lib unit** (analysis.rs: symbol index, resolve_at, completions; lsp.rs: handlers; tests for each) **+1 LSP unit (bin)** (`lsp.rs` smoke). Suite 456 → 469.

- 2026-06-24 Importable `std.*` library (branch `stdlib-importable-path`). `import std.fifo` (and `serkka nuulagam.varisai` / `சேர்க்க நூலகம்.வரிசை`) now resolve to an **embedded** standard library - `crates/mimz-core/src/stdlib.rs` `include_str!`s the already-tested `examples/english/std/*.mimz` + `examples/tamil-pure/*.mimz` (zero duplication), so resolution needs no install path and works in WASM. Routing keys on the written alias: English stem → canonical module, twin name/romanization → pure-Tamil twin. `src/project.rs` gained a `std` branch (`load_project_with_lib`) that parses the embedded `&str` into a synthetic in-memory file, or loads `<dir>/<m>.mimz` when `mimz.toml [lib] std` overrides; `src/config.rs` gained the `[lib]` section + `resolve_with_path`; `mimz eject std` (`src/commands/eject.rs`, `stdlib::eject_to`) vendors the library all-or-nothing. New loader code **E1202** (bad std import) added to `crates/mimz-core/src/explain.rs` + `06-diagnostics.md`. **+8 lib unit** (5 in `crates/mimz-core/src/stdlib.rs`: aliases, canonical/twin routing, unknown-module, no-transitive-imports invariant; 3 in `src/config.rs`: `[lib]` parse, unknown-key reject, `resolve_with_path` location) **+11 stdlib integration** (new `tests/stdlib.rs`: embedded resolve + entry-stays-`files[0]` ordering, Tamil twin routing, unknown/arity E1202, relative-import regression, `[lib]` override wins + twin-spelling override matches eject, 3 eject + all-or-nothing partial-conflict). Spec/02 section 1.5 gained the `std.*` clause. A post-review fix corrected two bugs the green suite missed: embedded std modules were pushed ahead of the entry (breaking the `files[0] == entry` invariant `sim`/`test` rely on), and the `[lib]` override keyed the filename on the raw written alias instead of the resolved variant (so a Tamil-twin-name import missed the ejected `varisai.mimz`). Suite 455 → 456.

- 2026-06-23 BUG-6 (left-shift truncation) fixed in `crates/mimz-sim/src/sim/value.rs`. +1 lib unit (`shl_does_not_truncate_to_left_operand_width`). The shift example (`examples/english/shift.mimz`) was rewritten to follow the template (header + inline tests), mixed flavor added, and a real pure-Tamil twin `tamil-pure/nakartthi.mimz` created (replacing the old `shift.mimz` which had English identifiers). Both registered: `BASE_EXAMPLES` 28 → 29, `PURE_TAMIL` 12 → 13 (`tests/examples.rs`); `nakartthi` added to the `tests/icarus.rs` differential. The FIFO workaround (explicit `DEPTH` param) was reverted - all 4 flavors + `varisai` now use `1 << AW`. The FIFO doc page was updated accordingly (removed `DEPTH` parameter row). **No new test functions** beyond the shl unit test - the example and the revert ride the existing parametrized loops. Suite count 436 → 437.

- 2026-06-23 stdlib modules `seg7`, `pwm`, `fifo`, `uart_tx` shipped (after `debouncer`), each in all four flavors + a pure-Tamil twin (`ennkaatti`, `minukki`, `varisai`, `anuppi`), with inline `test` blocks, module + emitted-testbench goldens, and a hand-written self-checking Icarus testbench. **No new test functions** - the modules ride the existing parametrized loops, so `BASE_EXAMPLES` 24 → 28, `PURE_TAMIL` 8 → 12 (`tests/examples.rs`) and `TESTBENCHES` 17 → 21, `PURE_TESTBENCHES` 7 → 11 (`tests/icarus.rs`) auto-extend coverage. Suite count unchanged at 436.

- 2026-06-22 Parser AST error recovery (`phase-4-parser-ast-error-recovery` branch; Phase 4 LSP prerequisite, `architectural_ideas.md` idea 1). New `Error(Span)` variant on `TopItem`/`ModuleItem`/`SeqStmt`/`TestStmt` + a non-discarding `parser::parse_recover` entry point that leaves an `Error` placeholder at each recovery boundary instead of dropping the broken construct (the strict `parse` is unchanged - any error still discards the tree, so codegen never sees an `Error` node). Every consumer handles the variant (checker skips, codegen treats as unreachable). +4 lib unit (`parser`: `parse_recover_keeps_good_items_around_a_bad_one`, `parse_recover_top_level_error_keeps_following_module`, `parse_recover_seq_and_test_blocks_emit_error_nodes`, `strict_parse_still_errs_on_bad_input`). Suite 432 → 436.
- 2026-06-22 Fuzz crash fix: `is_word_byte` was missing `?`, so `push_guarded` in `translate::reskin` didn't insert a separating space when a `MaskedInt` ending with `?` (e.g. `0b1?`) abutted a romanized identifier, causing the re-lexer to consume `0b1?rrrram` as a single invalid number. +2 lib unit (`masked_int_q_does_not_glue_onto_romanized_identifier`, `masked_int_q_does_not_glue_onto_english_keyword`). Also: rebuilt `crates/mimz-wasm/pkg/` with `--target nodejs` + fixed `pkg/package.json` `"type": "commonjs"` - `wasm_parity` now passes locally on Node 24 (was a pre-existing ESM/CJS interop failure). Site `npm run build` auto-runs `build:wasm` to regenerate the web glue. Suite 430 → 432.

- 2026-06-22 Reserved `extern` (external-Verilog / black-box-IP module; `docs/Ideas/architectural_ideas.md` idea 3) ahead of the v0.1.0 freeze (R11): added to `lang/keywords.toml` `reserved` + spec/03 v0.2.11 + the grammar invalid pattern + a lexer test. The three separate reserved-word keyword-table tests (`fn_and_function_are_reserved`, `the_v03_backlog_keywords_are_reserved`, `the_section8_keywords_are_reserved`) were merged into one data-driven `future_keywords_are_reserved_not_usable` that also covers `extern`. Net −2 lib unit (3 removed, 1 added). Suite 432 → 430.

- 2026-06-22 WASM↔CLI Verilog parity + testbench golden/Icarus coverage. New `tests/wasm_parity.rs` asserts the `mimz-wasm` `compileToVerilog` binding emits byte-identical Verilog to the CLI's `compile` - the CLI writes to a temp `-o` path the test reads then deletes (cleaned up even if the assertion fails), so the comparison is file-content vs binding output, not status-line vs Verilog; skips with a note when `crates/mimz-wasm/pkg/` isn't built. The `--emit-testbench` work also landed `emitted_testbench_matches_the_goldens` + `emit_testbench_without_test_blocks_notes_and_writes_only_v` (`tests/examples.rs`) and `every_emitted_testbench_passes_iverilog` (`tests/icarus.rs`). +2 example integration, +1 Icarus differential, +1 wasm_parity integration. Suite 428 → 432.
- 2026-06-21 Testbench emitter (`crates/mimz-core/src/emit_verilog/testbench.rs`) `--emit-testbench` fixes: `test_env` now merges the DUT's module-parameter defaults for any arg a test doesn't override (mirrors `sim::elaborate::elaborate_module`'s override-or-default order), and args chain left-to-right so a later arg can reference an earlier one (mirrors `sim::harness::params`) - without this, a defaulted param omitted by a test, or `M(W: 8, DEPTH: W * 2)`-style chaining, failed to resolve width expressions. Also: two tests whose names sanitize to the same Verilog module identifier (e.g. `"edge case"` and `"edge_case"` both → `edge_case_tb`) are now rejected with a diagnostic instead of silently emitting two same-named modules. +3 lib unit (`test_env_falls_back_to_module_param_defaults`, `test_env_chains_earlier_args`, `colliding_sanitized_test_names_are_rejected`). Suite 425 → 428.
- 2026-06-21 Testbench emitter (`crates/mimz-core/src/emit_verilog/testbench.rs`) security and logic hardening - added `sanitize_verilog_ident` helper, bounded loop iteration counts, properly recursed into nested conditionals within inline tests, and pushed `consteval` errors gracefully. +1 lib unit (`sanitize_verilog_ident_replaces_invalid_chars`). Suite 424 → 425.
- 2026-06-20 Re-audit `crates/mimz-sim/src/sim/value.rs`: Finding A - `BinOp::Shl` used bare `r.bits as u32` to cast the shift amount, silently truncating when bit ≥ 32 was set (e.g. `1 << (1 << 32)` became `1 << 0` = 1 instead of 0). Also corrected `BinOp::Shr`'s `.min(127)` guard which avoided the truncation panic but produced wrong results (shift-by-128 became shift-by-127 instead of 0). Both fixed with `if r.bits >= 128 { 0 } else { … as u32 }`. +7 lib unit in `sim::comb::tests` (all new, section below). Suite 417 → 424.
- 2026-06-19 Two new pure-Tamil showcase examples so the playground's six curated examples (counter, adder, comparator, mux4, blinker, traffic*light) exist in **every** flavor - `examples/tamil-pure/kuutti.mimz` (adder twin) and `saalaivilakku.mimz` (traffic-light FSM twin), both Tamil keywords AND identifiers. `PURE_TAMIL` (in `tests/examples.rs` and `tests/translate.rs`) grew 4 → 6, so the equivalence, golden, and round-trip checks now cover them (new goldens `tests/golden/tamil_pure*{kuutti,saalaivilakku}.v`); the Icarus suite gained matching self-checking testbenches (`tests/icarus/{kuutti,saalaivilakku}\_tb.v`) + bit-for-bit differentials. **No new `#[test]` functions\*\* (these ride existing loop-driven tests), so the count is unchanged at 417.
- 2026-06-19 Website Phase 4 - the interactive playground waveform. The runner (`crates/mimz-sim/src/runner.rs`) gained a `ports` command (emits the module interface as JSON - `{module, clocked, inputs[], outputs[]}` - so the browser can build input controls without re-parsing) and a `sim --steps "a=3,b=5;a=7,b=1"` flag (explicit per-step input vectors, fed straight into the existing `comb_run`; rejected for clocked designs). The `/playground` got a stimulus panel - an editable step table for combinational designs (the fix for "an adder with a fixed input draws flat") and held-inputs + cycles for clocked ones - that re-simulates live, plus a hover cursor on the canvas reading each signal's value at a time point. +4 lib unit (`runner`: ports×2, sim_steps×2). Suite 413 → 417.
- 2026-06-18 Website Phase 4 step 5 - the playground waveform viewer. The runner's `sim` gained a `--vcd` flag (returns the 2-state VCD from `sim::vcd::to_vcd` instead of a console trace), so the in-browser **Simulate** button gets a waveform via the existing `runCommand` (no new wasm binding). New `site/src/components/WaveformViewer.tsx` - a self-contained canvas renderer behind the stable `vcd` prop (parses the VCD; square waves for 1-bit, value-labelled buses for wider signals; Surfer is the documented future drop-in). +1 lib unit (`runner::sim_vcd_emits_a_vcd_document`). Suite 412 → 413.
- 2026-06-18 Website Phase 4 step 4 - the in-browser playground console. New `crates/mimz-sim/src/runner.rs` (private lib module, re-exported): a filesystem-free `run_command(source, command, argv)` that runs `check`/`compile`/`eval`/`sim`/`test` against a source string and returns the text a user would see, composing the existing lib pipeline (`comb::eval_outputs`, `elaborate`, `run`/`comb_run`, `trace::render`). The `--in`/`--param`/`--sweep`/trace-scope parsers were **lifted from the CLI's `commands/helpers.rs` into the lib** (single source; the CLI now re-exports them), and `compile_string` is now a thin wrapper over `run_command`. The wasm crate gained `runCommand`; the site got a `/playground` page (textarea editor + console, a `client:only` React island over the web wasm). +5 lib unit (`runner`: sweep×2, check, eval, sim), −2 command unit (the moved `sweep_vectors` tests). Suite 409 → 412.
- 2026-06-18 Website Phase 2 (WASM groundwork) - `mimz::compile_string` (`src/lib.rs`): the filesystem-free `lex→parse→check→transliterate→emit` entry point behind the browser playground (single-file; `import` rejected with a plain message). New `crates/mimz-wasm` (wasm-bindgen `compileToVerilog`) + a Cargo workspace; the CLI-only deps (`tokio`/`tower-lsp`/`memory-stats`) were made optional and feature-gated (`default = ["lsp", "bench"]`) so the lib builds for `wasm32` under `default-features = false`. +5 compile_string integration (`tests/compile_string.rs`: valid compile names the module, trilingual byte-identical output, E0401 width mismatch, syntax error reported, `import` rejected). Verified: full native gate green, `cargo build -p mimz-wasm --target wasm32-unknown-unknown`, and a headless Node smoke test (`crates/mimz-wasm/smoke-test.cjs`) compiling the counter through wasm. Suite 404 → 409.
- 2026-06-17 Workstream B versioning + language edition - new `crates/mimz-core/src/version.rs`: the compiler-version vs language-edition axes, `EDITION_HISTORY` (first edition **Wingless Butterfly** `wingless-butterfly-2026-1`), `version_block()` (uname-style `mimz --version`), and `KEYWORD_SET_VERSION` cross-checked against `lang/keywords.toml`'s `version` (now parsed + exposed via `KeywordTable::version`). The Verilog header carries both axes. +3 lib unit (`version`: `current_is_the_last_history_row`, `keyword_set_version_matches_keywords_toml`, `version_block_shows_both_axes`). Crate stays `0.1.0-dev` (drops `-dev` at the v0.1.0 tag, Workstream D). Suite 401 → 404.
- 2026-06-17 A5 asynchronous reset `async reset` (pre-v0.1.0 RTL-parity batch) - `async` promoted from reserved to an active keyword KW_ASYNC (Tanglish/Tamil `otthisaivatra`/`ஒத்திசைவற்ற` PROVISIONAL, pending native review). `ModuleItem::Reset` became `{ name, is_async }`; the emitter widens the sensitivity list to `@(posedge clk or posedge rst)` for an async reset. Active-high only (active-low polarity deferred). The cycle-based kernel is unchanged - async and sync reset are observationally identical at per-cycle sample points, so it's an emitter-only distinction. +5 lib unit (lexer `async_is_an_active_keyword`; parser `async_reset_parses_with_the_async_flag`, `a_plain_reset_is_synchronous`; emitter `async_reset_widens_the_sensitivity_list`, `a_sync_reset_stays_clock_only`). New four-flavor `async_reset` example (`BASE_EXAMPLES` 21 → 22, golden + the Icarus three-way differential). Spec `02` → v0.2.12, `03` → v0.2.10. Suite 396 → 401.
- 2026-06-17 A4 memories `mem` (pre-v0.1.0 RTL-parity batch) - `mem` promoted from reserved to an active keyword KW_MEM (Tanglish/Tamil `ninaivagam`/`நினைவகம்` PROVISIONAL, pending native review). New `ModuleItem::Mem`; checker `Ty::Memory` (indexed read/write yields the element type, address range-checked against `depth`); emitter `reg [W-1:0] m [0:DEPTH-1]` + an `initial` power-on seed; the sim kernel gained a sparse cell store (`is_mem`/`mem_read` on the `Resolver`, indexed write into `next_mems`). +10 lib unit (lexer `mem_is_an_active_keyword`; parser `mem_declaration_parses_to_a_mem_item`, `a_mem_without_an_init_value_is_e1104`; checker `register_file_passes`, `a_non_constant_memory_depth_is_e0201`, `a_zero_memory_depth_is_e0410`, `a_memory_init_that_overflows_the_element_is_e0405`, `a_constant_address_past_the_depth_is_e0406`, `a_memory_inside_repeat_is_e0303`; kernel `memory_write_then_read_round_trips_a_cell`). New four-flavor `regfile` example (`BASE_EXAMPLES` 20 → 21, golden + the Icarus three-way differential; the `regfile` cells are internal-only - not dumped to VCD, like the tamil-pure exemption note). Spec `02` → v0.2.11, `03` → v0.2.9. Suite 386 → 396.
- 2026-06-17 A3 falling-edge `on fall(clk)` (pre-v0.1.0 RTL-parity batch) - `fall` promoted from reserved to an active keyword KW_FALL (Tanglish/Tamil `irakkam`/`இறக்கம்` PROVISIONAL, pending native review); `OnBlock`/`Reg`/`Process` gained an `edge`; emitter lowers `posedge`/`negedge`; the sim kernel is now edge-aware (rise → sample → fall per period) so mixed-edge designs match Icarus bit-for-bit. +4 lib unit (parser `on_fall_parses_with_the_fall_edge`, `thamizh_order_on_fall_parses_to_the_fall_edge`; emitter `on_fall_emits_negedge`; kernel `dual_edge_negedge_reg_captures_posedge_within_a_period`); 2 lexer tests renamed (`fall_is_an_active_keyword`, `a_reserved_word_is_an_error`). New four-flavor `dual_edge` example (`BASE_EXAMPLES` 19 → 20, golden + the Icarus three-way differential). Spec `02` → v0.2.10, `03` → v0.2.8. Suite 382 → 386.
- 2026-06-17 A2 don't-care `match` patterns `0b1??` (pre-v0.1.0 RTL-parity batch) - new `TokKind::MaskedInt` / `Pattern::IntMask` (binary `?` don't-care), mirroring the literal-pattern path; additive, no new keyword. +6 lib unit (lexer `dont_care_binary_literal_lexes_to_masked_int`; parser `dont_care_pattern_parses_to_intmask`; checker `dont_care_pattern_must_match_the_scrutinee_width`, `a_dont_care_match_still_needs_a_wildcard`, `a_dont_care_pattern_on_an_enum_is_e0409`; sim `dont_care_match_picks_the_masked_arm`). New four-flavor example `priority` (`BASE_EXAMPLES` 18 → 19, golden + the Icarus three-way differential) - no new test functions. Exact-width reuses E0409, still-needs-`_` is E0601 (no new code). Spec `02` → v0.2.9. Suite 376 → 382.
- 2026-06-17 A1 replication `{N{x}}` (pre-v0.1.0 RTL-parity batch) - new `ExprKind::Replicate` mirroring concat through the whole pipeline; purely additive, no new keyword. +7 lib unit (parser `replication_parses_to_replicate`, `braces_without_an_inner_group_stay_concat`; checker `replication_width_is_count_times_inner`, `replication_width_mismatch_is_e0401`, `a_non_constant_replication_count_is_e0201`, `a_zero_replication_count_is_e0410`; sim `replication_repeats_the_group`). New four-flavor example `replicate` (`BASE_EXAMPLES` 17 → 18, golden + the Icarus three-way differential) - no new test functions (existing parametrized iterators). Width reuses E0410, non-const count reuses E0201 (no new code). Spec `02` → v0.2.8. Suite 369 → 376.
- 2026-06-17 SEC-6 hardening audit - C2–C4 elaboration-time DoS bounds: `mimz sim`/`mimz test` skip the checker, so the structural elaborator (`crates/mimz-sim/src/sim/elaborate.rs`) gained `MAX_INSTANCE_DEPTH = 16` (recursive/cyclic instantiation → clean error, not a stack-overflow abort), `checked_sub` on the `repeat` span (extreme `hi - lo` → over-budget error, not an overflow panic), a `0..128` bound on bit-index drives (no silent `as u32` truncation), and a flatten name-collision error (no silent overwrite). A same-day follow-up pass added a 5th finding (SIM-5): `int_expr`, which lowers each flattened child const to a literal, built a negative value via a raw `i128` negation that overflow-panicked on `i128::MIN` (reachable via `(-i128::MAX) - 1`) - now non-recursive and `unsigned_abs`-based. +5 lib unit (`recursive_instantiation_errors_not_overflows`, `extreme_repeat_bounds_error_not_overflow`, `an_out_of_range_bit_index_errors`, `a_flatten_name_collision_errors`, `an_i128_min_const_elaborates_without_overflow` - `crates/mimz-sim/src/sim/elaborate.rs`). See SEC-6/HARD-6 in `docs/audit/`.
- 2026-06-16 Phase 1.5 C3 + C4 - full simulator parity: the sim elaborator now unrolls `repeat` (array instances `fa__i`, bit-indexed drives assembled into a Concat - ripple\*adder) and encodes enum-typed signals by variant index with width `clog2(variants)` (variant reads/patterns → index - traffic_light), via a unified `Rw` elaborate-time rewriter (`crates/mimz-sim/src/sim/elaborate.rs`). The Layer-3 differential now covers the **entire single-file corpus, 18 → 21 examples** (added ripple_adder, traffic_light, vilakku) - every example the emitter compiles also simulates bit-for-bit vs Icarus. +2 lib unit (`unrolls_repeat_with_instance_array_and_bit_drives`, `elaborates_an_enum_signal_and_match`). Phase 1.5 full-parity simulator complete (C1–C4).
- 2026-06-16 Phase 1.5 C2 - module-instance flattening in the sim elaborator: `elaborate_project` (`crates/mimz-sim/src/sim/elaborate.rs`) flattens `let` instances (incl. across `import`s) by inlining each child with signals name-prefixed `{inst}*{name}`, so `inst.port`reads resolve to the wire`inst*port`the emitter auto-declares - the flattened`Design`matches the emitted Verilog bit-for-bit.`mimz sim`/`mimz test`now`load_project`; the Layer-3 differential gained **alu** (`Top`instantiating the imported`Adder`) and **chained** (two chained `FullAdder`s), 16 → **18 examples**. +2 lib unit (`flattens_a_same_file_instance`, `rejects_unknown_instance_module`, replacing `rejects_instances_for_now`); the differential is one `#[test]`so the new examples add no separate count. Remaining sim parity: C3`repeat`(ripple_adder), C4 enum FSM (traffic_light).
- 2026-06-16 security/bug audit (SEC-5) - bound the simulator's unbounded count inputs: a critical→medium audit (core pipeline clean) found the new sim skipped the "bound every count" doctrine. Caps`MAX_SIM_CYCLES`/`MAX_SWEEP_VECTORS` (`crates/mimz-sim/src/sim/run.rs`) now bound `tick(clk, n)`(untrusted-input hang/OOM via`mimz test`), the `--sweep`cartesian product (unchecked`usize`mul), and`--cycles`; plus a `translate`no-panic fix and a`mimz.toml` walk-up cap. +2 command unit (`sweep_vectors`cap -`src/commands/helpers.rs`), +1 sim integration (`cycles_over_the_limit_is_rejected_by_the_cli`), +1 test integration (`a_tick_count_over_the_cycle_limit_errors_fast_not_hangs`). The auditor's `cycle * PERIOD`overflow "highs" are unreachable once the loops are bounded - recorded checked-safe, see`docs/audit/`.
- 2026-06-16 C1 carry-forward closed - the Layer-3 Icarus differential (`our*simulator_matches_icarus_bit_for_bit`) now also covers the four pure-Tamil examples (kanakki/cimitti/oppidi/thervi), so its list equals the emitter's single-module list, **12 english + 4 tamil-pure = 16**. The testbench romanizes interface names via the emitter's own `transliterate` (`interface_name_map`in`tests/icarus.rs`) to match the compiled Verilog while the kernel keeps source names; no new test function, so the count is unchanged.
- 2026-06-16 Phase 1.5 C1 - combinational `mimz sim`+ signed-aware differential:`comb_run` (`crates/mimz-sim/src/sim/run.rs`) settles a clockless design one frame per input vector, so `mimz sim`now runs combinational modules too -`--in`is one settled frame,`--sweep a=0|1|2` a frame each - emitting the same VCD/trace. The Layer-3 Icarus differential (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) was broadened to **12 ASCII-named english examples** (clocked AND combinational, incl. SIGNED `bitops`/`signed_math`), auto-routing on whether the design is clocked, comparing via Verilog `%b`(binary ⇒ signedness-agnostic) with per-example param overrides. It caught a real bug: the shared evaluator's lossless signed`+`/`*` (`crates/mimz-sim/src/sim/value.rs`) added raw bits without sign-extending a negative operand - fixed to use `as_i128`(matching Verilog), which also corrects`mimz eval`. +5 lib unit (4 `comb_run` + 1 signed regression) + 2 net sim integration (−1 clockless-reject removed, +3 combinational). Romanized tamil-pure + instance/`repeat`/enum designs are deferred (C2–C4).
- 2026-06-16 Phase 1.5 B8 - differential vs Icarus + perf baseline + golden VCD: a Layer-3 Icarus test (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) runs each design through OUR event-driven kernel in-process AND reconstructs the values from the VCD our writer emits, comparing both against `iverilog`/`vvp` under the SAME stimulus - three views (kernel == VCD waveform == Icarus) must agree bit-for-bit per cycle (counter + shift register + edge detector). A byte-for-byte golden lock (`tests/sim.rs::the_counter_vcd_matches_the_golden_byte_for_byte`vs`tests/golden/counter.vcd`, `MIMZ_UPDATE_GOLDENS=1` to regenerate) pins the writer's exact output format. A perf test (`tests/sim.rs::the_counter_kernel_clears_the_perf_baseline`) gates the kernel at ≥1M cycle-events/sec on the counter in release (best of 5 to reject load-induced dips; measured ~2.3M; debug uses a low sanity floor). +1 Icarus differential + 2 sim integration. Phase 1.5 (simulator) is now feature-complete: B1 elaborate, B2 kernel, B3 comb propagation, B4 stimulus, B5 VCD+trace+`mimz sim`, B6 `mimz test`, B7 test-header flip, B8 differential+perf+golden.
- 2026-06-16 Phase 1.5 B7 - test-header thamizh-order flip: `M(args) kaaga "…" sodhanai { }`parses to the SAME`TestDecl`as the code-order`test "…" for M(args) { }` (`crates/mimz-core/src/parser/items/test.rs::test_decl_thamizh`, dispatched from the file loop when `syntax thamizh`is active and a bare identifier leads), and`crates/mimz-core/src/pretty.rs`flips it for`mimz translate --order thamizh`- completing all five clause flips of the word-order engine. Execution is the oracle: a passing thamizh-order test re-parsing to the same tree replaces the same-Verilog check`test` blocks can't provide. +3 parser lib unit + 1 test integration (`a_thamizh_order_test_header_runs_like_its_code_order_twin`) + 1 translate integration (`pretty_print_thamizh_flips_the_test_header_and_reparses`).
- 2026-06-16 Phase 1.5 B6 - `mimz test`: the `test`-block runner in `crates/mimz-sim/src/sim/harness.rs` runs each block (`drive`/`tick`/`expect`/`if`) on the kernel, halts a failing `expect`with a teaching message (expression source + cycle + each comparison side's value), and exits non-zero on any failure;`--filter`/`--trace`/`--verbose`/`--signals`supported, the trace-scope logic shared with`mimz sim`via`commands/helpers.rs::trace_scope`. `async`was reserved alongside`await` (spec/03 v0.2.7, R11/R13) so the v0.3 backlog list is now 9 words. +6 lib unit (`crates/mimz-sim/src/sim/harness.rs`) + 5 test integration (`tests/test_run.rs`).
- 2026-06-16 Phase 1.5 B4+B5 - `mimz sim`: default stimulus + a hand-written 2-state VCD writer + the `--trace`/`--trace=changes`console table (scope via`--verbose`/`--signals`), all riding one per-cycle snapshot from the kernel. +9 lib unit (`crates/mimz-sim/src/sim/{run,vcd,trace}.rs`) + 5 sim integration (`tests/sim.rs`).
- 2026-06-16 Phase 1.5 B1 - simulator elaboration: +5 lib unit in `crates/mimz-sim/src/sim/elaborate.rs`, the `Design`flattener (signals/regs/comb/processes, widths + reset folded) the event-driven kernel will interpret.
- 2026-06-16 Phase 1.5 B2 - event-driven two-phase kernel: +7 lib unit in`crates/mimz-sim/src/sim/kernel.rs` (counting/reset, width-wrap, the two-phase register swap, statement-`if`, the per-cycle snapshot seam, leaf validation). The shared 2-state value model + expression evaluator were extracted to `crates/mimz-sim/src/sim/value.rs`behind a`Resolver`trait that both`comb`and`kernel`implement -`comb`'s 7 tests are unchanged and verify the extraction.
- 2026-06-16 Phase 1.5 B3 - combinational propagation: +2 kernel lib unit locking multi-level `wire → wire → output`settling order and the kernel's comb-cycle guard; B3 needed no new code - the kernel's memoized resolver already settles drivers in dependency order.
- 2026-06-16 close Phase 1.8 + pre-freeze keyword reservation: Phase 1.8 closed by bumping`spec/04`DRAFT → stable (docs only, no test change); and`fn`/`function`reserved for a future combinational-function construct ahead of the v0.1.0 freeze (R11/R13) - +1 keyword-table lib unit`fn_and_function_are_reserved`. Also listed `the_section8_keywords_are_reserved` in the keyword-table section below, present since 2026-06-13 but previously unlisted.
- 2026-06-16 native-authored error catalog + audit/coverage follow-up: the Tamil/Tanglish catalog (`lang/messages.toml`, decision C3 ratified) grew from a one-shape stub to **33 of 36** localized codes with structured-arg interpolation; an audit of PRs #14–#17 found no bug/overflow/security/perf issue, so the work was test-coverage + prevention guards only. +2 morph lib unit (`arg_code_without_args_falls_back_to_english`, `fill_with_empty_name_leaves_no_stray_fragment`), +4 morph integration (`e0402`/`e0408`/`e0601`interpolation tests +`message_catalog_placeholders_are_known_tokens`- a guard that every active`{token}`in`lang/messages.toml`is one`morph::fill` fills, so a typo'd placeholder can't silently fall back to English forever), +1 grammar-sync (`keywords_toml_has_no_superseded_spelling` - a superseded v1 spelling may not return as a keyword/alias). The remaining +9 morph integration vs. the prior count are #16's newly-localized codes (`e0502`/`e0505`/`e0202`/`e0401`), the `message_catalog_keys_are_real_checker_codes` guard, and the W0001 mixed-flavor lint tests.
- 2026-06-15 fuzz/security audit of the since-2026-06-14 changes: a deterministic stress harness over adversarial Tamil/keyword/ASCII input found that reskinning a numeric literal directly abutting a Tamil keyword/identifier (`42தொகுதி`) glued it into an unlexable lexeme - fixed by a boundary-space guard in `reskin`; and that `--names-map`accepted any`NameMap.version`- fixed by a version check in`load_name_map`. +1 translate integration (boundary guard regression), +1 config integration (unknown-version rejected). No overflow/unsafe/crash found. A `translate_roundtrip`cargo-fuzz target was added to close the coverage gap, CI-only, outside this count.
- 2026-06-15`mimz.toml`config + name-map auto-discovery: a new`config`module reads per-project flag defaults from`mimz.toml`(discovered by walking up from the input file; precedence CLI › config › default), and reverse`translate`auto-loads the`<input>.names.json` sidecar with no flag (`--no-names-map` opts out). +4 lib unit (`config`: parse, defaults, unknown-key reject, walk-up discovery), +4 config integration (auto-restore, --no-names-map, config precedence, malformed-config error).
- 2026-06-15 reversible romanization: `--romanize-names`now writes a per-file sidecar`<out>.names.json` (`NameMap`, romanized→Tamil) beside `-o`, and `mimz translate --names-map <file>`restores the exact Tamil names - so`Tamil → Latin → Tamil`is lossless. New`romanize_with_map`/`restore_with_map`share a factored`reskin` helper. +3 lib unit (`translate`: inverse map, restore inverts romanize, NameMap serde), +2 translate integration (lib round-trip via map, CLI forward+reverse).
- 2026-06-15 pure-Tamil showcase + opt-in `translate --romanize-names`: a new `examples/tamil-pure/`folder holds fully-Tamil programs - Tamil keywords AND identifiers - exempt from the four-flavor byte-identity rule (R9) and instead proven equivalent to their English counterparts by canonical identifier renaming.`mimz translate --romanize-names`reuses the emitter's`romanize` to rewrite Tamil identifiers to Latin (opt-in, one-way; lossless default unchanged). +2 lib unit (`translate`), +2 example integration (pure-Tamil golden + equivalence), +1 Icarus (pure-Tamil testbenches), +3 translate integration.
- 2026-06-15 mixed-flavor lint: a non-fatal warning **W0001** fires when a file mixes Tamil keywords with English/Tanglish - `Diag`gained a`Severity`(Error/Warning),`check`/`compile`/`eval`print it and still succeed, and the LSP shows it as a WARNING. +2`morph`lib unit, +1 LSP unit, +3`morph`integration.
- 2026-06-15 robustness follow-up to the 2026-06-14 batch audit: +9 lib unit - 2`morph`(tie-break + empty-stem inflection), 5 checker (two-literal`min`E0407,`nand`of a bare`bit`, nested `abs(min)`/`min(abs)`, `abs`at the width boundary), 1 parser (a long flat binary chain parses without tripping the E1113 depth guard), 1 emitter (a built-in lowers parenthesized inside a larger expression) - and +2`fmt` integration (`-o`onto the input path round-trips via the new atomic write; an unknown`--to`is a clean error). A`pretty_roundtrip`cargo-fuzz target was added (CI-only, outside this count).
- A QA pass for the new built-ins added the`bitops`example in all four flavors - golden + a self-checking Icarus testbench incl. the abs(MIN) width-growth case - plus edge tests: parser arity E1110, checker literal-adapt + abs-of-literal, fmt keyword-free/non-lexing, and`compile --lang`localization.
- Arithmetic built-ins`min`/`max`/`abs`/`nand`/`nor`/`xnor`added 6 checker unit tests + 1`eval`integration test.
- Phase 1.8 error-language plumbing added 8`morph`lib unit tests + 7`tests/morph.rs`integration tests for selection, inflection, and the additive English-fallback path.
- 2026-06-14, after merging the security-hardening and Phase 1.8 grammar branches: the security audit added 2 parser unit tests + 3`eval`integration tests for overflow/recursion guards; the Phase 1.8 thamizh-order flips - conditional / if-expression / match - added 10 grammar integration tests incl. the profile-boundary and depth-guard regressions. Then`mimz translate --order`(the`pretty`AST printer) added 4 translate integration tests + 1 grammar test for the Tamil thamizh-order traffic light.
- The error-fixture tests are data-driven over ~70 broken`.mimz`fixtures; one locks`ALL_CHECKER_CODES`- now`pub`in`crates/mimz-core/src/diag.rs`- to the 11-checker.md catalog, one locks the`--json`wire format.
- The 2026-06-13 quick-wins block added the tooling tests below:`explain`(+3),`translate`(+3 unit, +3 integration),`sim::comb`(+7 unit, +6`eval` integration).
