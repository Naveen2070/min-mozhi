# Error fixtures (`tests/errors.rs`, 4 tests — run the real binary on broken code)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

End-to-end **failure** validation, the mirror of the checker unit tests: those
prove the checker _function_ rejects bad code; these prove the _CLI_ surfaces it.
`tests/fixtures/errors/*.mimz` holds ~119 intentionally-broken files (kept OUT of
`examples/`, which is asserted valid), each declaring its expected code in a
`// expect: Exxxx` header. Source bodies are lifted from `crates/mimz-core/src/checker/tests.rs`.

| Test                                           | Locks in                                                                                                                                                             |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_error_fixture_reports_its_code`         | each fixture, run through `mimz check`, exits non-zero AND prints `error[<code>]` to stderr — the rendered code is the stable user-facing contract, checked for real |
| `error_corpus_covers_every_checker_code`       | completeness guard: every code in `ALL_CHECKER_CODES` (the 75 stable checker codes) has at least one fixture — a new E-code can't ship without an end-to-end fixture |
| `checker_code_list_matches_the_catalog`        | `ALL_CHECKER_CODES` must equal the 11-checker.md catalog table (reserved rows exempt) — the corpus, the docs, and the code can't drift apart                         |
| `json_flag_emits_machine_readable_diagnostics` | the `--json` wire format (docs/code/06): one JSON array on stdout with code/path/line/help; lexer errors included; `[]` + exit 0 on success                          |

`every_error_fixture_reports_its_code` also asserts a `help:` line per
fixture — the teaching contract, proven at the CLI surface.

Coverage is **every distinct edge case**, not one per code: E0302 missing-input
AND duplicate-conn; E0407 extend-narrowing AND `-` on bits; E0303 all eight
forbidden declaration kinds; E0601 enum/`bits[N]`/`bit`; E0701's three crossings;
etc. The assertion is "stderr _contains_ the code", tolerant of a fixture that
incidentally trips a second rule. Convention + how-to: `tests/fixtures/errors/README.md`.
