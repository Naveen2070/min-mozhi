# Integration: morph (`tests/morph.rs`, 20 tests — run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The end-to-end `--lang` path through `mimz check`/`compile`. The catalog is now
the native-authored one (34 of the 75 checker E-codes, decision C3); these assert the
MECHANISM, the structured-arg interpolation, the W0001 mixed-flavor lint, and —
crucially — the **English-fallback invariant**: codes the catalog does not cover
(E0405) render byte-identically across every flavor.

| Test                                                 | Locks in                                                                                                                                                                          |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `majority_and_effective_lang_track_the_keywords`     | selection: majority + override, via the public lib API                                                                                                                            |
| `inflect_attaches_the_spec_case_suffixes`            | inflection: the four suffixes across Tamil / Tanglish / English                                                                                                                   |
| `covered_code_renders_tamil_with_the_inflected_name` | E0501 under `--lang ta` shows the localized Tamil line with `y-க்கு`                                                                                                              |
| `covered_code_auto_selects_tamil_from_the_file`      | a Tamil-keyword file with no `--lang` auto-renders E0501 in Tamil                                                                                                                 |
| `covered_code_stays_english_with_lang_en`            | `--lang en` keeps the original English wording                                                                                                                                    |
| `uncovered_code_is_identical_across_languages`       | **the fallback invariant** — E0405 is byte-identical under en / ta / tanglish                                                                                                     |
| `compile_also_localizes_diagnostics`                 | the localization path is shared — `compile --lang ta` shows Tamil E0501 too                                                                                                       |
| `unknown_lang_is_a_clean_error`                      | `--lang klingon` fails with a clear "unknown language" message                                                                                                                    |
| `e0502_renders_tamil`                                | an undriven output (E0502, a `{name}`-only template) localizes in Tamil                                                                                                           |
| `e0505_renders_tamil`                                | `=` on a reg (E0505) localizes under `--lang ta`                                                                                                                                  |
| `e0202_renders_tanglish_nameless`                    | a name-less template (E0202 const overflow) localizes with no `{name}` slot                                                                                                       |
| `e0401_interpolates_expected_and_found`              | E0401's `{expected}`/`{found}` widths interpolate; no `{token}` leaks                                                                                                             |
| `e0402_interpolates_op_lhs_rhs`                      | E0402's `{op}`/`{lhs}`/`{rhs}` (operator + both operand widths) interpolate                                                                                                       |
| `e0408_interpolates_first_and_second`                | E0408's `{first}`/`{second}` arm types interpolate (width-inferred position)                                                                                                      |
| `e0601_interpolates_type`                            | E0601's `{type}` scrutinee type interpolates on a non-exhaustive `match`                                                                                                          |
| `message_catalog_keys_are_real_checker_codes`        | every `[message.Exxxx]` key in `lang/messages.toml` is a real `ALL_CHECKER_CODES` code — a typo'd key (dead localization) fails naming it                                         |
| `message_catalog_placeholders_are_known_tokens`      | every active `{token}` in `lang/messages.toml` is one `morph::fill` fills — a typo'd placeholder / unsupplied arg would silently fall back to English forever; this fails instead |
| `mixing_tamil_with_english_warns_but_check_succeeds` | a Tamil+English file emits W0001 yet `check` still succeeds (non-fatal lint)                                                                                                      |
| `a_single_flavor_file_has_no_mix_warning`            | a clean single-flavor file does not warn                                                                                                                                          |
| `json_check_carries_the_warning_and_still_succeeds`  | `--json` includes the W0001 entry with `"severity":"warning"`, exit 0                                                                                                             |
