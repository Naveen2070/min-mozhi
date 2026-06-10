# Phase 1 — Verilog Backend

> **Get something working end-to-end.**
> Window: months 3–7 · Target: **31 Dec 2026** (solo, ~8–10 h/wk) ·
> Status: 🟡 **in progress** — skeleton/lexer/parser ✅, emitter v1 ✅ (2026-06-10); checker (item 4) is next

## Goal

`mimz compile adder.mimz → adder.v` that simulates correctly in Icarus
Verilog. The full front half of the compiler exists after this phase.
v0.1.0 is tagged when the compiler is executable and testable (decision D6).

## Work items

### 1. Project skeleton

- [x] `cargo init` — single crate `mimz` (Rust stable 1.96, edition 2024; build/fmt/clippy green 2026-06-10)
- [x] **CI (GitHub Actions) from the first commit** (decision D5): fmt, clippy, tests — `.github/workflows/ci.yml`
- [x] LICENSE-MIT + LICENSE-APACHE files (dual license, decision A5) + `.gitignore`/`.gitattributes`/`.editorconfig`
- [x] `keywords.toml` — trilingual keyword table as data; loaded into a static map (root keys, disjointness asserted at load)
- [x] CLI skeleton (`clap`): `mimz compile <file>`, `mimz check <file>` (+ `--tokens` debug dump)

### 2. Lexer ✅ (2026-06-10)

- [x] Tokenizer: idents (Unicode XID, NFC-normalized), numbers (`0b`/`0x`/dec, `_`), operators, comments
- [x] Trilingual keyword recognition (union of all three columns, mixable; flavor recorded per token)
- [x] Newline-as-terminator rules (Go-style continuation)
- [x] Span tracking on every token (for diagnostics)
- [x] Teaching rejections: `/` and `%` (don't exist), reserved words, Tamil digits
- [x] Unit tests: flavors, Tamil idents, operators, continuation, errors

### 3. Parser → AST ✅ (2026-06-10)

- [x] Recursive-descent parser for the `code-order` profile (EBNF in `spec/02` section 5)
- [x] AST types: module, ports, clock/reset, wire/reg, const, enum, instance, on-block, repeat, tests, expressions
- [x] `import` resolution: file-relative, project-unique module names, cycle-safe visited set (spec/02 section 1.5)
- [ ] `repeat` compile-time unrolling (parses; unrolling needs const-eval — work item 4)
- [x] Error recovery good enough to report >1 error per run (verified: 3 errors in one `check`)
- [x] Rust precedence incl. non-associative comparisons (`x & 1 == 0` test locked in)

### 4. Semantic checks (the safety rules, `spec/02` section 6)

- [ ] Name resolution + duplicate detection (project-wide, post-import)
- [ ] Const-folding for widths/params/`const`/`repeat` bounds
- [ ] Width checking incl. `+`/`-`/`*` growth and `+%` family exact-match
- [ ] Signed rules: no mixing, `signed()`/`unsigned()` casts, type-directed `extend`, negative literals (spec/02 section 1.7)
- [ ] Single-driver check; combinational cycle (DAG) check
- [ ] Exhaustiveness: `match` total, wire-`if` has `else`
- [ ] `=` vs `<-` placement enforcement; clock/reset domain typing incl. per-reg clock ownership
- [ ] Reg-requires-reset rule (module with regs must declare `reset`)
- [ ] Teaching error messages via `miette`/`ariadne` (English first)

### 5. Verilog emitter (first working version ✅ 2026-06-10; hardening open)

- [x] AST → synthesizable Verilog-2005: modules, assigns, always-blocks, FSM enums as localparams, match→ternary chains, instances with auto-wired outputs, implicit clk/rst connection
- [x] Reset generation from reg reset values (sync reset, active-high, v1)
- [x] Integration tests: all 5 examples compile; EN and Tanglish counters emit **identical** Verilog; FSM localparams verified
- [ ] `repeat` emission (blocked on const-eval); non-ASCII identifier transliteration; width-aware `extend`
- [ ] Golden-file tests: each example → expected `.v` (string-contains asserts exist; full goldens pending)
- [ ] Icarus Verilog smoke tests in CI: compile + run a self-checking TB per example (Icarus not installed locally yet)

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
- Resist scope creep: the deferred-features table in spec/02 section 7 is the fence
  (no memories, no inout, no structs, no CDC in this phase).
