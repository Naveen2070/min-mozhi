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
