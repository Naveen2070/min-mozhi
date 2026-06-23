# 1 — Overview: What Is This Thing?

Hey there! This guide walks you through **every Rust file** in the Min-Mozhi compiler — what each piece does, why it's there, and how it ticks. I've written it for someone who's brand new to this project, so I'll skip the jargon and keep things conversational.

Min-Mozhi ("Electricity Language") is the **first Tamil-rooted Hardware Description Language (HDL)** ever built. You write digital circuit designs — processors, counters, ALUs, anything — in a mix of English, Tanglish, or Tamil (you can even mix all three in one file). The compiler then turns it into Verilog-2005, which you can synthesize into real silicon.

It also has a built-in simulator so you can test your designs without needing any Verilog tools at all. Pretty neat.

The compiler itself is written in **Rust** with a strict `#![forbid(unsafe_code)]` policy — meaning there's zero unsafe code anywhere. It's safe code all the way down.

Here's the big picture of how source code flows through the system:

```
Your Code (.mimz) → Lexer → Parser → AST → Checker (6 safety passes)
                  → Verilog Output   OR   Interactive Simulator
```

Think of it like a factory assembly line: raw text comes in one end, and either Verilog code or simulation results come out the other.

---

## A Quick Map of the Codebase

```
src/
├── main.rs              # The front door — CLI that reads your commands
├── lib.rs               # Library root — everything re-exported from here
│
├── span.rs              # Source positions (every error knows WHERE)
├── diag.rs              # Error messages with pretty underlines
├── morph.rs             # Picking error language + Tamil grammar helpers
├── config.rs            # Reading mimz.toml project settings
├── project.rs           # Loading files and resolving imports
├── runner.rs            # Running commands in memory (powers the web playground)
├── translate.rs         # Switching keywords between English/Tanglish/Tamil
├── pretty.rs            # Turning the AST back into readable source
├── explain.rs           # Long-form explanations for error codes
├── version.rs           # Compiler version + language edition
│
├── lexer/               # The tokenizer (4 files)
├── parser/              # Tokens → structured tree (9 files)
├── ast/                 # The tree itself (2 files)
├── checker/             # Safety checks — 6 passes (12 files)
├── emit_verilog/        # Verilog code generator (5 files)
├── sim/                 # Event-driven simulator (9 files)
└── commands/            # CLI command handlers (10 files)
```

Now let's walk through each piece, one at a time. The rest of this guide is split into chapters — each chapter covers one folder or group of related files.

| Chapter                       | What it covers                                                                        |
| ----------------------------- | ------------------------------------------------------------------------------------- |
| [02](02-foundations.md)       | span, diag, morph, config, project, runner — the support modules                      |
| [03](03-lexer.md)             | The lexer (tokenizer) — all 3 files                                                   |
| [04](04-parser.md)            | The parser — all 7 files                                                              |
| [05](05-ast.md)               | The Abstract Syntax Tree                                                              |
| [06](06-checker.md)           | Six safety passes — all 9 files                                                       |
| [07](07-verilog-emitter.md)   | Verilog code generator — all 5 files                                                  |
| [08](08-simulator.md)         | Event-driven simulator — all 8 files                                                  |
| [09](09-tooling-and-entry.md) | CLI commands, main.rs, lib.rs, translate, pretty, explain, version                    |
| [10](10-ecosystem.md)         | LSP, WASM, VS Code, benchmarks, fuzzing, tests, CI, examples, demos, lang, spec, site |

---

## Design Principles

These eight principles guide every decision in the codebase. Keep them in mind as you read through the chapters:

1. **No unsafe code** — `#![forbid(unsafe_code)]` everywhere
2. **One AST** — flavor-blind and word-order-blind; the lexer and parser absorb all surface variation
3. **Stack overflow protection** — parser, elaborator, and emitter all have depth/budget limits
4. **Multi-error reporting** — no pass stops at the first error; all diagnostics are collected
5. **Stable E-codes** — error codes are never renumbered; `mimz explain` covers them permanently
6. **Teaching errors** — every error has a help line; long-form explanations live in `explain.rs`
7. **Shared expression semantics** — `sim/value.rs` has the single evaluator used by both the comb evaluator and the event-driven kernel
8. **Dumb emitter** — Verilog emission is deliberately naive, preserving symbolic widths and parenthesizing everything for correctness
