# 10 - Test Map: What Is Covered, What Isn't, and Why

Every test, what it locks in, and what a failure means. Update this page
when tests are added or removed (the count below is asserted nowhere -
this page is the human ledger).

> **Live breakdown:** run **`cargo test-summary --workspace`** instead of
> `cargo test` - it runs the suite, then prints a per-binary table (lib unit,
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
> `mimz-core` (678 lib unit + 2 crate integration) and `mimz-sim` (172 lib
> unit + 81 crate integration) - **933 tests invisible without the flag**.
> CI (`.github/workflows/ci.yml`) had this exact gap for one day
> (2026-07-10 - 2026-07-11) after the workspace split landed; fixed by
> adding `--workspace` to its clippy/test/doc/build steps.

**1439 tests** as of 2026-09-10 (`cargo test --workspace`; the count is
re-derived from source by `tests/docs_sync.rs`, so this page must track it —
+3 from `crates/mimz-core/src/ir/tests/lower_bitselect_write.rs` (2026-09-10
IR-base-gap-closure plan, Task 4) —
`lowers_constant_bit_select_write_as_a_masked_merge`,
`lowers_runtime_bit_select_write_via_eq_mux_chain`, and
`lowers_constant_slice_write_as_a_masked_merge`, pinning that a bit-select or
slice LValue write (`q[3] <- ...`, `q[7:4] <- ...`) lowers as a per-bit merge
into the target's current value instead of panicking — a constant index/
bounds is pure re-pointing (no new cell), a runtime index builds one Eq +
one merge Mux per bit position; +1 from
`crates/mimz-core/src/ir/tests/lower_fn_inline.rs` (2026-09-10
IR-base-gap-closure plan, Task 3) —
`fn_if_return_mux_sizes_literals_to_the_declared_return_width`, pinning that
`lower_fn_stmts`'s if/return mux tree sizes each literal branch to the
function's declared return width instead of the widest literal's own
natural width — the third call site of the literal/const context-sizing bug,
fixed via the existing `lower_expr_sized` helper; a further +2 from
`crates/mimz-core/src/ir/tests/lower_consts.rs` (GAP-1 residual
Task 2, 2026-09-08) —
`fn_body_references_file_level_const_as_shift_amount` and
`module_body_references_module_parameter_directly`, pinning that
`ir::lower` resolves a module parameter/file-level `const` referenced as a
plain identifier instead of panicking; see GAP-1's "panicked on any module
parameter or file-level `const`" sub-gap in `docs/audit/gaps.md`; +4 from
`crates/mimz-core/src/ir/tests/lower_binops.rs` (GAP-1 residual Task 3,
2026-09-08) —
`debug_wrapper_shaped_bare_literal_in_a_wire_driver_sizes_to_the_declared_output_width`,
`traffic_light_shaped_enum_match_arms_size_their_tags_to_the_scrutinees_width`,
`sync_loop_search_shaped_compile_time_subtraction_sizes_down_to_the_narrower_counter_width`,
and (added in a same-day review fix)
`a_wide_compile_time_constant_expression_lowers_exactly_not_saturated_to_i128_max`,
pinning that a bare literal, a const/param identifier, or a larger
compile-time-constant expression built from those now sizes to its
use-context width instead of its own natural/arithmetic-growth width. The
third test pins the direction-reversal case — a constant EXPRESSION can
come out WIDER than its sibling, not just narrower, so the fix picks which
side to re-lower by asking "is this side const-foldable", not "is this side
narrower". The fourth pins a review finding: the constant-expression
fallback must fold through `crate::value::const_eval_wide`, not the
`i128`-saturating `const_eval`, or a checker-legal wide constant (`bits[200]
w = 1 << 190;`) silently lowers to a wrong (`i128::MAX`-saturated) value with
no error anywhere. See GAP-1's "sized at its own natural width" sub-gap in
`docs/audit/gaps.md` (this also accounts for a pre-existing +4 gap between
the prior "1418" figure and HEAD's actual count that predates this round
and wasn't re-derived here — a pre-2026-09-08 doc-sync miss, not
attributable to either of the two rounds just described); a further +2 from
`crates/mimz-core/src/ir/tests/lower_binops.rs` and
`crates/mimz-core/src/ir/tests/validate.rs`, pinning `Shl`'s `out` pin at
`width_rules::shift_result`'s worst-case growth instead of the left operand's
own width (a growing left shift used to truncate silently) and the matching
`validate` cross-check; see GAP-1's `Shl`/`Shr` sub-gap in
`docs/audit/gaps.md`; a further +2 from `crates/mimz-core/src/ir/tests/validate.rs`'s
`rejects_an_output_port_never_driven_by_any_cell` and
`tests/ir_validation.rs`'s `undriven_output_port_fixture_is_rejected`, closing
`validate.rs`'s direction-blind driven-set seeding — an `out` port's nets are
no longer marked "driven" just by being a port; a further +4 from
`crates/mimz-core/src/ir/tests/lower_builtins.rs` — the re-added
`extend(signed(a), 16)` refusal fixture plus three `ir::exec`-executed
`nand`/`nor`/`xnor` value checks; a further +2 from
`crates/mimz-core/src/ir/tests/lower_binops.rs` (2026-09-05, GAP-1 residual
Task 1) —
`shl_with_a_compile_time_constant_amount_sizes_exactly_not_worst_case` and
`shl_result_feeding_a_matched_width_cell_validates_cleanly_when_amount_is_constant`,
pinning `lower_binop`'s new exact sizing for a compile-time-constant shift
amount (and `ir::validate`'s matching `shl_const_amount` cross-check) — see
GAP-1's "narrower than originally scoped" sub-gap in `docs/audit/gaps.md`; a
final +3 from `crates/mimz-core/src/ir/tests/validate.rs` (2026-09-05,
GAP-1 residual Task 2) — `accepts_a_legitimately_sized_output_port`,
`rejects_an_extend_no_op_output_wider_than_its_declaration`, and
`hand_parsed_fixture_with_no_declared_width_skips_the_port_width_check`,
pinning `validate`'s new sixth check (`Module::port_declared_widths` vs.
each output port's lowered `Bits::width()`) that catches an over-wide
output port silently reaching a `bits[N]` port declaration — see GAP-1's
"silent, not a loud `WidthMismatch`" sub-gap in `docs/audit/gaps.md`; a
further +1 from `tests/ir_validation.rs`'s
`shift_growth_too_wide_fixture_is_rejected` (2026-09-05, GAP-1 residual
Task 3) — pinning that `validate()` now REPORTS a pathologically-wide
`Shl` growth as `ValidationError::ShiftGrowthTooWide` instead of
panicking; see GAP-1's "checker-legal program can panic" sub-gap in
`docs/audit/gaps.md`; a final +1 from
`crates/mimz-core/src/ir/tests/lower_binops.rs`'s
`shift_chains_lowered_per_node_match_the_ast_kernels_fused_evaluation`
(2026-09-05, GAP-1 residual Task 4) — confirms `ir::lower` + `ir::exec`
already agree numerically with the AST kernel's fused
`value::binary::eval_shift_chain` on BUG-34's repro shape, its mirror, and
a 3-step chain, exhaustively over an 8-bit domain, as a side effect of
Task 1's exact constant-amount `Shl` sizing — no lowering-side fusion
needed; see GAP-1's "fused shift chains" sub-gap (now RESOLVED) in
`docs/audit/gaps.md`; a final +4 (2026-09-05, GAP-1 residual Task 5) — +2 in
`crates/mimz-core/src/ir/tests/lower_binops.rs`
(`signed_ordering_comparisons_execute_with_the_right_sign`,
`a_natural_width_literal_operand_keeps_the_comparison_unsigned` — renamed
`a_literal_operand_is_sized_to_its_signed_siblings_width_and_the_comparison_is_signed`
by GAP-1 residual Task 3, 2026-09-08, which closed the boundary this test
used to pin) and +2 in
`crates/mimz-core/src/ir/tests/parse_line.rs`
(`round_trips_signed_and_unsigned_ordering_comparisons`,
`an_unknown_comparison_bracket_argument_is_rejected`), pinning the new
`signed` flag on `CellKind::{Lt,Le,Gt,Ge}` — its sign-aware execution, the
literal-width boundary it deliberately does NOT cross (later closed, see
above), and its text-format round trip; see GAP-1's signed-comparison
sub-gap in `docs/audit/gaps.md`; a
final +2 in `crates/mimz-core/src/ir/tests/lower_binops.rs` from Task 5's
review fix round (`a_negated_operand_keeps_the_comparison_unsigned`,
`a_signed_cast_over_an_identifier_makes_the_comparison_signed`), pinning the
two shapes that decide whether `expr_is_definitely_signed`'s answer is
trustworthy — a negated operand, whose lowered width is one bit short of the
checker's type width, and the `signed(<Ident>)` headline case, in both its
accepting and refusing forms; a final net +1 (2026-09-07, GAP-1 residual
Task 6) in `crates/mimz-core/src/ir/tests/lower_mem.rs` — replaced
`a_second_read_at_a_different_address_panics` with two tests,
`a_second_read_at_a_different_address_grows_a_second_port` and
`a_second_read_at_the_same_address_reuses_the_port`, pinning that
`ir::lower` now grows an independent `(raddr, rdata)` port per distinct
lowered read address instead of panicking on a second one — see GAP-1's
single-memory-read-port sub-gap (now RESOLVED) in `docs/audit/gaps.md`); a
further +2 from `crates/mimz-core/src/ir/tests/lower_unary_concat_slice.rs`
(GAP-1 residual Task 4, 2026-09-08) —
`lowers_replicate_reuses_same_nets` and
`lowers_replicate_preserves_msb_first_ordering`, pinning
`ExprKind::Replicate`'s new lowering (pure bit-vector reassembly, same
net-reuse/MSB-first-source-order strategy as `Concat`); see GAP-1's
`ExprKind::Replicate` sub-gap (now RESOLVED) in `docs/audit/gaps.md`; a
further +2 from the same file (GAP-1 residual Task 5, 2026-09-08) —
`lowers_constant_index_to_a_single_bit_repoint` and
`lowers_runtime_index_via_shr_and_a_zero_slice`, pinning `ExprKind::Index`'s
new plain-vector bit-select lowering (constant index: pure re-pointing;
runtime index: composes the existing `Shr` lowering with a fixed net-0
slice); see GAP-1's plain-vector-index sub-gap (now RESOLVED) in
`docs/audit/gaps.md`; a final +2 from the new
`crates/mimz-core/src/ir/tests/lower_array_fn_params.rs` (GAP-1 residual
Task 6, 2026-09-08) —
`lowers_constant_index_into_array_param_to_the_flattened_elements_bits` and
`lowers_runtime_index_into_array_param_via_eq_mux_chain`, pinning
array-typed `fn` params' N-scalar flattening (`call_locals`/`call_arrays`
keyed `"{param}_{i}"`, ported unchanged from `emit_verilog`/the AST value
evaluator) AND the matching `ExprKind::Index` array-element branch (constant
index: direct re-pointing to the flattened element; runtime index: an
`Eq`/`Mux` chain over the flattened elements, clamping out-of-range to the
last one) that makes those locals reachable from the fn body — see GAP-1's
array-typed-fn-param sub-gap (now RESOLVED) in `docs/audit/gaps.md`); a
final +1 from `crates/mimz-core/src/ir/tests/lower_binops.rs` (Task 8,
2026-09-08) —
`lower_coalesce_is_unreachable_for_both_source_forms`, running the real
lex -> parse -> check -> elaborate_project -> lower pipeline over both of
`??`'s source forms and confirming empirically that no `BinOp::Coalesce`
node survives elaboration into a `Design` — pins `lower_binop`'s new
`BinOp::Coalesce => unreachable!(...)` arm; see GAP-1's `BinOp::Coalesce`
sub-gap (now RESOLVED) in `docs/audit/gaps.md`):

