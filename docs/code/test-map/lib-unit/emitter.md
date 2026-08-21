# Unit: emitter (`crates/mimz-core/src/emit_verilog/`, 93 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The emitter's units live in five places, plus two sibling files documented
on their own pages ([`transliteration.md`](transliteration.md)'s
`translit.rs`, 7 tests; [`testbench-emitter.md`](testbench-emitter.md)'s
`testbench.rs`, 5 tests — neither is counted below, to avoid double-counting
against this page's total). `mod.rs`'s own single-file test module was split
into `emit_verilog/tests/` (topic files, 63 tests across 11 files) on the
`oversized-test-file-split` branch; the remaining four are small pockets
inside the file they test.

| Location                          | Tests | Covers                                                                         |
| --------------------------------- | ----: | ------------------------------------------------------------------------------ |
| `emit_verilog/tests/` (11 files)  |    63 | end-to-end emission behavior, by topic (tables below)                          |
| `emit_verilog/module/tests.rs`    |    10 | `build_decls` internals + `sync.*`/`sync loop`/`assert`/`cover` lowering shape |
| `emit_verilog/kinds.rs`           |    16 | `infer_kind` — mimz's own width/signedness for an expression                   |
| `emit_verilog/self_determined.rs` |     3 | what real Verilog would self-determine for the same expression                 |
| `emit_verilog/expr.rs`            |     1 | the `is_plain_identifier` hoist predicate                                      |

## emit_verilog/tests/builtin_and_loops.rs (9 tests)

| Test                                                        | Locks in                                                                                      |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `repeat_unrolls_drives_with_folded_indices`                 | `repeat i: 0..4 { y[i] = … }` emits `assign y[0..3]`; the half-open range stops at 3          |
| `repeat_var_folds_in_index_arithmetic`                      | `y[i + 1]` folds to `y[1]`/`y[3]` — index arithmetic over the loop var collapses to a literal |
| `empty_and_reversed_ranges_emit_nothing`                    | `0..0` and `4..0` generate no hardware (no crash, no partial output)                          |
| `repeat_over_budget_errors_cleanly`                         | a range past `REPEAT_BUDGET` (4096) is a clean error, not a runaway unroll                    |
| `nested_repeat_folds_both_variables`                        | nested loops bind both `i` and `j` per iteration                                              |
| `repeat_instance_array_gets_flat_names`                     | `let u[i] = …` → `u__<i>` with outputs `u__<i>_<port>`                                        |
| `on_block_loop_unrolls_to_n_copies`                         | `loop` inside an `on` block emits one statement copy per iteration                            |
| `on_block_loop_over_budget_is_rejected`                     | the same budget cap applies to `loop`, not just `repeat`                                      |
| `a_builtin_lowers_parenthesized_inside_a_larger_expression` | `abs(x) + 1` parenthesizes the builtin so precedence cannot shift                             |

## emit_verilog/tests/bundle_flatten.rs (9 tests)

BUG-73 (`docs/audit/bugs.md`): a bundle-typed wire's or an instance's
auto-wired output's flattened field name never checked whether that name was
already taken by an ordinary port — silently producing a duplicate Verilog
declaration and a self-referential tautology instead of a diagnostic.

| Test                                                                                 | Locks in                                                                                                    |
| ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `bug_73_bundle_wire_field_colliding_with_a_port_is_a_diagnostic_not_invalid_verilog` | a bundle wire's flattened field name colliding with a real port is a diagnostic naming both colliding names |
| `bug_73_instance_auto_wire_colliding_with_a_port_is_a_diagnostic`                    | the same collision axis for an instance's auto-wired output (`{inst}_{port}`)                               |
| `bug_73_non_colliding_bundle_wire_still_flattens_normally`                           | control: a bundle wire whose flattened names are genuinely free still compiles and flattens normally        |
| `bundle_typed_port_flattens_at_instantiation`                                        | a bundle-typed port becomes one flat Verilog wire per field                                                 |
| `bundle_typed_fn_param_flattens_to_per_field_inputs`                                 | same flattening for a bundle-typed `fn` parameter                                                           |
| `bundle_port_forwarding_a_module_parameter_stays_symbolic`                           | a parametric bundle keeps its parameter expression in the declaration                                       |
| `bundle_port_forwarding_a_module_parameter_resolves_per_instance`                    | …and resolves to the concrete width at each instantiation site                                              |
| `bare_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog`                    | returning a bundle from a `fn` errors cleanly (no bogus Verilog)                                            |
| `parametric_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog`              | same for the parametric case                                                                                |

## emit_verilog/tests/valid_bundle_sugar.rs (10 tests)

The `T?` / `??` sugar — a valid-bundle is `{ valid: bit, data: T }`, and
`??` either unwraps it or OR-muxes two of them.

| Test                                                               | Locks in                                                          |
| ------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `qq_unwrap_form_emits_a_ternary_on_validity`                       | `a ?? fallback` becomes `a_valid ? a_data : fallback`             |
| `qq_unwrap_form_emits_a_ternary_via_drive`                         | same through a `=` drive                                          |
| `qq_or_mux_form_emits_per_field_ternaries_at_wire_init`            | `a ?? b` between two valid-bundles muxes each field separately    |
| `qq_or_mux_form_emits_per_field_ternaries_via_drive`               | same through a drive                                              |
| `qq_or_mux_form_emits_per_field_ternaries_at_port_connection`      | same at an instance port connection                               |
| `qq_or_mux_form_expands_at_fn_call_site`                           | same as a `fn` call argument                                      |
| `qq_or_mux_chain_emits_valid_nested_verilog`                       | `a ?? b ?? c` nests without producing invalid Verilog             |
| `qq_or_mux_chain_expands_correctly_at_fn_call_site`                | …and the chain still expands correctly as an argument             |
| `user_bundle_shaped_like_valid_bundle_emits_same_as_builtin_sugar` | a hand-written `{valid, data}` bundle behaves identically to `T?` |
| `a_decimal_literal_past_128_bits_emits_correctly`                  | wide literals survive emission (the `bits`/`wide` path)           |

## emit_verilog/tests/clocking.rs (3 tests)

| Test                                      | Locks in                                                            |
| ----------------------------------------- | ------------------------------------------------------------------- |
| `on_fall_emits_negedge`                   | `on fall(clk)` lowers to `always @(negedge clk)` (A3)               |
| `async_reset_widens_the_sensitivity_list` | `async reset` lowers to `always @(posedge clk or posedge rst)` (A5) |
| `a_sync_reset_stays_clock_only`           | a plain `reset` keeps `always @(posedge clk)` — no widening (A5)    |

## emit_verilog/tests/clog2.rs (4 tests)

| Test                                                               | Locks in                                                          |
| ------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `clog2_of_a_const_derives_the_width`                               | `clog2(CONST)` folds to a literal width at compile time           |
| `clog2_folds_into_the_port_width`                                  | …including in a port declaration                                  |
| `clog2_of_a_parameter_in_a_body_width_emits_the_constant_function` | over a PARAMETER, the emitter writes a Verilog `function` instead |
| `clog2_of_a_parameter_in_a_port_is_an_emit_error`                  | but a port width cannot use it — a clean error, not bad Verilog   |

## emit_verilog/tests/consts_and_translit.rs (3 tests)

| Test                                                            | Locks in                                                                                 |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `module_const_folds_in_widths_and_emits_no_hardware`            | a `const` folds to a literal in widths and declares no Verilog of its own                |
| `tamil_identifiers_emit_as_romanized_verilog`                   | the transliterated pipeline end to end; no non-ASCII outside the banner comment          |
| `colliding_romanizations_get_suffixes_and_ascii_names_are_safe` | ந/ன clash + an existing ASCII `nii`: user names are never stolen; clashes get `_2`, `_3` |

## emit_verilog/tests/consts_scoping.rs (5 tests)

| Test                                                                      | Locks in                                                                                                                  |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `bug_71_unevaluable_const_if_condition_is_a_diagnostic_not_a_silent_else` | BUG-71: an unevaluable `const if` condition pushes a `Diag` instead of `unwrap_or(0)`-ing silently into the `else` branch |
| `child_consts_fold_into_parent_auto_wires`                                | the CHILD's const sizes the auto-wire (regression: `wire [(W)-1:0]` leaked and iverilog rejected)                         |
| `parent_const_never_substitutes_into_child_widths`                        | same const NAME in parent and child: the child's value wins                                                               |
| `two_same_named_modules_emit_their_own_bodies`                            | cross-file name reuse emits two distinct module bodies, not one shared                                                    |
| `diags_carry_the_file_index`                                              | project-level diagnostics record WHICH file they point into, so multi-file errors render right                            |

## emit_verilog/tests/extern_and_arrays.rs (3 tests)

| Test                                                                | Locks in                                                     |
| ------------------------------------------------------------------- | ------------------------------------------------------------ |
| `extern_instantiation_emits_only_the_instance_line_no_definition`   | an `extern module` is instantiated but never defined by us   |
| `extern_instantiation_uses_the_alias_when_set`                      | the `verilog "RealName"` alias is what appears in the output |
| `zero_length_array_param_runtime_index_is_a_clean_diag_not_a_panic` | a degenerate array parameter errors instead of panicking     |

## emit_verilog/tests/fn_loop.rs (4 tests)

| Test                                                 | Locks in                                                                  |
| ---------------------------------------------------- | ------------------------------------------------------------------------- |
| `fn_loop_with_return_finds_first_match`              | a `loop` + `return` inside a `fn` short-circuits at the first hit         |
| `fn_loop_with_return_first_match_wins_on_duplicate`  | with duplicates, the FIRST match wins (priority, not last-write)          |
| `emitter_injects_function_called_only_from_a_return` | a `fn` reachable only through a `return` is still inlined                 |
| `flattened_loop_shape_fails_the_nesting_assertion`   | a malformed lowered shape trips an internal assertion instead of emitting |

## emit_verilog/tests/structural_match.rs (4 tests)

Bundle compatibility is STRUCTURAL (same field names and types), not
nominal (same declared bundle name) — these prove the emitted Verilog is
byte-identical either way.

| Test                                                                   | Locks in                                   |
| ---------------------------------------------------------------------- | ------------------------------------------ |
| `structurally_matched_port_connection_emits_same_as_nominal_match`     | at an instance port connection             |
| `structurally_matched_drive_emits_same_as_nominal_match`               | at a `=` drive                             |
| `structurally_matched_fn_arg_emits_same_as_nominal_match`              | as a `fn` argument                         |
| `structurally_matched_fn_return_is_a_diagnostic_same_as_nominal_match` | and the return case errors identically too |

## emit_verilog/tests/hoist_declaration_order.rs (9 tests)

Round-7 plan Task 1 (GAP-18) widened by round-8 plan Task 2: the hoist
buffer's flush point (`hoist_pos`) is a second scoping axis alongside
`hoist_unresolved`'s own — a hoisted wire can resolve its `Kind` correctly
and still land after its own use. `assert_hoists_declared_before_use`
(`emit_verilog/mod.rs`) is the runtime invariant these tests pin.

| Test                                                                             | Locks in                                                                                                                                              |
| -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `identifiers_in_finds_whole_words_and_skips_radix_specifiers`                    | the identifier scanner finds whole-word names and skips sized-literal radix specifiers (`4'd10`)                                                      |
| `strip_instance_port_names_drops_only_the_dotted_port_half`                      | `.d(...)`/`.q(...)` (the child's own port names) are stripped; the connected signal names survive                                                     |
| `task2_widened_invariant_ignores_identifiers_inside_string_literals`             | a `$display` string literal's own text is never read as a signal "use"                                                                                |
| `task2_widened_invariant_ignores_port_names_in_instance_connections`             | BUG-70 construction 1 (post-fix): a child module's own port names (`d`/`q`) are never misread as a same-named module-level signal                     |
| `task2_widened_invariant_does_not_false_positive_on_clog2_helper_name_collision` | the injected `CLOG2_FN` helper's own local `value` param doesn't collide with an ordinary module-level `reg value`                                    |
| `task2_widened_invariant_fires_on_bug_70_construction_1`                         | BUG-70 construction 1 (pre-fix fixture): an ordinary (non-hoisted) wire used before its own declaration fires the invariant                           |
| `task2_widened_invariant_fires_on_bug_70_construction_2`                         | BUG-70 construction 2: the same axis through a `mem`-init render site instead of an instance-port connection                                          |
| `task5_declaration_order_violation_is_a_diagnostic_not_a_panic_outside_tests`    | round-8 Task 5: the `Diag` is pushed BEFORE the `cfg!(test)`-gated `debug_assert!`, so a real (non-test) binary returns an error instead of panicking |
| `task3_bug_66_a2_reg_reset_hoist_no_longer_fires_the_declaration_order_assert`   | round-8 Task 3: BUG-66's own repro (a reg reset hoist) no longer trips the declaration-order assert after the fix                                     |

## emit_verilog/module/tests.rs (10 tests)

| Test                                                     | Locks in                                                                                                            |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `comb_assert_emits_a_guarded_always_block`               | a combinational `assert` emits an `` `ifndef SYNTHESIS`` `always @(*)` block that `$fatal(1)`s on a false condition |
| `comb_assert_with_a_message_embeds_it`                   | `assert(a, "msg")`'s message string appears in the emitted Verilog                                                  |
| `comb_cover_emits_a_sensitized_counter`                  | a combinational `cover` emits a zero-initialized 32-bit hit counter that increments when sensitized                 |
| `clocked_cover_emits_an_inline_increment`                | a clocked `cover` increments its counter inline inside the `posedge` block                                          |
| `clocked_assert_emits_inline_in_the_posedge_block`       | a clocked `assert` emits its guard inline inside the `posedge` block, no separate always block                      |
| `build_decls_maps_names_to_kinds`                        | the declaration table records each signal's kind (wire/reg/mem/…)                                                   |
| `build_decls_resolves_port_and_wire_widths`              | …and its folded concrete width                                                                                      |
| `sync_loop_emits_fsm_and_ports`                          | a `sync loop` lowers to a real index reg + `start`/`done` handshake                                                 |
| `sync_double_flop_emits_a_plain_reg_chain`               | `sync.double_flop` becomes two ordinary registers — no special Verilog                                              |
| `sync_pulse_emits_a_toggle_reg_and_a_src_clock_on_block` | `sync.pulse` becomes a toggle plus an `on` block on the SOURCE clock                                                |

## emit_verilog/kinds.rs (16 tests)

`infer_kind` is the emitter-local counterpart to the checker's `Ty` — it
answers "how wide, and signed or not, is this expression?" straight from
the AST.

| Test                                                                | Locks in                                                                                                      |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `literal_gets_its_minimal_width`                                    | a bare literal is as narrow as it can be                                                                      |
| `ident_looks_up_declared_kind`                                      | an identifier takes the kind of its declaration                                                               |
| `ident_not_in_decls_is_none`                                        | an identifier missing from `decls` resolves to `None`, not a panic                                            |
| `lossless_add_grows_by_one_bit`                                     | `+` grows — the exact-widths promise                                                                          |
| `concat_sums_part_widths`                                           | `{a, b}` is `width(a) + width(b)`                                                                             |
| `concat_with_an_unresolvable_part_is_none`                          | a concat with one unresolvable part resolves the whole thing to `None`                                        |
| `wrap_add_with_a_narrower_bare_literal_adapts_to_the_sized_operand` | `x +% 1` sizes the literal to `x`, not the other way round                                                    |
| `lossless_mul_with_a_module_parameter_adapts_to_the_sized_operand`  | BUG-46: `dur * TICK` (`TICK` a module `int` param, absent from `decls`) adapts instead of resolving to `None` |
| `encoding_kind_matches_its_argument_width_unsigned`                 | `encoding(e)` reports the same width as `e`, always unsigned                                                  |
| `index_on_a_plain_vector_is_one_bit`                                | indexing an ordinary vector signal resolves to a 1-bit kind                                                   |
| `index_on_a_memory_yields_the_element_kind`                         | indexing a `mem` yields the memory's ELEMENT kind, not the vector's                                           |
| `index_on_an_unknown_name_is_none`                                  | indexing an unresolvable name is `None`, not a panic                                                          |
| `fn_call_resolves_from_the_reserved_return_kind_key`                | a `fn` call's kind resolves via the reserved return-kind key in `decls`                                       |
| `field_on_an_instance_resolves_from_the_mangled_port_key`           | `inst.field` resolves via the mangled `{inst}_{port}` key                                                     |
| `if_expr_resolves_from_either_branch`                               | an if-expression's kind resolves from whichever branch has one                                                |
| `if_expr_is_none_when_neither_branch_resolves`                      | …and is `None` only when NEITHER branch resolves                                                              |

## emit_verilog/self_determined.rs (3 tests)

The mirror of `kinds.rs`: what real Verilog's own self-determined-width
rule computes for the same expression. Where the two disagree, the
emitter hoists the subexpression into an explicitly-sized wire (BUG-19/20).

| Test                                                           | Locks in                                                                       |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `lossless_sub_self_determines_to_max_operand_width_not_growth` | Verilog does NOT grow `-`; this is exactly the disagreement that needs a hoist |
| `comparison_has_no_verilog_specific_rule`                      | comparisons agree — no hoist needed                                            |
| `plain_identifier_has_no_verilog_specific_rule`                | a bare identifier agrees — no hoist needed                                     |

## emit_verilog/expr.rs (1 test)

| Test                                                | Locks in                                                                   |
| --------------------------------------------------- | -------------------------------------------------------------------------- |
| `is_plain_identifier_accepts_and_rejects_correctly` | the predicate that decides an expression is simple enough to skip hoisting |
