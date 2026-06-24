# Standard library

A small, growing set of polished, tested building blocks written in Min-Mozhi
itself — real hardware you can drop into a design or read to learn idioms. Every
module is:

- **Trilingual** — shipped in all four keyword flavors (`english`, `tanglish`,
  `tamil`, `mixed`), each compiling to **byte-identical** Verilog.
- **Pure-Tamil too** — every module has a pure-Tamil twin (Tamil keywords _and_
  identifiers, natural SOV word order), proven equivalent to the English module
  by canonical renaming.
- **Tested three ways** — inline `test` blocks (`mimz test`), golden-locked
  emitted Verilog, and a hand-written self-checking Icarus testbench that runs
  the compiled Verilog under `vvp`.

> These are **importable** — `import std.fifo` works from any project, with no
> install path (the modules are embedded in the compiler). See
> [Importing](#importing) below.

## The modules

| Module                      | What it is                | Built from                         | Pure-Tamil twin                 |
| --------------------------- | ------------------------- | ---------------------------------- | ------------------------------- |
| [`Debouncer`](debouncer.md) | switch / button debouncer | 2-FF synchronizer + sample counter | `நிலைப்படுத்தி` (nilaippaduthi) |
| [`Seg7`](seg7.md)           | BCD → 7-segment decoder   | combinational `match` table        | `எண்காட்டி` (ennkaatti)         |
| [`Pwm`](pwm.md)             | pulse-width modulator     | free-running counter + compare     | `மினுக்கி` (minukki)            |
| [`Fifo`](fifo.md)           | synchronous FIFO queue    | `mem` ring + head/tail/count       | `வரிசை` (varisai)               |
| [`UartTx`](uart_tx.md)      | UART transmitter (8-N-1)  | enum FSM + baud divider + shifter  | `அனுப்பி` (anuppi)              |

Each page documents the ports, parameters, how it works, a reproducible ASCII
waveform from `mimz sim --trace`, and the tests.

## Importing

Standard-library modules are built into the compiler — `import std.fifo` works
from any project, no install path. The namespace and module are trilingual; the
written alias picks the canonical English module or its pure-Tamil twin:

| Module    | English                              | Tanglish                        | Tamil                            |
| --------- | ------------------------------------ | ------------------------------- | -------------------------------- |
| FIFO      | `import std.fifo` → `Fifo`           | `serkka nuulagam.varisai`       | `சேர்க்க நூலகம்.வரிசை` → `வரிசை` |
| Debouncer | `import std.debouncer` → `Debouncer` | `serkka nuulagam.nilaippaduthi` | `சேர்க்க நூலகம்.நிலைப்படுத்தி`   |
| PWM       | `import std.pwm` → `Pwm`             | `serkka nuulagam.minukki`       | `சேர்க்க நூலகம்.மினுக்கி`        |
| 7-seg     | `import std.seg7` → `Seg7`           | `serkka nuulagam.ennkaatti`     | `சேர்க்க நூலகம்.எண்காட்டி`       |
| UART TX   | `import std.uart_tx` → `UartTx`      | `serkka nuulagam.anuppi`        | `சேர்க்க நூலகம்.அனுப்பி`         |

### Vendoring (eject)

To customize a module, write the library into your project and point
`mimz.toml` at it:

```bash
mimz eject std --to ./std        # English canonical (--flavor tamil for twins)
```

```toml
# mimz.toml
[lib]
std = "./std"
```

After this, `import std.fifo` loads `./std/fifo.mimz` — your copy wins.

## Try one

```sh
mimz test    examples/english/std/fifo.mimz          # run its inline tests
mimz sim     examples/english/std/pwm.mimz --in duty=8 --cycles 16 --trace
mimz compile examples/english/std/uart_tx.mimz -o uart_tx.v
```
