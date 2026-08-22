# Integration: stdlib (`tests/stdlib.rs`, 11 tests - drive the lib in-process)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The importable `std.*` library: embedded resolution, trilingual alias routing,
the `[lib]` override, the `mimz eject std` core, and the regression that plain
file-relative imports still work. The 5 catalog-level unit tests
(`crates/mimz-core/src/stdlib.rs`: namespace aliases, canonical-vs-twin routing, unknown-module,
the no-transitive-imports invariant) and the 3 config-level unit tests
(`src/config.rs`: `[lib]` parse, unknown-key reject, `resolve_with_path`) back
these.

| Test                                                         | Locks in                                                                                                                                                                      |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `embedded_std_import_resolves_without_filesystem`            | `import std.fifo` loads the compiler-embedded module (synthetic `std:fifo.mimz`) AND the entry stays `files[0]` (std appended last) - `sim`/`test` elaborate `files[0]`       |
| `tamil_twin_routes_to_twin_source`                           | `சேர்க்க நூலகம்.வரிசை` routes to the **pure-Tamil twin** source (`தொகுதி வரிசை`), not the English canonical                                                                   |
| `unknown_std_module_errors_with_available_list`              | `import std.nope` is **E1202** and the message lists the available stems (`fifo`, …)                                                                                          |
| `wrong_std_arity_errors`                                     | `import std.fifo.extra` (three segments) is rejected - a std import is exactly `std.<module>` (E1202)                                                                         |
| `plain_relative_import_still_works`                          | a non-std `import helper` still resolves file-relative - the std branch is no regression                                                                                      |
| `lib_std_override_wins_over_embedded`                        | `[lib] std = "<dir>"` makes `import std.fifo` load `<dir>/fifo.mimz` (a sentinel), not the embedded `Fifo`                                                                    |
| `lib_std_override_filename_matches_eject_for_twin_spellings` | with an ejected Tamil dir, both `import std.வரிசை` and `import std.varisai` resolve to `varisai.mimz` - the override filename keys on the resolved variant, not the raw alias |
| `eject_writes_english_modules`                               | `eject_to(dir, false, false)` writes all 5 English canonical modules; `fifo.mimz` contains `module Fifo`                                                                      |
| `eject_tamil_writes_twins`                                   | `eject_to(dir, true, false)` writes the pure-Tamil twins (`varisai.mimz` contains `தொகுதி வரிசை`)                                                                             |
| `eject_refuses_overwrite_without_force`                      | a second eject over existing files fails; `force = true` overwrites                                                                                                           |
| `eject_is_all_or_nothing_on_partial_conflict`                | one pre-existing target aborts the whole eject before any other file is written - no half-vendored directory                                                                  |
