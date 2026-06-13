# 10 — Test Map: What Is Covered, What Isn't, and Why

Every test, what it locks in, and what a failure means. Update this page
when tests are added or removed (the count below is asserted nowhere —
this page is the human ledger).

**182 tests** as of 2026-06-13: 141 lib unit + 3 LSP unit (bin) + 6 benchmark unit (bin) + 9 example integration + 6 eval integration + 3 translate integration + 2 Icarus differential + 4 error-fixture + 1 LSP smoke + 4 docs-sync + 3 grammar-sync. (The error-fixture tests are data-driven over ~67 broken `.mimz` fixtures; one locks `ALL_CHECKER_CODES` — now `pub` in `src/diag.rs` — to the 11-checker.md catalog, one locks the `--json` wire format.) The 2026-06-13 quick-wins block added the tooling tests below: `explain` (+3), `translate` (+3 unit, +3 integration), `sim::comb` (+7 unit, +6 `eval` integration).

## Unit: keyword table (`src/lexer/keywords.rs`, 5 tests)

| Test                                        | Locks in                                              | If it fails…                                  |
| ------------------------------------------- | ----------------------------------------------------- | --------------------------------------------- |
| `all_three_flavors_resolve_to_same_keyword` | EN/Tanglish/Tamil spellings → one `Kw` token          | `keywords.toml` edit broke a mapping          |
| `flavors_are_recorded`                      | the lexer remembers which column a spelling came from | flavor tracking broke (P1.8 depends on it)    |
| `include_is_an_alias_for_import`            | `include` lexes to the exact same token as `import`   | the alias mechanism or table entry broke      |
| `fall_is_reserved`                          | `fall` errors as reserved, is not a keyword           | someone un-reserved `fall` without a decision |
| `the_v03_backlog_keywords_are_reserved`     | all 8 v0.3 backlog words (`secret`…`await`) reserved  | a backlog word was claimed without a decision |

Note: the table's structural rules (disjoint columns, known keys, valid
TOML) need no dedicated test — the `LazyLock` panics at startup, so
**every** test fails if the table is broken. That's by design.

## Unit: lexer (`src/lexer/tests.rs`, 8 tests)

| Test                                       | Locks in                                                        |
| ------------------------------------------ | --------------------------------------------------------------- |
| `lexes_mixed_flavors`                      | mixing three flavors in ONE line works — the migration path     |
| `tamil_identifiers_work`                   | Tamil-script identifiers lex as identifiers (XID rules)         |
| `numbers`                                  | decimal / `0b` / `0x` parse, `_` separators, correct values     |
| `wrapping_operators`                       | `+%` / `-%` are single tokens                                   |
| `larrow_vs_comparison`                     | `<-` vs `<=` vs `<<` disambiguation — longest match             |
| `newline_continuation_after_operator`      | the Go-style newline policy, both directions (kept AND dropped) |
| `division_is_rejected_with_teaching_error` | `/` errors AND the help text teaches the alternative            |
| `fall_is_reserved_error`                   | reserved-word path produces a real diagnostic                   |

## Unit: parser (`src/parser/tests.rs`, 12 tests)

| Test                                           | Locks in                                                                  |
| ---------------------------------------------- | ------------------------------------------------------------------------- |
| `parses_counter`                               | the canonical example parses; module has the expected 6 items             |
| `parses_tanglish_counter_to_same_shape`        | Tanglish source → structurally identical AST (the thesis, AST level)      |
| `rust_precedence_defuses_the_c_trap`           | `x & 1 == 0` parses as `(x & 1) == 0` — **never** change this             |
| `monotonic_chained_comparison_desugars_to_and` | `0 <= x <= 7` desugars to `(0<=x) && (x<=7)` — the safe Python form (8.9) |
| `mixed_direction_chain_is_an_error`            | `a < b > c` stays E1109 (the confusing form)                              |
| `equality_cannot_be_chained`                   | `a == b == c` stays E1109                                                 |
| `wire_if_without_else_teaches_about_latches`   | mandatory `else` on if-expressions + the latch help text                  |
| `reg_without_reset_value_is_an_error`          | mandatory reg reset (safety rule)                                         |
| `assign_arrow_confusion_teaches`               | `=` inside `on` → help text pointing to `<-`                              |
| `parses_repeat_and_const`                      | `repeat i: 0..8` and file-level `const` parse                             |
| `parses_test_block`                            | `test "..." for M(...) { tick/expect }` parses                            |
| `every_parse_error_carries_a_code`             | the E11xx retrofit, locked from outside: no parse error is codeless       |

