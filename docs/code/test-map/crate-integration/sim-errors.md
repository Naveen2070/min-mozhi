# Integration: sim runtime errors (`crates/mimz-sim/tests/sim_errors.rs`, 81 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The contract test for the `S0xxx` runtime catalog
([`13-tooling.md`](13-tooling.md#s0xxx--runtime-diagnostic-codes-r2-docsauditreview-2026-07-17md)).
**One test per live code, plus one completeness guard** —
`every_sim_code_has_a_fixture_above` fails if `ALL_SIM_CODES` gains an
entry with no firing fixture, so a new runtime code cannot ship uncovered.

Each test is named for the code it fires (`s0102_ambiguous_bare_reference`,
`s0238_combinational_cycle_fires_with_its_own_code`, …) and asserts BOTH
that the operation fails and that the failure carries exactly that code —
a code silently downgraded at a trait boundary (BUG-27) fails the test.

Unlike `tests/errors.rs`, these call straight into `mimz-sim`'s public API
rather than shelling out to the binary: most `S0xxx` conditions are ALSO
rejected by the checker, so a fixture routed through the real CLI would
stop at the checker gate and never reach the runtime code it exists to
exercise. That trade-off is recorded in the test file's own module doc.

| Group                                | Tests | Covers                                                                                   |
| ------------------------------------ | ----: | ---------------------------------------------------------------------------------------- |
| `s0102`–`s0136`                      |    26 | elaboration and wiring: reference resolution, ports, `repeat`, bit drives                |
| `nested_repeat_elaborates`           |     1 | a positive `repeat`-nesting regression, not tied to a specific S-code                    |
| `s0137`–`s0139`                      |     3 | in-memory `import` resolution (the playground's single-source path)                      |
| `s0201`–`s0229`                      |    29 | expression evaluation: widths, indexes, `fn` calls, builtins                             |
| `s0230`–`s0240`                      |    11 | the combinational-only evaluator (`mimz eval`), `Sim::set`, and signedness-mismatch adds |
| `s0301`–`s0305`                      |     5 | test-harness control flow (`tick`, `sim { speed … }`)                                    |
| `s0401`–`s0404`                      |     4 | peripheral bind errors                                                                   |
| `s0501`                              |     1 | a clocked `assert` that fires                                                            |
| `every_sim_code_has_a_fixture_above` |     1 | the completeness guard itself                                                            |
