# Crate Integration: Width Rules Conformance (crates/mimz-core/tests/width_rules_conformance.rs, 2 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

| Test                                         | Locks in                                                                                                                                                       |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `shift_result_matches_the_table`             | a shift (`<<`/`>>`) width-growth conformance table (unsigned/signed LHS, single-bit LHS, signed-amount rejection) against `width_rules::shift_result` directly |
| `checker_and_simulator_agree_with_the_table` | the SAME table, checked against the checker's `Ty`-level inference and the simulator's `Val`-level evaluator — all three authorities must agree                |

These are crate-level conformance tests pinning shift-operator width semantics (BUG-30's dynamic growth formula: `lhs.width + (2^amount.width - 1)`) across all three authorities — the shared `width_rules` module, the checker, and the simulator — a cross-check, not a new feature.