The error-path tests assert on message/help **substrings** (loose, so
wording can be polished) AND on the stable E-code (tight — the
contract). Lexer error tests do the same with E10xx.

## Unit: checker (`src/checker/tests.rs`, 85 tests)

One test per error code plus clean-pass cases — the codes are the
stable contract, so each test asserts the CODE and a message substring
(loose on wording). The full catalog with meanings lives in
[`11-checker.md`](11-checker.md); the test names map one-to-one
(`unknown_name_is_e0101_with_teaching_help`, `assignment_width_mismatch_is_e0401`, …).
The width slice (E0401–E0410) added 26: error paths for every code
(several codes get two angles, e.g. `extend`-narrowing AND
`trunc`-widening for E0407) plus six clean passes. The driver slice
(E0501–E0505) added 16: every code's error paths (both halves where a
code covers two mistakes, e.g. zero AND multiple `on` blocks for E0503)
plus four clean passes guarding against false positives. The completion
slice (E0302/E0601/E0602/E0701, 2026-06-12) added 20: exhaustiveness
over enums/`bit`/`bits[N]` incl. the v0.2.3 rulings as clean passes,
instantiation completeness both ways (missing AND duplicate), and the
clock-domain matrix (independent domains clean, direct read,
through-a-wire, domain-mixing wire, unused-second-clock clean). A few
deserve a note:

| Test                                                                  | Locks in                                                                               |
| --------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `clean_module_passes` / `const_arithmetic_and_repeat_bounds_evaluate` | clean code produces ZERO diagnostics — the checker must never cry wolf                 |
| `duplicate_module_across_files_is_e0001_in_the_right_file`            | checker diagnostics carry the file index (multi-file rendering contract)               |
| `plus_into_same_width_target_teaches_wrap_in_e0401`                   | the dropped-carry moment teaches `+%` — the spec/02 section 1.2 promise, executable    |
| `defaultless_param_module_is_checked_per_instantiation`               | a module with no param defaults is checked under each instantiation's concrete binding |
| `repeat_index_out_of_range_at_the_last_iteration_is_e0406`            | `repeat` bodies are width-checked per iteration value, not just once                   |
| `extend_of_a_bit_into_bitwise_passes`                                 | the fixed shift-register shape — explicit `extend` where widths differ                 |
| `disjoint_per_bit_drives_via_repeat_pass`                             | the Chaser idiom: eight `led[i] = ...` drives are eight drivers for eight bits — legal |
| `repeat_instance_array_ripple_carry_is_not_a_cycle`                   | per-index instance-output nodes: `fa[1] -> fa[0]` is a chain, not a loop               |
| `a_cycle_through_instances_is_e0504`                                  | combinational loops THROUGH child modules are caught via the comb summaries            |
| `feedback_through_a_register_is_not_a_cycle`                          | a reg breaks the loop — the normal shape of hardware never false-positives             |
| `enum_match_covering_every_variant_needs_no_wildcard`                 | the v0.2.3 ruling, executable: full coverage IS exhaustive, no `_` ceremony            |
| `wildcard_after_full_enum_coverage_is_allowed`                        | the defensive `_` (bit-flip recovery) is never flagged unreachable                     |
| `clock_and_reset_ports_may_be_omitted`                                | E0302 exempts clock/reset — implicit-by-name stays the emitter's contract              |
| `same_domain_logic_under_two_declared_clocks_passes`                  | E0701 colors by USE, not by declaration count — an unused clock changes nothing        |

## Unit: transliteration (`src/emit_verilog/translit.rs`, 5 tests)

| Test                                      | Locks in                                                              |
| ----------------------------------------- | --------------------------------------------------------------------- |
| `pure_tamil_words_romanize_readably`      | விளக்கு → `villakku`, நிலை → `nilai` — the readable-output promise    |
| `ascii_and_mixed_names_keep_their_ascii`  | ASCII passes through untouched, even mixed into a Tamil name          |
| `non_tamil_unicode_falls_back_to_hex`     | other scripts → `_uXXXX`, never dropped                               |
| `results_always_start_like_an_identifier` | output is always a valid Verilog identifier start                     |
| `the_two_n_letters_romanize_identically`  | ந/ன → `n` is a DOCUMENTED collision; the suffix counter disambiguates |

