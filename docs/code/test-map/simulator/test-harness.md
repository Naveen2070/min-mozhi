# Unit: test harness (`crates/mimz-sim/src/sim/harness/`, 25 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

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
