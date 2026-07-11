# 07 — Decisions That Shaped the Code, and Where It Goes Next

The full dated record lives in [`docs/log/`](../log/) (Decision blocks)
and the contract in [`docs/architecture.md`](../architecture.md). This
page is the code-focused digest: what a contributor must understand
before "improving" something that is the way it is on purpose.

## Decisions baked into the current code

### Rust, stable toolchain, tiny dependency set

Rust for the compiler (memory-safe systems language, great error-message
ecosystem, single static binary — full rationale in the roadmap's Why-Rust
decision record). MSRV 1.85 (edition 2024). Dependencies are deliberately
few — each one earns its place; resist adding more: `clap`,
`serde`+`serde_json`+`toml` (the keyword table and the `--json` wire
format), `unicode-ident`, `unicode-normalization`, and — for the LSP
only, used by the bin-only `lsp.rs` module so the lib stays async-free —
`tower-lsp`+`tokio`.

### Lib + thin binary, modules per stage — then a 3-crate workspace

The trigger named in architecture section 5 FIRED on 2026-06-12: the LSP
(plus `--json` consumers) is the second consumer, so the module tree
moved into a `lib.rs` with `main.rs` as a thin CLI shell (arg parsing +
human/JSON rendering only). `project.rs` returns `LoadError` values
instead of printing and exiting — the lib never touches stdout/stderr.

A second trigger fired on 2026-07-09/10: the WASM playground needed a
dependency-optional-free build without feature-flag gymnastics, so the
lib split into a 3-crate workspace along the pure/impure axis —
`mimz-core` (lexer/parser/ast/checker/emit_verilog/etc, zero optional
deps), `mimz-sim` (event-driven simulator, depends only on mimz-core),
and the root shell crate (CLI, fs I/O, LSP, hw-emulation), which
re-exports both as a facade so every `mimz::…` path a caller already used
keeps compiling. Full rationale in
`docs/plan/workspace-split.local.md`. An IR, a query system, and
incremental compilation remain trigger-based and have not fired — named
triggers, not speculative scaffolding.

### The module-scoping pattern (refactor of 2026-06-10)

Big files were split with one repeating pattern, used by `lexer/`,
`parser/`, `ast/`, `emit_verilog/`, and (since 2026-06-12, when the
width pass hit 1859 lines) `checker/widths/`:

- `mod.rs` owns the struct, its state, and private plumbing;
- sibling files (`items.rs`, `expr.rs`, `module.rs`…) hold `impl` blocks
  for one concern each, reached via a few `pub(super)` entry points;
- Rust privacy does the rest: `mod.rs` items are visible to descendant
  modules without being `pub` anywhere else.

Follow the pattern when a file outgrows ~600 lines; don't pre-split.

### Keywords are data (`lang/keywords.toml`), loaded once, validated loudly

Native-speaker word review must be a data change, never a code change.
The table panics at startup on any inconsistency — CI catches table bugs
before any user can. See [`02-lexer.md`](02-lexer.md).

### Errors are values; multi-error everywhere

See [`06-diagnostics.md`](06-diagnostics.md). This shapes every
function signature in the pipeline (`Result<_, Vec<Diag>>`, parser
`Option` + recorded diags).

### The emitter is a string formatter, on purpose

Architecture invariant #6: "deliberately dumb and readable". Symbolic
widths, parens around everything, no const-eval, errors instead of
guesses for unsupported features. The Phase 2 IR demotes it to a
debugging backend; do not grow it into a compiler in the meantime.

### Safety rules enforced at the earliest possible stage

| Rule                                                             | Enforced today by                      | Eventually by |
| ---------------------------------------------------------------- | -------------------------------------- | ------------- |
| no `/` / `%` operators                                           | lexer                                  | lexer         |
| `=` wires-only vs `<-` regs-only                                 | parser                                 | parser        |
| reg requires reset value                                         | parser                                 | parser        |
| if-expression requires `else`                                    | parser                                 | parser        |
| no comparison chaining                                           | parser                                 | parser        |
| widths, single-driver, exhaustiveness, comb-DAG, clock ownership | **checker** (all landed 2026-06-11/12) | checker       |

The earlier a rule fires, the better the span and the simpler the
message. Push enforcement as early as it can correctly live.

## How the code has actually turned out (honest notes)

- The front end (lexer → parser → emitter) went from empty repo to
  25 passing tests in one day (2026-06-10) — the string-emitter decision
  is what made that possible.
- The trilingual thesis held with **zero** extra code outside the lexer:
  the EN/Tanglish byte-identical-output test passed on first run once the
  table loaded. The "one shared AST" invariant is doing real work.
- Pitfalls hit during implementation, recorded so they don't recur: the TOML
  root-key ordering trap (`02-lexer.md`), `use TokKind::*` glob shadowing
  the `Kw` type in pattern positions (write `Kw(token::Kw::And)`),
  clippy's `manual_strip` pushing us to the cleaner `verilog_literal`
  helper.

## Where the code goes next (in order)

1. ~~**Checker**~~ — ✅ landed 2026-06-11/12, seven passes in
   `crates/mimz-core/src/checker/` (symbols, consteval, names, widths, drivers, funcs, clocks);
   every spec/02 section 6 rule is now compiler-enforced.
2. ~~**Stable error codes**~~ — ✅ complete 2026-06-12: every diagnostic
   in the compiler carries one (checker E0xxx, lexer E10xx, parser
   E11xx, loader E12xx — full map in docs/code/06).
3. ~~**Icarus Verilog differential tests**~~ — ✅ landed 2026-06-11
   (`tests/icarus.rs`, self-checking TBs per example).
4. ~~**`repeat` emission + emitter hardening**~~ — ✅ done: unrolling via
   const-eval, transliteration, golden files (Phase 1 complete).
5. ~~**Phase 1.8 grammar engine**~~ — ✅ done: the second parser profile
   (`thamizh-order`) ships all five clause flips (clocked block,
   conditional, if-expression, match, test header), same productions, same
   AST. The parser was built one-token-lookahead to keep this cheap.
6. ~~**Phase 1.5 simulator**~~ — ✅ done: the event-driven engine, VCD/console
   trace, `mimz sim`/`mimz test`, and full parity (C1 combinational, C2
   instance flattening, C3 `repeat` unroll, C4 enum signals) all ship,
   validated by the Icarus differential. The first consumer of
   `TestDecl`/`TestStmt`.

Each of these has a full plan file in [`docs/plan/`](../plan/). The next
pipeline work is the Phase 2 IR (the named trigger that demotes the string
emitter to a debugging backend).
