# Phase 1 — Verilog Backend

> **Get something working end-to-end.**
> Window: months 3–7 · Target: **31 Dec 2026** (solo, ~8–10 h/wk) ·
> Status: 🟢 **COMPLETE 2026-06-12** — every work item ticked, gating and
> non-gating (150 tests; the 31 Dec 2026 target beaten by six months).
> v0.1.0 tag is the founder's call.

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
- [x] `repeat` compile-time unrolling — ✅ 2026-06-12: checker walks each iteration (widths/drivers), emitter unrolls into flat instance arrays (`name__<i>`); declarations inside `repeat` are E0303; `ripple_adder` example (4 flavors) + exhaustive Icarus TB
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
- [x] Teaching error messages: own caret renderer + stable E-codes —
      ✅ complete 2026-06-12: lexer E1001–E1008, parser E1101–E1111,
      loader E1201 (catalog in docs/code/06); `Parser::error` now makes
      the code mandatory, same as `Checker::err`. Also landed here, as
      the Phase-4/LSP prerequisites: **lib/bin split** (`src/lib.rs`,
      thin CLI; `project.rs` returns `LoadError` values, never exits)
      and **`--json` diagnostics** on `check`/`compile` (stable wire
      format, locked by an end-to-end test)

### 5. Verilog emitter (first working version ✅ 2026-06-10; hardening open)

- [x] AST → synthesizable Verilog-2005: modules, assigns, always-blocks, FSM enums as localparams, match→ternary chains, instances with auto-wired outputs, implicit clk/rst connection
- [x] Reset generation from reg reset values (sync reset, active-high, v1)
- [x] Integration tests: all 56 examples compile (14 base examples × 4 flavor folders: english/tanglish/tamil/mixed); each base example emits **byte-identical** Verilog from all four flavors; FSM localparams verified (2026-06-11)
- [x] `repeat` emission — ✅ 2026-06-12: env-based unrolling reusing the const-eval engine; instance arrays flatten to `name__<i>` with outputs `name__<i>_<port>`; compile-time `if`/index folding; `REPEAT_BUDGET` runaway guard
- [x] Non-ASCII identifier transliteration — ✅ 2026-06-12 (Phase C): `emit_verilog::transliterate` AST pre-pass; Tamil → readable ASCII (விளக்கு → `villakku`), `_uXXXX` hex fallback, deterministic collision suffixes; `vilakku` example (4 flavors) proves it end to end
- [x] Width-aware `extend` — ✅ 2026-06-12 (Phase C), closed by FIXING signed emission: `signed[N]` now declares `wire signed`/`reg signed`, so Verilog sign-extends `extend` and compares signed natively (sound under E0403's no-mixing rule); `signed_math` example + exhaustive 256-pair Icarus TB verify it (unsigned extension was already correct via assignment context)
- [x] Golden-file tests — ✅ 2026-06-12 (Phase C): `tests/golden/<base>.v` pins every base example's FULL output (banner stripped); `MIMZ_UPDATE_GOLDENS=1` regenerates after intended changes
- [x] Icarus Verilog differential tests, local + CI — ✅ 2026-06-11, `tests/icarus.rs`: all 56 emitted `.v` pass `iverilog -t null`; one self-checking TB per base example (`tests/icarus/*_tb.v`, 13 files) simulates to PASS under `vvp` (incl. `ripple_adder` exhaustive 512-combo add and `signed_math` exhaustive signed semantics). Skips with a note when Icarus is absent; CI installs it and sets `REQUIRE_IVERILOG=1` so it can never silently skip
- [x] End-to-end **error** validation — ✅ 2026-06-12, `tests/errors.rs` + `tests/fixtures/errors/` (~67 broken `.mimz`): every checker E-code (E0001–E0701) has a fixture the real binary must reject with that code on stderr; a completeness guard blocks shipping a new code without one. Compile-time only by design — broken code never reaches the emitter

### 6. Visibility (decision D4)

- [x] Minimal VS Code syntax highlighting: TextMate grammar for `.mimz` (all keyword flavors) — ✅ 2026-06-11, `editors/vscode/`, kept in lockstep with keywords.toml by `tests/grammar_sync.rs`
- [x] **LSP v0 — diagnostics only** — ✅ 2026-06-12: `mimz lsp` via
      `tower-lsp` (bin-only module `src/lsp.rs`; the lib stays async-free
      for the Phase 4 WASM build). Full pipeline on
      didOpen/didChange/didSave over the in-memory text (imports from
      disk — documented v0 limitation); per-file publishes with stale
      clearing; positions in UTF-16 (Tamil-safe); E-code + help on every
      squiggle. VS Code client added to `editors/vscode` (plain JS,
      `mimz.serverPath` setting, packaged as `mimz-0.2.0.vsix`).
      Smoke-tested over the real wire protocol (`tests/lsp.rs`).
      Hover/go-to-def/completion stay in Phase 4.

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
