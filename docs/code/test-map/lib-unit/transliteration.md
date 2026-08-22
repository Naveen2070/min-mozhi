# Unit: transliteration (`crates/mimz-core/src/emit_verilog/translit.rs`, 7 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

| Test                                              | Locks in                                                                               |
| ------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `pure_tamil_words_romanize_readably`              | விளக்கு → `villakku`, நிலை → `nilai` - the readable-output promise                     |
| `ascii_and_mixed_names_keep_their_ascii`          | ASCII passes through untouched, even mixed into a Tamil name                           |
| `non_tamil_unicode_falls_back_to_hex`             | other scripts → `_uXXXX`, never dropped                                                |
| `results_always_start_like_an_identifier`         | output is always a valid Verilog identifier start                                      |
| `the_two_n_letters_romanize_identically`          | ந/ன → `n` is a DOCUMENTED collision; the suffix counter disambiguates                  |
| `enum_construct_romanizes_enum_and_variant_names` | `Enum.Variant(payload)` construction sites romanize BOTH the enum and the variant name |
| `translate_preserves_fn_return_and_if_semantics`  | romanizing a `fn` with `return`/`if` does not change what the function computes        |
