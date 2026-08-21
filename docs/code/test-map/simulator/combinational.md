# Unit: combinational evaluator (`crates/mimz-sim/src/sim/comb.rs`, 22 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The Phase 1.5 simulator's combinational slice behind `mimz eval`.

| Test                                                                   | Locks in                                                                                                                                 |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `adder_grows_losslessly`                                               | `+` grows `bits[W]` → `bits[W+1]`; 200+100 carries into the 9th bit (no wrap)                                                            |
| `wrapping_add_keeps_width`                                             | `+%` keeps width and wraps (300 → 44 in `bits[8]`)                                                                                       |
| `comparator_if_and_compares`                                           | `==`, `>`, and a value `if/else` evaluate together                                                                                       |
| `mux_match_selects`                                                    | `match` on `bits[2]` picks the right arm                                                                                                 |
| `chained_comparison_window`                                            | `lo <= value <= hi` (desugared) incl. the inclusive boundary                                                                             |
| `rejects_sequential_logic`                                             | a module with `reg`/`on` is rejected with a clear message (out of the comb slice)                                                        |
| `reports_missing_input`                                                | a missing `--in` value names the input                                                                                                   |
| `replication_repeats_the_group`                                        | `{2{a}}`/`{3{a}}` repeat the group (a=0b1010 → 0xAA / 0xAAA) (A1)                                                                        |
| `dont_care_match_picks_the_masked_arm`                                 | `0b1??`/`0b01?`/`_` priority decoder picks the right arm per input (A2)                                                                  |
| `shift_left_zero_amt`                                                  | `a << 0` is identity                                                                                                                     |
| `shift_right_zero_amt`                                                 | `a >> 0` is identity                                                                                                                     |
| `shift_left_max_width`                                                 | `1 << 127` yields `2¹²⁷` (max valid shift)                                                                                               |
| `shift_left_exceeding_width_is_zero`                                   | `1 << 128`, `1 << 200`, `1 << u128::MAX` → 0 (regression for the `as u32` bug)                                                           |
| `shift_right_exceeding_width_is_zero`                                  | `2 >> 128`, `2 >> 200`, `2 >> u128::MAX` → 0                                                                                             |
| `shift_left_bit_32_set_in_amt`                                         | `1 << (1 << 32)` → 0 (the specific `as u32` truncation trigger)                                                                          |
| `shift_right_bit_32_set_in_amt`                                        | `(1 << 63) >> (1 << 32)` → 0                                                                                                             |
| `eval_outputs_handles_a_wide_input`                                    | an input wider than 128 bits evaluates through the wide (`bits`/`wide`) path                                                             |
| `sim_fn_call_mac_basic`                                                | a user `fn` call (multiply-accumulate) evaluates inside `mimz eval`                                                                      |
| `sim_fn_call_mac_wrap_truncation`                                      | the same call truncates exactly like the emitted Verilog would                                                                           |
| `chained_signed_shift_context_extends_before_the_shift`                | BUG-34: a signed `>>` feeding an outer `<<` sign-extends to the FINAL enclosing width before either shift runs                           |
| `zero_length_array_param_index_is_a_clean_err_not_a_panic`             | indexing a zero-length array parameter is an `Err`, never a panic                                                                        |
| `match_pattern_referencing_an_unknown_enum_is_a_clean_err_not_a_panic` | BUG-40: a fuzzed `match` pattern naming an enum that doesn't exist falls through to "no arm matched" instead of hitting `unreachable!()` |
