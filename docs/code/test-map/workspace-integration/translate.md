# Integration: translate (`tests/translate.rs`, 15 tests - the four-flavor oracle + the `--order` pretty-printer + `--romanize-names` + the sidecar name-map)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The `examples/{english,tanglish,tamil}/` folders are byte-identical
keyword-swaps (R9), so they validate the reskin against committed truth. Four
cover `--order` (the `pretty` AST printer): it reformats and drops comments, so
its oracle is semantic (same Verilog) + idempotency, not bytes. The final three
cover `--romanize-names` over the pure-Tamil showcase (Tamil identifiers → Latin,
opt-in and one-way; the default stays lossless).

| Test                                                               | Locks in                                                                                                                                                                       |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `round_trip_to_every_flavor_is_byte_identical`                     | translate-and-back reproduces the canonical source byte-for-byte (lossless; anchored past alias normalize)                                                                     |
| `translating_english_matches_the_committed_flavor_token_for_token` | translating english `X` to flavor `T` lexes identically to the committed `T/X` (comments excluded)                                                                             |
| `every_keyword_token_is_in_the_target_flavor`                      | the reskin actually fires - English `module` is gone, Tamil `தொகுதி` present                                                                                                   |
| `pretty_print_preserves_verilog_across_flavor_and_order`           | every import-free example × flavor × order pretty-prints to byte-identical Verilog (meaning preserved)                                                                         |
| `pretty_print_is_idempotent`                                       | the pretty-printer is a stable canonical form (re-printing its own output is a fixed point), all examples                                                                      |
| `thamizh_order_emits_the_directive`                                | thamizh output starts with `syntax thamizh` / `இலக்கணம் தமிழ்`; code order emits none                                                                                          |
| `cli_translate_order_thamizh_compiles`                             | `--order thamizh --to tamil` on the traffic light yields compilable, same-Verilog Tamil SOV source                                                                             |
| `romanize_names_converts_tamil_identifiers_to_latin`               | `--romanize-names` rewrites Tamil identifiers to Latin in the CODE (comments keep the original); no Tamil-script char survives outside comments                                |
| `romanized_translation_compiles_to_the_same_verilog`               | romanizing then compiling a pure-Tamil file is byte-identical to compiling the original - the romanization matches the emitter's, so meaning is preserved                      |
| `pure_tamil_round_trips_losslessly`                                | the DEFAULT (no flag) still round-trips Tamil → English → Tamil byte-for-byte - the lossless contract holds for Tamil-named files too                                          |
| `romanized_round_trips_losslessly_via_the_name_map`                | romanize (capturing the `NameMap`) then `restore_with_map` reproduces the canonical Tamil source - the one-way romanization made lossless by the sidecar                       |
| `cli_romanize_then_restore_round_trips`                            | end-to-end through the binary: `--romanize-names -o` writes a parseable `<out>.names.json`; a reverse run with `--names-map` restores the exact Tamil source                   |
| `number_abutting_tamil_keeps_a_separator_when_reskinned`           | fuzz-audit regression: `42தொகுதி`/`42கணக்கி` (number + Tamil token, script change as the only separator) stays lexable + token-equivalent after reskin (guard inserts a space) |
| `fn_keyword_translates_across_all_flavors`                         | the `fn` keyword reskins correctly in every flavor (it was the newest keyword when added)                                                                                      |
| `pretty_print_thamizh_flips_the_test_header_and_reparses`          | `--order thamizh` flips a `test "…" for M(args)` header into `M(args) kaaga "…" sodhanai` and the result re-parses to the SAME tree (the B7 oracle)                            |
