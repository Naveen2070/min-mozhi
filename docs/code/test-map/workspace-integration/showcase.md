# Integration: showcase (`tests/showcase.rs`, 6 tests — run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Mirrors `tests/examples.rs` for `showcase/`, the demo set behind the web
playground and documentation site: same flavor-identity and golden-file
rules, plus the pure-Tamil equivalence check.

| Test                                       | Locks in                                                                      |
| ------------------------------------------ | ----------------------------------------------------------------------------- |
| `showcase_every_example_checks_clean`      | every showcase file passes `mimz check` with zero diagnostics                 |
| `showcase_every_example_compiles`          | every showcase file emits Verilog without error                               |
| `showcase_all_four_flavors_identical`      | english/tanglish/tamil/mixed showcase folders emit byte-identical Verilog     |
| `showcase_emitted_verilog_matches_goldens` | showcase output matches `tests/golden/showcase_*.v`                           |
| `showcase_pure_tamil_equivalent`           | the pure-Tamil showcase circuits emit Verilog equivalent to their base flavor |
| `showcase_pure_tamil_match_goldens`        | pure-Tamil showcase output matches its own golden files                       |
