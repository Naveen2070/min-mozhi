# 08 — Contributor Recipes

Practical how-tos for the changes people actually make. The quick
orientation summary lives in the root
[`CONTRIBUTING.md`](../../CONTRIBUTING.md); this is the detailed version.
Read [`docs/RULES.md`](../RULES.md) too — it governs how plans, specs,
and logs stay in sync; this page covers the code side.

## The gate (run before every commit)

```text
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
npx prettier --check "**/*.md"
npx markdownlint-cli2
```

CI runs exactly this. Zero warnings is the bar, not a goal.

## Recipe: change a Tanglish/Tamil keyword spelling

The native-speaker-review case. **Data change only:**

1. Edit the spelling in `keywords.toml` (keep `reserved` above the first
   `[keywords.*]` table).
2. Update the table in `spec/03-keywords-trilingual.md` + its changelog.
3. Update any example that used the old spelling.
4. Log a Decision block (RULES R3 — keyword table changes are major).

No Rust changes. `cargo test` proves the table still loads and is
disjoint.

## Recipe: add a NEW keyword

1. Spec first: `spec/02` (grammar) and `spec/03` (all three spellings) —
   bump versions, changelogs.
2. `keywords.toml`: add the `[keywords.<key>]` entry.
3. `src/lexer/token.rs`: add the `Kw` variant.
4. `src/lexer/keywords.rs`: add the `kw_for_key` arm. (Miss this and
   every test fails at startup with "unknown keyword key" — by design.)
5. Use it in the parser; add lexer + parser tests.
6. Log entry.

## Recipe: add a syntax form (new statement/expression)

1. Spec first: grammar production in `spec/02` section 5 + a syntax-tour
   example. Bump the spec version.
2. AST node in `src/ast/` — with a `Span`, with rustdoc explaining the
   form and any safety-rule angle.
3. Parse routine in `src/parser/items.rs` or `expr.rs`:
   - doc comment = the EBNF production (house rule);
   - return `Option<T>`, record errors before returning `None`;
   - `expect(..., "learner-phrased what")` for every required token.
4. Emit it in `src/emit_verilog/` — or emit a clean
   "not yet supported" error (never wrong output).
5. Tests: parser unit test (including the error path — assert the help
   text teaches), plus an example/integration test if user-visible.
6. Log entry; update the phase plan if scope changed (RULES R2).

## Recipe: extend the emitter

- Look up symbols via `self.project` (modules/enums by name).
- Render expressions with `self.expr(e)`; inside child-width contexts use
  `expr_subst` with the parameter substitution map.
- New output must be valid **Verilog-2005** (the floor — decision in the
  log). Parenthesize compound expressions unconditionally.
- If the construct can't be emitted correctly yet: `self.err(span, msg,
help)` and emit nothing. Errors, never guesses.
- Mind the auto-wire naming contract: instance outputs are
  `{instance}_{port}`, created in `module.rs::instance` AND assumed in
  `expr.rs` field rendering. Change both or neither.
- Add an integration test in `tests/examples.rs` asserting on the output
  text — the Icarus suite (`tests/icarus.rs`) then judges it with a real
  tool.
- Emission changed on purpose? Regenerate the pinned outputs with
  `MIMZ_UPDATE_GOLDENS=1 cargo test --test examples`, then review the
  `tests/golden/` diff like any other code change.

## Recipe: add a checker pass

One safety rule = one pass = one file with its own tests (architecture
principle 4; six passes exist — the full how-to lives in
[`11-checker.md`](11-checker.md)). Passes take the AST + symbol table,
return diagnostics through `Checker::err`, which makes the stable
`E####` code, the file index, and the teaching help text structurally
mandatory. Claim the next code block and add the catalog row in the
same commit; the error corpus (`tests/errors.rs`) will refuse a new
code without an end-to-end fixture.

## Testing conventions

The full per-test ledger — what each test locks in and what is
deliberately uncovered — is [`10-test-map.md`](10-test-map.md). Update it
when you add or remove tests.

- **Unit tests** live in `src/<module>/tests.rs` (lexer, parser) or a
  `#[cfg(test)] mod tests` block (keywords, emitter).
- **Integration tests** in `tests/examples.rs` compile real examples
  end-to-end. `every_example_checks_clean` means: add an example file and
  it is automatically under test.
- **Docs-sync tests** in `tests/docs_sync.rs` mechanically check the
  structural facts in `docs/code/` (module lists, file-layout tables).
  If one fails, fix the named doc page — don't weaken the test.
- Error-path tests assert on message/help **substrings** — enough to
  catch regressions, loose enough to allow wording polish.
- The trilingual guarantee is CI-enforced: EN and Tanglish counters must
  produce byte-identical Verilog. Don't break that test; it is the
  project's thesis.

## Code style

- rustfmt + clippy decide formatting/idiom arguments.
- Every type and non-trivial function gets rustdoc; parser routines carry
  their EBNF production (RULES R6). `cargo doc --document-private-items`
  must stay warning-free.
- Comments explain WHY (the constraint), not WHAT (the next line).
- Teaching-quality errors are part of every change — see
  [`06-diagnostics.md`](06-diagnostics.md).
- Keep files under ~600 lines; split with the module-scoping pattern
  ([`07-decisions-and-evolution.md`](07-decisions-and-evolution.md)).

## When you change how the code works…

…update the matching page in this folder in the same session, and stamp
the "last synced" line in [`README.md`](README.md). Stale maintainer docs
are worse than none.
