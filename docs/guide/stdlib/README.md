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

> These ship as **example content**, not yet an importable `std.*` package: copy
> a module into your project (or point `import` at its path). A global `std.*`
> search path is planned — see `docs/plan/phase-4-ecosystem.md`.

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

## Try one

```sh
mimz test    examples/english/std/fifo.mimz          # run its inline tests
mimz sim     examples/english/std/pwm.mimz --in duty=8 --cycles 16 --trace
mimz compile examples/english/std/uart_tx.mimz -o uart_tx.v
```
