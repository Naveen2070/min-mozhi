# Integration: test (`tests/test_run.rs`, 9 tests - run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

End-to-end `mimz test`: exit codes, the teaching message, `--filter`, `--trace`,
the cycle-limit guard, and the thamizh-order test header (B7).

| Test                                                        | Locks in                                                                                                                                                                                           |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `a_passing_test_exits_zero`                                 | a passing block prints `ok` + the summary and exits 0                                                                                                                                              |
| `mimz_test_prints_a_coverage_summary`                       | a passing test with a `cover(...)` in the design prints a `cover` line in the summary output                                                                                                       |
| `a_tick_count_over_the_cycle_limit_errors_fast_not_hangs`   | SEC: `tick(clk, n)` past `MAX_SIM_CYCLES` (1_000_000) fails fast with a clean error - no untrusted-input frame-push DoS                                                                            |
| `a_failing_expect_exits_nonzero_with_a_teaching_message`    | a failing block prints `FAIL` + the expression/operands and exits 1                                                                                                                                |
| `the_filter_selects_tests_by_name`                          | `--filter` runs only the matching test (skips the failing other one)                                                                                                                               |
| `trace_shows_a_per_cycle_table`                             | `--trace` prints the per-cycle table for a test                                                                                                                                                    |
| `a_file_with_no_tests_is_reported`                          | a file with no `test` blocks reports cleanly and exits 0                                                                                                                                           |
| `a_thamizh_order_test_header_runs_like_its_code_order_twin` | a fully thamizh-order, all-tanglish program (`yetram(clk) pothu` + `M(args) kaaga "…" sodhanai`) runs and passes (the B7 oracle)                                                                   |
| `a_negative_test_input_drives_its_twos_complement_pattern`  | BUG-43: `p = -9` on a `signed[6]` port drives the real two's-complement bit pattern (55), not a raw-masked wrong value (23) - the harness's `Sim::set` drive path, across the whole negative table |
