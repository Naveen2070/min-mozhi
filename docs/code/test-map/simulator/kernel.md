# Unit: kernel (`crates/mimz-sim/src/sim/kernel.rs`, 29 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Phase 1.5 step B2: the event-driven, two-phase simulation kernel that interprets
a `Design` over clock cycles (regs init to reset; each rising edge settles
combinational signals, computes next reg values, then commits all at once).
Shares the value model + expression evaluator with `comb` via
`crates/mimz-sim/src/sim/value/`.

| Test                                                                  | Locks in                                                                                                  |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `a_clocked_assert_that_fires_stops_the_tick_with_s0501`               | an `assert` inside `on rise(clk)` that goes false halts the tick with `S0501`                             |
| `a_clocked_assert_that_never_fires_never_fails`                       | a clocked `assert` that always holds never errors, across repeated ticks                                  |
| `a_clocked_cover_tallies_one_hit_per_true_edge`                       | a clocked `cover` tallies exactly once per rising edge where the condition is true                        |
| `a_comb_cover_tallies_once_per_settle`                                | a combinational `cover` tallies once per settle, not per read                                             |
| `counter_counts_and_resets`                                           | the counter counts 0→1→2→3 on rising edges; asserting `rst` forces it back to 0 (synchronous reset)       |
| `dual_edge_negedge_reg_captures_posedge_within_a_period`              | a `posedge` reg feeds a `negedge` reg; the rise→fall tick lets `b` see the new `a` same period (A3)       |
| `memory_write_then_read_round_trips_a_cell`                           | a `mem` cell reads init until written, then holds the clocked value; another cell still reads init (A4)   |
| `regs_init_to_their_reset_value`                                      | before any tick a reg holds its (non-zero) folded reset value                                             |
| `bit_indexed_register_write_sets_one_bit`                             | BUG-8: `shift[i] <- v` on a plain register sets that bit, leaving the rest untouched                      |
| `slice_indexed_register_write_sets_a_range`                           | BUG-8: `r[hi:lo] <- v` replaces that bit range, keeping bits outside it from the prior value              |
| `disjoint_bit_indexed_writes_in_one_on_block_combine`                 | BUG-8: two `reg[i] <- v` writes to disjoint bits of the same register in one `on` block both take effect  |
| `wraps_at_declared_width`                                             | `+%` on a `bits[2]` reg wraps 3→0 - width masking on the next value                                       |
| `two_phase_commit_swaps_registers`                                    | `a <- b; b <- a` SWAPS (non-blocking): each reads the OLD value, proving the two-phase commit             |
| `statement_if_picks_the_next_value`                                   | a statement-level `if` in the `on` block selects the reg's next value from the current state              |
| `snapshot_covers_every_signal`                                        | `snapshot()` lists leaves (clk/rst/inputs), regs, and combinational outputs - the VCD/trace seam          |
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
