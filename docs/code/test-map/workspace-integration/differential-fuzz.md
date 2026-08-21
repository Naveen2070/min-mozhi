# Integration: differential fuzzing (`tests/differential_fuzz.rs`, 6 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

A generative differential harness: it BUILDS random-but-valid Min-Mozhi
programs, compiles them, and runs the result through both our simulator and
real Icarus, demanding identical waveforms. Deterministic (seeded), so a
failure is reproducible. This is the test that found BUG-23.

| Test                                                                            | Locks in                                                                                                                                                           |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `differential_fuzz_generates_checker_valid_programs`                            | the generator only emits programs the checker accepts (else the run is vacuous)                                                                                    |
| `differential_fuzz_matches_icarus`                                              | every generated combinational program simulates identically to Icarus                                                                                              |
| `task9_reduction_fuzz_bias_reaches_both_bare_and_extended_operands`             | round-6 Task 9: the `nand`/`nor`/`xnor` generator bias reaches both bare-port and `extend(...)`-wrapped operand shapes, not unconditional extend-wrapping (BUG-60) |
| `differential_fuzz_clocked_generates_checker_valid_programs`                    | the same guarantee for the clocked generator                                                                                                                       |
| `task9_instance_port_connection_reaches_a_hoisting_expression_within_400_seeds` | round-8 Task 9: the clocked generator reaches a hoisting cross-instance port connection (BUG-70's shape) within the first 400 seeds                                |
| `differential_fuzz_clocked_matches_icarus`                                      | …and clocked designs match Icarus cycle for cycle                                                                                                                  |

`gen_special_leaves` is what decides which SHAPES the generator can reach at
all — a `fn` call, a nested `fn` call (`inner{w}(x)` offered to the outer
`fn`'s body as a `special` leaf, round-7 Task 11, so [BUG-67](../audit/bugs.md)'s
shape is reachable), a `const`-bounded slice, and — clocked only — a plain
instance-port read, an array-instance-port read and a `mem` read. Depth
(`MIMZ_DIFF_FUZZ_N`) cannot compensate for a shape the vocabulary does not
contain: see [`gaps.md`](../audit/gaps.md) GAP-13 direction 2.
