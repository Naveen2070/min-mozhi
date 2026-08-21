# Unit: source normalization + spans (`crates/mimz-core/src/lib.rs` 1, `crates/mimz-core/src/span.rs` 2, 3 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

| Test                                      | Locks in                                                                                                                                                                          |
| ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nfc_normalize_composes_decomposed_forms` | every source string is NFC-normalized on the way in, so `e`+◌́ and `é` are ONE identifier — the precondition every span, keyword lookup, and Tamil comparison downstream relies on |
| `line_col_finds_the_first_line`           | `Span::line_col` reports the right (1-based) line/column on the file's first line                                                                                                 |
| `line_col_finds_a_later_line`             | …and correctly across a `\n` boundary onto a later line                                                                                                                           |