| Where it lives                                      |    Count | Kind                                                   |
| --------------------------------------------------- | -------: | ------------------------------------------------------ |
| `crates/mimz-core/src/**` (lib unit)                |      852 | in-process, `#[cfg(test)] mod tests`                   |
| `crates/mimz-sim/src/**` (lib unit)                 |       90 | in-process                                             |
| `src/**` (mimz shell crate, lib unit)               |       51 | in-process (`config`, `emulate`, `project`)            |
| `src/lsp.rs` + `src/main.rs` (bin/lib `mod lsp`)    |        7 | in-process (`lsp`)                                     |
| `src/bin/mimz-bench/` (bin unit)                    |        6 | in-process                                             |
| `crates/mimz-wasm` (lib unit)                       |        0 | no unit tests - covered via `wasm_parity`              |
| doctests (×4 crates)                                |        0 | none currently - runnable examples live in `examples/` |
| `crates/mimz-sim/tests/sim_errors.rs`               |       81 | crate integration                                      |
| `crates/mimz-core/tests/width_rules_conformance.rs` |        2 | crate integration                                      |
| `tests/cli.rs`                                      |        6 | workspace integration (runs the binary)                |
| `tests/compile_string.rs`                           |       14 | workspace integration (in-process lib)                 |
| `tests/config.rs`                                   |        7 | workspace integration                                  |
| `tests/differential_fuzz.rs`                        |        8 | workspace integration (generative + Icarus + IR)       |
| `tests/docs_sync.rs`                                |        6 | workspace integration (doc staleness guard)            |
| `tests/errors.rs`                                   |        4 | workspace integration (error fixtures)                 |
| `tests/eval.rs`                                     |       15 | workspace integration                                  |
| `tests/examples.rs`                                 |       13 | workspace integration (golden `.v`)                    |
| `tests/extern.rs`                                   |        5 | workspace integration                                  |
| `tests/fmt.rs`                                      |        9 | workspace integration                                  |
| `tests/grammar.rs`                                  |       16 | workspace integration                                  |
| `tests/grammar_sync.rs`                             |        6 | workspace integration (spec staleness guard)           |
| `tests/icarus.rs`                                   |       16 | differential (needs `iverilog`)                        |
| `tests/ir_golden.rs`                                |        5 | workspace integration (golden IR-text snapshots)       |
| `tests/ir_validation.rs`                            |        6 | workspace integration (IR validation-rejection corpus) |
| `tests/lab_lessons.rs`                              |        1 | workspace integration (lab content gate, site plan W6) |
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
| **Total**                                           | **1418** |                                                        |

