# Unit: pretty-printer (`crates/mimz-core/src/pretty/`, 11 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The AST → Min-Mozhi source printer behind `mimz translate --order
code|thamizh` and `mimz fmt`. Every test is a ROUND TRIP: print the AST,
re-parse the output, and demand the same tree - the oracle that proves the
printer is not quietly losing information.

| Test                                                                    | Locks in                                                                    |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `sync_loop_round_trips_through_pretty_print`                            | `sync loop` headers and bodies                                              |
| `sync_double_flop_call_round_trips_through_pretty_print`                | `sync.double_flop(...)` call sites                                          |
| `foreach_round_trips_through_pretty_print`                              | both `foreach` forms                                                        |
| `enum_construct_pretty_prints_with_args`                                | `Enum.Variant(payload)` construction                                        |
| `extern_module_round_trips_through_pretty_print`                        | `extern module` declarations                                                |
| `extern_module_with_verilog_alias_round_trips_through_pretty_print`     | …including the `verilog "Name"` alias                                       |
| `qualified_reference_round_trips_through_pretty_print`                  | `a.b.Name` qualified references                                             |
| `sim_speed_clause_round_trips_through_pretty_print`                     | a `sim { speed mhz(50) }` clause                                            |
| `assert_stmt_round_trips_in_a_module_body`                              | a module-body `assert(cond)` statement                                      |
| `cover_stmt_round_trips_in_a_module_body`                               | a module-body `cover(cond)` statement                                       |
| `assert_message_with_a_control_byte_round_trips_byte_identical_verilog` | an `assert` message containing a control byte prints byte-identical Verilog |
