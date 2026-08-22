# Unit: sim runner / VCD / console trace (`crates/mimz-sim/src/sim/{run,vcd,trace}.rs`, 18 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Phase 1.5 step B4/B5 (+ C1): the default stimulus + clocked timeline capture
(`run.rs::run`), the combinational `comb_run` (one settled frame per input
vector), the hand-written 2-state VCD writer (`vcd.rs`), and the console trace
renderer (`trace.rs`) - all over one per-cycle snapshot from the kernel.

| Test (module)                                              | Locks in                                                                            |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `counter_timeline_counts_after_reset` (run)                | the default stimulus resets cycle 0 then counts; the clock renders as a square wave |
| `inputs_are_held_for_the_run` (run)                        | `--in` values hold across the whole run (`r +% x` accumulates)                      |
| `a_clockless_module_is_rejected` (run)                     | the CLOCKED `run` rejects a clockless module (callers route it to `comb_run`)       |
| `an_unknown_input_is_rejected` (run)                       | an unknown `--in` name is a clean error                                             |
| `a_comb_assert_that_fires_fails_comb_run_with_s0501` (run) | an `assert` that goes false during `comb_run` fails with an assertion message       |
| `comb_run_settles_one_frame_per_vector` (run)              | a combinational design settles its outputs for one input vector (lossless add)      |
| `a_comb_cover_that_hits_is_reported_in_the_timeline` (run) | a `cover` that hits is tallied in the returned timeline                             |
| `comb_run_sweeps_a_frame_per_vector` (run)                 | N input vectors → N frames, one per settle, on the clocked period                   |
| `comb_run_with_no_vectors_is_one_zero_frame` (run)         | no vectors → a single all-zero-input frame                                          |
| `comb_run_rejects_a_clocked_design` (run)                  | `comb_run` refuses a clocked/registered design                                      |
| `signed_lossless_add_sign_extends` (run)                   | C1 regression: lossless signed `+` sign-extends a negative operand (`-2+7=5`)       |
| `header_scope_and_vars_present` (vcd)                      | the VCD has `$timescale`/`$scope`/`$var`/`$enddefinitions`                          |
| `has_initial_dump_and_timestamps` (vcd)                    | `$dumpvars` + `#<time>` blocks + a multi-bit `b…` vector line                       |
| `id_codes_are_unique` (vcd)                                | the base-94 signal id codes never collide                                           |
| `dumps_a_wide_signal_as_a_binary_vector` (vcd)             | a >128-bit signal dumps as a full binary vector, not a truncated one                |
| `table_has_a_row_per_cycle` (trace)                        | `--trace` renders one table row per cycle with the right count                      |
| `changes_style_omits_unchanged_frames` (trace)             | `--trace=changes` only prints when a watched signal changes (`$monitor`-style)      |
| `table_renders_a_wide_signal_in_decimal` (trace)           | a >128-bit signal prints as a decimal number, not limbs                             |