## Unit: emitter (`src/emit_verilog/mod.rs`, 12 tests)

| Test                                                            | Locks in                                                                                                                                    |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `diags_carry_the_file_index`                                    | project-level diagnostics (duplicate module, emit errors) record WHICH file they point into, so multi-file errors render the right excerpt  |
| `repeat_unrolls_drives_with_folded_indices`                     | `repeat i: 0..4 { y[i] = … }` emits `assign y[0..3]`; the half-open range stops at 3                                                        |
| `repeat_var_folds_in_index_arithmetic`                          | `y[i + 1]` folds to `y[1]`/`y[3]` — index arithmetic over the loop var collapses to a literal                                               |
| `empty_and_reversed_ranges_emit_nothing`                        | `0..0` and `4..0` generate no hardware (no crash, no partial output)                                                                        |
| `repeat_over_budget_errors_cleanly`                             | a range past `REPEAT_BUDGET` (4096) is a clean error, not a runaway unroll                                                                  |
| `nested_repeat_folds_both_variables`                            | nested loops bind both `i` and `j` per iteration; `y[1] = 1` proves the inner+outer fold                                                    |
| `repeat_instance_array_gets_flat_names`                         | `let u[i] = …` → `u__<i>` with outputs `u__<i>_<port>`; `u[i].o` reads back the same flat wire                                              |
| `module_const_folds_in_widths_and_emits_no_hardware`            | a `const` folds to a literal in port widths and bounds, and declares no Verilog of its own                                                  |
| `child_consts_fold_into_parent_auto_wires`                      | instantiating a const-widthed module folds the CHILD's const into the auto-wire (regression: `wire [(W)-1:0]` leaked and iverilog rejected) |
| `parent_const_never_substitutes_into_child_widths`              | same const NAME in parent and child: the child's value sizes the wire — never the parent's (silently wrong hardware otherwise)              |
| `tamil_identifiers_emit_as_romanized_verilog`                   | the transliterated pipeline end to end: module/ports/regs/always all use the SAME romanization; no non-ASCII outside the banner comment     |
| `colliding_romanizations_get_suffixes_and_ascii_names_are_safe` | ந/ன clash + an existing ASCII `nii`: user names are never stolen; clashes get `_2`, `_3` deterministically                                  |

## Integration (`tests/examples.rs`, 9 tests — run the real binary)

`examples/` holds four flavor folders — `english/`, `tanglish/`, `tamil/`,
`mixed/` — each with the SAME 15 base examples (identical identifiers,
only keywords differ; `lib/` subfolders hold dotted-import targets). The
base-example list lives in the `BASE_EXAMPLES` const in the test file.

| Test                                            | Locks in                                                                                                                                                                                                                                                          |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_example_checks_clean`                    | every `.mimz` under `examples/` (recursive) passes `mimz check` — which now runs the CHECKER over the file and its imports, so this is also a zero-false-positives test for every checker rule. At least 4 × 11 files — RULES R6 made executable                  |
| `every_example_compiles`                        | every example **compiles to Verilog**, including the `lib/` helpers. A new example that doesn't compile fails CI by name                                                                                                                                          |
| `all_four_flavors_compile_to_identical_verilog` | each base example → **byte-identical** Verilog from all four flavors. The project's thesis. Never break it                                                                                                                                                        |
| `counter_compiles_to_verilog`                   | end-to-end compile; asserts the parameter, the always-block, the **generated reset**, the assign                                                                                                                                                                  |
| `alu_with_import_compiles`                      | `import` resolution end-to-end; instances with params; auto-wired child outputs (`add_sum`)                                                                                                                                                                       |
| `include_alias_compiles_with_dotted_path`       | `include lib.full_adder` works through the whole pipeline — the alias AND dotted-path resolution, in one example (`english/chained.mimz`)                                                                                                                         |
| `ripple_adder_unrolls_repeat`                   | `repeat` end-to-end: four `FullAdder fa__0..3` with the carry chained, folded indices, `const WIDTH` folded into widths — compile-time generation proven through the real binary                                                                                  |
| `traffic_light_fsm_compiles`                    | enums → localparams (`STATE_RED` …)                                                                                                                                                                                                                               |
| `emitted_verilog_matches_the_goldens`           | every base example's FULL output equals `tests/golden/<base>.v` byte for byte (banner stripped). On an INTENDED emitter change: `MIMZ_UPDATE_GOLDENS=1 cargo test --test examples`, then review the golden diff like code. Failure names the first differing line |

## Icarus differential (`tests/icarus.rs`, 2 tests — run a REAL Verilog tool)

The independent judge: our substring asserts check OUR expectations of
the output; these check a real tool's. **Skips with a printed note when
`iverilog` is not installed** (probe order: `MIMZ_IVERILOG` env →
PATH → the Windows installer default `C:\iverilog\bin`); in CI
`REQUIRE_IVERILOG=1` makes a missing install a hard failure, so CI can
never skip silently. Local install: the Windows installer
(bleyer.org/icarus) or `apt-get install iverilog`.

| Test                                    | Locks in                                                                                                                                                                                                                                          |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_emitted_verilog_passes_iverilog` | all 60 examples' emitted `.v` pass `iverilog -t null` — syntax AND elaboration, by Icarus's judgment (incl. the transliterated `vilakku` and `wire signed` `signed_math`)                                                                         |
| `self_checking_testbenches_pass`        | one hand-written TB per base example (`tests/icarus/*_tb.v`, 14) encodes Min-Mozhi's documented semantics (`+%` wraps, sync reset, non-blocking `<-`, FSM timing, SIGNED extension/comparison) and must print PASS under `vvp` — the differential |

