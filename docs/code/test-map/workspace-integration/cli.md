# Integration: CLI (`tests/cli.rs`, 6 tests — run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The new `init`, `doctor`, `completions`, and `check --watch` subcommands.
See `docs/code/13-tooling.md` for the full command reference.

| Test                                                | Locks in                                                                                      |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `init_scaffolds_a_project_that_passes_its_own_test` | `mimz init myproject` creates a documented `mimz.toml` + a counter module with a passing test |
| `init_refuses_to_clobber_a_non_empty_dir`           | re-running `mimz init myproject` on an existing dir fails with a clean message                |
| `doctor_reports_sections_and_pipeline_ok`           | `mimz doctor` prints version/edition, platform, and an in-memory compile smoke test           |
| `doctor_dev_adds_developer_section`                 | `--dev` adds the Rust/WASM/test toolchain section                                             |
| `env_is_an_alias_for_doctor`                        | `mimz env` produces identical output to `mimz doctor`                                         |
| `watch_starts_and_enters_watch_mode`                | `mimz check --watch` starts the watcher and shows the "watching N dir(s)" banner              |
