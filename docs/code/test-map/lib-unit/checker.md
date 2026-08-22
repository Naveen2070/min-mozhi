# Unit: checker (`crates/mimz-core/src/checker/tests/`, 286 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

One test per error code plus clean-pass cases - the codes are the
stable contract, so each test asserts the CODE and a message substring
(loose on wording). The full catalog with meanings lives in
[`11-checker.md`](11-checker.md); the test names map one-to-one
(`unknown_name_is_e0101_with_teaching_help`, `assignment_width_mismatch_is_e0401`, …).

Split 2026-07-26 (`oversized-test-file-split`) from a single 3026-line
`tests.rs` into 11 topic files under `tests/`; `mod.rs` keeps only the
shared `check_one`/`first_err`/`first_err_multi`/`any_code` helpers. Zero
test-behavior change - every row below is the same test that existed
before, just organized by file and given a row if it lacked one.

## checker/tests/names_and_consts.rs (35 tests)

| Test                                                                   | Locks in                                                                                           |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `clean_module_passes`                                                  | clean code produces ZERO diagnostics - the checker must never cry wolf                             |
| `clog2_in_a_width_position_is_clean`                                   | `clog2(N)` used in a width position checks clean                                                   |
| `clog2_of_a_module_const_is_clean`                                     | `clog2` of a module-level `const` checks clean                                                     |
| `clog2_of_zero_is_e0202`                                               | `clog2(0)` is E0202 (undefined - no width represents zero values)                                  |
| `clog2_in_a_runtime_value_position_is_e0407`                           | `clog2` used in a runtime (non-width) value position is E0407                                      |
| `same_name_module_in_different_files_is_not_an_error_until_referenced` | packages/namespacing: cross-file name collisions are legal until referenced (spec/02 section 1.5b) |
| `ambiguous_bare_module_reference_is_e0110`                             | a bare module reference that's ambiguous across imports is E0110                                   |
| `qualified_module_reference_resolves_unambiguously`                    | a qualified (`a.b.Foo`) reference resolves unambiguously even when a bare name would be ambiguous  |
| `qualified_reference_actually_resolves_via_a_real_import_path`         | a qualified reference resolves through a REAL import path end to end, not just a synthetic one     |
| `qualified_reference_with_unmatched_path_is_e0111`                     | a qualified reference whose path segments don't match any actual import is E0111                   |
| `qualified_reference_to_a_file_that_doesnt_declare_the_name_is_e0111`  | a qualified reference to a real file that doesn't declare the named module is E0111                |
| `same_name_module_in_the_same_file_is_still_e0001`                     | two same-named modules in the SAME file is still E0001 - only cross-file collisions are legal      |
| `duplicate_signal_in_module_is_e0003`                                  | a duplicate signal name within one module is E0003                                                 |
| `duplicate_file_const_is_e0004`                                        | a duplicate file-level `const` name is E0004                                                       |
| `unknown_name_is_e0101_with_teaching_help`                             | an unknown name is E0101 with teaching help text                                                   |
| `array_param_length_referencing_an_unbound_name_is_e0101`              | an array param's length expression referencing an unbound name is E0101                            |
| `unknown_module_in_inst_is_e0102_and_mentions_import`                  | instantiating an unknown module is E0102, mentioning `import`                                      |
| `unknown_enum_variant_is_e0103_and_lists_variants`                     | referencing an unknown enum variant is E0103, listing the real variants                            |
| `reading_an_input_of_an_instance_is_e0104`                             | reading an instance's OWN input port (not an output) is E0104                                      |
| `field_on_a_wire_is_e0105`                                             | field access on a plain (non-bundle) wire is E0105                                                 |
| `unknown_param_in_inst_is_e0106_and_lists_params`                      | an unknown parameter name in an instantiation is E0106, listing the real params                    |
| `connecting_an_output_is_e0107`                                        | connecting to an instance's output port (outputs aren't connectable) is E0107                      |
| `assigning_an_input_is_e0108`                                          | assigning a module's own `in` port is E0108                                                        |
| `on_rise_of_a_non_clock_is_e0109`                                      | `on rise(x)` where `x` isn't declared `clock` is E0109                                             |
| `const_arithmetic_and_repeat_bounds_evaluate`                          | compile-time const arithmetic and `repeat` bounds evaluate correctly with zero diagnostics         |
| `non_constant_repeat_bound_is_e0201`                                   | a non-constant `repeat` bound is E0201                                                             |
| `foreach_elements_form_on_scalar_is_e0417`                             | `foreach v in <scalar>` (not array/mem-typed) is E0417                                             |
| `foreach_range_form_checks_clean`                                      | `foreach i in 0..N { }` (range form) checks clean, lowering to `repeat`/`loop` as expected         |
| `foreach_elements_form_checks_clean_over_mem`                          | `foreach v in <mem>` (elements form over a `mem`) checks clean                                     |
| `foreach_elements_form_variable_resolves_inside_on_block`              | the bound element variable resolves correctly inside a clocked `on` block                          |
| `foreach_elements_form_at_module_level_checks_clean`                   | `foreach` as a bare module item (not inside `on`/`fn`) checks clean                                |
| `foreach_elements_form_in_fn_body_resolves_via_own_param`              | inside a `fn` body, the elements-form source resolves against the `fn`'s own parameter list        |
| `const_using_a_later_const_is_e0201`                                   | a `const` referencing another `const` declared LATER in the file is E0201 (no forward ref)         |
| `const_overflow_is_e0202`                                              | a `const` expression that overflows is E0202                                                       |
| `reg_without_reset_declaration_is_e0301`                               | a `reg` with no `reset` line is E0301                                                              |

