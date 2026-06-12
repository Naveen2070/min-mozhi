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
- [x] `import` resolution: file-relative, project-unique module names, cycle-safe visited set (spec/02 section 1.5); `include` accepted as en alias (v0.2.1, 2026-06-11)
- [ ] `repeat` compile-time unrolling (parses; unrolling needs const-eval — work item 4)
- [x] Error recovery good enough to report >1 error per run (verified: 3 errors in one `check`)
- [x] Rust precedence incl. non-associative comparisons (`x & 1 == 0` test locked in)

### 4. Semantic checks (the safety rules, `spec/02` section 6)

- [x] Name resolution + duplicate detection (project-wide, post-import) — ✅ 2026-06-11, `src/checker/` first slice, stable E-codes E0001–E0109 (catalog: docs/code/11-checker.md)
- [x] Const-evaluation engine: `const` decls, `repeat` bounds (✅ 2026-06-11, E02xx); width-position folding lands with width checking
- [x] Width checking incl. `+`/`-`/`*` growth and `+%` family exact-match — ✅ 2026-06-11, `src/checker/widths.rs` (E0401–E0410); concrete-binding strategy (defaults + per-instantiation), literal fitting, connection checking
- [x] Signed rules: no mixing, `signed()`/`unsigned()` casts, type-directed `extend`, negative literals — ✅ 2026-06-11 (same pass: E0403/E0405/E0407)
- [x] Single-driver check; combinational cycle (DAG) check — ✅ 2026-06-11, `src/checker/drivers.rs` (E0501–E0505): per-bit drive extents, output coverage, reg-per-on-block, through-instance cycles via comb summaries
- [x] Exhaustiveness: `match` total, wire-`if` has `else` — ✅ 2026-06-12
      (E0601 non-exhaustive naming the gap, E0602 unreachable/duplicate
      arms; wire-`if` was already parser-enforced). Spec ruling v0.2.3:
      full coverage needs no `_`; defensive `_` after full coverage is
      legal (`docs/Ideas/language_plan.md` 1.4 resolved)
- [x] `=` vs `<-` placement enforcement (✅ 2026-06-11, E0505) — clock/reset
      domain typing incl. per-reg clock ownership ✅ 2026-06-12
      (`src/checker/clocks.rs`, E0701: cross-domain reads and
      domain-mixing wires rejected, module-local; `sync` relaxes it in
      Phase 2)
- [x] Instantiation completeness: every input connected exactly once —
      ✅ 2026-06-12 (E0302, missing inputs listed; clock/reset stay
      implicit-by-name)
- [x] Reg-requires-reset rule (module with regs must declare `reset`) — ✅ 2026-06-11 (E0301)
- [ ] Teaching error messages: own caret renderer + stable E-codes ✅ (checker); retrofit codes onto lexer/parser errors before the Phase 1.8 catalogs (`miette`/`ariadne` not adopted — custom renderer kept)

### 5. Verilog emitter (first working version ✅ 2026-06-10; hardening open)

- [x] AST → synthesizable Verilog-2005: modules, assigns, always-blocks, FSM enums as localparams, match→ternary chains, instances with auto-wired outputs, implicit clk/rst connection
- [x] Reset generation from reg reset values (sync reset, active-high, v1)
- [x] Integration tests: all 44 examples compile (11 base examples × 4 flavor folders: english/tanglish/tamil/mixed); each base example emits **byte-identical** Verilog from all four flavors; FSM localparams verified (2026-06-11)
- [ ] `repeat` emission (blocked on const-eval); non-ASCII identifier transliteration; width-aware `extend`
- [ ] Golden-file tests: each example → expected `.v` (string-contains asserts exist; full goldens pending)
- [x] Icarus Verilog differential tests, local + CI — ✅ 2026-06-11, `tests/icarus.rs`: all 44 emitted `.v` pass `iverilog -t null`; one self-checking TB per base example (`tests/icarus/*_tb.v`, 11 files) simulates to PASS under `vvp`. Skips with a note when Icarus is absent; CI installs it and sets `REQUIRE_IVERILOG=1` so it can never silently skip

### 6. Visibility (decision D4)

- [x] Minimal VS Code syntax highlighting: TextMate grammar for `.mimz` (all keyword flavors) — ✅ 2026-06-11, `editors/vscode/`, kept in lockstep with keywords.toml by `tests/grammar_sync.rs`
- [ ] **LSP v0 — diagnostics only** (pulled forward from Phase 4, Decision
      2026-06-12): `mimz lsp` via `tower-lsp` — parse + check on open/save,
      publish the checker's diagnostics (E-codes + help lines) in-editor.
      Rides the lib/bin split + `--json` work (item 4's E-code retrofit) and
      IS the "second consumer" that architecture section 5 names as the
      split trigger. **Non-gating**: v0.1.0 and the safety slices outrank
      it; hover/go-to-def/completion stay in Phase 4.

## Milestone

All `examples/*.mimz` compile and simulate correctly under Icarus.
**Tag v0.1.0. Repo goes public after Phase 1.8 (decision D7).**

## Exit criteria

1. `mimz compile` works on every example, in all four flavor folders (english/tanglish/tamil/mixed) — ✅ 2026-06-11, CI-asserted.
2. Each safety rule has at least one test proving it rejects bad input with a helpful message.
3. CI runs lexer/parser/check/emit tests + Icarus simulation green.

## Risks / notes

- Keep the emitter dumb and readable — optimization belongs to Phase 2 IR.
- Resist scope creep: the deferred-features table in spec/02 section 7 is the fence
  (no memories, no inout, no structs, no CDC in this phase).