Fixture counts (current): **120** error fixtures (`tests/fixtures/errors/*.mimz`,
plus a `README.md` and the `e0110_support/` helper folder) · **8** grammar
fixtures · **3** extern fixtures · **3** package fixtures · **70** golden
module `.v` outputs + **17** `_tb.v` testbench goldens (**88** `.v` files
total in `tests/golden/`) + **1** `.vcd` ·
**50** Icarus self-checking testbenches · **43** `BASE_EXAMPLES` × 4
flavors + **16** pure-Tamil twins.

---

## Legend - how to read this page

| Term                   | Means                                                                                                                                                                                                                                  |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **lib unit test**      | A `#[cfg(test)] mod tests` block INSIDE the source file it tests. Fast, in-process, sees private items. Most of the suite.                                                                                                             |
| **integration test**   | A file under `tests/`. Compiled as its own crate, so it only sees the PUBLIC API - or shells out to the real `mimz` binary.                                                                                                            |
| **golden file**        | A committed expected output (`tests/golden/*.v`). The test regenerates and byte-compares. Regenerate on purpose with `MIMZ_UPDATE_GOLDENS=1`.                                                                                          |
| **fixture**            | A small input file the test loads (`tests/fixtures/`). Error fixtures declare their expected code in a header comment (`// expect: E0401`).                                                                                            |
| **differential test**  | Runs the SAME design two ways and demands identical results - usually our simulator vs. real `iverilog`/`vvp`. Catches bugs asserts cannot.                                                                                            |
| **completeness guard** | A test that fails when a list and its documentation drift apart (e.g. every error code must own a fixture). It is how this page stays honest.                                                                                          |
| **parametrized loop**  | One `#[test]` that iterates a table (`BASE_EXAMPLES`, `TESTBENCHES`). Adding a table row adds coverage WITHOUT changing the test count.                                                                                                |
| **flavor**             | One of the keyword spellings: `english`, `tanglish`, `tamil`, `mixed`, plus `tamil-pure` (Tamil keywords AND Tamil identifiers).                                                                                                       |
| **E-code / W-code**    | Compile-time diagnostic: `E` fails the build, `W` warns. Catalogs in [`11-checker.md`](11-checker.md) and [`06-diagnostics.md`](06-diagnostics.md).                                                                                    |
| **S-code**             | Run-time diagnostic from `mimz-sim` (after the checker already accepted the program). Catalog in [`13-tooling.md`](13-tooling.md).                                                                                                     |
| **Layer 1/1.5/2/3**    | Icarus depth (`tests/icarus.rs`'s own terms): 1 = every emitted `.v` is valid Verilog, 1.5 = every auto-generated `_tb.v` is too, 2 = a hand-written self-checking testbench passes, 3 = OUR simulator matches Icarus cycle-for-cycle. |

---

## Detailed Breakdown by Category

### Lib Unit Tests

- [`lib-unit/keyword-table.md`](test-map/lib-unit/keyword-table.md) - Keyword table (15 tests)
- [`lib-unit/lexer.md`](test-map/lib-unit/lexer.md) - Lexer (15 tests)
- [`lib-unit/parser.md`](test-map/lib-unit/parser.md) - Parser (102 tests across 13 files)
- [`lib-unit/checker.md`](test-map/lib-unit/checker.md) - Checker (286 tests across 11 files)
- [`lib-unit/widths-pass.md`](test-map/lib-unit/widths-pass.md) - Widths pass internals (5 tests)
- [`lib-unit/transliteration.md`](test-map/lib-unit/transliteration.md) - Transliteration (6 tests)
- [`lib-unit/emitter.md`](test-map/lib-unit/emitter.md) - Emitter (93 tests, excl. translit + testbench rows)
- [`lib-unit/testbench-emitter.md`](test-map/lib-unit/testbench-emitter.md) - Testbench emitter (5 tests)
- [`lib-unit/lint.md`](test-map/lib-unit/lint.md) - Lint (5 tests)
- [`lib-unit/explain.md`](test-map/lib-unit/explain.md) - Explain (3 tests)
- [`lib-unit/translate.md`](test-map/lib-unit/translate.md) - Translate (10 tests)
- [`lib-unit/config.md`](test-map/lib-unit/config.md) - Config (8 tests)
- [`lib-unit/version.md`](test-map/lib-unit/version.md) - Version (3 tests)
- [`lib-unit/morph.md`](test-map/lib-unit/morph.md) - Morph (14 tests)
- [`lib-unit/pretty.md`](test-map/lib-unit/pretty.md) - Pretty-printer (11 tests)
- [`lib-unit/stdlib.md`](test-map/lib-unit/stdlib.md) - Standard-library routing (5 tests)
- [`lib-unit/hardware-emulation.md`](test-map/lib-unit/hardware-emulation.md) - Hardware-emulation peripherals (42 tests)
- [`lib-unit/source-normalization.md`](test-map/lib-unit/source-normalization.md) - Source normalization (1 test)
- [`lib-unit/ast-lowering.md`](test-map/lib-unit/ast-lowering.md) - AST lowering passes (21 tests)
- [`lib-unit/checker-internals.md`](test-map/lib-unit/checker-internals.md) - Checker internals (consteval 6, drivers 2, names 3)
- [`lib-unit/wide-integers.md`](test-map/lib-unit/wide-integers.md) - Wide integers and width rules (bits 17, wide 18, width_rules 19)

### Crate Integration Tests

- [`crate-integration/sim-errors.md`](test-map/crate-integration/sim-errors.md) - Sim runtime errors (81 tests)
- [`crate-integration/width-rules-conformance.md`](test-map/crate-integration/width-rules-conformance.md) - Width rules conformance (2 tests)

### Workspace Integration Tests

- [`workspace-integration/cli.md`](test-map/workspace-integration/cli.md) - CLI (6 tests)
- [`workspace-integration/compile-string.md`](test-map/workspace-integration/compile-string.md) - Compile string (14 tests)
- [`workspace-integration/config.md`](test-map/workspace-integration/config.md) - Config (7 tests)
- [`workspace-integration/differential-fuzz.md`](test-map/workspace-integration/differential-fuzz.md) - Differential fuzzing (6 tests)
- [`workspace-integration/docs-sync.md`](test-map/workspace-integration/docs-sync.md) - Docs sync (5 tests)
- [`workspace-integration/errors.md`](test-map/workspace-integration/errors.md) - Error fixtures (4 tests)
- [`workspace-integration/eval.md`](test-map/workspace-integration/eval.md) - Eval (15 tests)
- [`workspace-integration/examples.md`](test-map/workspace-integration/examples.md) - Examples (13 tests)
- [`workspace-integration/extern.md`](test-map/workspace-integration/extern.md) - Extern module (5 tests)
- [`workspace-integration/fmt.md`](test-map/workspace-integration/fmt.md) - Fmt (9 tests)
- [`workspace-integration/grammar.md`](test-map/workspace-integration/grammar.md) - Grammar engine (16 tests)
- [`workspace-integration/grammar-sync.md`](test-map/workspace-integration/grammar-sync.md) - Grammar sync (6 tests)
- [`workspace-integration/icarus.md`](test-map/workspace-integration/icarus.md) - Icarus differential (16 tests)
- [`workspace-integration/lsp.md`](test-map/workspace-integration/lsp.md) - LSP (1 test)
- [`workspace-integration/morph.md`](test-map/workspace-integration/morph.md) - Morph (20 tests)
- [`workspace-integration/packages.md`](test-map/workspace-integration/packages.md) - Packages (2 tests)
- [`workspace-integration/self-determined-regression.md`](test-map/workspace-integration/self-determined-regression.md) - Self-determined regression (116 tests)
- [`workspace-integration/showcase.md`](test-map/workspace-integration/showcase.md) - Showcase (6 tests)
- [`workspace-integration/sim.md`](test-map/workspace-integration/sim.md) - Sim (17 tests)
- [`workspace-integration/stdlib.md`](test-map/workspace-integration/stdlib.md) - Stdlib (11 tests)
- [`workspace-integration/test-run.md`](test-map/workspace-integration/test-run.md) - Test run (9 tests)
- [`workspace-integration/translate.md`](test-map/workspace-integration/translate.md) - Translate (15 tests)
- [`workspace-integration/wasm-parity.md`](test-map/workspace-integration/wasm-parity.md) - WASM parity (2 tests)

### Simulator Tests

- [`simulator/combinational.md`](test-map/simulator/combinational.md) - Combinational evaluator (22 tests)
- [`simulator/value-model.md`](test-map/simulator/value-model.md) - Value model + fn-body interpreter (38 tests)
- [`simulator/elaboration.md`](test-map/simulator/elaboration.md) - Elaboration (26 tests)
- [`simulator/kernel.md`](test-map/simulator/kernel.md) - Kernel (30 tests)
- [`simulator/run-vcd-trace.md`](test-map/simulator/run-vcd-trace.md) - Sim runner / VCD / console trace (18 tests)
- [`simulator/playground-runner.md`](test-map/simulator/playground-runner.md) - Playground runner (14 tests)
- [`simulator/test-harness.md`](test-map/simulator/test-harness.md) - Test harness (27 tests)
- [`simulator/sim-integration.md`](test-map/simulator/sim-integration.md) - Sim integration (17 tests)

---

## Changelog of Test-Count Changes

See [`test-map-changelog.md`](test-map/test-map-changelog.md) for the full history of test count changes.