House rule for the testbenches: each prints `PASS` exactly once or
`FAIL: reason` and stops — the Rust side asserts on those markers, so a
broken TB fails loudly, never silently. The Blinker TB overrides the
`LIMIT` parameter (`#(.LIMIT(3))`) instead of simulating 50M cycles.

## Error fixtures (`tests/errors.rs`, 4 tests — run the real binary on broken code)

End-to-end **failure** validation, the mirror of the checker unit tests: those
prove the checker _function_ rejects bad code; these prove the _CLI_ surfaces it.
`tests/fixtures/errors/*.mimz` holds ~67 intentionally-broken files (kept OUT of
`examples/`, which is asserted valid), each declaring its expected code in a
`// expect: Exxxx` header. Source bodies are lifted from `src/checker/tests.rs`.

| Test                                           | Locks in                                                                                                                                                             |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_error_fixture_reports_its_code`         | each fixture, run through `mimz check`, exits non-zero AND prints `error[<code>]` to stderr — the rendered code is the stable user-facing contract, checked for real |
| `error_corpus_covers_every_checker_code`       | completeness guard: every code in `ALL_CHECKER_CODES` (the 36 stable checker codes) has at least one fixture — a new E-code can't ship without an end-to-end fixture |
| `checker_code_list_matches_the_catalog`        | `ALL_CHECKER_CODES` must equal the 11-checker.md catalog table (reserved rows exempt) — the corpus, the docs, and the code can't drift apart                         |
| `json_flag_emits_machine_readable_diagnostics` | the `--json` wire format (docs/code/06): one JSON array on stdout with code/path/line/help; lexer errors included; `[]` + exit 0 on success                          |

`every_error_fixture_reports_its_code` also asserts a `help:` line per
fixture — the teaching contract, proven at the CLI surface.

Coverage is **every distinct edge case**, not one per code: E0302 missing-input
AND duplicate-conn; E0407 extend-narrowing AND `-` on bits; E0303 all eight
forbidden declaration kinds; E0601 enum/`bits[N]`/`bit`; E0701's three crossings;
etc. The assertion is "stderr _contains_ the code", tolerant of a fixture that
incidentally trips a second rule. Convention + how-to: `tests/fixtures/errors/README.md`.

## Docs-sync (`tests/docs_sync.rs`, 4 tests)

The mechanical staleness guard for `docs/code/` — these verify the
structural facts the docs state, so doc drift fails CI. When one fails,
**fix the named doc page, don't weaken the test.**

| Test                                                | Locks in                                                                                             |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crate_map_lists_every_module`                      | both crate-map copies (`src/main.rs` `//!` table, `docs/code/README.md`) name every top-level module |
| `module_pages_list_every_source_file`               | each module page's file-layout table lists every `.rs` file actually in that `src/` directory        |
| `every_module_is_documented_somewhere_in_docs_code` | a new pipeline stage (e.g. `src/checker/`) cannot land without a docs mention                        |
| `code_docs_have_a_sync_stamp`                       | the "Last synced" tripwire line survives                                                             |

