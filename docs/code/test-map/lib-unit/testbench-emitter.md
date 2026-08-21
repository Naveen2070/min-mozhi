# Unit: testbench emitter (`crates/mimz-core/src/emit_verilog/testbench.rs`, 5 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

| Test                                                   | Locks in                                                                                                                                                         |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sanitize_verilog_ident_replaces_invalid_chars`        | spaces/symbols/leading digits/empty string all sanitize to a valid Verilog identifier                                                                            |
| `expect_guard_uses_case_inequality_not_plain_negation` | an `expect` guard emits case-inequality (`!== 1'b1`) against the condition, not the old plain-negation shape that silently passes on x                           |
| `test_env_falls_back_to_module_param_defaults`         | `--emit-testbench` resolves a width expression for a module parameter the test never overrides, from its `default` (BUG-3)                                       |
| `test_env_chains_earlier_args`                         | a test's later `(NAME: expr, …)` arg may reference an earlier one in the same list, e.g. `DOUBLE: WIDTH * 2`                                                     |
| `colliding_sanitized_test_names_are_rejected`          | two tests whose names sanitize to the same Verilog module id (`"edge case"`/`"edge_case"` -> `edge_case_tb`) error instead of emitting duplicate modules (BUG-4) |
