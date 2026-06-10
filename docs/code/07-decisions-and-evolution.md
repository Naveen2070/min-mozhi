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
few: `clap`, `serde`+`toml`, `unicode-ident`, `unicode-normalization` —
each one earns its place; resist adding more.

### One binary crate, modules per stage — no workspace yet

`main.rs` + seven modules. A `lib.rs`/workspace split is **trigger-based**
(architecture section 5): it happens when a second consumer (LSP, web
playground) exists, not before. Same for an IR, a query system, and
incremental compilation — named triggers, not speculative scaffolding.

### The module-scoping pattern (refactor of 2026-06-10)

Big files were split with one repeating pattern, used by `lexer/`,
`parser/`, `ast/`, `emit_verilog/`:

- `mod.rs` owns the struct, its state, and private plumbing;
- sibling files (`items.rs`, `expr.rs`, `module.rs`…) hold `impl` blocks
  for one concern each, reached via a few `pub(super)` entry points;
- Rust privacy does the rest: `mod.rs` items are visible to descendant
  modules without being `pub` anywhere else.

Follow the pattern when a file outgrows ~600 lines; don't pre-split.

### Keywords are data (`keywords.toml`), loaded once, validated loudly

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

| Rule                                                             | Enforced today by | Eventually by |
| ---------------------------------------------------------------- | ----------------- | ------------- |
| no `/` / `%` operators                                           | lexer             | lexer         |
| `=` wires-only vs `<-` regs-only                                 | parser            | parser        |
| reg requires reset value                                         | parser            | parser        |
| if-expression requires `else`                                    | parser            | parser        |
| no comparison chaining                                           | parser            | parser        |
| widths, single-driver, exhaustiveness, comb-DAG, clock ownership | — (not yet)       | **checker**   |

The earlier a rule fires, the better the span and the simpler the
message. Push enforcement as early as it can correctly live.

## How the code has actually turned out (honest notes)

- The front end (lexer → parser → emitter) went from empty repo to
  25 passing tests in one day (2026-06-10) — the string-emitter decision
  is what made that possible.
- The trilingual thesis held with **zero** extra code outside the lexer:
  the EN/Tanglish byte-identical-output test passed on first run once the
  table loaded. The "one shared AST" invariant is doing real work.
- Things that bit us, recorded so they don't bite twice: the TOML
  root-key ordering trap (`02-lexer.md`), `use TokKind::*` glob shadowing
  the `Kw` type in pattern positions (write `Kw(token::Kw::And)`),
  clippy's `manual_strip` pushing us to the cleaner `verilog_literal`
  helper.

## Where the code goes next (in order)

1. **Checker** (Phase 1 work item 4) — the next big component. New
   `src/checker/` with one pass per safety rule, each with its own tests.
   Needs: name resolution, const-eval (this unblocks `repeat` unrolling
   in the emitter), width rules, single-driver, exhaustiveness,
   reg-reset, clock ownership. The `Project` symbol table likely moves
   here from `emit_verilog`.
2. **Stable error codes** (`E0001`…) — flagged as the one thing to do
   EARLY (before the catalog grows past ~30), so Phase 1.8 message
   translation keys off codes.
3. **Icarus Verilog differential tests** — CI compiles every example and
   runs the output through a real Verilog tool.
4. **Phase 1.8 grammar engine** — second parser profile
   (`thamizh-order`), same productions with flipped clause heads, same
   AST. The parser was built one-token-lookahead to keep this cheap.
5. **Phase 1.5 simulator** — first consumer of `TestDecl`/`TestStmt`
   (and the point where `ast`'s `#![allow(dead_code)]` gets deleted).

Each of these has a full plan file in [`docs/plan/`](../plan/).
