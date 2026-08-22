# Unit: version (`crates/mimz-core/src/version.rs`, 3 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The two version axes - compiler (crate) vs language edition - and the
`EDITION_HISTORY` source of truth (Workstream B).

| Test                                        | Locks in                                                                                             |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `current_is_the_last_history_row`           | `current()` is the tail of `EDITION_HISTORY`, which stays ordered oldest-first by (year, code)       |
| `keyword_set_version_matches_keywords_toml` | `KEYWORD_SET_VERSION` == `lang/keywords.toml`'s `version` == the current edition's `code` (no drift) |
| `version_block_shows_both_axes`             | `mimz --version` block has the variant on top + the compiler and edition (language) lines            |
