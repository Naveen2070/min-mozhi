# Integration: self-determined-width regressions (`tests/self_determined_regression.rs`, 116 tests)

> **This page is abridged**: it walks a representative subset of the suite's
> regressions (the BUG-19/20/23/24 families). The remaining tests follow the
> same shape - see the file itself for the full list.
> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Verilog computes some subexpression widths by its OWN rule ("self-determined
positions": concat members, comparison operands, `$signed`/`$unsigned`
arguments), which can differ from the width mimz checked. Where they differ
the emitter hoists the subexpression into an explicitly-sized wire. Each
test here is a named bug that this hoist exists to prevent, and most run the
result through real Icarus rather than asserting on text.

| Test                                                           | Locks in                                                                   |
| -------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `bug_19_lossless_sub_in_a_concat_hoists_exactly_one_wire`      | BUG-19: a lossless `-` inside `{…}` hoists - exactly ONE wire, not two     |
| `bug_19_lossless_sub_in_a_concat_matches_icarus`               | …and the hoisted result matches Icarus                                     |
| `bug_19_wrapping_sub_in_a_bitand_matches_icarus`               | the wrapping form inside `&` likewise                                      |
| `bug_20_slice_of_a_composite_expression_matches_icarus`        | BUG-20: slicing a composite expression needs a sized intermediate          |
| `bug_23_top_level_wrap_needs_no_hoist`                         | BUG-23: a top-level `+%` is already context-determined - no wasted wire    |
| `bug_23_wrap_directly_inside_a_concat_matches_icarus`          | …but inside a concat it does need one                                      |
| `bug_23_wrap_under_sibling_add_matches_icarus`                 | …and under a sibling `+`                                                   |
| `bug_23_wrap_under_sibling_add_inside_a_concat_matches_icarus` | …and in both at once                                                       |
| `bug_23_signed_wrap_operand_hoist_preserves_sign_extension`    | the hoisted wire keeps signedness, so sign extension still happens         |
| `bug_24_shl_under_sibling_add_matches_icarus`                  | BUG-24: a shift under a sibling `+` hoists correctly                       |
| `bug_24_regression_shift_in_if_branch_stays_unhoisted`         | …but a shift in an `if` branch must NOT hoist (over-hoisting is a bug too) |
| `bug_24_regression_nested_shift_lhs_of_shift_stays_unhoisted`  | …nor a shift on the left of another shift                                  |
