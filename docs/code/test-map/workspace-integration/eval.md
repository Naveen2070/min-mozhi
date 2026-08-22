# Integration: eval (`tests/eval.rs`, 15 tests - run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

End-to-end `mimz eval` over corpus examples - proves the lib evaluator AND the
`--in`/`--module` plumbing. The security cases matter because the `eval` path
skips the checker, so `comb.rs` is the only overflow guard (audit SEC-2).

| Test                                                                            | Locks in                                                                           |
| ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `adder_carries`                                                                 | `mimz eval adder --in a=200,b=100` prints `sum = 300`                              |
| `mux4_selects_with_hex_and_binary_inputs`                                       | `--in sel=0b10,...` parses bases; selects the right input                          |
| `comparator_reports_all_three_outputs`                                          | all three outputs print with correct values                                        |
| `window_chained_comparison_boundaries`                                          | inclusive boundary in / below out                                                  |
| `arithmetic_builtins_compute_min_max_abs_and_negated_reductions`                | `min`/`max`/`abs`/`nand`/`nor`/`xnor` evaluate correctly                           |
| `fn_call_guard_clause_return_short_circuits`                                    | an early `return` in a `fn` guard clause wins                                      |
| `fn_call_guard_clause_falls_through_to_tail`                                    | …and falls through to the tail expression when it does not fire                    |
| `fn_call_with_array_literal_argument_indexes_by_constant`                       | an array literal argument indexes at a constant position                           |
| `fn_call_with_array_argument_indexes_by_runtime_value`                          | …and at a runtime-computed position                                                |
| `fn_call_with_array_argument_out_of_range_runtime_index_clamps_to_last_element` | an out-of-range RUNTIME index clamps (matching the emitted mux), it does not error |
| `multi_module_file_needs_module_flag`                                           | a 2-module file asks for `--module`, then accepts it                               |
| `instances_are_rejected_clearly`                                                | a file with sub-module instances is rejected with a clear message                  |
| `oversized_shift_const_does_not_panic`                                          | `a[1 << 200]` → clean overflow error, no panic/wrap (debug+release)                |
| `overflowing_multiply_const_does_not_panic`                                     | a const product past i128::MAX → overflow error, not a panic                       |
| `out_of_range_index_is_rejected_cleanly`                                        | a literal index past the width → clean error, not a truncating cast                |
