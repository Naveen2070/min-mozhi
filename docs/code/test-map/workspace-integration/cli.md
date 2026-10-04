# Integration: CLI (`tests/cli.rs`, 6 tests - run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The new `init`, `doctor`, `completions`, and `check --watch` subcommands.
See `docs/code/13-tooling.md` for the full command reference.

| Test                                                          | Locks in                                                                                       |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `init_scaffolds_a_project_that_passes_its_own_test`           | `mimz init myproject` creates a documented `mimz.toml` + a counter module with a passing test  |
| `init_refuses_to_clobber_a_non_empty_dir`                     | re-running `mimz init myproject` on an existing dir fails with a clean message                 |
| `doctor_reports_sections_and_pipeline_ok`                     | `mimz doctor` prints version/edition, platform, and an in-memory compile smoke test            |
| `doctor_dev_adds_developer_section`                           | `--dev` adds the Rust/WASM/test toolchain section                                              |
| `env_is_an_alias_for_doctor`                                  | `mimz env` produces identical output to `mimz doctor`                                          |
| `watch_starts_and_enters_watch_mode`                          | `mimz check --watch` starts the watcher and shows the "watching N dir(s)" banner               |
| `build_reports_an_unpinned_port_with_its_code`                | `mimz build` on a port with no pin exits 1 with `E1501` naming the port (no tools needed)      |
| `build_reports_an_unknown_board`                              | an unknown `--board` is `E1503` and lists the presets                                          |
| `build_reports_a_pcf_name_that_is_not_a_port`                 | a PCF line naming a non-port is `E1502`                                                        |
| `build_reports_a_missing_toolchain`                           | a `MIMZ_OSS_CAD` with no tools is `E1504` naming `yosys`                                       |
| `build_reports_an_extern_without_verilog`                     | an extern module with no `--extern-src` is `E1505` naming `Pll`, exit 1                        |
| `build_resolves_config_verilog_files_against_mimz_toml`       | `[compile] verilog_files` is relative to `mimz.toml`, not the cwd (gets past E1505 to E1504)   |
| `build_rejects_a_zero_freq`                                   | `--freq 0` is a clap usage error (exit 2)                                                      |
| `build_accepts_a_work_folder_and_does_not_create_the_default` | `--work <dir>` is accepted and `<source dir>/build/` is never created (E1504, no tools needed) |
