# House rules for new tests

> Back to [Test Map Index](index.md) · [Overview](../10-test-map.md)

- New parser/emitter behavior ships with a test **in the same commit**;
  safety-rule behaviors also test the error path (message + help).
- Prefer the existing layers: table-driven facts → keyword tests; token
  shapes → lexer tests; tree shapes & teaching errors → parser tests;
  output text → integration tests on a real example.
- A new example goes into ALL FOUR flavor folders with identical
  identifiers (only keywords change — take spellings from
  `lang/keywords.toml`, never invent), plus a row in `BASE_EXAMPLES` in
  `tests/examples.rs`. `every_example_compiles` and the
  flavor-identity test then enforce it automatically.
- Update THIS page in the same session (it is the "what does a failing
  test mean" ledger — see also `tests/docs_sync.rs`, which mechanically
  guards the structural facts in these docs).

## Counting conventions (decided 2026-08-22, doc-code audit §8)

One corpus, one number, stated the same way everywhere:

- **Examples**: count TOP-LEVEL files per flavor folder (currently
  200 = 44 english + 44 tanglish + 44 tamil + 43 mixed + 25 tamil-pure).
  The `lib/`/`std/` twin subfolders (+6 in each of the four non-pure
  flavors = 224 recursive) get a one-time "+N twins" note, never a bare
  recursive total.
- **Error fixtures**: count the flat `tests/fixtures/errors/*.mimz` files
  (currently **120** — exactly what `error_corpus_covers_every_checker_code`
  scans; it ignores subdirectories). The `e0110_support/` helper folder is
  mentioned separately, never added into the number.
- **Goldens**: state the TOTAL with its split (currently **88 = 71 module +
  17 `_tb.v`**), never as two additive numbers that read like a sum.
- **Suite sizes**: index rows mirror their per-suite page header verbatim.
  Pages that exclude sibling suites say so (emitter = 93 excluding
  translit + testbench); the index never re-adds those.
- **Workspace total**: never hardcode. `tests/docs_sync.rs` counts
  `#[test]` lines from source and enforces docs/badge parity, so a new
  test updates the enforced number automatically.
