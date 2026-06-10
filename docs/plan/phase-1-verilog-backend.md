# Phase 1 — Verilog Backend

> **Get something working end-to-end.**
> Window: months 3–7 · Target: **31 Dec 2026** (solo, ~8–10 h/wk) · Status: ⚪ not started

## Goal

`mimz compile adder.mimz → adder.v` that simulates correctly in Icarus
Verilog. The full front half of the compiler exists after this phase.
v0.1.0 is tagged when the compiler is executable and testable (decision D6).

## Work items

### 1. Project skeleton
- [x] `cargo init` — single crate `mimz` (Rust stable 1.96, edition 2024; build/fmt/clippy green 2026-06-10)
- [x] **CI (GitHub Actions) from the first commit** (decision D5): fmt, clippy, tests — `.github/workflows/ci.yml`
- [x] LICENSE-MIT + LICENSE-APACHE files (dual license, decision A5) + `.gitignore`/`.gitattributes`/`.editorconfig`
- [ ] `keywords.toml` — trilingual keyword table as data; loaded into a static map
- [ ] CLI skeleton (`clap`): `mimz compile <file>`, `mimz check <file>`

### 2. Lexer
- [ ] Tokenizer: idents (Unicode, NFC-normalized), numbers (`0b`/`0x`/dec, `_`), operators, comments
- [ ] Trilingual keyword recognition (union of all three columns, mixable)
- [ ] Newline-as-terminator rules (Go-style continuation)
- [ ] Span tracking on every token (for diagnostics)
- [ ] Golden tests: every example file tokenizes; flavor-mix file tokenizes

### 3. Parser → AST
- [ ] Recursive-descent parser for the `code-order` profile (EBNF in `spec/02` §5)
- [ ] AST types: module, ports, clock/reset, wire/reg, const, enum, instance, on-block, repeat, expressions
- [ ] `import` resolution: file-relative, project-unique module names, cycle detection (spec/02 §1.5)
- [ ] `repeat` compile-time unrolling (spec/02 §1.6)
- [ ] Error recovery good enough to report >1 error per run

### 4. Semantic checks (the safety rules, `spec/02` §6)
- [ ] Name resolution + duplicate detection (project-wide, post-import)
- [ ] Const-folding for widths/params/`const`/`repeat` bounds
- [ ] Width checking incl. `+`/`-`/`*` growth and `+%` family exact-match
- [ ] Signed rules: no mixing, `signed()`/`unsigned()` casts, type-directed `extend`, negative literals (spec/02 §1.7)
- [ ] Single-driver check; combinational cycle (DAG) check
- [ ] Exhaustiveness: `match` total, wire-`if` has `else`
- [ ] `=` vs `<-` placement enforcement; clock/reset domain typing incl. per-reg clock ownership
- [ ] Reg-requires-reset rule (module with regs must declare `reset`)
- [ ] Teaching error messages via `miette`/`ariadne` (English first)

### 5. Verilog emitter
- [ ] AST → synthesizable Verilog-2005: modules, assigns, always-blocks, FSM enums as localparams
- [ ] Reset generation from reg reset values (sync reset, active-high, v1)
- [ ] Golden-file tests: each example → expected `.v`
- [ ] Icarus Verilog smoke tests in CI: compile + run a self-checking TB per example

### 6. Visibility (decision D4)
- [ ] Minimal VS Code syntax highlighting: TextMate grammar for `.mimz` (all keyword flavors)

## Milestone

All `examples/*.mimz` compile and simulate correctly under Icarus.
**Tag v0.1.0. Repo goes public after Phase 1.8 (decision D7).**

## Exit criteria

1. `mimz compile` works on every example, English and Tanglish flavors.
2. Each safety rule has at least one test proving it rejects bad input with a helpful message.
3. CI runs lexer/parser/check/emit tests + Icarus simulation green.

## Risks / notes

- Keep the emitter dumb and readable — optimization belongs to Phase 2 IR.
- Resist scope creep: the deferred-features table in spec/02 §7 is the fence
  (no memories, no inout, no structs, no CDC in this phase).
