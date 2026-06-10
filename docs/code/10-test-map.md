# 10 — Test Map: What Is Covered, What Isn't, and Why

Every test, what it locks in, and what a failure means. Update this page
when tests are added or removed (the count below is asserted nowhere —
this page is the human ledger).

**30 tests** as of 2026-06-10: 21 unit + 5 integration + 4 docs-sync.

## Unit: keyword table (`src/lexer/keywords.rs`, 3 tests)

| Test                                        | Locks in                                              | If it fails…                                  |
| ------------------------------------------- | ----------------------------------------------------- | --------------------------------------------- |
| `all_three_flavors_resolve_to_same_keyword` | EN/Tanglish/Tamil spellings → one `Kw` token          | `keywords.toml` edit broke a mapping          |
| `flavors_are_recorded`                      | the lexer remembers which column a spelling came from | flavor tracking broke (P1.8 depends on it)    |
| `fall_is_reserved`                          | `fall` errors as reserved, is not a keyword           | someone un-reserved `fall` without a decision |

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

## Unit: parser (`src/parser/tests.rs`, 9 tests)

| Test                                         | Locks in                                                             |
| -------------------------------------------- | -------------------------------------------------------------------- |
| `parses_counter`                             | the canonical example parses; module has the expected 6 items        |
| `parses_tanglish_counter_to_same_shape`      | Tanglish source → structurally identical AST (the thesis, AST level) |
| `rust_precedence_defuses_the_c_trap`         | `x & 1 == 0` parses as `(x & 1) == 0` — **never** change this        |
| `chained_comparison_is_an_error`             | `a < b < c` is rejected (non-associative comparisons)                |
| `wire_if_without_else_teaches_about_latches` | mandatory `else` on if-expressions + the latch help text             |
| `reg_without_reset_value_is_an_error`        | mandatory reg reset (safety rule)                                    |
| `assign_arrow_confusion_teaches`             | `=` inside `on` → help text pointing to `<-`                         |
| `parses_repeat_and_const`                    | `repeat i: 0..8` and file-level `const` parse                        |
| `parses_test_block`                          | `test "..." for M(...) { tick/expect }` parses                       |

The four error-path tests assert on message/help **substrings** —
deliberately loose so wording can be polished without breaking tests,
tight enough that the teaching content can't silently vanish.

## Unit: emitter (`src/emit_verilog/mod.rs`, 1 test)

| Test                         | Locks in                                                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `diags_carry_the_file_index` | project-level diagnostics (duplicate module, emit errors) record WHICH file they point into, so multi-file errors render the right excerpt |

## Integration (`tests/examples.rs`, 5 tests — run the real binary)

| Test                                             | Locks in                                                                                                                                                  |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `every_example_checks_clean`                     | every `examples/*.mimz` passes `mimz check` — RULES R6 ("examples always match the spec") made executable. Add an example file → automatically under test |
| `counter_compiles_to_verilog`                    | end-to-end compile; asserts the parameter, the always-block, the **generated reset**, the assign                                                          |
| `tanglish_counter_compiles_to_identical_verilog` | EN and Tanglish → **byte-identical** Verilog. The project's thesis. Never break it                                                                        |
| `alu_with_import_compiles`                       | `import` resolution end-to-end; instances with params; auto-wired child outputs (`add_sum`)                                                               |
| `traffic_light_fsm_compiles`                     | enums → localparams (`STATE_RED` …)                                                                                                                       |

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

## Deliberately NOT covered (and what would close each gap)

| Gap                                                                                      | Why it's open                                                  | Closes when                                                            |
| ---------------------------------------------------------------------------------------- | -------------------------------------------------------------- | ---------------------------------------------------------------------- |
| **Is the emitted Verilog VALID Verilog?**                                                | substring asserts check OUR expectations, not a tool's         | Icarus Verilog differential tests in CI (planned, Phase 1 plan item 5) |
| Semantic safety rules (widths, single-driver, exhaustiveness, comb-DAG, clock ownership) | the checker doesn't exist yet                                  | each checker pass lands WITH its own test file                         |
| `repeat` emission                                                                        | unsupported (clean error, tested implicitly by none)           | checker const-eval; add an unrolling golden test then                  |
| Diagnostic rendering format (`render`'s caret layout)                                    | low risk, changes are cosmetic                                 | worth a snapshot test if/when output stabilizes for E-codes            |
| CLI surface (`--tokens`, exit codes, `-o` default path)                                  | thin wrappers; breakage is loud in manual use                  | cheap `assert_cmd`-style tests if the CLI grows                        |
| Golden-file (full output) comparison                                                     | deliberate: substring asserts survive cosmetic emitter changes | revisit when the emitter output is contractual (Phase 2)               |
| `mimz translate`, `fmt`, simulator, grammar engine                                       | not built yet                                                  | with their phases                                                      |

## House rules for new tests

- New parser/emitter behavior ships with a test **in the same commit**;
  safety-rule behaviors also test the error path (message + help).
- Prefer the existing layers: table-driven facts → keyword tests; token
  shapes → lexer tests; tree shapes & teaching errors → parser tests;
  output text → integration tests on a real example.
- A new example in `examples/` must compile — `every_example_checks_clean`
  enforces it automatically.
- Update THIS page in the same session (it is the "what does a failing
  test mean" ledger — see also `tests/docs_sync.rs`, which mechanically
  guards the structural facts in these docs).
