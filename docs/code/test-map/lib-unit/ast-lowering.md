# Unit: AST lowering passes (`crates/mimz-core/src/ast/`, 22 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Four sugar constructs never reach the emitter or the simulator as
themselves - a shared lowering function rewrites each into primitives that
both back ends already understand. These tests pin the SHAPE of that
rewrite, which is what makes "the emitter and the simulator agree" cheap
(there is only one implementation to agree with).

## ast/foreach_lower.rs (10 tests)

`foreach` has two forms: the RANGE form (`foreach i in 0..8`) is pure
sugar for `repeat`, and the ELEMENTS form (`foreach v in values`) also
substitutes the loop variable with `values[i]` throughout the body.

| Test                                                               | Locks in                                                                             |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| `range_form_lowers_to_repeat_unchanged`                            | the range form is byte-for-byte the `repeat` it desugars to                          |
| `elements_form_resolves_array_port_length`                         | the elements form reads its iteration count from the array's declared length         |
| `elements_form_on_undeclared_name_returns_none`                    | an unresolvable source returns `None` (the checker's E0417 already failed the build) |
| `seq_elements_form_substitutes_var_with_index_expr`                | inside an `on` block, `v` is replaced by `values[i]` everywhere it appears           |
| `fn_elements_form_resolves_via_own_param_and_binds_with_let`       | inside a `fn`, the array parameter supplies the length and the body binds via `let`  |
| `fn_elements_form_on_undeclared_param_returns_none`                | …and an unknown parameter returns `None` rather than guessing                        |
| `loop_var_shadowing_outer_foreach_var_is_not_substituted`          | an inner `loop` reusing the name SHADOWS it - no substitution leaks in               |
| `nested_repeat_var_shadowing_outer_foreach_var_is_not_substituted` | same for a nested `repeat`                                                           |
| `nested_sync_loop_body_substitutes_outer_foreach_var`              | but a nested `sync loop` (different variable) still gets the outer substitution      |
| `subst_expr_match_arm_binding_shadows_target`                      | a `match` arm binding of the same name shadows too - scoping is respected            |

## ast/sync_loop_lower.rs (3 tests)

| Test                                                        | Locks in                                                                                                                            |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `lower_produces_twelve_items_in_order`                      | one `sync loop` expands to exactly twelve primitive items (index reg, `start`/`done` handshake, ports, `on` block) in a fixed order |
| `counter_width_is_clog2_hi_not_clog2_range_when_lo_nonzero` | the index register is `clog2(hi)` wide, not `clog2(hi - lo)` - the counter counts to `hi`, so a nonzero `lo` does not shrink it     |
| `rename_expr_match_arm_binding_shadows_accumulator_name`    | a `match` arm binding named like the accumulator shadows it instead of being rewritten                                              |

## ast/sync_prim_lower.rs (4 tests)

`sync.double_flop` and `sync.pulse` are CDC (clock-domain-crossing)
primitives. They lower to ordinary registers with hidden names derived
from the call's own target, so the emitted Verilog has no special
construct at all.

| Test                                                                       | Locks in                                                                |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `double_flop_call_lowers_to_one_hidden_reg_and_a_rewritten_assign`         | the two-flop synchronizer is one hidden reg plus a rewritten assignment |
| `pulse_call_lowers_to_four_hidden_regs_two_on_blocks_and_a_rewritten_wire` | the pulse synchronizer is four regs across BOTH clock domains           |
| `two_double_flop_calls_in_the_same_on_block_both_get_lowered`              | two calls in one block both lower (no first-one-wins bug)               |
| `two_sync_prim_calls_in_one_module_get_distinct_hidden_names`              | the derived hidden names never collide                                  |

## ast/mod.rs (4 tests)

Constructor smoke tests - cheap guards that a node type still builds after
a field is added or reordered.

| Test                          | Locks in               |
| ----------------------------- | ---------------------- |
| `array_type_constructs`       | `Type::Array`          |
| `bundle_decl_node_constructs` | `BundleDecl`           |
| `func_decl_node_constructs`   | `FuncDecl`             |
| `sync_loop_node_constructs`   | `ModuleItem::SyncLoop` |

## ast/expr.rs (1 test)

| Test                            | Locks in                                                       |
| ------------------------------- | -------------------------------------------------------------- |
| `from_name_recognizes_encoding` | the builtin-name → `Encoding` lookup recognizes every spelling |
