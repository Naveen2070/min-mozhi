<h1 align="center">Min-Mozhi · மின்மொழி</h1>

<p align="center">
  <img src="site/public/mascot.png" alt="Min-Mozhi Mascot" width="160" />
</p>

<p align="center">
  <b>A modern, safe-by-default hardware description language — built to teach digital design, and the first Tamil-rooted HDL.</b><br>
  <i>Reads like Go/TypeScript. Safe like Rust. Speaks English, Tanglish, and Tamil.</i>
</p>

<p align="center">
  <a href="https://github.com/Naveen2070/min-mozhi/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/Naveen2070/min-mozhi/actions/workflows/ci.yml/badge.svg"></a>
  <a href="docs/code/10-test-map.md"><img alt="Tests" src="https://img.shields.io/badge/tests-433%20passing-brightgreen.svg"></a>
  <img alt="Status" src="https://img.shields.io/badge/status-compiler%20%2B%20simulator-success.svg">
  <a href="https://rustup.rs"><img alt="Rust" src="https://img.shields.io/badge/rust-%E2%89%A5%201.85-orange.svg"></a>
  <img alt="License" src="https://img.shields.io/badge/license-MIT%20%2B%20Apache--2.0-blue.svg">
  <img alt="Made in Tamil Nadu" src="https://img.shields.io/badge/made%20in-Tamil%20Nadu%20%F0%9F%87%AE%F0%9F%87%B3-blueviolet.svg">
</p>

---

Min-Mozhi ("language of electricity") is a modern HDL for designing digital
circuits. It **compiles to synthesizable Verilog** today and ships with its **own
event-driven simulator** — with a native FPGA path on the roadmap.

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

<details>
<summary>The same module in <b>Tanglish</b> — same grammar, only the keywords change (flavors mix freely in one file)</summary>

```mimz
thoguthi Counter(WIDTH: int = 8) {
  thudippu clk
  meettamai rst
  veliyeedu count: bits[WIDTH]

  pathivedu value: bits[WIDTH] = 0

  pothu yetram(clk) {
    value <- value +% 1
  }

  count = value
}
```

</details>

## Why

- **Modern syntax** — Go/TypeScript-style braces and `: type` annotations,
  expression-oriented `if`/`match`. No `begin/end`, no preprocessor.
- **Safe by default** — no inferred latches, silent truncation, multiple
  drivers, uninitialized registers, or signed/unsigned mixing. Every one is a
  compile-time error with a stable `E`-code. (Compile-time **security** checks —
  `secret` information-flow, fail-secure faults — are a first-class design goal
  on the roadmap, post-v0.1.0.)
- **Beginner-first** — understand the basics in 1–2 hours; compile a counter
  within 5 minutes of installing, with errors that teach.
- **Trilingual by design** — English, Tanglish, and Tamil are keyword skins over
  one grammar; `mimz translate` converts losslessly between them. The first
  Tamil-rooted HDL.

Files use the **`.mimz`** extension; the CLI is **`mimz`**.

## Quick start

Prerequisite: [Rust](https://rustup.rs) stable ≥ 1.85.

```text
cargo build                                   # binary: target/debug/mimz

mimz check   examples/english/counter.mimz    # lex + parse + safety checks
mimz compile examples/english/counter.mimz -o counter.v --emit-testbench  # emit Verilog & testbench
mimz sim     demo/cpu.mimz --cycles 8 -o demo/cpu.vcd     # simulate → VCD waveform
mimz test    demo/cpu.mimz                     # run tick/expect test blocks
```

> Replace `mimz` with `cargo run --` if you haven't installed the binary.

A full showcase — an accumulator CPU you can check, test, simulate, and view as a
waveform — lives in **[`demo/`](demo/)**.

Before committing (exactly what CI runs):

```text
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
```

## Status

**Phases 1, 1.8, and 1.5 complete — a working compiler _and_ simulator**, with
**433 passing tests**.

- **Compiler** — lexer (all three flavors) → parser → checker (every spec safety
  rule, stable `E`-codes) → Verilog emitter (`repeat` unrolling, Tamil→ASCII
  transliteration, real `signed` two's-complement). Every example compiles to
  **byte-identical** Verilog from all four flavor folders and is **validated by
  Icarus Verilog**.
- **Simulator** — `mimz sim` runs clocked and combinational designs
  (`--in`/`--sweep`, `--cycles`, `--trace`, deterministic `-o file.vcd`) and
  `mimz test` runs `tick`/`expect` blocks, cross-checked against Icarus
  bit-for-bit (`our_simulator_matches_icarus_bit_for_bit`).
- **Tooling** — `mimz lsp` (live VS Code diagnostics), `mimz check --json`, and
  `mimz-bench` (speed / accuracy / safety / coverage → HTML report).

## Who it's for (and not for)

Min-Mozhi is an **educational project, honestly framed** — built to teach digital
design to **students everywhere**, and equally (`spec/01` v0.3) for developers who
want a safe-by-default, ergonomic HDL drawn by the compile-time checks rather than
the Tamil roots.

Native Tamil serves a **double purpose**:

- reaching Tamil-speaking learners who hit the English barrier, and
- growing Tamil as a language you can actually program in.

It is new and experimental, **not** a production replacement: if you need the
completeness of Verilog or Chisel, keep using them. But it always emits Verilog —
so nothing you build here is a dead end.

## Documentation

| Where                                      | What                                                |
| ------------------------------------------ | --------------------------------------------------- |
| [`docs/guide/`](docs/guide/README.md)      | **Learn the language** — from-scratch tutorial book |
| [`spec/`](spec/01-goals-and-philosophy.md) | Language spec — goals, grammar, keywords, simulator |
| [`examples/`](examples/)                   | 23 designs × 4 flavor folders + stdlib + tamil-pure |
| [`demo/`](demo/)                           | Accumulator-CPU showcase: check → test → sim → wave |
| [`docs/`](docs/README.md)                  | Docs hub — phase plans, architecture, dev log       |
| [`docs/code/`](docs/code/)                 | How the code works (maintainers & contributors)     |
| [`editors/vscode/`](editors/vscode/)       | VS Code syntax highlighting for `.mimz`             |
| [`CONTRIBUTING.md`](CONTRIBUTING.md)       | How to contribute                                   |

## Roadmap

| Phase | Status | Summary                                                         |
| ----- | ------ | --------------------------------------------------------------- |
| 1     | ✅     | Rust compiler: lexer → parser → AST → Verilog (Icarus-tested)   |
| 1.8   | ✅     | Grammar Engine — natural Tamil SOV word order (`thamizh-order`) |
| 1.5   | ✅     | Own event-driven simulator + VCD, Icarus-differentiated         |
| 2     | ⏳     | Own IR + synthesis via open toolchain (Yosys/nextpnr)           |
| 3     | ⏳     | Native iCE40 bitstream generation                               |
| 4     | ⏳     | Stdlib (UART/SPI/PWM), package manager, docs site, community    |

## License

MIT **+** Apache-2.0 dual-licensed (the Rust ecosystem norm). Free and open
source forever — that's constitutional (`spec/01` section 4). © 2026 Naveen R —
see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

---

<div align="center">

_Min-Mozhi — மின்மொழி — Speak in Circuits_

Made with ♥ by [Naveen R](https://github.com/Naveen2070)

</div>
