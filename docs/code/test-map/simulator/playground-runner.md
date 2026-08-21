# Unit: playground runner (`crates/mimz-sim/src/runner.rs`, 14 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

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
| `playground_test_reports_cover_hits`                    | `test` reports `cover` hit counts in its output text                          |
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
