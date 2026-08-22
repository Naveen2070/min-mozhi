# Grammar-sync (`tests/grammar_sync.rs`, 6 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Same philosophy as docs-sync, for the keyword data: the keyword table is
data, so the TextMate grammar and the human-readable spec mirror can silently
drift. Whole-member matching throughout, because `in` is a substring of
`include` - a plain `contains` would pass vacuously. When one fails: fix the
grammar / the spec, don't weaken the test.

| Test                                           | Locks in                                                                                                                                              |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_keyword_spelling_is_in_the_grammar`     | every spelling (canonical + aliases) appears as a whole alternation member in the VS Code grammar                                                     |
| `every_reserved_word_is_marked_invalid`        | every reserved word appears in the grammar's `invalid.illegal` rule                                                                                   |
| `spec_03_keyword_table_matches_keywords_toml`  | every spelling appears in `spec/03` as a backtick word - the spec mirror can't drift after the v1 lock                                                |
| `spec_04_uses_no_superseded_keyword_spellings` | `spec/04`'s worked examples contain none of the 14 superseded v1 spellings (whole-word, Tamil-aware)                                                  |
| `keywords_toml_has_no_superseded_spelling`     | a superseded v1 spelling may never return in `lang/keywords.toml` as a canonical spelling or any alias - guards the reintroduction risk at the source |
| `grammar_and_extension_manifest_agree`         | `package.json` registers `.mimz` and its scope name matches the grammar                                                                               |