## Grammar-sync (`tests/grammar_sync.rs`, 3 tests)

Same philosophy as docs-sync, for the VS Code extension: the keyword
table is data, so the TextMate grammar can silently drift. These verify
every spelling (canonical + aliases) and every reserved word appears as
a whole alternation member in `editors/vscode/syntaxes/mimz.tmLanguage.json`
(whole-member matching, because `in` is a substring of `include` — a
plain `contains` would pass vacuously), and that the manifest registers
`.mimz` with the matching scope name. When one fails: fix the grammar.

## LSP (`src/lsp.rs` unit + `tests/lsp.rs` smoke, 4 tests)

| Test                                                        | Locks in                                                                                                                                     |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `positions_are_utf16_lines_and_columns`                     | byte span → LSP Position math (0-based lines)                                                                                                |
| `tamil_text_counts_utf16_units_not_bytes`                   | LSP columns are UTF-16 code units — a Tamil identifier before the error must not skew the squiggle                                           |
| `analyze_reports_checker_errors_with_codes`                 | the in-memory pipeline (didOpen text, never on disk) produces coded checker diagnostics                                                      |
| `opening_a_broken_file_publishes_coded_diagnostics` (smoke) | the REAL binary over the real wire protocol: framed JSON-RPC initialize → didOpen → publishDiagnostics with code, source, help, and position |

## Benchmark harness (`src/bin/mimz-bench/`, 6 unit tests)

The harness itself (docs in [`12-benchmark.md`](12-benchmark.md))
re-measures what this suite asserts — rates and timings instead of
pass/fail — so its own logic is unit-tested here:

| Test                                       | Locks in                                                       |
| ------------------------------------------ | -------------------------------------------------------------- |
| `rate_percent_handles_zero_and_partial`    | rate math (0/0 reads as 100%, never NaN)                       |
| `expect_header_parses_only_the_convention` | the `// expect: Exxxx` fixture-header parse, same as errors.rs |
| `banner_strip_matches_the_golden_rule`     | banner stripping byte-matches the golden test's rule           |
| `median_is_the_middle_run`                 | timing aggregation (median, robust to one cold run)            |
| `report_renders_a_complete_page` (html)    | the HTML report renders whole: charts, tables, embedded JSON   |
| `failures_flip_the_verdict_and_are_listed` | a failing validation turns the verdict red and is named        |

The `criterion` micro-benchmark harness (`benches/compile.rs`, run with
`cargo bench`) carries **no `#[test]`s** — `criterion` benchmarks aren't
test functions, so it doesn't affect the count above. It's a separate
performance tool, not part of the assertion suite.

## Unit: explain (`src/explain.rs`, 3 tests)

The 8.1 long-form diagnostic catalog behind `mimz explain <CODE>`.

| Test                                       | Locks in                                                                                       |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| `every_checker_code_has_an_explanation`    | every `ALL_CHECKER_CODES` entry has long-form text — a new checker code can't ship without one |
| `table_is_sorted_unique_and_self_labelled` | the `EXPLANATIONS` table is ordered, duplicate-free, and each entry opens with its own code    |
| `lookup_is_case_insensitive_and_trims`     | `explain("e0501")` / `" E0501 "` resolve; unknown codes return `None`                          |

## Unit: translate (`src/translate.rs`, 3 tests)

The keyword-flavor reskin behind `mimz translate --to`.

| Test                                                       | Locks in                                                             |
| ---------------------------------------------------------- | -------------------------------------------------------------------- |
| `parse_flavor_accepts_the_three_columns`                   | `english`/`tanglish`/`tamil` (case-insensitive) parse; junk → `None` |
| `reskins_keywords_keeps_everything_else`                   | keywords swap; comments, layout, identifiers, numbers stay verbatim  |
| `translating_to_the_same_flavor_is_identity_for_canonical` | canonical English → English is a no-op                               |

## Integration: translate (`tests/translate.rs`, 3 tests — the four-flavor oracle)

The `examples/{english,tanglish,tamil}/` folders are byte-identical
keyword-swaps (R9), so they validate the reskin against committed truth.

