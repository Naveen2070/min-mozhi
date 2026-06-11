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

| Document                                                         | Covers                                                                    |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------- |
| [`01-pipeline.md`](01-pipeline.md)                               | End-to-end: what happens when you run `mimz compile`                      |
| [`09-walkthrough-counter.md`](09-walkthrough-counter.md)         | The same pipeline SHOWN: real tokens, AST, and Verilog for `counter.mimz` |
| [`02-lexer.md`](02-lexer.md)                                     | Tokens, the trilingual keyword table, the newline policy                  |
| [`03-parser.md`](03-parser.md)                                   | Recursive descent, error recovery, operator precedence                    |
| [`04-ast.md`](04-ast.md)                                         | The one shared AST and its design rules                                   |
| [`05-emit-verilog.md`](05-emit-verilog.md)                       | How `.mimz` becomes Verilog text                                          |
| [`06-diagnostics.md`](06-diagnostics.md)                         | The teaching-error system and how to write a good error                   |
| [`07-decisions-and-evolution.md`](07-decisions-and-evolution.md) | The code-shaping decisions, and how the code is planned to grow           |
| [`08-contributing.md`](08-contributing.md)                       | Recipes: add a keyword, a syntax form, an emitter feature, a test         |
| [`10-test-map.md`](10-test-map.md)                               | Every test's intent, what's deliberately uncovered, failure meaning       |

## The 60-second version

```text
 .mimz file ──read_source (NFC)──▶ source text
 source text ──lexer::lex──▶ Vec<Token>          (all 3 keyword flavors)
 Vec<Token> ──parser::parse──▶ ast::File         (one shared AST)
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

## Keeping these docs honest

The structural facts in this folder (module lists, file-layout tables)
are **mechanically checked** by `tests/docs_sync.rs` — add a module or a
source file without updating the docs and `cargo test` fails, naming the
stale page. Prose truthfulness can't be automated: when you change how
the code works, update the matching page in the same session (RULES R1)
and refresh the stamp below.

_Last synced with the code: 2026-06-11 (Phase 1 — checker not yet built)._
