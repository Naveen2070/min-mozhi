# Code Documentation — How the Compiler Works

> Maintainer docs for `src/`. If you are going to read or change the
> compiler's code, start here.

These documents explain **how the code works today** and **why it is
shaped the way it is** — the things a future contributor cannot get from
reading one file at a time.

## How this folder relates to the other docs

| You want…                              | Go to                                        |
| -------------------------------------- | -------------------------------------------- |
| What the _language_ means (normative)  | [`spec/`](../../spec/)                       |
| The architecture contract & invariants | [`docs/architecture.md`](../architecture.md) |
| **How the code implements it**         | **this folder**                              |
| Item-level API reference               | `cargo doc --document-private-items --open`  |
| Why a decision was made, with date     | [`docs/log/`](../log/) (Decision blocks)     |
| What to build next                     | [`docs/plan/`](../plan/)                     |

Rule of thumb: `architecture.md` says what must stay true; this folder
says how the current code makes it true. When they disagree, one of them
is a bug — fix it the same day (RULES R1).

## Reading order

(File numbers are stable IDs, not reading order — read top to bottom.)

| Document                                                         | Covers                                                                            |
| ---------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| [`01-pipeline.md`](01-pipeline.md)                               | End-to-end: what happens when you run `mimz compile`                              |
| [`09-walkthrough-counter.md`](09-walkthrough-counter.md)         | The same pipeline SHOWN: real tokens, AST, and Verilog for `counter.mimz`         |
| [`02-lexer.md`](02-lexer.md)                                     | Tokens, the trilingual keyword table, the newline policy                          |
| [`03-parser.md`](03-parser.md)                                   | Recursive descent, error recovery, operator precedence                            |
| [`04-ast.md`](04-ast.md)                                         | The one shared AST and its design rules                                           |
| [`11-checker.md`](11-checker.md)                                 | The checker passes + the stable error-code catalog                                |
| [`05-emit-verilog.md`](05-emit-verilog.md)                       | How `.mimz` becomes Verilog text                                                  |
| [`06-diagnostics.md`](06-diagnostics.md)                         | The teaching-error system and how to write a good error                           |
| [`07-decisions-and-evolution.md`](07-decisions-and-evolution.md) | The code-shaping decisions, and how the code is planned to grow                   |
| [`08-contributing.md`](08-contributing.md)                       | Recipes: add a keyword, a syntax form, an emitter feature, a test                 |
| [`10-test-map.md`](10-test-map.md)                               | Every test's intent, what's deliberately uncovered, failure meaning               |
| [`12-benchmark.md`](12-benchmark.md)                             | The `mimz-bench` harness: speed/accuracy/safety/coverage + HTML report            |
| [`13-tooling.md`](13-tooling.md)                                 | Tooling modules: `explain`, `translate`/`pretty`, `morph` (error language), `sim` |

## The 60-second version

```text
 .mimz file ──read_source (NFC)──▶ source text
 source text ──lexer::lex──▶ Vec<Token>          (all 3 keyword flavors)
 Vec<Token> ──parser::parse──▶ ast::File         (one shared AST)
 [ast::File] ──checker::check──▶ names/consts/rules verified (E-codes)
 [ast::File] ──Project::from_files──▶ symbol table (modules + enums by name)
 symbol table + ASTs ──emit_verilog::emit──▶ Verilog-2005 text
```

- Every stage returns `Result<_, Vec<Diag>>` — errors are **values**, collected
  and rendered once, never printed mid-pass and never panicked.
- Every token and AST node carries a `Span` (byte range into the source), so
  every error can point at real code with a caret.
- The keyword table is **data** (`keywords.toml`), embedded at build time.
  English, Tanglish, and Tamil spellings all map to the same token, so
  everything after the lexer is flavor-blind.

Five **tooling** modules consume the pipeline rather than forming a stage in
it (page 13): `explain` (long-form text per E-code, `mimz explain`),
`translate` (keyword-flavor reskin, `mimz translate --to`), `pretty` (the
AST → source pretty-printer behind `mimz translate --order code|thamizh`),
`morph` (error-language selection + Tamil case-suffix inflection, behind
`--lang`), and `sim` (the Phase 1.5 simulator — the combinational evaluator
behind `mimz eval` plus the event-driven kernel, VCD/trace, and `test` runner
behind `mimz sim` / `mimz test`). A sixth, `config`, reads per-project defaults
from `mimz.toml` (CLI flags override it) — also page 13. A seventh, `version`,
holds the two version axes — the compiler (crate) version vs the language edition
(`EDITION_HISTORY`) — surfaced by `mimz --version` and the Verilog header (see
`spec/06-editions.md`).

