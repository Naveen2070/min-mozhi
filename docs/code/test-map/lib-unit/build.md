# Unit: build (`src/build/`, 26 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

`mimz build`'s I/O half (synthesis v1 phase 1 Tasks 5-6): board presets, PCF
pin files, the OSS CAD Suite toolchain, and the Yosys/nextpnr/icepack flow
(its pure parts; the real-toolchain flow is tested in Task 7).

| Test                                                                           | Locks in                                                                                                              |
| ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| `icebreaker_is_up5k_sg48_at_12_mhz`                                            | the iCEBreaker preset: `up5k`, `sg48`, 12 MHz, `clk` on pin 35; `names()` lists it                                    |
| `an_unknown_board_is_none`                                                     | an unknown board name is `None`, never a default                                                                      |
| `parse_pcf_reads_set_io_lines_and_skips_comments`                              | `set_io` lines parse; `#` comments, blank lines and `-nowarn` flags are skipped                                       |
| `a_malformed_pcf_line_is_an_error`                                             | a short `set_io` line is `BadLine` with its 1-based line number                                                       |
| `the_board_preset_pins_a_port_with_a_preset_name`                              | a port named like a preset entry (`clk`) gets the board's pin                                                         |
| `a_user_pcf_overrides_the_preset`                                              | a user PCF entry wins over the preset                                                                                 |
| `a_port_with_no_pin_is_reported_by_name`                                       | an unpinned port is `Unpinned` naming it                                                                              |
| `a_pcf_name_that_is_not_a_port_is_an_error`                                    | a PCF name that is not a top-level port is `NotAPort` (typo guard)                                                    |
| `every_bit_of_a_vector_port_needs_a_pin`                                       | a vector port needs `name[i]` for every bit; the missing bit is named                                                 |
| `a_pcf_can_name_a_port_by_its_source_or_verilog_name`                          | a Tamil port can be pinned by its source or its romanized Verilog name                                                |
| `write_pcf_writes_one_set_io_per_pin`                                          | the PCF written for nextpnr is one `set_io NAME PIN` per pin                                                          |
| `a_suite_root_resolves_tools_under_its_bin`                                    | with a suite root, a tool is `<root>/bin/<tool><exe suffix>`                                                          |
| `a_suite_root_puts_bin_and_lib_first_on_the_child_path`                        | the child's PATH starts with `<root>/bin`, `<root>/lib`; `YOSYSHQ_ROOT` is set (child only)                           |
| `a_missing_tool_is_none`                                                       | a tool absent from the suite root is `None`                                                                           |
| `the_suite_datdir_is_share_yosys`                                              | Yosys's data dir under a suite root is `<root>/share/yosys` (holds `ice40/cells_sim.v`)                               |
| `the_yosys_script_reads_every_file_and_synthesizes_the_top`                    | `flow.rs`: the exact Yosys script (read every file, `synth_ice40 -top`, JSON/Verilog/stat outputs)                    |
| `noabc_is_passed_to_synth_ice40`                                               | `flow.rs`: `-noabc` goes right after `synth_ice40` when asked (Windows workaround)                                    |
| `max_frequency_lines_are_picked_from_the_nextpnr_log`                          | `flow.rs`: `Max frequency for clock` lines become `clk: 80.62 MHz (PASS at 12.00 MHz)`                                |
| `only_the_last_max_frequency_line_per_clock_survives_with_its_suffix_stripped` | `flow.rs`: the post-route line wins over the pre-route one (PASS then FAIL keeps FAIL); `clk$...` becomes `clk`       |
| `clocks_that_differ_only_after_a_dollar_are_kept_apart`                        | `flow.rs`: clocks are keyed by the full nextpnr name (`clk$a`, `clk$b` are two lines), displayed shortened            |
| `a_quote_in_an_extern_path_is_rejected_before_anything_runs`                   | `flow.rs`: an extern `.v` path with `"` is `QuoteInPath` before the work folder is created (Yosys `-p` has no escape) |
| `the_flag_beats_the_config_which_beats_the_board_which_beats_12`               | `commands/build.rs`: `--freq` > `[build] freq` > board MHz > 12                                                       |
| `a_pcf_name_that_is_a_port_but_not_a_pin_gets_a_bit_hint`                      | `commands/build.rs`: E1502 hint for a whole vector name, a bit past the end, or an index on a 1-bit port              |
| `two_different_pins_for_one_port_are_a_conflict`                               | `pins.rs`: a source and a Verilog spelling of one port with different pins is `Conflict`                              |
| `the_same_pin_written_twice_is_not_a_conflict`                                 | `pins.rs`: a repeated identical `set_io` is accepted                                                                  |
| `the_missing_tool_help_matches_where_the_tools_were_looked_for`                | `commands/build.rs`: E1504 help names the suite folder when a root is set, PATH otherwise                             |
| `the_target_line_names_the_chip_even_without_a_board`                          | `commands/build.rs`: `target: up5k sg48 ...` printed with or without `--board` (default noted)                        |
| `a_renamed_port_shows_both_spellings`                                          | `commands/build.rs`: E1501 labels `விளக்கு (villakku)`, `ப[1] (pa[1])`; an unrenamed port stays plain                 |
