# Unit: morph (`crates/mimz-core/src/morph.rs`, 14 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Error-language selection + Tamil case-suffix inflection (Phase 1.8, spec/04 section 5),
the W0001 mixed-flavor lint, and the structured-arg / English-fallback guards.

| Test                                                      | Locks in                                                                                                                                     |
| --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `majority_picks_the_dominant_keyword_flavor`              | all-English vs all-Tamil keyword files resolve to English / Tamil                                                                            |
| `majority_falls_back_to_english_with_no_keywords`         | a keyword-free token stream defaults to English                                                                                              |
| `majority_breaks_ties_toward_the_earliest_keyword_column` | a flavor tie resolves deterministically to the earliest keyword column                                                                       |
| `effective_lang_override_beats_majority`                  | `--lang` wins over the file majority; absence uses the majority                                                                              |
| `parse_lang_matches_translate_flavor`                     | `--lang` parsing reuses `translate::parse_flavor` (spellings never drift)                                                                    |
| `inflect_attaches_each_case_suffix`                       | each case attaches its spec suffix; Latin stems hyphenate, Tamil joins, English none                                                         |
| `inflect_of_an_empty_stem_is_empty_not_a_bare_suffix`     | inflecting an empty stem yields empty - never a dangling case suffix                                                                         |
| `suffix_table_has_every_case`                             | `lang/case_suffixes.toml` parses and defines all four cases (startup validation)                                                             |
| `localized_is_none_for_uncovered_codes_and_for_english`   | the catalog returns `None` for English and for codes it does not localize                                                                    |
| `fill_inflects_the_stub_template`                         | the template's `{name.dat}` slot renders the inflected identifier                                                                            |
| `arg_code_without_args_falls_back_to_english`             | a code whose template has `{expected}/{found}` but no args attached leaves a leftover `{`, so `localized_msg` returns `None` - the fail-safe |
| `fill_with_empty_name_leaves_no_stray_fragment`           | `fill` with an empty `name` renders cleanly - no orphaned bracket or suffix                                                                  |
| `flavor_mix_warns_only_when_tamil_meets_the_others`       | W0001 fires only when Tamil mixes with English/Tanglish (the SVO pair mixes freely)                                                          |
| `flavor_mix_warning_is_a_nonfatal_w0001`                  | the mixed-flavor diagnostic is a non-fatal W0001 warning, not an error                                                                       |
