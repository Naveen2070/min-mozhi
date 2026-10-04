# Unit: build (`src/build/`, 15 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

`mimz build`'s I/O half (synthesis v1 phase 1 Task 5): board presets, PCF
pin files, and the OSS CAD Suite toolchain.

| Test                                                    | Locks in                                                                                    |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `icebreaker_is_up5k_sg48_at_12_mhz`                     | the iCEBreaker preset: `up5k`, `sg48`, 12 MHz, `clk` on pin 35; `names()` lists it          |
| `an_unknown_board_is_none`                              | an unknown board name is `None`, never a default                                            |
| `parse_pcf_reads_set_io_lines_and_skips_comments`       | `set_io` lines parse; `#` comments, blank lines and `-nowarn` flags are skipped             |
| `a_malformed_pcf_line_is_an_error`                      | a short `set_io` line is `BadLine` with its 1-based line number                             |
| `the_board_preset_pins_a_port_with_a_preset_name`       | a port named like a preset entry (`clk`) gets the board's pin                               |
| `a_user_pcf_overrides_the_preset`                       | a user PCF entry wins over the preset                                                       |
| `a_port_with_no_pin_is_reported_by_name`                | an unpinned port is `Unpinned` naming it                                                    |
| `a_pcf_name_that_is_not_a_port_is_an_error`             | a PCF name that is not a top-level port is `NotAPort` (typo guard)                          |
| `every_bit_of_a_vector_port_needs_a_pin`                | a vector port needs `name[i]` for every bit; the missing bit is named                       |
| `a_pcf_can_name_a_port_by_its_source_or_verilog_name`   | a Tamil port can be pinned by its source or its romanized Verilog name                      |
| `write_pcf_writes_one_set_io_per_pin`                   | the PCF written for nextpnr is one `set_io NAME PIN` per pin                                |
| `a_suite_root_resolves_tools_under_its_bin`             | with a suite root, a tool is `<root>/bin/<tool><exe suffix>`                                |
| `a_suite_root_puts_bin_and_lib_first_on_the_child_path` | the child's PATH starts with `<root>/bin`, `<root>/lib`; `YOSYSHQ_ROOT` is set (child only) |
| `a_missing_tool_is_none`                                | a tool absent from the suite root is `None`                                                 |
| `the_suite_datdir_is_share_yosys`                       | Yosys's data dir under a suite root is `<root>/share/yosys` (holds `ice40/cells_sim.v`)     |
