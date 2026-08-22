# Unit: explain (`crates/mimz-core/src/explain.rs`, 3 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The 8.1 long-form diagnostic catalog behind `mimz explain <CODE>`.

| Test                                       | Locks in                                                                                       |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| `every_checker_code_has_an_explanation`    | every `ALL_CHECKER_CODES` entry has long-form text - a new checker code can't ship without one |
| `table_is_sorted_unique_and_self_labelled` | the `EXPLANATIONS` table is ordered, duplicate-free, and each entry opens with its own code    |
| `lookup_is_case_insensitive_and_trims`     | `explain("e0501")` / `" E0501 "` resolve; unknown codes return `None`                          |
