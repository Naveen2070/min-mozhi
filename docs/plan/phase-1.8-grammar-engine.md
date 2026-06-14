# Phase 1.8 — Grammar Engine (இலக்கண இயந்திரம்)

> **Tamil code that reads like Tamil, not transliterated English.**
> Window: months 8–9, **directly after Phase 1, before 1.5** (solo-dev order,
> decision D3) · Target: 28 Feb 2027, then repo goes public (D7) ·
> Status: 🟡 in progress — keystone landed 2026-06-13 (the `syntax thamizh`
> directive + the clocked-block flip, same-AST proven)

## Goal

Add the `thamizh-order` syntax profile: SOV/postpositional clause forms over
the **same AST**, plus grammar-correct Tamil error messages. Full design:
`spec/04-grammar-engine.md`.

## Work items

### Parser profile

- [x] `syntax thamizh` file directive (no auto-detection) — `Profile` on the
      parser, parsed by `syntax_directive`, never enters the AST. `syntax`
      promoted from reserved to KW_SYNTAX; KW_THAMIZH added (spec/03 v0.2.5).
- [~] Flipped productions per `spec/04` section 3: **clocked block, seq
  conditional (`<cond> endral { }`), if-expression (`c endral { } illaiyel
{ }`), and match (`<expr> poruthu { }`) done** (2026-06-14). The **test**
  form remains — deferred to Phase 1.5 (test blocks emit no Verilog yet, so no
  same-Verilog oracle).
- [x] Expression-first parsing with one-token lookahead after the operand (no
      backtracking) — the clocked-block flip dispatches on the leading `Kw::Rise`;
      the conditional/if-expr/match flips parse the operand with `binary(0)` then
      dispatch on the trailing `endral`/`poruthu` (`expr_thamizh`,
      `seq_stmt_thamizh`, with `expr_to_lvalue` recovering the assignment lhs).
- [x] Same-AST guarantee tests via the profile-blind backend: a thamizh-order
      file and its code-order twin emit **byte-identical Verilog**
      (`tests/grammar.rs`, fixtures in `tests/fixtures/grammar/`). (Verilog
      equality is the span-free oracle the four-flavor rule already uses; a raw
      AST dump would differ only in spans.)

### Translate / format

- [ ] Pretty-printer with per-profile output templates
- [ ] `mimz translate --to <flavor> --order code|thamizh` — lossless, trivia-preserving
- [ ] Round-trip tests: translate A→B→A is identity

### Morphology helper (error messages)

- [ ] Tamil case-suffix table (-ஐ, -க்கு, -இல், -ஆல்) + sandhi-joining rules for interpolated identifiers
- [ ] Error catalog authored in Tamil + Tanglish by humans (not machine-translated); helper only inflects names
- [ ] Error-language selection: file flavor majority, `--lang` override

### Validation

- [ ] Native-speaker panel (tech/coder friends, decision C3) reviews the section 3 word-order table and 10 rendered error messages
- [ ] Rewrite `examples/traffic_light` in pure Tamil script, thamizh-order, added to test suite

## Milestone

The traffic-light FSM in natural-word-order Tamil script compiles to the same
Verilog as its English twin; its error messages read as correct Tamil.

## Exit criteria

1. Same-AST and round-trip test suites green.
2. Panel sign-off on word order + error rendering.
3. Docs: `spec/04` bumped from DRAFT to stable.

## Risks / notes

- Requires the Phase 1 parser to exist — do not start earlier.
- Scope fence is strict (`spec/04` section 6): no free word order, no flipped
  declarations, no inflected keywords. Any expansion is a logged Decision.
