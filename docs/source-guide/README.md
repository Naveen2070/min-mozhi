# Source Code Guide

> A friendly walkthrough of every Rust file in the Min-Mozhi compiler — what each
> piece does, why it's there, and how it ticks. Written for someone brand new to
> the project. This is the **friendliest entry point** if you want to understand
> the codebase without getting into design-decision detail.

## How this folder relates to the other docs

| You want…                                    | Go to                                        |
| -------------------------------------------- | -------------------------------------------- |
| **A friendly tour of every Rust file**       | **this folder**                              |
| How the compiler internals work (maintainer) | [`docs/code/`](../code/)                     |
| How to **write** Min-Mozhi code              | [`docs/guide/`](../guide/)                   |
| What the _language_ means (normative)        | [`spec/`](../../spec/)                       |
| The architecture contract & invariants       | [`docs/architecture.md`](../architecture.md) |

## The chapters

| #   | Chapter                                    | Covers                                                                                                                            |
| --- | ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| 1   | [Overview](01-overview.md)                 | Project intro, pipeline overview, codebase map, design principles                                                                 |
| 2   | [Foundations](02-foundations.md)           | `span.rs`, `diag.rs`, `morph.rs`, `config.rs`, `project.rs`, `runner.rs`, `translate.rs`, `pretty.rs`, `explain.rs`, `version.rs` |
| 3   | [Lexer](03-lexer.md)                       | `lexer/mod.rs`, `lexer/token.rs`, `lexer/keywords.rs`                                                                             |
| 4   | [Parser](04-parser.md)                     | `parser/mod.rs`, `parser/expr.rs`, `parser/items/*`                                                                               |
| 5   | [AST](05-ast.md)                           | `ast/mod.rs`, `ast/expr.rs`                                                                                                       |
| 6   | [Checker](06-checker.md)                   | `checker/` — all 6 safety passes (9 files)                                                                                        |
| 7   | [Verilog Emitter](07-verilog-emitter.md)   | `emit_verilog/` — code generation (5 files)                                                                                       |
| 8   | [Simulator](08-simulator.md)               | `sim/` — event-driven simulation (8 files)                                                                                        |
| 9   | [Tooling & Entry](09-tooling-and-entry.md) | `commands/`, `main.rs`, `lib.rs`, LSP, WASM, VS Code                                                                              |
| 10  | [Ecosystem](10-ecosystem.md)               | Benchmarks, fuzzing, tests, CI, examples, demo, lang, spec, site                                                                  |
