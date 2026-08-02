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

| #   | Chapter                                        | Covers                                                                                                                      |
| --- | ---------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| 1   | [Overview](01-overview.md)                     | Project intro, pipeline overview, codebase map, design principles                                                           |
| 2   | [Foundations](02-foundations.md)               | `span.rs`, `diag.rs`, `morph.rs`, `bits.rs`/`wide.rs`/`width_rules.rs`, `config.rs`, `project.rs`, `runner.rs`, `stdlib.rs` |
| 3   | [Lexer](03-lexer.md)                           | `lexer/mod.rs`, `lexer/token.rs`, `lexer/keywords.rs`                                                                       |
| 4   | [Parser](04-parser.md)                         | `parser/mod.rs`, `parser/expr.rs`, `parser/items/*`                                                                         |
| 5   | [AST](05-ast.md)                               | `ast/mod.rs`, `ast/expr.rs`                                                                                                 |
| 6   | [Checker](06-checker.md)                       | `checker/` — 9 safety pass calls over 8 files, plus `names/`, `widths/`, `tests/`                                           |
| 7   | [Verilog Emitter](07-verilog-emitter.md)       | `emit_verilog/` — code generation (6 files + `module/`, `tests/`)                                                           |
| 8   | [Simulator](08-simulator.md)                   | `sim/` — event-driven simulation (11 files + `elaborate/`, `value/`, `harness/`, incl. the `EmulationHost` seam)            |
| 9   | [Tooling & Entry](09-tooling-and-entry.md)     | `commands/`, `main.rs`, `lib.rs`, LSP, WASM, VS Code                                                                        |
| 10  | [Ecosystem](10-ecosystem.md)                   | Benchmarks, fuzzing, tests, CI, examples, demo, lang, spec, site                                                            |
| 11  | [Hardware Emulation](11-hardware-emulation.md) | `src/emulate/` — LED/speaker/UART peripherals, the live dashboard, `--emulate`/`--step`                                     |

## Words you will meet in every chapter

The codebase reuses a small vocabulary. Knowing these eleven makes the rest
read as plain description rather than jargon.

| Word              | What it means here                                                                                                                                                       |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **HDL**           | Hardware Description Language. You describe a CIRCUIT, not a program. Nothing "runs top to bottom" — everything exists at once.                                          |
| **token**         | One word or symbol the lexer produced from your text: `module`, `+%`, `clk`, `42`.                                                                                       |
| **AST**           | Abstract Syntax Tree — your program as a tree of nodes instead of text. Every stage after the parser works on this, never on characters.                                 |
| **span**          | A byte range pointing back into your source file. Every token and node carries one, which is how errors can underline the right code.                                    |
| **diagnostic**    | An error or warning, held as a VALUE and printed at the end. No stage prints or crashes mid-way, so you get all errors in one run.                                       |
| **pass**          | One numbered step of the checker. Nine of them run in a fixed order (chapter 6).                                                                                         |
| **lowering**      | Rewriting a convenience feature into simpler pieces the back ends already handle — `foreach` becomes `repeat`, `sync loop` becomes registers plus a small state machine. |
| **elaboration**   | Turning a parametric design into a concrete one: widths become numbers, child modules get inlined, loops get unrolled. Simulator-side.                                   |
| **emission**      | Writing out Verilog text. Compiler-side counterpart of elaboration.                                                                                                      |
| **flavor**        | Which keyword spelling a file uses: English, Tanglish (Tamil words in Latin letters), or Tamil script. All three produce the same tokens.                                |
| **thamizh order** | Tamil's natural subject-object-verb clause order, turned on per file with `syntax thamizh`. Same keywords, different arrangement.                                        |
