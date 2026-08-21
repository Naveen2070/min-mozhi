# Unit: value model + fn-body interpreter (`crates/mimz-sim/src/sim/value/`, 38 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The shared value model and expression evaluator behind BOTH `comb.rs` and
the kernel. A `Val` is a 2-state bit-vector carrying a width and a
signedness — small values stay on a `u128` fast path and only promote to
the multi-limb `wide` representation past 128 bits. This pocket also
covers `fn`-body statement evaluation: `fn` bodies are interpreted
directly (no elaborate-time lowering pass exists for them, unlike module
items and `on` blocks), so `loop`/`foreach` are lowered on the spot
inside the evaluator itself.

Split across `value/mod.rs` (the `Val` type + statement evaluation),
`value/binary.rs` (binary operators), `value/fn_eval.rs` (`fn` calls),
and `value/tests.rs`.

**Width and representation**

| Test                                                     | Locks in                                                  |
| -------------------------------------------------------- | --------------------------------------------------------- |
| `val_new_stays_on_the_small_fast_path`                   | a narrow value never allocates the wide representation    |
| `val_new_wide_auto_narrows_to_small_at_128_bits_or_less` | a wide value at ≤128 bits collapses back to the fast path |
| `val_new_wide_masks_to_the_declared_width`               | bits above the declared width are dropped, never kept     |
| `checked_width_accepts_up_to_the_shared_max_width`       | the width ceiling matches `mimz_core::width_rules`'s own  |
| `concat_can_exceed_128_bits`                             | `{a, b}` may cross the fast-path boundary                 |

**Wide (>128-bit) arithmetic**

| Test                                                   | Locks in                                                                                                                                  |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `wide_unsigned_add_carries_past_128_bits`              | carry propagates across limbs                                                                                                             |
| `wide_neg_of_a_512_bit_value`                          | two's-complement negation at 512 bits                                                                                                     |
| `wide_bitand_of_two_512_bit_values`                    | bitwise ops are element-wise across limbs                                                                                                 |
| `wide_eq_compares_two_equal_512_bit_values`            | equality over multi-limb values                                                                                                           |
| `wide_lt_compares_signed_512_bit_values`               | signed ordering over multi-limb values                                                                                                    |
| `wide_shl_crosses_a_limb_boundary_in_a_512_bit_result` | a shift that straddles two limbs                                                                                                          |
| `wide_extend_builtin_widens_past_128_bits`             | `extend` into a wide target                                                                                                               |
| `builtin_abs_wide_negative`                            | `abs` of a wide negative value                                                                                                            |
| `builtin_trunc_wide_limb_count`                        | `trunc` drops whole limbs correctly                                                                                                       |
| `pattern_matches_handles_wide_value_no_saturation`     | a `match` pattern over a wide value does not saturate                                                                                     |
| `pattern_matches_never_panics_on_an_unlowered_variant` | BUG-40: an enum pattern naming a nonexistent variant just fails to match, instead of hitting `comb::eval_outputs`'s missing lowering pass |

**Operators and Verilog agreement**

BUG-30 (`docs/audit/bugs.md`): `<<` now GROWS by the shift amount instead of
widening to an ambient context width — its declared type already bounds the
true value, so no context threading is needed. The three former
`shl_widens_to_context_like_verilog` / `shl_self_determined_preserves_left_operand_width` /
`shl_chain_stays_at_shared_context_width` tests were replaced by the
grows-by-exactly-the-amount tests below.

| Test                                                            | Locks in                                                                                                                            |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `shl_with_a_constant_amount_grows_by_exactly_that_amount`       | BUG-30: `<<` with a known compile-time amount grows the width by exactly that amount, no truncation                                 |
| `shl_with_a_dynamic_amount_grows_by_the_amounts_own_worst_case` | with no compile-time amount, width grows by the shift amount's own worst case (`2^width - 1`), matching `width_rules::shift_result` |
| `shl_then_shr_loses_no_bits_once_shl_grows`                     | BUG-11/BUG-30: `(a << 2) >> 2` recovers the original value exactly, since `<<` growing first means no bits are ever truncated       |
| `shl_rejects_a_signed_shift_amount`                             | a signed shift amount is an error (`S0221`), not a silent reinterpretation                                                          |
| `bitand_widens_a_narrower_literal_operand`                      | a bare literal adapts to the sized operand                                                                                          |
| `cmp_eq_signed_different_widths`                                | signed comparison across widths sign-extends first                                                                                  |
| `sub_of_two_signed_values_is_signed`                            | signedness propagates through `-`                                                                                                   |
| `sub_of_two_unsigned_values_is_unsigned`                        | …and unsignedness does too (BUG-22: `binary_known`'s `Sub` arm used to hardcode `signed: true`)                                     |

**Unknown-value (`x`) taint — extern modules in warn mode**

| Test                            | Locks in                                             |
| ------------------------------- | ---------------------------------------------------- |
| `known_vals_are_never_tainted`  | an ordinary value never carries the unknown flag     |
| `unknown_val_taints_binary_ops` | any binary op with an unknown operand yields unknown |
| `unknown_val_taints_unary_ops`  | …and so does any unary op                            |

**`fn` bodies**

| Test                                                         | Locks in                                                                                          |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `fn_call_arity_mismatch_is_err_not_panic`                    | calling a `fn` with the wrong argument count is a clean `Err`, not a panic                        |
| `fn_call_sign_extends_narrower_signed_arg_to_wider_param`    | BUG-7: a narrower signed argument sign-extends (not zero-masks) when bound to a wider `fn` param  |
| `fn_loop_with_return_finds_first_match_in_sim`               | a `loop` + `return` inside a `fn` body finds the first match when interpreted                     |
| `fn_loop_with_return_first_match_wins_on_duplicate_in_sim`   | on a duplicate match, `loop` + `return` returns the FIRST (lowest-index) match                    |
| `fn_loop_over_budget_errors_in_sim`                          | a `loop` past the unroll budget errors instead of hanging                                         |
| `fn_foreach_range_form_with_return_finds_first_match_in_sim` | `foreach i in 0..N` + `return` lowers via `ast::lower_foreach_fn` and finds the first match       |
| `fn_foreach_elements_form_with_return_finds_match_in_sim`    | elements-form `foreach v in vals` + `return v` on match propagates as an early return             |
| `fn_foreach_elements_form_no_match_falls_through_in_sim`     | elements-form `foreach` with no match falls through to the tail expression, not a spurious return |

**Negative literals (BUG-43)**

BUG-43 (`docs/audit/bugs.md`): a negative literal is a CONSTANT, not an
operation applied to its magnitude — negating "in place" at the literal's own
minimal width silently wrapped instead of sign-extending correctly.

| Test                                                    | Locks in                                                                                            |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `negated_literal_sign_extends_into_a_wider_signed_slot` | `-n` at `natural_width(n) + 1` bits signed sign-extends correctly into any wider signed destination |
| `negated_literal_minus_one_is_all_ones_not_one`         | `-1` evaluates to `-1`, not `+1` (the narrowest, most common case)                                  |
| `negated_literal_handles_a_wide_magnitude`              | a magnitude needing more than 128 bits still negates correctly via the wide path                    |
