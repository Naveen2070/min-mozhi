# Phase 1.8 — Grammar Engine (இலக்கண இயந்திரம்)

> **Tamil code that reads like Tamil, not transliterated English.**
> Window: months 8–9, **directly after Phase 1, before 1.5** (solo-dev order,
> decision D3) · Target: 28 Feb 2027, then repo goes public (D7) ·
> Status: ⚪ designed (`spec/04`), not started

## Goal

Add the `thamizh-order` syntax profile: SOV/postpositional clause forms over
the **same AST**, plus grammar-correct Tamil error messages. Full design:
`spec/04-grammar-engine.md`.

## Work items

### Parser profile

- [ ] `syntax thamizh` file directive (no auto-detection)
- [ ] Flipped productions per `spec/04` §3: `<cond> endral { }`, `yetram(clk) pothu { }`, `<expr> poruthu { }`, test form
- [ ] Expression-first parsing with one-token lookahead after expression (no backtracking)
- [ ] Same-AST guarantee tests: thamizh-order file and its code-order twin produce byte-identical AST dumps

### Translate / format

- [ ] Pretty-printer with per-profile output templates
- [ ] `mimz translate --to <flavor> --order code|thamizh` — lossless, trivia-preserving
- [ ] Round-trip tests: translate A→B→A is identity

### Morphology helper (error messages)

- [ ] Tamil case-suffix table (-ஐ, -க்கு, -இல், -ஆல்) + sandhi-joining rules for interpolated identifiers
- [ ] Error catalog authored in Tamil + Tanglish by humans (not machine-translated); helper only inflects names
- [ ] Error-language selection: file flavor majority, `--lang` override

### Validation

- [ ] Native-speaker panel (tech/coder friends, decision C3) reviews the §3 word-order table and 10 rendered error messages
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
- Scope fence is strict (`spec/04` §6): no free word order, no flipped
  declarations, no inflected keywords. Any expansion is a logged Decision.