## checker/tests/widths.rs (68 tests)

The width slice (E0401–E0410) added error paths for every code (several
codes get two angles, e.g. `extend`-narrowing AND `trunc`-widening for
E0407) plus clean passes.

| Test                                                                  | Locks in                                                                                                                  |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `assert_condition_must_be_a_single_bit`                               | an `assert(cond)` whose condition isn't a single bit is a width error                                                     |
| `a_well_typed_assert_in_a_module_body_checks_clean`                   | a correctly-typed module-body `assert` checks clean                                                                       |
| `a_well_typed_assert_inside_an_on_block_checks_clean`                 | a correctly-typed `assert` inside `on rise(clk)` checks clean                                                             |
| `cover_condition_must_be_a_single_bit`                                | a `cover(cond)` whose condition isn't a single bit is a width error                                                       |
| `a_well_typed_cover_in_a_module_body_checks_clean`                    | a correctly-typed module-body `cover` checks clean                                                                        |
| `a_well_typed_cover_inside_an_on_block_checks_clean`                  | a correctly-typed `cover` inside `on rise(clk)` checks clean                                                              |
| `assignment_width_mismatch_is_e0401`                                  | an assignment with mismatched widths is E0401                                                                             |
| `plus_into_same_width_target_teaches_wrap_in_e0401`                   | the dropped-carry moment teaches `+%` - the spec/02 section 1.2 promise, executable                                       |
| `shift_left_into_same_width_target_teaches_growth_in_e0401`           | BUG-30: a `<<` result narrowed back into its own operand's width teaches the growth rule, not the generic mismatch text   |
| `shift_right_width_mismatch_uses_the_generic_help_not_the_growth_one` | `>>` (which never grows) keeps the plain E0401 help text, distinguishing it from the `<<`-specific one                    |
| `connection_width_mismatch_is_e0401_naming_the_port`                  | an instance port connection at the wrong width is E0401, naming the port                                                  |
| `replication_width_is_count_times_inner`                              | `{2{bits[4]}}` is `bits[8]`, `{3{bits[4]}}` is `bits[12]` (A1)                                                            |
| `replication_width_mismatch_is_e0401`                                 | `{2{a}}` (bits[8]) into a `bits[4]` is the usual assignment width error                                                   |
| `a_non_constant_replication_count_is_e0201`                           | `{n{a}}` with a signal count is "not a compile-time constant" (reused code)                                               |
| `a_zero_replication_count_is_e0410`                                   | `{0{a}}` has zero width - reuses the "not a valid width" code                                                             |
| `dont_care_pattern_must_match_the_scrutinee_width`                    | `0b1??` is fine on `bits[3]`, a width error (E0409) on `bits[4]` (A2)                                                     |
| `a_dont_care_match_still_needs_a_wildcard`                            | masked patterns earn no coverage - `0b1??`+`0b0??` without `_` is E0601 (A2)                                              |
| `a_dont_care_pattern_on_an_enum_is_e0409`                             | a masked pattern on an enum scrutinee is rejected (match variants by name) (A2)                                           |
| `min_max_take_two_same_width_operands`                                | `min`/`max` require both operands at the same width                                                                       |
| `min_of_mismatched_widths_is_e0402`                                   | `min` of two mismatched-width operands is E0402                                                                           |
| `abs_of_signed_grows_one_bit`                                         | `abs` of a signed value grows the result by one bit (sign-removal headroom)                                               |
| `abs_of_unsigned_is_e0407`                                            | `abs` of an already-unsigned value is E0407 (nothing to make absolute)                                                    |
| `nand_reduces_to_a_bit`                                               | `nand` reduces its operand to a single bit                                                                                |
| `nor_of_signed_is_e0403`                                              | `nor` of a signed operand is E0403 (bitwise built-ins reject signed)                                                      |
| `max_with_a_literal_operand_adapts`                                   | `max` with one literal operand adapts the literal to the other operand's width                                            |
| `abs_of_a_literal_is_e0407`                                           | `abs` of a bare literal (no signed context) is E0407                                                                      |
| `min_of_two_literals_is_e0407`                                        | `min` of two bare literals is E0407 (needs a signal to establish width context)                                           |
| `nand_of_a_bare_bit_is_a_bit`                                         | `nand` of a bare `bit` operand stays `bit`-typed                                                                          |
| `nested_abs_of_min_type_checks`                                       | `abs(min(a, b))` (nested built-ins) type-checks through both layers                                                       |
| `min_of_two_abs_type_checks`                                          | `min(abs(a), abs(b))` type-checks, each `abs` growing independently before `min` compares                                 |
| `abs_grows_at_the_width_boundary`                                     | `abs` at the width boundary (widest representable signed value) grows correctly, no overflow                              |
| `bitwise_operand_mismatch_is_e0402`                                   | mismatched-width operands to a bitwise op (`&`/`\|`/`^`) is E0402                                                         |
| `wrapping_add_operand_mismatch_is_e0402`                              | mismatched-width operands to `+%` is E0402                                                                                |
| `signed_bits_mixing_is_e0403`                                         | mixing `signed[N]` and unsigned `bits[N]` in one expression is E0403                                                      |
| `clock_in_a_data_expression_is_e0403`                                 | using a `clock` signal inside a data expression is E0403                                                                  |
| `enum_in_concat_is_e0403_with_enum_specific_help`                     | an enum-typed operand inside a `{...}` concat is E0403 with enum-specific help text                                       |
| `logical_and_on_a_bus_is_e0404`                                       | using `&&` (logical) on a multi-bit bus is E0404 (logical ops are bit-only)                                               |
| `literal_that_does_not_fit_is_e0405`                                  | a literal that doesn't fit its declared/target width is E0405                                                             |
| `negative_literal_in_unsigned_context_is_e0405`                       | a negative literal used in an unsigned context is E0405                                                                   |
| `a_wide_literal_fits_a_wide_declared_width`                           | a wide literal (past 128 bits) fits cleanly when the declared width is wide enough                                        |
| `index_out_of_range_is_e0406`                                         | a bit index past the signal's width is E0406                                                                              |
| `reversed_slice_is_e0406`                                             | a slice with `hi < lo` (reversed bounds) is E0406                                                                         |
| `huge_slice_bound_that_would_wrap_u32_is_still_e0406`                 | a slice bound large enough to wrap a `u32` cast is still a clean E0406, not a silently-wrapped index                      |
| `extend_to_a_smaller_width_is_e0407`                                  | `extend` to a SMALLER width is E0407 (that's narrowing - use `trunc`)                                                     |
| `trunc_to_a_larger_width_is_e0407`                                    | `trunc` to a LARGER width is E0407 (that's widening - use `extend`)                                                       |
| `negating_bits_is_e0407`                                              | unary `-` on an unsigned `bits[N]` is E0407 (negation needs `signed`)                                                     |
| `if_arms_that_disagree_are_e0408`                                     | an if-expression whose arms disagree in width is E0408                                                                    |
| `match_pattern_wider_than_scrutinee_is_e0409`                         | a match pattern literal wider than the scrutinee is E0409                                                                 |
| `match_on_signed_is_e0409`                                            | matching on a `signed[N]` scrutinee is E0409 (match patterns are unsigned-only)                                           |
| `zero_width_is_e0410`                                                 | a zero-width declaration (`bits[0]`) is E0410                                                                             |
| `zero_width_output_with_indexed_drivers_does_not_panic`               | a zero-width output with indexed per-bit drivers is a clean E0410-family error, not a panic                               |
| `adder_growth_passes`                                                 | the adder-growth idiom (`bits[W] + bits[W] -> bits[W+1]`) checks clean                                                    |
| `alu_match_arms_pass`                                                 | an ALU's `match`-selected arithmetic arms all check clean together                                                        |
| `enum_state_machine_passes`                                           | an enum-driven FSM module checks clean end to end                                                                         |
| `register_file_passes`                                                | a `mem` with a clocked indexed write + combinational indexed read checks clean (A4)                                       |
| `a_non_constant_memory_depth_is_e0201`                                | a memory `DEPTH` that is not a compile-time constant is E0201 (A4)                                                        |
| `a_zero_memory_depth_is_e0410`                                        | a memory `DEPTH` of 0 is E0410 - a memory needs at least one cell (A4)                                                    |
| `a_memory_init_that_overflows_the_element_is_e0405`                   | a `mem` init value too wide for the element type is E0405 (A4)                                                            |
| `a_constant_address_past_the_depth_is_e0406`                          | a compile-time address `≥ DEPTH` is E0406 (out of range) (A4)                                                             |
| `a_memory_inside_repeat_is_e0303`                                     | declaring a `mem` inside `repeat` is E0303 (declare once, outside) (A4)                                                   |
| `extend_of_a_bit_into_bitwise_passes`                                 | the fixed shift-register shape - explicit `extend` where widths differ                                                    |
| `shift_register_without_the_trunc_no_longer_matches_widths`           | BUG-30 regression: the old shift-register idiom without an explicit `trunc` no longer matches widths, now that `<<` grows |
| `comparison_with_a_const_passes`                                      | comparing a signal against a compile-time `const` checks clean                                                            |
| `defaultless_param_module_is_checked_per_instantiation`               | a module with no param defaults is checked under each instantiation's concrete binding                                    |
| `repeat_index_out_of_range_at_the_last_iteration_is_e0406`            | `repeat` bodies are width-checked per iteration value, not just once                                                      |

## checker/tests/drivers.rs (17 tests)

The driver slice (E0501–E0505) covers every code's error paths (both
halves where a code covers two mistakes, e.g. zero AND multiple `on`
blocks for E0503) plus clean passes guarding against false positives.

| Test                                                        | Locks in                                                                                  |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `driving_a_signal_twice_is_e0501`                           | driving the same signal twice is E0501                                                    |
| `driving_a_wire_after_its_declaration_is_e0501`             | a second drive of a wire after its own declaration-with-init is E0501                     |
| `overlapping_slice_drives_are_e0501`                        | two overlapping bit-slice drives of the same signal are E0501                             |
| `an_undriven_output_is_e0502`                               | an undriven output is E0502                                                               |
| `a_partially_driven_output_is_e0502_naming_the_bit`         | a partially-driven output (some bits undriven) is E0502, naming the specific bit          |
| `a_reg_assigned_in_two_on_blocks_is_e0503`                  | a reg assigned in two different `on` blocks is E0503                                      |
| `a_reg_never_assigned_is_e0503`                             | a reg with no assignment at all is E0503                                                  |
| `a_self_referential_wire_is_e0504`                          | a wire that references itself is E0504                                                    |
| `a_two_wire_cycle_is_e0504_showing_the_path`                | a two-wire combinational cycle is E0504, showing the cycle path                           |
| `a_cycle_through_instances_is_e0504`                        | combinational loops THROUGH child modules are caught via the comb summaries               |
| `forward_reference_to_unknown_output_field_is_e0104`        | a forward reference to an unknown instance output field is E0104                          |
| `arrow_assignment_to_a_wire_is_e0505`                       | using `<-` (clocked assign) on a wire is E0505                                            |
| `combinational_drive_of_a_reg_is_e0505`                     | using `=` (combinational drive) on a reg is E0505                                         |
| `disjoint_per_bit_drives_via_repeat_pass`                   | the Chaser idiom: eight `led[i] = ...` drives are eight drivers for eight bits - legal    |
| `feedback_through_a_register_is_not_a_cycle`                | a reg breaks the loop - the normal shape of hardware never false-positives                |
| `repeat_instance_array_ripple_carry_is_not_a_cycle`         | per-index instance-output nodes: `fa[1] -> fa[0]` is a chain, not a loop                  |
| `defaultless_module_with_param_indexed_drives_is_not_e0501` | a defaultless-param module whose per-index drives depend on the param isn't falsely E0501 |

## checker/tests/clocks.rs (14 tests)

The clock-domain matrix (E0701–E0705): independent domains clean,
direct read, through-a-wire, domain-mixing wire, unused-second-clock
clean, plus the `sync.*` arg-shape and domain/placement rules.

| Test                                                            | Locks in                                                                                              |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `two_clocks_with_separate_logic_pass`                           | two independent clock domains with no cross-talk pass clean                                           |
| `reading_another_domains_reg_is_e0701`                          | reading a register owned by another clock domain directly is E0701                                    |
| `cross_domain_through_a_wire_is_e0701`                          | crossing clock domains through an intermediate wire is still caught as E0701                          |
| `a_wire_mixing_two_domains_is_e0701`                            | a wire that mixes signals from two clock domains is E0701                                             |
| `same_domain_logic_under_two_declared_clocks_passes`            | E0701 colors by USE, not by declaration count - an unused clock changes nothing                       |
| `sync_double_flop_with_non_clock_second_arg_is_e0702`           | `sync.double_flop`'s second argument not being a clock is E0702                                       |
| `sync_double_flop_with_matching_src_and_dst_clock_is_e0702`     | `sync.double_flop` with identical src/dst clock arguments is E0702 (needs two distinct clocks)        |
| `sync_double_flop_with_a_2_bit_signal_is_e0703`                 | `sync.double_flop` on a signal wider than 1 bit is E0703 (single-bit-only crossing primitive)         |
| `sync_double_flop_signal_from_a_third_unrelated_clock_is_e0704` | `sync.double_flop`'s source signal belonging to neither declared clock is E0704                       |
| `sync_pulse_signal_that_is_domain_free_is_e0704`                | `sync.pulse`'s source must be exactly a register owned by `src_clock` - a domain-free source is E0704 |
| `sync_double_flop_used_outside_its_own_on_block_clock_is_e0705` | `sync.double_flop`'s result used outside the `on`-block clock it was assigned in is E0705             |
| `sync_pulse_used_as_a_reg_source_is_e0705`                      | `sync.pulse`'s result feeding a register clocked wrong is E0705                                       |
| `sync_double_flop_hidden_in_a_reg_reset_value_is_e0705`         | a `sync.double_flop` call hidden inside a reg's reset-value expression is still caught, E0705         |
| `sync_double_flop_hidden_in_a_sync_loop_body_is_e0705`          | a `sync.double_flop` call hidden inside a `sync loop` body is still caught, E0705                     |

## checker/tests/insts.rs (4 tests)

| Test                                                 | Locks in                                                                  |
| ---------------------------------------------------- | ------------------------------------------------------------------------- |
| `unconnected_input_is_e0302_naming_it`               | an unconnected instance input is E0302, naming it                         |
| `several_unconnected_inputs_are_listed_in_one_error` | multiple unconnected inputs are listed together in one E0302              |
| `clock_and_reset_ports_may_be_omitted`               | E0302 exempts clock/reset - implicit-by-name stays the emitter's contract |
| `connecting_an_input_twice_is_e0302`                 | connecting the same instance input twice is E0302                         |

## checker/tests/enums.rs (38 tests)

Instantiation/exhaustiveness completeness (E0302/E0601/E0602/E0701,
2026-06-12) plus tagged-union/enum-variant construction and OR-arm
binding intersection (E0808, algorithm in `checker/names.rs`).

| Test                                                                                | Locks in                                                                                                         |
| ----------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `tagged_enum_unknown_payload_type_is_e0103`                                         | a tagged-enum variant field with an unknown type is E0103                                                        |
| `tagged_enum_toplevel_unknown_payload_type_is_e0103`                                | a top-level tagged-enum declaration with an unknown payload type is E0103                                        |
| `tagged_pattern_arity_mismatch_is_e0806`                                            | a match pattern binding the wrong number of payload fields is E0806                                              |
| `tag_only_pattern_with_bindings_is_e0806`                                           | a tag-only variant's match pattern supplying bindings (it has no fields) is E0806                                |
| `valid_tagged_pattern_compiles_clean`                                               | a correctly-arity-matched tagged pattern checks clean                                                            |
| `enum_construct_unknown_enum_name`                                                  | `Enum.Variant(...)` construction with an unknown enum name is rejected                                           |
| `enum_construct_unknown_variant_name`                                               | `Enum.Variant(...)` construction with an unknown variant name is rejected                                        |
| `enum_construct_arity_mismatch_is_e0806`                                            | enum construction with the wrong argument count is E0806                                                         |
| `enum_construct_tag_only_with_extra_args_is_e0806`                                  | constructing a tag-only variant with extra args is E0806                                                         |
| `enum_construct_recurses_into_args_for_name_resolution`                             | enum-construct arguments are name-resolved recursively                                                           |
| `match_arm_binding_field_width_resolves_against_enum_declaring_file_not_match_site` | a match-arm binding's field width resolves against the enum's DECLARING file, not the matching file (cross-file) |
| `tagged_enum_total_width_is_tag_plus_max_payload`                                   | a tagged enum's total width is the tag width plus the widest variant's payload                                   |
| `pattern_binding_types_match_payload_fields`                                        | match-pattern bindings take on their corresponding payload field's type                                          |
| `enum_payload_enum_type_is_e0807`                                                   | an enum variant field typed as another enum is E0807 (no nested enum payloads)                                   |
| `enum_payload_array_type_is_e0807`                                                  | an enum variant field typed as an array is E0807                                                                 |
| `enum_construct_wrong_arg_width_is_e0401`                                           | an enum-construct argument at the wrong width is E0401                                                           |
| `enum_construct_valid_use_checks_clean_and_infers_enum_ty`                          | a correct enum-construct checks clean and infers the enum type for the expression                                |
| `enum_construct_literal_arg_adapts_to_field_width`                                  | a literal argument to enum-construct adapts to the target field's width                                          |
| `enum_construct_emits_tag_and_payload_concat`                                       | enum-construct emits a tag+payload concat in the lowered representation                                          |
| `enum_construct_literal_arg_is_sized_to_field_width_in_concat`                      | a literal enum-construct argument is explicitly sized to the field width in the emitted concat                   |
| `enum_construct_negative_literal_arg_is_masked_and_sized_not_left_bare`             | a negative literal enum-construct argument is masked and sized, not left bare                                    |
| `enum_construct_tag_only_zero_args_emits_bare_tag`                                  | constructing a tag-only variant with zero args emits just the bare tag (no payload concat)                       |
| `or_arm_same_names_same_widths_is_clean`                                            | a 2-way OR-arm match pattern with identical binding names and widths is clean                                    |
| `or_arm_three_alts_same_bindings_is_clean`                                          | a 3-way OR-arm match pattern with identical bindings across all alternatives is clean                            |
| `or_arm_different_names_is_e0808`                                                   | an OR-arm pattern whose alternatives bind different names is E0808                                               |
| `or_arm_tag_only_alt_is_e0808`                                                      | an OR-arm mixing a tag-only alternative with a binding alternative is E0808                                      |
| `or_arm_subset_binding_is_e0808`                                                    | an OR-arm alternative binding a subset of the other alternatives' names is E0808                                 |
| `or_arm_width_mismatch_is_e0808`                                                    | an OR-arm alternative binding the same name at a different width is E0808                                        |
| `e0809_default_target_not_reg`                                                      | `default` keyword must target a `reg`                                                                            |
| `e0810_duplicate_default`                                                           | a duplicate `default` assignment to the same reg is E0810                                                        |
| `e0811_const_if_condition_not_const`                                                | `const if` conditions must be compile-time constants                                                             |
| `e0813_fn_let_shadow_width_mismatch`                                                | BUG-9: a `fn`-body `let` re-binding a name (earlier `let` or param) at a different width is E0813                |
| `fn_let_shadow_same_width_stays_clean`                                              | the fold/accumulator idiom (same-width shadow, e.g. `foreach_sum.mimz`'s `acc`) stays legal                      |
| `fn_let_shadowing_a_param_at_a_different_width_is_e0813`                            | shadowing a PARAM (not just an earlier `let`) at a different width is the same E0813 conflict                    |
| `or_arm_wildcard_not_binding_e0808`                                                 | an OR-arm alternative that's a bare wildcard `_` mixed with a binding alternative is E0808                       |
| `encoding_of_tag_only_enum_is_bits_of_tag_width`                                    | a tag-only enum's `encoding()` is `bits` of just the tag width                                                   |
| `encoding_of_payload_enum_is_bits_of_tag_plus_payload_width`                        | a payload-bearing enum's `encoding()` is `bits` of tag width plus the widest payload                             |
| `encoding_of_non_enum_is_e0418`                                                     | calling `encoding()` on a non-enum type is E0418                                                                 |

## checker/tests/funcs_and_loops.rs (24 tests)

`repeat`/`fn`/`loop`/`sync loop` declaration restrictions, return-width
checking, and per-scope name/width leakage guards.

| Test                                                                             | Locks in                                                                                                    |
| -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `wire_inside_repeat_is_e0303`                                                    | declaring a `wire` inside `repeat` is E0303                                                                 |
| `reg_inside_repeat_is_e0303`                                                     | declaring a `reg` inside `repeat` is E0303                                                                  |
| `on_block_inside_repeat_is_e0303`                                                | an `on` block inside `repeat` is E0303                                                                      |
| `const_inside_repeat_is_e0303`                                                   | a `const` declared inside `repeat` is E0303                                                                 |
| `repeat_with_only_drives_and_nested_repeat_is_clean`                             | a `repeat` containing only drives and a nested `repeat` checks clean                                        |
| `fn_body_width_mismatch_is_e0804`                                                | `fn` return values must match the declared return width                                                     |
| `return_width_mismatch_is_e0804`                                                 | a `return` statement's value at the wrong width is E0804                                                    |
| `return_width_match_is_accepted`                                                 | a `return` statement's value at the matching width is accepted                                              |
| `mac_function_type_checks_clean`                                                 | the canonical `mac` combinational function type-checks clean                                                |
| `fn_with_const_local_compiles_clean`                                             | a `fn` body with a local `const` compiles clean                                                             |
| `unbound_name_inside_fn_return_is_rejected`                                      | an unbound name inside a `fn`'s return expression is rejected                                               |
| `fn_if_branch_names_are_resolved`                                                | names declared inside a `fn`'s `if` branch resolve correctly within that branch                             |
| `let_bound_only_inside_an_if_branch_does_not_leak_outside`                       | a `let` bound only inside an `if` branch does not leak outside the branch                                   |
| `let_bound_only_inside_one_if_branch_is_not_visible_in_the_sibling_branch`       | a `let` bound in one `if` branch is not visible in the sibling (`else`) branch                              |
| `let_bound_only_inside_an_if_branch_is_not_visible_to_width_checking_outside_it` | a branch-local `let` is invisible to width-checking outside the branch, not just name resolution            |
| `fn_loop_variable_resolves_inside_its_own_body`                                  | a `loop` variable resolves correctly inside its own body                                                    |
| `seq_loop_variable_resolves_inside_on_block`                                     | a `sync loop` variable resolves correctly inside its `on`-block body                                        |
| `fn_loop_variable_does_not_leak_outside_the_loop`                                | `loop` variables are scoped strictly to the loop body                                                       |
| `seq_loop_variable_does_not_leak_outside_the_loop`                               | a `sync loop` variable does not leak outside the loop, mirroring the `fn`-loop scoping rule                 |
| `fn_loop_local_let_does_not_leak_outside_the_loop`                               | a `let` declared inside a `fn`-body `loop` does not leak outside the loop                                   |
| `non_constant_seq_loop_bound_is_e0201`                                           | loop boundary conditions must be compile-time constants                                                     |
| `non_constant_fn_loop_bound_is_e0201`                                            | a non-constant `fn`-body `loop` boundary is E0201, mirroring the `sync loop` rule                           |
| `fn_loop_body_width_mismatch_is_checked`                                         | width mismatches inside a `fn`-body `loop`'s body are still checked per iteration                           |
| `fn_loop_width_bug_independent_of_loop_var_reports_once`                         | a width bug inside a `fn` loop independent of the loop variable is reported exactly once, not per-iteration |

## checker/tests/patterns.rs (12 tests)

| Test                                                          | Locks in                                                                                                 |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `enum_match_covering_every_variant_needs_no_wildcard`         | the v0.2.3 ruling, executable: full coverage IS exhaustive, no `_` ceremony                              |
| `enum_match_missing_a_variant_is_e0601_naming_it`             | an enum `match` missing a variant is E0601, naming the missing variant                                   |
| `wildcard_after_full_enum_coverage_is_allowed`                | the defensive `_` (bit-flip recovery) is never flagged unreachable                                       |
| `duplicate_variant_pattern_is_e0602`                          | matching the same enum variant twice is E0602                                                            |
| `arm_after_wildcard_is_e0602`                                 | a match arm after the `_` wildcard is E0602 (unreachable)                                                |
| `bits2_match_covering_all_four_values_passes`                 | a `bits[2]` match covering all 4 values passes without a wildcard                                        |
| `bits2_match_missing_a_value_is_e0601_naming_it`              | a `bits[2]` match missing one value is E0601, naming it                                                  |
| `bit_match_missing_one_is_e0601`                              | a `bit` match missing one of its two values (0/1) is E0601                                               |
| `wide_match_without_wildcard_is_e0601`                        | a match on a wide (`bits[N]`, N large) scrutinee with no wildcard is E0601 (can't enumerate every value) |
| `wide_match_with_a_past_128_bit_pattern_is_e0601_not_a_panic` | a match with a pattern literal past 128 bits is E0601, not a panic                                       |
| `multi_pattern_arms_count_toward_coverage`                    | a single arm matching multiple patterns (`0 \| 1 => ...`) counts all toward exhaustiveness coverage      |
| `duplicate_value_in_multi_pattern_arm_is_e0602`               | a duplicate value within one multi-pattern arm's own pattern list is E0602                               |

## checker/tests/regressions.rs (4 tests)

Kept as one file, not split further - a named historical batch (Task 15
sweep) tied to `.superpowers/sdd/progress.md`'s notes.

| Test                                                                                 | Locks in                                                                                         |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| `overlapping_import_prefixes_disambiguate_correctly`                                 | two imports whose paths share a prefix still disambiguate to the correct file                    |
| `no_default_param_module_only_discovered_via_instantiation_still_gets_width_checked` | a defaultless-param module only ever discovered via instantiation still gets its width pass run  |
| `two_same_named_modules_each_get_their_own_clock_check_reversed_order`               | the two-same-named-modules clock-check isolation holds regardless of file load order             |
| `recursive_call_inside_return_is_e0805`                                              | a `fn` recursively calling itself from inside a `return` expression is E0805 (cyclic-call guard) |

## checker/tests/bundles.rs (31 tests)

| Test                                                                 | Locks in                                                                                                |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `bundle_duplicate_name_is_e0909`                                     | duplicate bundle declarations error out                                                                 |
| `bundle_clean_declaration_passes`                                    | a well-formed bundle declaration checks clean                                                           |
| `bundle_named_field_as_module_port_passes`                           | a bundle field used as a module port name checks clean                                                  |
| `bundle_unknown_parametric_type_in_field_is_e0906`                   | a bundle field with an unknown parametric type is E0906                                                 |
| `bundle_nested_bundle_field_is_e0807`                                | nested bundles (bundles inside bundles) are caught as an error                                          |
| `bundle_array_field_is_e0807`                                        | an array-typed bundle field is E0807 (same nesting restriction as nested bundles)                       |
| `builtin_valid_bundle_resolves_by_name`                              | the built-in `__Valid` bundle type resolves by name                                                     |
| `builtin_valid_signed_bundle_resolves_by_name`                       | the built-in `__ValidSigned` bundle type resolves by name                                               |
| `qq_unwrap_form_types_as_the_data_field_type`                        | `a??` (unwrap) types as the `data` field's own type                                                     |
| `qq_or_mux_form_types_as_still_optional`                             | `a ?? b` (or-mux) types as still-optional when both sides are optional                                  |
| `qq_lhs_not_optional_is_e0911`                                       | `??`'s LHS not being an optional (valid-bundle) type is E0911                                           |
| `qq_rhs_wrong_width_is_e0912`                                        | `??`'s RHS at the wrong width vs the LHS's data field is E0912                                          |
| `builtin_valid_bundle_shows_as_surface_syntax_in_diagnostics`        | a diagnostic naming `__Valid` renders it as the surface `bit?`/`bits[N]?` syntax, not the internal name |
| `builtin_valid_bundle_bit_question_collapses_to_bit_in_diagnostics`  | `bit?`'s diagnostic rendering collapses `bits[1]?` to `bit?`                                            |
| `builtin_valid_signed_bundle_shows_as_surface_syntax_in_diagnostics` | a diagnostic naming `__ValidSigned` renders it as `signed[N]?`                                          |
| `qq_same_shaped_user_bundle_satisfies_a_valid_bundle_slot`           | a user bundle with the same shape as `__Valid` structurally satisfies a `T?` slot                       |
| `qq_lhs_missing_valid_field_is_e0911`                                | a user bundle missing the `valid` field used as `??`'s LHS is E0911                                     |
| `qq_or_mux_rhs_with_extra_field_is_e0912`                            | `??`'s RHS with an extra field beyond the LHS's data type is E0912                                      |
| `bundle_field_typed_as_valid_bundle_sugar_is_rejected_e0807`         | a bundle field typed with the `T?` sugar (`bit?`) is rejected as nested-bundle-like, E0807              |
| `bundle_literal_missing_field`                                       | bundle literal missing a declared field is an error                                                     |
| `bundle_literal_unknown_field`                                       | a bundle literal naming an unknown field is an error                                                    |
| `bundle_type_mismatch`                                               | bundle type mismatch is caught correctly                                                                |
| `structurally_compatible_bundles_check_clean_in_a_drive`             | a drive between structurally compatible (not nominally identical) bundles checks clean                  |
| `structurally_compatible_bundle_with_extra_fields_checks_clean`      | a bundle with extra fields beyond the target's requirement still checks clean structurally              |
| `drive_bundle_missing_required_field_is_e0910`                       | a drive whose bundle source is missing a required field is E0910                                        |
| `drive_bundle_shared_field_wrong_width_is_e0907`                     | a drive between bundles with a shared field at mismatched width is E0907                                |
| `drive_bundle_same_name_regression_still_checks_clean`               | two same-named-and-shaped bundle types in a drive still check clean (no false E0907 from name alone)    |
| `bundle_destructure_duplicate_binding`                               | a bundle destructure with a duplicate binding name is an error                                          |
| `two_same_named_modules_each_get_their_own_driver_check`             | two same-named modules from different files each get their OWN driver-pass check, not a merged one      |
| `two_same_named_modules_each_get_their_own_width_check`              | two same-named modules from different files each get their own width-pass check                         |
| `two_same_named_modules_each_get_their_own_clock_check`              | two same-named modules from different files each get their own clock-domain check                       |

## checker/tests/arrays.rs (39 tests)

Array-typed params/literals/indices, `extern module`, and structural
(shape-based, not nominal) bundle compatibility across drives/fn
args-returns/port connections.

| Test                                                                  | Locks in                                                                                                |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `unreachable_code_after_return_is_e0812`                              | a statement after `return` (not the tail) is unreachable - E0812                                        |
| `return_as_last_statement_before_tail_is_not_e0812`                   | `return` immediately before the tail expression is NOT flagged unreachable (no code follows it)         |
| `fn_loop_body_return_followed_by_more_code_is_unreachable`            | code after `return` inside a `loop` body is still E0812                                                 |
| `fn_loop_after_return_in_sibling_branch_is_not_flagged`               | code after an `if` branch's `return` in a SIBLING branch is not falsely flagged                         |
| `array_param_with_bundle_element_type_is_e0411`                       | an array param whose element type is a bundle is E0411                                                  |
| `array_param_with_zero_length_is_e0412`                               | a zero-length array param is E0412                                                                      |
| `array_literal_infers_its_own_type`                                   | an array literal infers its own element type and length                                                 |
| `array_literal_with_mismatched_element_widths_is_e0414`               | an array literal with mismatched element widths is E0414                                                |
| `array_literal_argument_length_mismatch_is_e0413`                     | an array literal argument with the wrong length is E0413                                                |
| `array_param_forwarded_by_name_with_matching_type_is_accepted`        | forwarding an array param by name with a matching type is accepted                                      |
| `array_param_forwarded_by_name_with_mismatched_length_is_rejected`    | forwarding an array param by name with a mismatched length is rejected                                  |
| `constant_array_index_out_of_range_is_e0415`                          | a compile-time-constant array index past the length is E0415                                            |
| `runtime_array_index_is_accepted`                                     | a runtime (non-constant) array index is accepted - only constant indices are range-checked              |
| `indexing_an_array_literal_directly_is_e0419`                         | indexing a bare array literal (`[1,2,3][0]`, no named binding) is E0419                                 |
| `indexing_a_named_array_still_works_after_e0419`                      | …while indexing a NAMED array still works - the restriction is literals-only                            |
| `array_typed_module_port_is_e0416`                                    | an array-typed module port is E0416 (arrays aren't a port type)                                         |
| `array_typed_wire_is_e0416`                                           | an array-typed `wire` is E0416                                                                          |
| `array_typed_output_with_constant_indexed_drive_is_e0416_not_a_panic` | an array-typed output driven by a constant index is E0416, not a panic (regression)                     |
| `extern_module_duplicate_in_same_file_is_e1301`                       | two `extern module`s with the same name in one file is E1301                                            |
| `extern_module_bundle_typed_port_is_e1302`                            | a bundle-typed port on an `extern module` is E1302 (extern ports must be scalar)                        |
| `extern_module_array_typed_port_is_e1302`                             | an array-typed port on an `extern module` is E1302                                                      |
| `extern_module_scalar_ports_check_clean`                              | an `extern module` with only scalar ports checks clean                                                  |
| `extern_instantiation_checks_clean_with_correct_connections`          | instantiating an `extern module` with correct port connections checks clean                             |
| `extern_instantiation_missing_input_connection_is_reported`           | instantiating an `extern module` with a missing input connection is reported (E0302)                    |
| `extern_instantiation_unknown_port_is_reported`                       | connecting an unknown port on an `extern module` instance is reported                                   |
| `extern_instantiation_wrong_width_connection_is_e0401`                | connecting an `extern module` port at the wrong width is E0401                                          |
| `structurally_compatible_bundle_wire_binding_checks_clean`            | a wire bound to a structurally (not nominally) compatible bundle checks clean                           |
| `structurally_compatible_fn_arg_checks_clean`                         | a `fn` argument passed a structurally compatible bundle checks clean                                    |
| `wire_binding_bundle_missing_field_is_e0910`                          | a wire's bundle initializer missing a required field is E0910                                           |
| `structurally_compatible_fn_return_checks_clean`                      | a `fn` returning a structurally compatible bundle checks clean                                          |
| `fn_return_bundle_missing_field_is_e0910`                             | a `fn` bundle-tail return missing a required field is E0910                                             |
| `fn_return_same_name_bundle_regression_still_e0804`                   | two same-named-but-different bundle types on a `fn` return stays E0804, not silently structural-matched |
| `fn_return_bundle_shared_field_wrong_width_is_e0804`                  | a `fn` bundle return with a shared field at the wrong width is E0804                                    |
| `structurally_compatible_bundle_port_connection_checks_clean`         | a module port connection with a structurally compatible bundle checks clean                             |
| `port_connection_bundle_missing_field_is_e0910`                       | a port connection's bundle argument missing a required field is E0910                                   |
| `port_connection_bundle_shared_field_wrong_width_is_e0401`            | a port connection's bundle argument with a shared field at the wrong width is E0401                     |
| `structural_match_composes_across_fn_return_and_port_connection`      | structural bundle matching composes across a `fn` return feeding a port connection                      |
| `drive_bundle_zero_required_fields_always_compatible`                 | a bundle type with zero required fields is always structurally compatible (trivial case)                |
| `matched_ty_same_shaped_bundle_equality_passes`                       | two identically-shaped bundle types compare as matched via structural equality                          |
