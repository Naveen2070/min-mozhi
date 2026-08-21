# Unit: checker internals (`consteval` 6, `drivers` 2, `names` 3 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Small test pockets living inside individual checker passes, separate from
the by-error-code `checker/tests/` tables above.

## checker/consteval.rs (6 tests)

| Test                                                    | Locks in                                                                |
| ------------------------------------------------------- | ----------------------------------------------------------------------- |
| `clog2_bits_matches_spec_table`                         | `clog2` agrees with the table in the spec, value for value              |
| `small_arithmetic_still_works_exactly_as_before`        | ordinary constant folding is unchanged by the wide-integer work         |
| `a_literal_past_the_old_i128_ceiling_folds_cleanly`     | a literal past 128 bits folds instead of overflowing                    |
| `addition_past_128_bits_folds_to_a_wide_constval`       | …and arithmetic promotes to the wide `ConstVal` representation          |
| `negation_round_trips_through_shrink`                   | negating and re-shrinking a wide value returns the same number (BUG-13) |
| `a_constant_exceeding_max_width_is_a_clean_e0202_error` | past the ceiling it is `E0202`, never a wrap or a panic                 |

## checker/drivers.rs (2 tests)

| Test                                                            | Locks in                                                                   |
| --------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `separate_on_block_writing_the_same_name_is_still_multi_driver` | splitting a register across two `on` blocks is still E0503                 |
| `sync_loop_body_is_one_driver_block`                            | a `sync loop`'s generated body counts as ONE driver, not one per iteration |

## checker/names/tests.rs (3 tests)

The lowering passes generate hidden signal names. These prove a user name
that collides with one is caught as an ordinary duplicate (`E0003`) rather
than silently overwritten.

| Test                                                 | Locks in                            |
| ---------------------------------------------------- | ----------------------------------- |
| `sync_loop_generated_name_collision_is_e0003`        | for `sync loop`'s generated names   |
| `sync_double_flop_generated_name_collision_is_e0003` | for `sync.double_flop`'s hidden reg |
| `sync_pulse_generated_name_collision_is_e0003`       | for `sync.pulse`'s hidden regs      |
