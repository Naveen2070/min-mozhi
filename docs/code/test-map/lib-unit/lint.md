# Unit: lint (`crates/mimz-core/src/lint.rs`, 5 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Style and hygiene warnings — the `mimz lint` passes (W0002 snake_case,
W0003 PascalCase, W0004 unused signal). Additive and always warning-only;
no spec or grammar change. Note the unused-signal rule (W0004) has no
dedicated unit test here — it is exercised through `mimz lint`'s own
surface rather than in this pocket.

| Test                                   | Locks in                                                  |
| -------------------------------------- | --------------------------------------------------------- |
| `snake_case_rejects_bad_names`         | a port/wire/reg named `BadStyle` or `UPPER_CASE` is W0002 |
| `snake_case_accepts_valid_names`       | `my_signal`, `data_bus_0` pass with no warning            |
| `pascal_case_rejects_bad_names`        | a module named `bad_style` or `UPPER_MODULE` is W0003     |
| `pascal_case_accepts_valid_names`      | `MyModule`, `TrafficLight` pass with no warning           |
| `lint_empty_file_produces_no_warnings` | no lints fire on a file with zero items                   |
