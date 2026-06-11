# Min-Mozhi (மின்மொழி)

> **The first Tamil-rooted Hardware Description Language.**
> Reads like Go/TypeScript. Safe like Rust. Speaks English, Tanglish, and Tamil.
> Built in Tamil Nadu, India. 🇮🇳

Min-Mozhi ("language of electricity") is a modern HDL for designing digital
circuits. It compiles to Verilog today, with a native FPGA path on the roadmap.

```mimz
module Counter(WIDTH: int = 8) {
  clock clk
  reset rst
  out count: bits[WIDTH]

  reg value: bits[WIDTH] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

The same module in **Tanglish** — same grammar, only keywords change, and
flavors can be mixed freely in one file:

```mimz
thoguthi Counter(WIDTH: int = 8) {
  kadigaram clk
  meetamai rst
  veli count: bits[WIDTH]

  nilai value: bits[WIDTH] = 0

  pothu yetram(clk) {
    value <- value +% 1
  }

  count = value
}
```

## Why

- **Beginner-first, measurably** — understand the basics in 1–2 hours; compile
  a counter within 5 minutes of installing.
- **Safe by construction** — no inferred latches, no silent truncation, no
  multiple drivers, no uninitialized registers, no blocking/non-blocking
  confusion, no signed/unsigned mixing, no `x & 1 == 0` precedence traps.
- **Trilingual by design** — English, Tanglish, and Tamil are keyword skins
  over one grammar; `mimz translate` converts losslessly between them.

## Who it's for (and not for)

For students and the curious — an **educational project** first. If you are a
professional Verilog/Chisel user who needs production completeness, keep using
Verilog/Chisel: Min-Mozhi is new, experimental, and not a replacement. It will,
however, always emit Verilog, so nothing you build here is a dead end.

Files use the **`.mimz`** extension; the CLI is **`mimz`**.

## Project Status

**Phase 1 — compiler under construction (spec v0.2.1).** The front end works:
`mimz compile` turns `.mimz` files into synthesizable Verilog today — lexer
(all three keyword flavors), full parser, and a first Verilog emitter, with
33 passing tests. Every example exists in all four flavor folders
(`english/`, `tanglish/`, `tamil/`, `mixed/`) and compiles to
**byte-identical** Verilog from each (CI-asserted). Still to come in
Phase 1: the safety-checker passes, `repeat` unrolling, and Icarus Verilog
differential tests. The repo stays private until Phase 1 is done.

## Build, Test, Run

Prerequisite: [Rust](https://rustup.rs) stable ≥ 1.85.

```text
cargo build                # build the compiler  (binary: target/debug/mimz)
cargo test                 # run all unit + integration tests
cargo run -- --help        # CLI help
cargo doc --document-private-items --open   # browsable API reference

# check a file (lex + parse, teaching diagnostics):
cargo run -- check examples/english/counter.mimz

# compile to Verilog (resolves imports, writes counter.v):
cargo run -- compile examples/english/counter.mimz -o counter.v

# see the token stream (debugging):
cargo run -- check examples/english/counter.mimz --tokens
```

Before committing: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`
(this is exactly what CI runs).

Docs are checked too (needs Node.js):

```text
npx prettier --write "**/*.md"   # format markdown
npx markdownlint-cli2            # lint markdown (config: .markdownlint-cli2.jsonc)
```

| Document                                                             | Contents                                                        |
| -------------------------------------------------------------------- | --------------------------------------------------------------- |
| [`spec/01-goals-and-philosophy.md`](spec/01-goals-and-philosophy.md) | Goals, safety guarantees, non-goals, design principles          |
| [`spec/02-syntax-and-grammar.md`](spec/02-syntax-and-grammar.md)     | Syntax tour, operators, types, formal EBNF grammar              |
| [`spec/03-keywords-trilingual.md`](spec/03-keywords-trilingual.md)   | The trilingual keyword mechanism + draft word tables            |
| [`spec/04-grammar-engine.md`](spec/04-grammar-engine.md)             | Grammar Engine — natural Tamil word order (Phase 1.8)           |
| [`examples/`](examples/)                                             | 11 examples × 4 flavor folders: english, tanglish, tamil, mixed |
| [`docs/`](docs/README.md)                                            | Docs hub: per-phase plans, dev log, repo rules, architecture    |
| [`docs/plan/`](docs/plan/)                                           | Detailed per-phase plans (source of truth for execution)        |
| [`docs/architecture.md`](docs/architecture.md)                       | Compiler architecture — pipeline, components, layout            |
| [`docs/code/`](docs/code/)                                           | How the code works — maintainer & contributor docs              |
| [`CONTRIBUTING.md`](CONTRIBUTING.md)                                 | How to contribute — quick start (details in `docs/code/`)       |
| [`docs/RULES.md`](docs/RULES.md)                                     | Repo working rules (plans, logs, spec versioning)               |
| [`min-mozhi-roadmap.md`](min-mozhi-roadmap.md)                       | Roadmap summary (details live in `docs/plan/`)                  |

## Roadmap (short version, solo-dev order)

1. **Phase 1** — Rust compiler: lexer → parser → AST → Verilog emitter, tested with Icarus Verilog (+ VS Code syntax highlighting, CI from first commit)
2. **Phase 1.8** — Grammar Engine: `thamizh-order` syntax profile so Tamil/Tanglish code reads in natural SOV word order
3. **Phase 1.5** — own event-driven simulator with VCD waveform output
4. **Phase 2** — own IR + synthesis via open toolchain (Yosys/nextpnr)
5. **Phase 3** — native iCE40 bitstream generation
6. **Phase 4** — stdlib (UART, SPI, PWM), package manager, docs site, community

## License

MIT + Apache-2.0 dual-licensed (the Rust ecosystem norm). Free and open
source forever — that's constitutional (`spec/01` section 4).

---

_Min-Mozhi — மின்மொழி — Speak in Circuits_
