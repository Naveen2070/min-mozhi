# 13 — Tooling modules (`explain`, `translate`, `sim`)

Three lib modules that **consume** the pipeline rather than forming a stage in
it. Each is the lib-backed core of one CLI subcommand, kept in the library (not
`main.rs`) so editors over the LSP and the future WASM playground reuse them.
All three landed 2026-06-13 as the "quick wins" block
(`docs/Ideas/language_plan.md` section 9 / `docs/plan/`): incremental,
edition-safe, and each a down-payment on a later phase.

## `explain` (`src/explain.rs`) — `mimz explain <CODE>`

The classroom version of every diagnostic. The one-line `help:` on a `Diag`
says how to fix an error in the moment (spec/01 G1); `explain` gives the long
form: what the rule is, why silicon needs it, the corrected shape, and a small
honest ASCII diagram where it helps. This is the incremental build-out of idea
8.1 (Elm-style didactic errors).

- `explain(code) -> Option<&'static str>` — case-insensitive, trims; `None` for
  an unknown code (the CLI then lists the valid ones).
- `codes()` — every code with an entry, in catalog order.
- Keyed off the stable E-codes (catalogs: [`11-checker.md`](11-checker.md) for
  E0xxx, [`06-diagnostics.md`](06-diagnostics.md) for lexer E10xx / parser
  E11xx / loader E12xx). A unit test pins every `diag::ALL_CHECKER_CODES` entry
  to a row, so a new checker code cannot ship without its explanation — the same
  docs-sync spirit as the error-fixture guard in `tests/errors.rs`.

## `translate` (`src/translate.rs`) — `mimz translate --to <flavor>`

Reskins a file's KEYWORDS into another language flavor (english / tanglish /
tamil), losslessly. This is the flavor-only half of `spec/04`'s `translate`; the
natural Tamil WORD-ORDER half (`--order thamizh`, which reorders the AST) is
Phase 1.8.

- Mechanism: re-lex, copy the source verbatim, and substitute **only** keyword
  token spans with the target column's canonical spelling (via the reverse
  `Kw → spellings` map `keywords::KeywordTable::canonical`). Comments, layout,
  identifiers, and numbers are untouched — lossless by construction — and any
  accepted alias (e.g. `include`) normalizes to its canonical spelling.
- Oracle: the `examples/{english,tanglish,tamil}/` folders are byte-identical
  keyword-swaps (R9), so `tests/translate.rs` checks that translating one
  flavor reproduces another at the token level, and that round-trips are
  byte-identical (modulo alias canonicalization).
- NOTE: tanglish/tamil targets ride the DRAFT keyword columns until
  native-speaker review closes (keywords.toml header).

## `sim` (`src/sim/`) — `mimz eval` (combinational slice of Phase 1.5)

`sim::comb::eval_outputs` interprets a single **combinational** module: given a
value per input, it computes the outputs by walking the AST. No clock, no `reg`,
no instances, no `repeat` — those are rejected with a clear message, not
half-evaluated (the full event-driven engine, VCD, and `test` execution are
Phase 1.5 proper, `docs/plan/phase-1.5-simulator.md`).

- Values are unsigned bit-vectors up to 128 bits, carrying a width and a signed
  flag; it honors the spec's width semantics (lossless `+ - *` grow, the
  `+% -% *%` family wraps, slices/concat/`extend`/`trunc` resize), so a result
  matches what the Verilog emitter would produce for the same logic.
- A private `const_eval` mirrors `checker::consteval` on the subset needed for
  widths, parameters, consts, and slice/index bounds.
- This is the engine the 8.5 hardware REPL and the WASM playground will ride on,
  which is why it lives in the lib and stays callable on a single module. The
  `mimz eval` CLI is its experimental surface (`--in a=3,b=5`, `--module`,
  `--param`).

## Scope discipline

These are intentionally small slices, not finished features: `explain` grows one
code at a time, `translate` does flavor only (word order is Phase 1.8), and
`sim::comb` is combinational only (the kernel is Phase 1.5). Each documents its
own limits in its module header so the honesty rule (spec/01) holds for the
tooling too.