## Keeping these docs honest

The structural facts in this folder (module lists, file-layout tables)
are **mechanically checked** by `tests/docs_sync.rs` — add a module or a
source file without updating the docs and `cargo test` fails, naming the
stale page. Prose truthfulness can't be automated: when you change how
the code works, update the matching page in the same session (RULES R1)
and refresh the stamp below.

_Last synced with the code: 2026-06-17 (post–Phase 1.5 RTL-parity batch A1–A5 —
replication, don't-care patterns, `on fall`, `mem`, `async reset` — plus
Workstream B: the new `version` module (compiler vs language-edition axes,
`EDITION_HISTORY`, `mimz --version`, `spec/06-editions.md`, `CHANGELOG.md`).
Prior 2026-06-16 (a docs-currency pass for the **completed
Phase 1.5 simulator** (C1–C4)): refreshed the `sim` description on pages 1, 13 and
this README (full event-driven engine + `mimz sim`/`mimz test`, not just the
combinational slice); flipped pages 1 and 7's "next pipeline work" from Phase 1.8
/ Phase 1.5 to the Phase 2 IR; added `sim`/`test` to the page-1 subcommand list;
and corrected the test map (page 10) per-section counts to match reality
(parser 21 → 24, checker 98 → 99, elaboration 5 → 8, sim integration 9 → 10, test
integration 6 → 7) and broadened the Layer-3 Icarus differential row to the full
21-example single-file corpus. The 364 grand total was already correct. Prior:
Phase 0 closed + **keyword set v1 locked** 2026-06-15; the **native-authored Tamil/Tanglish error catalog** shipped
(decision C3 ratified) — `messages.toml` + structured-arg interpolation through
`Diag::with_arg`/`Checker::err_args`, 33 of 36 checker codes localized (pages 6,
13); no longer a stub. A docs-currency pass refreshed pages 1, 6, 13, the test map
(page 10), and this stamp. Prior 2026-06-15 (adds: the `config` module — `mimz.toml`
project defaults for CLI flags, discovered by walking up from the input file,
with precedence CLI › config › default (page 13); and reversible romanization +
auto name-map discovery on `mimz translate`. A same-day fuzz/security audit then
added the `reskin` script-boundary guard + a `--names-map` version check
(`docs/audit/bugs.md` BUG-2) and a fourth `translate_roundtrip` cargo-fuzz target.
A behavior-preserving code-split then broke three oversized files into submodules:
`src/parser/items.rs` → `items/`, `src/main.rs` handlers → `src/commands/`, and
`src/bin/mimz-bench/metrics.rs` → `metrics/` (pages 3, 13, 12).
Prior 2026-06-14 (adds: the `morph` module — error-language
selection (file-flavor majority + `--lang`) and the Tamil case-suffix inflection
mechanism behind localized diagnostics (Phase 1.8, spec/04 section 5), an additive
English-fallback layer documented in page 13; the catalog content + final sandhi
are panel-gated (C3). Earlier the same day: the `pretty` module — the AST →
source pretty-printer behind `mimz translate --order code|thamizh` (Phase 1.8),
documented in page 13; and the Phase 1.8 thamizh-order parser flips. Prior
2026-06-13 (adds: the quick-wins tooling block —
`explain` (`mimz explain <CODE>`), `translate` (`mimz translate --to <flavor>`),
and `sim::comb` (`mimz eval`), documented in page 13; and earlier the same day:
monotonic chained comparison
`a <= b <= c` in the parser; the `window` example; the `mimz-bench` memory
metric (peak RSS) + an upgraded HTML report; a `criterion` per-phase
micro-benchmark harness (`benches/compile.rs`, `cargo bench`); CI extended
with rustdoc/bench/perf-batch jobs and a committed `bench-history.jsonl`.
Prior 2026-06-12 baseline: Phase 1 COMPLETE — checker all six passes,
`repeat` emission, transliteration, signed emission, golden files, full
E-code coverage incl. lexer E10xx/parser E11xx/loader E1201, lib/bin split
(`src/lib.rs`), `--json` diagnostics, LSP v0, `mimz-bench` harness page 12)._
