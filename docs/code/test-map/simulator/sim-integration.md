# Integration: sim (`tests/sim.rs`, 17 tests - run the real binary + lib in-process)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

End-to-end `mimz sim` over a counter (clocked) and an adder (combinational): the
stimulus, the VCD, the console trace, the `--sweep`; plus the B8 kernel perf
baseline and the golden VCD byte-lock (both run the lib in-process).

| Test                                                                    | Locks in                                                                                                |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `trace_table_shows_a_row_per_cycle`                                     | `--trace` prints the per-cycle table (header + separator + N rows)                                      |
| `cycles_over_the_limit_is_rejected_by_the_cli`                          | SEC: `--cycles` past `MAX_SIM_CYCLES` (1_000_000) is rejected at clap parse time - no unbounded loop    |
| `changes_trace_is_monitor_style`                                        | `--trace=changes` prints `$monitor`-style lines (reaches `count=3`)                                     |
| `writes_a_gtkwave_vcd`                                                  | `-o` writes a VCD with `$timescale`/`$enddefinitions`/`$dumpvars`/`count`                               |
| `signals_flag_limits_the_trace`                                         | `--signals count` shows only `count`, excluding `value`                                                 |
| `a_combinational_module_settles_one_frame`                              | C1: a clockless module simulates - `--in a=200,b=100` → one settled frame, `sum=300`                    |
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
