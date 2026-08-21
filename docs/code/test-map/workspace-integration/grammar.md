# Integration: grammar engine (`tests/grammar.rs`, 16 tests — run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The `syntax thamizh` word-order profile (spec/04, Phase 1.8). Oracle = the
profile-blind backend: a thamizh-order file and its code-order twin must emit
byte-identical Verilog, so equal Verilog proves the same AST. Fixtures live in
`tests/fixtures/grammar/` (not `examples/`, which stays byte-identical
four-flavor per R9).

| Test                                                  | Locks in                                                                                               |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `thamizh_order_counter_matches_code_order_twin`       | Tanglish `rise(clk) on { }` → same Verilog as code-order twin                                          |
| `thamizh_order_tamil_counter_matches_code_order_twin` | pure Tamil script + SOV order → same Verilog as the Tamil twin                                         |
| `thamizh_order_agrees_with_english_golden`            | profile and keyword skin are fully orthogonal                                                          |
| `thamizh_order_blinker_matches_code_order_twin`       | seq conditional `<cond> enil { } illaiyenil { }` → same Verilog                                        |
| `thamizh_order_blinker_tamil_matches_code_order_twin` | the conditional flip in pure Tamil script → same Verilog                                               |
| `thamizh_order_blinker_agrees_with_english_golden`    | conditional flip is invisible to the backend (English golden)                                          |
| `thamizh_order_comparator_matches_code_order_twin`    | if-expression `c enil { } illaiyenil { }` → same Verilog                                               |
| `thamizh_order_match_matches_code_order_twin`         | match `<expr> thernthedu { }` → same Verilog (self-contained pair)                                     |
| `traffic_light_tamil_thamizh_matches_code_order_twin` | Tamil thamizh-order FSM (all four flips at once) → same Verilog; the committed `pretty`-built artifact |
| `unknown_syntax_profile_is_an_error`                  | `syntax wibble` fails to compile with E1112                                                            |
| `flipped_on_block_is_rejected_in_code_order`          | the clocked-block flip is gated on the profile                                                         |
| `flipped_conditional_is_rejected_in_code_order`       | `<cond> enil { }` rejected without the directive                                                       |
| `flipped_if_expr_is_rejected_in_code_order`           | `a > b enil { } illaiyenil { }` rejected without the directive                                         |
| `flipped_match_is_rejected_in_code_order`             | `op thernthedu { }` rejected without the directive                                                     |
| `code_order_if_is_rejected_in_thamizh`                | leading `enil` (code order) in a thamizh file errors — symmetric profile boundary                      |
| `deeply_nested_thamizh_else_if_errors_not_overflows`  | deep thamizh `illaiyenil … enil` chain → clean E1113, no stack overflow (SEC-1 guard on the flip path) |
