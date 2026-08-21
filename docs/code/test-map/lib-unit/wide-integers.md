# Unit: wide integers and width rules (`bits` 17, `wide` 18, `width_rules` 19 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Min-Mozhi allows bit-vectors wider than a machine word, so values live on
two representations: a `u128` fast path for the common case, and a
multi-limb `wide` representation past it. `bits.rs` owns the boundary
between them, `wide.rs` owns the multi-limb arithmetic, and
`width_rules.rs` owns the ONE table both the checker and the simulator
consult for "how wide is the result of this operator?".

## bits.rs (17 tests) — the small/wide boundary

| Test                                                                   | Locks in                                                    |
| ---------------------------------------------------------------------- | ----------------------------------------------------------- |
| `from_u128_impl_matches_bits_small`                                    | the fast-path constructor and the general one agree         |
| `from_limbs_auto_narrows_at_128_bits_or_less`                          | a wide value that fits collapses back to the fast path      |
| `from_limbs_stays_wide_past_128_bits`                                  | …and stays wide when it does not                            |
| `to_limbs_promotes_a_small_value`                                      | promotion in the other direction is lossless                |
| `retag_trims_a_padded_wide_vector_to_the_new_widths_limb_count`        | re-tagging a width drops the now-unused limbs               |
| `mask_of_128_or_more_is_all_ones`                                      | the width mask saturates instead of shifting out of range   |
| `natural_width_of_zero_is_one`                                         | zero is one bit wide, not zero                              |
| `natural_width_of_a_small_value_is_tight`                              | a value's natural width is minimal                          |
| `natural_width_of_a_wide_value_scans_limbs`                            | …including across limbs                                     |
| `top_bit_set_reads_the_correct_position`                               | sign detection reads the declared top bit, not the limb's   |
| `leading_ones_counts_from_the_top_bit_down`                            | leading-ones counting is width-relative                     |
| `leading_ones_of_all_ones_is_the_full_width`                           | …and saturates correctly                                    |
| `shrink_of_a_nonnegative_value_finds_the_tight_unsigned_width`         | shrinking a positive value finds the minimal unsigned width |
| `shrink_of_negative_one_round_trips`                                   | shrinking `-1` and re-expanding gives `-1`                  |
| `shrink_of_negative_four_reproduces_the_same_value_at_a_smaller_width` | …and the same for other negatives                           |
| `shrink_of_zero_is_never_reported_negative`                            | zero never comes back signed (BUG-13's original symptom)    |
| `bits_to_decimal_string_renders_a_small_negative_value`                | decimal rendering handles the signed small path             |

## wide.rs (18 tests) — multi-limb arithmetic

| Test                                                              | Locks in                          |
| ----------------------------------------------------------------- | --------------------------------- |
| `from_u128_round_trips_through_bit_at`                            | bit addressing across limbs       |
| `add_carries_across_a_limb_boundary`                              | addition carry                    |
| `sub_borrows_across_a_limb_boundary`                              | subtraction borrow                |
| `mul_of_two_wide_values_carries_correctly`                        | multiplication carry              |
| `neg_of_one_is_all_ones`                                          | two's-complement negation         |
| `shl_crosses_a_limb_boundary`                                     | left shift across limbs           |
| `shl_masks_bits_that_overflow_result_width`                       | …and masks what falls off the top |
| `shr_crosses_a_limb_boundary`                                     | right shift across limbs          |
| `bitwise_ops_are_elementwise`                                     | `&`/`\|`/`^` are per-limb         |
| `is_zero_and_count_ones`                                          | population count and zero test    |
| `cmp_unsigned_orders_by_magnitude`                                | unsigned ordering                 |
| `cmp_signed_a_negative_value_is_less_than_a_positive_one`         | signed ordering                   |
| `extend_zero_fills_an_unsigned_value`                             | zero extension                    |
| `extend_sign_fills_a_negative_signed_value`                       | sign extension                    |
| `to_binary_string_has_no_leading_zeros_except_for_the_value_zero` | binary rendering                  |
| `to_decimal_string_renders_zero`                                  | decimal rendering of zero         |
| `to_decimal_string_matches_a_known_large_unsigned_value`          | …of a large unsigned value        |
| `to_decimal_string_renders_a_negative_signed_value`               | …and of a negative one            |

## width_rules.rs (19 tests) — the shared operator table

BUG-30 (`docs/audit/bugs.md`): `<<` now GROWS by the shift amount instead of
matching the left operand's width — the former `shift_result_preserves_lhs_kind`
/ `shift_result_preserves_signed_lhs` pair was renamed to `shr_*` (still true
for `>>`, which does not grow) and four new `shl_*` tests cover the growth rule.

| Test                                                            | Locks in                                                                               |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `lossless_result_add_grows_by_one_bit`                          | `+` grows by one bit                                                                   |
| `lossless_result_mul_sums_widths`                               | `*` sums the operand widths                                                            |
| `lossless_result_preserves_signed_when_both_operands_are`       | signedness survives when both sides agree                                              |
| `lossless_result_rejects_mixed_signedness`                      | …and mixing is rejected, never silently coerced                                        |
| `matched_result_returns_the_shared_kind`                        | the width-matching family (`+%`, bitwise, comparisons) keeps the shape                 |
| `matched_result_rejects_different_widths`                       | …and rejects a width mismatch                                                          |
| `matched_result_rejects_different_signedness`                   | …and a signedness mismatch                                                             |
| `shr_preserves_lhs_kind`                                        | `>>` (which never grows) takes the left operand's width                                |
| `shr_preserves_signed_lhs`                                      | …and its signedness                                                                    |
| `shift_result_rejects_signed_amount`                            | a signed shift AMOUNT is rejected, for both `<<` and `>>`                              |
| `shl_with_a_constant_amount_grows_by_exactly_that_amount`       | BUG-30: `<<` with a known compile-time amount grows by exactly that amount             |
| `shl_with_a_dynamic_amount_grows_by_the_amounts_own_worst_case` | with no compile-time amount, growth covers the amount's own worst case (`2^width - 1`) |
| `shl_preserves_signedness_while_growing`                        | a signed left operand stays signed as `<<` grows it                                    |
| `shl_growth_past_max_width_is_an_error`                         | growth past `MAX_WIDTH` is a clean `ShiftGrowthTooWide` error, not a silent clamp      |
| `slice_result_single_bit`                                       | `x[i]` is one bit                                                                      |
| `slice_result_computes_width_and_is_always_unsigned`            | `x[hi:lo]` is `hi-lo+1` bits and always unsigned                                       |
| `slice_result_rejects_out_of_range_hi`                          | an out-of-range bound is rejected                                                      |
| `slice_result_rejects_reversed_bounds`                          | …and so are reversed bounds                                                            |
| `max_width_matches_the_checkers_own_ceiling`                    | the module's ceiling equals the checker's, so neither can drift                        |

## Conformance: checker vs simulator (`crates/mimz-core/tests/width_rules_conformance.rs`, 2 tests)

The reason `width_rules.rs` exists: two independent implementations used
to compute widths (the checker's `widths` pass and the simulator's
evaluator), and they drifted. These two tests replay a shared table
through BOTH and demand the same answer.

| Test                                         | Locks in                                                                    |
| -------------------------------------------- | --------------------------------------------------------------------------- |
| `checker_and_simulator_agree_with_the_table` | for every operator row, the checker's width and the simulator's width match |
| `shift_result_matches_the_table`             | the same for shifts, whose Verilog rule is the easiest to get subtly wrong  |