| Test                                                               | Locks in                                                                                                   |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `round_trip_to_every_flavor_is_byte_identical`                     | translate-and-back reproduces the canonical source byte-for-byte (lossless; anchored past alias normalize) |
| `translating_english_matches_the_committed_flavor_token_for_token` | translating english `X` to flavor `T` lexes identically to the committed `T/X` (comments excluded)         |
| `every_keyword_token_is_in_the_target_flavor`                      | the reskin actually fires — English `module` is gone, Tamil `தொகுதி` present                               |

## Unit: combinational evaluator (`src/sim/comb.rs`, 7 tests)

The Phase 1.5 simulator's combinational slice behind `mimz eval`.

| Test                         | Locks in                                                                          |
| ---------------------------- | --------------------------------------------------------------------------------- |
| `adder_grows_losslessly`     | `+` grows `bits[W]` → `bits[W+1]`; 200+100 carries into the 9th bit (no wrap)     |
| `wrapping_add_keeps_width`   | `+%` keeps width and wraps (300 → 44 in `bits[8]`)                                |
| `comparator_if_and_compares` | `==`, `>`, and a value `if/else` evaluate together                                |
| `mux_match_selects`          | `match` on `bits[2]` picks the right arm                                          |
| `chained_comparison_window`  | `lo <= value <= hi` (desugared) incl. the inclusive boundary                      |
| `rejects_sequential_logic`   | a module with `reg`/`on` is rejected with a clear message (out of the comb slice) |
| `reports_missing_input`      | a missing `--in` value names the input                                            |

## Integration: eval (`tests/eval.rs`, 6 tests — run the real binary)

End-to-end `mimz eval` over corpus examples — proves the lib evaluator AND the
`--in`/`--module` plumbing.

| Test                                      | Locks in                                                          |
| ----------------------------------------- | ----------------------------------------------------------------- |
| `adder_carries`                           | `mimz eval adder --in a=200,b=100` prints `sum = 300`             |
| `mux4_selects_with_hex_and_binary_inputs` | `--in sel=0b10,...` parses bases; selects the right input         |
| `comparator_reports_all_three_outputs`    | all three outputs print with correct values                       |
| `window_chained_comparison_boundaries`    | inclusive boundary in / below out                                 |
| `multi_module_file_needs_module_flag`     | a 2-module file asks for `--module`, then accepts it              |
| `instances_are_rejected_clearly`          | a file with sub-module instances is rejected with a clear message |

## Deliberately NOT covered (and what would close each gap)

| Gap                                                     | Why it's open                                                                                                                                 | Closes when                                                 |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Cross-INSTANCE clock-domain tracking                    | pass 6 is module-local (instance outputs carry no domain)                                                                                     | with the Phase 2 `sync`/multi-clock design                  |
| Diagnostic rendering format (`render`'s caret layout)   | low risk, changes are cosmetic                                                                                                                | worth a snapshot test if/when output stabilizes for E-codes |
| CLI surface (`--tokens`, exit codes, `-o` default path) | thin wrappers; breakage is loud in manual use                                                                                                 | cheap `assert_cmd`-style tests if the CLI grows             |
| `mimz-bench` end-to-end (a full run as a test)          | it is a measuring tool over this very suite — running it under `cargo test` would re-run everything for no new assertion                      | if its orchestration grows logic worth locking              |
| `fmt`, grammar engine, full simulator (clocked kernel)  | not built yet (`translate` flavor-reskin and the `sim::comb` combinational slice now exist; word-order `translate` + the event kernel remain) | with their phases (1.8 / 1.5)                               |

## House rules for new tests

- New parser/emitter behavior ships with a test **in the same commit**;
  safety-rule behaviors also test the error path (message + help).
- Prefer the existing layers: table-driven facts → keyword tests; token
  shapes → lexer tests; tree shapes & teaching errors → parser tests;
  output text → integration tests on a real example.
- A new example goes into ALL FOUR flavor folders with identical
  identifiers (only keywords change — take spellings from
  `keywords.toml`, never invent), plus a row in `BASE_EXAMPLES` in
  `tests/examples.rs`. `every_example_compiles` and the
  flavor-identity test then enforce it automatically.
- Update THIS page in the same session (it is the "what does a failing
  test mean" ledger — see also `tests/docs_sync.rs`, which mechanically
  guards the structural facts in these docs).
