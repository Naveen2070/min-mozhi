# Unit: translate (`crates/mimz-core/src/translate.rs`, 10 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The keyword-flavor reskin behind `mimz translate --to`, plus the opt-in
`--romanize-names` identifier rewrite (reuses the emitter's `romanize`) and the
reversible sidecar name-map (`romanize_with_map` / `restore_with_map`).

| Test                                                             | Locks in                                                                                       |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `parse_flavor_accepts_the_three_columns`                         | `english`/`tanglish`/`tamil` (case-insensitive) parse; junk → `None`                           |
| `reskins_keywords_keeps_everything_else`                         | keywords swap; comments, layout, identifiers, numbers stay verbatim                            |
| `translating_to_the_same_flavor_is_identity_for_canonical_input` | canonical English → English is a no-op                                                         |
| `romanize_names_rewrites_tamil_identifiers_only_when_asked`      | `--romanize-names` turns `கணக்கு` → `kannakku`; the default leaves the Tamil name              |
| `romanize_names_uniques_against_an_existing_ascii_name`          | a romanization clashing with an ASCII name gets `_2` — names never silently merge              |
| `romanize_with_map_returns_the_inverse_map`                      | the sidecar map is keyed by the Latin spelling → original Tamil (`kannakku` → `கணக்கு`)        |
| `restore_with_map_inverts_romanize`                              | `restore(romanize(src), map)` reproduces the canonical Tamil source — the round-trip identity  |
| `name_map_json_round_trips`                                      | `NameMap` serializes and deserializes through `serde_json` unchanged                           |
| `masked_int_q_does_not_glue_onto_romanized_identifier`           | fuzz regression: a `MaskedInt` ending in `?` abutting a romanized identifier keeps a separator |
| `masked_int_q_does_not_glue_onto_english_keyword`                | …and the same when it abuts an English keyword                                                 |
