# Benchmark harness (`src/bin/mimz-bench/`, 6 unit tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The harness itself (docs in [`12-benchmark.md`](12-benchmark.md))
re-measures what this suite asserts — rates and timings instead of
pass/fail — so its own logic is unit-tested here:

| Test                                       | Locks in                                                       |
| ------------------------------------------ | -------------------------------------------------------------- |
| `rate_percent_handles_zero_and_partial`    | rate math (0/0 reads as 100%, never NaN)                       |
| `expect_header_parses_only_the_convention` | the `// expect: Exxxx` fixture-header parse, same as errors.rs |
| `banner_strip_matches_the_golden_rule`     | banner stripping byte-matches the golden test's rule           |
| `median_is_the_middle_run`                 | timing aggregation (median, robust to one cold run)            |
| `report_renders_a_complete_page` (html)    | the HTML report renders whole: charts, tables, embedded JSON   |
| `failures_flip_the_verdict_and_are_listed` | a failing validation turns the verdict red and is named        |

The `criterion` micro-benchmark harness (`benches/compile.rs`, run with
`cargo bench`) carries **no `#[test]`s** — `criterion` benchmarks aren't
test functions, so it doesn't affect the count above. It's a separate
performance tool, not part of the assertion suite.
