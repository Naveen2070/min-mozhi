# Integration: fmt (`tests/fmt.rs`, 9 tests — run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

`mimz fmt` — the in-place keyword-flavor normalizer (the lossless `translate`
token reskin, not the comment-dropping `--order` printer).

| Test                                              | Locks in                                                                                 |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `normalizes_to_majority_and_is_idempotent`        | a mixed file normalizes to its majority flavor; comments survive; re-run no-ops          |
| `to_flag_forces_the_target_flavor`                | `--to tamil` overrides the majority; comment preserved                                   |
| `strict_warns_and_fails_on_mixed_but_still_fixes` | `--strict` warns + exits non-zero on a mixed file, still writing the fix                 |
| `strict_is_clean_on_a_single_flavor_file`         | a single-flavor file passes `--strict` (no warning, exit 0)                              |
| `a_keyword_free_file_is_left_intact`              | a comment-only file (no keywords) normalizes to a no-op                                  |
| `a_non_lexing_file_is_a_clean_error`              | a lex error (e.g. `/`) is reported, exits non-zero, and does not clobber input           |
| `output_flag_leaves_the_input_untouched`          | `-o <dest>` writes the result elsewhere; the input is unchanged                          |
| `output_to_the_input_path_round_trips`            | `-o <input>` writes atomically to a temp file then renames — input is never half-written |
| `unknown_to_flavor_is_a_clean_error`              | `--to wibble` fails with a clear "unknown flavor" message, never a panic                 |
