# Unit: elaboration (`crates/mimz-sim/src/sim/elaborate/`, 26 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Phase 1.5 steps B1 + C2–C4: flatten an AST module (and its instances) into a
`Design` (signals with folded widths, regs with folded reset + clock, comb
drivers, sequential processes), via the `elaborate_project` flattener and the
`Rw` elaborate-time rewriter (enum→index, `repeat` unroll, instance flattening,
array instances, bit-indexed drives). The event-driven kernel interprets a
`Design`.

Split into `mod.rs` (entry + `Design`), `module.rs` (one module's items),
`instance.rs` (instance flattening), `registry.rs` (module/bundle/enum
lookup), `rewrite.rs` (the `Rw` rewriter), `bundle.rs` (bundle field
expansion), and `tests.rs`.

| Test                                                                     | Locks in                                                                                                                                                     |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `a_module_body_assert_is_collected_into_design_asserts`                  | a module-body `assert(a)` (outside any `on` block) is collected into `Design.asserts`                                                                        |
| `a_module_body_cover_is_collected_into_design_covers`                    | a module-body `cover(a)` is collected into `Design.covers`                                                                                                   |
| `elaborates_the_counter`                                                 | the canonical counter flattens correctly: one reg (`value`, reset 0, clock `clk`), the `count` comb driver, `clk`/`rst` recorded, one process                |
| `param_override_folds_widths`                                            | passing `WIDTH=4` folds the reg and output widths to 4                                                                                                       |
| `elaborates_a_combinational_module`                                      | a clockless module has empty regs/procs/clocks/resets, only comb drivers                                                                                     |
| `reg_takes_a_nonzero_folded_reset_value`                                 | `reg r: bits[8] = 5` folds the reset to 5 and binds the reg to its `on`-block clock                                                                          |
| `flattens_a_same_file_instance`                                          | C2: `Top`'s `let u = Add()` inlines the child's signals prefixed `u_*`; the `u.s` field-read resolves to the flattened `u_s` wire                            |
| `rejects_unknown_instance_module`                                        | C2: a `let` instance of a module that doesn't exist is a clean `S0105` error                                                                                 |
| `two_same_named_modules_flatten_their_own_instance`                      | cross-file name reuse: each instance flattens the module from ITS OWN file                                                                                   |
| `ambiguous_bare_module_reference_errors_instead_of_silently_picking_one` | a bare name matching two files is `S0102`, never an arbitrary pick                                                                                           |
| `qualified_instance_reference_resolves_via_a_real_import_path`           | `a.b.Mod()` resolves through the written `import`, not a guess                                                                                               |
| `unrolls_repeat_with_instance_array_and_bit_drives`                      | C3: `repeat` inlines one child per bit (`fa__<i>`); the per-bit `s[i] = …` drives assemble into a whole-signal Concat                                        |
| `elaborates_an_enum_signal_and_match`                                    | C4: an enum reg gets width `clog2(variants)`, its reset folds to the variant index, and a `match` over the enum elaborates (patterns → indices)              |
| `unrolls_foreach_range_form_same_as_repeat`                              | `foreach i in 0..2` elaborates identically to the equivalent `repeat i: 0..2` (pure sugar)                                                                   |
| `foreach_elements_form_substitutes_var_with_mem_index`                   | elements-form `foreach v in values` over a `mem` lowers to a `Repeat` substituting `v` with `values[idx]` throughout the body                                |
| `foreach_nested_inside_if_in_on_block_lowers_via_recursion`              | a `foreach` nested inside an `if` inside `on rise(clk)` still lowers - the seq-lowering pass recurses into `If`'s `then` body, not just top-level statements |
| `bundle_typed_instance_input_port_connection_flattens_per_field`         | BUG-15: a bundle-typed instance input expands to one connection per field                                                                                    |
| `bundle_typed_fn_call_argument_expands_to_one_arg_per_field`             | BUG-15: the same expansion for a bundle passed to a `fn`                                                                                                     |
| `sync_loop_timing_and_no_mid_run_retrigger`                              | a `sync loop` runs its full iteration count and ignores a `start` pulse mid-run                                                                              |
| `sync_loop_nested_in_const_if_elaborates_and_ticks`                      | a `sync loop` inside the WINNING `const if` branch elaborates and runs                                                                                       |
| `sync_loop_in_const_if_losing_branch_is_not_lowered`                     | …and one inside the losing branch generates no hardware at all                                                                                               |
| `recursive_instantiation_errors_not_overflows`                           | SEC-6: a self-instantiating module hits `MAX_INSTANCE_DEPTH` (`S0119`) instead of overflowing the stack                                                      |
| `extreme_repeat_bounds_error_not_overflow`                               | SEC-6: a `repeat` span past `i128::MAX` is an over-budget error (`checked_sub`), not an overflow panic                                                       |
| `an_out_of_range_bit_index_errors`                                       | SEC-6: a bit-index drive ≥ 128 errors before the `as u32` cast (no silent truncation)                                                                        |
| `a_flatten_name_collision_errors`                                        | SEC-6: a parent signal colliding with a flattened `inst_port` wire errors (`S0128`) instead of silently overwriting                                          |
| `an_i128_min_const_elaborates_without_overflow`                          | SEC-6 (SIM-5): a flattened child const evaluating to `i128::MIN` lowers via `unsigned_abs` instead of overflow-panicking the negation in `int_expr`          |
