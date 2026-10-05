# Integration: CLI (`tests/cli.rs`, 34 tests - run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The `init`, `doctor`, `completions`, `check --watch`, `ir` and `build` subcommands.
See `docs/code/13-tooling.md` for the full command reference.

| Test                                                          | Locks in                                                                                                     |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `init_scaffolds_a_project_that_passes_its_own_test`           | `mimz init myproject` creates a documented `mimz.toml` + a counter module with a passing test                |
| `init_refuses_to_clobber_a_non_empty_dir`                     | re-running `mimz init myproject` on an existing dir fails with a clean message                               |
| `doctor_reports_sections_and_pipeline_ok`                     | `mimz doctor` prints version/edition, platform, and an in-memory compile smoke test                          |
| `doctor_dev_adds_developer_section`                           | `--dev` adds the Rust/WASM/test toolchain section                                                            |
| `env_is_an_alias_for_doctor`                                  | `mimz env` produces identical output to `mimz doctor`                                                        |
| `watch_starts_and_enters_watch_mode`                          | `mimz check --watch` starts the watcher and shows the "watching N dir(s)" banner                             |
| `build_reports_an_unpinned_port_with_its_code`                | `mimz build` on a port with no pin exits 1 with `E1501` naming the port (no tools needed)                    |
| `build_reports_an_unknown_board`                              | an unknown `--board` is `E1503` and lists the presets                                                        |
| `build_reports_a_pcf_name_that_is_not_a_port`                 | a PCF line naming a non-port is `E1502`                                                                      |
| `build_reports_a_missing_toolchain`                           | a `MIMZ_OSS_CAD` with no tools is `E1504` naming `yosys`                                                     |
| `build_reports_an_extern_without_verilog`                     | an extern module with no `--extern-src` is `E1505` naming `Pll`, exit 1                                      |
| `build_resolves_config_verilog_files_against_mimz_toml`       | `[compile] verilog_files` is relative to `mimz.toml`, not the cwd (gets past E1505 to E1504)                 |
| `build_rejects_a_zero_freq`                                   | `--freq 0` is a clap usage error (exit 2)                                                                    |
| `build_accepts_a_work_folder_and_does_not_create_the_default` | `--work <dir>` is accepted and `<source dir>/build/` is never created (E1504, no tools needed)               |
| `build_flag_board_beats_config_board`                         | `--board icebreaker` wins over `[build] board = "nope"` (no E1503; pins resolve, E1504)                      |
| `build_config_board_is_used_without_the_flag`                 | `[build] board` alone supplies the preset pins (no E1501)                                                    |
| `build_flag_pcf_beats_config_pcf`                             | `[build] pcf` alone is used (E1502 on its bad name); `--pcf` wins over it                                    |
| `build_rejects_a_zero_config_freq`                            | `[build] freq = 0` is exit 1 with `error:` and `= help:`                                                     |
| `build_shows_the_source_name_of_an_unpinned_tamil_port`       | E1501 lists a Tamil port as `` `விளக்கு (villakku)` `` (source name, Verilog name in brackets)               |
| `doctor_finds_the_config_toolchain`                           | `mimz doctor` honors `[build] toolchain` (relative to `mimz.toml`): the yosys line shows the fake suite path |
| `ir_prints_the_optimized_line_form`                           | `mimz ir` prints the optimized IR in line form on stdout                                                     |
| `ir_no_opt_keeps_what_the_optimizer_removes`                  | `--no-opt` prints the IR as lowered: cells the optimizer would remove are still there                        |
| `ir_sexpr_prints_the_s_expression_form`                       | `--sexpr` switches to the s-expression dump                                                                  |
| `ir_module_picks_the_top_of_a_two_module_file`                | `--module` chooses which module of a multi-module file is lowered                                            |
| `ir_output_writes_the_file`                                   | `-o <file>` writes the IR to the file instead of stdout                                                      |
| `ir_output_to_an_unwritable_path_is_a_clean_error`            | an unwritable `-o` path is a clean error (exit 1), not a panic                                               |
| `ir_limitation_is_a_clean_error_with_an_underline`            | an IR limitation is reported as such, with the source underlined                                             |
| `ir_internal_error_names_the_bug_class`                       | an internal compiler error is labelled as a compiler bug                                                     |
| `ir_debug_adds_a_backtrace`                                   | `-d` adds a backtrace to an internal-error report                                                            |
| `ir_panic_flag_crashes_on_an_internal_error`                  | `--panic` re-raises an internal error as a real crash (exit 101)                                             |
| `ir_panic_flag_keeps_a_limitation_clean`                      | `--panic` does not turn an IR limitation into a crash                                                        |
| `ir_extern_design_prints_no_simulation_warning`               | an `extern module` design lowers to a black box with no simulation-only warning                              |
| `ir_stats_prints_both_columns_to_stderr`                      | `--stats` prints lowered and optimized cell counts on stderr, leaving stdout for the IR                      |
| `ir_stats_without_the_optimizer_has_one_column`               | `--stats --no-opt` prints only the lowered column                                                            |
