# 1 — Overview: What Is This Thing?

Hey there! This guide walks you through **every Rust file** in the Min-Mozhi compiler — what each piece does, why it's there, and how it ticks. I've written it for someone who's brand new to this project, so I'll skip the jargon and keep things conversational.

Min-Mozhi ("Electricity Language") is the **first Tamil-rooted Hardware Description Language (HDL)** ever built. You write digital circuit designs — processors, counters, ALUs, anything — in a mix of English, Tanglish, or Tamil (you can even mix all three in one file). The compiler then turns it into Verilog-2005, which you can synthesize into real silicon.

It also has a built-in simulator so you can test your designs without needing any Verilog tools at all. Pretty neat.

The compiler itself is written in **Rust** with a strict `#![forbid(unsafe_code)]` policy — meaning there's zero unsafe code anywhere. It's safe code all the way down.

Here's the big picture of how source code flows through the system:

```
Your Code (.mimz) → Lexer → Parser → AST → Checker (7 safety passes)
                  → Verilog Output   OR   Interactive Simulator
```

Think of it like a factory assembly line: raw text comes in one end, and either Verilog code or simulation results come out the other.

---

## A Quick Map of the Codebase

The compiler is a 3-crate Cargo workspace, split along a pure/impure line
(the "workspace split", 2026-07-10): everything with zero optional
dependencies lives in `mimz-core`/`mimz-sim`, and everything that touches a
filesystem, terminal, or OS peripheral stays in the root shell crate. Root
`mimz` re-exports `mimz-core`/`mimz-sim` so every `mimz::…` path you'll see
elsewhere in this guide still resolves — only the physical file location
changed.

```
crates/mimz-core/src/     # pure frontend + middle + emit — no optional deps
├── span.rs                   # Source positions (every error knows WHERE)
├── diag.rs                   # Error messages with pretty underlines
├── morph.rs                  # Picking error language + Tamil grammar helpers
├── project.rs                # LoadedFile + render_diags (the pure half; fs I/O stays in root project.rs)
├── translate.rs              # Switching keywords between English/Tanglish/Tamil
├── pretty.rs                 # Turning the AST back into readable source
├── explain.rs                # Long-form explanations for error codes
├── version.rs                # Compiler version + language edition
├── stdlib.rs                 # Embedded standard library (seg7/pwm/fifo/uart_tx/debouncer)
├── analysis.rs               # Editor symbol index + offset→definition / completion (LSP)
├── lexer/                    # The tokenizer (4 files)
├── parser/                   # Tokens → structured tree (11 files)
├── ast/                      # The tree itself (3 files)
├── checker/                  # Safety checks — 7 passes (13 files)
└── emit_verilog/             # Verilog code generator (5 files)

crates/mimz-sim/src/      # event-driven simulator — depends only on mimz-core
├── runner.rs                 # Running commands in memory (powers the web playground)
└── sim/                      # Event-driven simulator + EmulationHost trait (10 files)

src/                       # shell crate — CLI, fs I/O, LSP, hw-emulation
├── main.rs                   # The front door — CLI that reads your commands
├── lib.rs                    # Library root — facade re-exporting mimz-core + mimz-sim
├── config.rs                 # Reading mimz.toml project settings
├── project.rs                # Loading files and resolving imports (fs-touching half)
├── emulate/                  # Native hw-emulation peripherals, `hw-emulation` feature (7 files)
├── lsp.rs                    # Language server (optional, `lsp` feature)
└── commands/                 # CLI command handlers (16 files)
```

Now let's walk through each piece, one at a time. The rest of this guide is split into chapters — each chapter covers one folder or group of related files.

| Chapter                       | What it covers                                                                          |
| ----------------------------- | --------------------------------------------------------------------------------------- |
| [02](02-foundations.md)       | span, diag, morph, config, project, runner, stdlib — the support modules                |
| [03](03-lexer.md)             | The lexer (tokenizer) — all 4 files                                                     |
| [04](04-parser.md)            | The parser — all 9 files                                                                |
| [05](05-ast.md)               | The Abstract Syntax Tree                                                                |
| [06](06-checker.md)           | Seven safety passes — all 13 files                                                      |
| [07](07-verilog-emitter.md)   | Verilog code generator — all 5 files                                                    |
| [08](08-simulator.md)         | Event-driven simulator — all 9 files                                                    |
| [09](09-tooling-and-entry.md) | CLI commands, main.rs, lib.rs, translate, pretty, explain, version, analysis.rs, lsp.rs |
| [10](10-ecosystem.md)         | LSP, WASM, VS Code, benchmarks, fuzzing, tests, CI, examples, demos, lang, spec, site   |

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
