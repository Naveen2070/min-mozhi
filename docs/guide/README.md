# The Min-Mozhi Guide

A learn-the-language book for Min-Mozhi (மின்மொழி) — from the very first
module to sequential FSMs, module reuse, and natural Tamil word order.

If you have never written hardware before, that is fine: this guide starts at
the ABCs and assumes only that you can run a command in a terminal. If you
already know Verilog, VHDL, or Chisel, skim chapter 1, then jump to the
[operators](05-operators.md) and [sequential-logic](08-sequential-logic.md)
chapters — the safety rules are where Min-Mozhi differs most.

> Min-Mozhi describes **hardware**, not software. A module is a circuit, not a
> program that runs top to bottom. Everything you write becomes wires and gates.
> Keep that picture in mind and the rules below stop being surprising.

## Read in order

| #   | Chapter                                                       | You will learn                                                                   |
| --- | ------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| 1   | [Getting started](01-getting-started.md)                      | Install, write your first module, compile it to Verilog                          |
| 2   | [Lexical basics](02-lexical-basics.md)                        | Comments, identifiers, keywords, the three flavors, mixing                       |
| 3   | [Types and values](03-types-and-values.md)                    | `bit`, `bits[N]`, `signed[N]`, enums, number literals                            |
| 4   | [Signals: ports, wires, registers](04-signals.md)             | `in`/`out`, `wire`, `reg`, `clock`, `reset`, `=` vs `<-`                         |
| 5   | [Operators](05-operators.md)                                  | Lossless vs wrapping math, shifts, bitwise, comparisons, slicing                 |
| 6   | [Built-in functions](06-builtins.md)                          | `extend`, `trunc`, `signed`/`unsigned`, `min`/`max`/`abs`, `nand`/`nor`/`xnor`   |
| 7   | [Expressions and control flow](07-expressions-and-control.md) | `if` expressions, `match`, statement-level `if`/`else`                           |
| 8   | [Sequential logic](08-sequential-logic.md)                    | Clocks, resets, `on rise`, registers, finite-state machines                      |
| 9   | [Modules and reuse](09-modules-and-reuse.md)                  | Parameters, instances, imports, `repeat`, instance arrays, `const`               |
| 10  | [Natural word order (thamizh)](10-word-order-thamizh.md)      | Reading code in Tamil SOV order with `syntax thamizh`                            |
| 11  | [The toolchain](11-toolchain.md)                              | `mimz check`/`compile`/`eval`/`sim`/`test`/`explain`/`translate`/`fmt`, `--lang` |
| 12  | [Cheat sheet](12-cheatsheet.md)                               | Every keyword (×3 flavors), operator, builtin, and error code                    |

## Standard library

Once you have the basics, the [standard-library gallery](stdlib/README.md)
collects polished, tested building blocks written in Min-Mozhi itself — a
[debouncer](stdlib/debouncer.md), a [7-segment decoder](stdlib/seg7.md), a
[PWM](stdlib/pwm.md), a [FIFO](stdlib/fifo.md), and a
[UART transmitter](stdlib/uart_tx.md) — each in all four flavors plus a
pure-Tamil twin, with reproducible waveforms.

## The ten-second tour

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

That is a complete, synthesizable 8-bit counter. By the end of chapter 8 every
line above will be obvious. The same circuit in Tamil keywords with natural word
order is in chapter 10 — it compiles to the _byte-identical_ Verilog.

## How Min-Mozhi keeps you safe

The compiler rejects, at build time, the classic hardware footguns:

- no inferred latches (an `if` that drives a value needs an `else`);
- no silent truncation (`+` grows a bit; wrapping is the explicit `+%`);
- no multiple drivers and no combinational loops;
- no uninitialized registers (every `reg` declares a reset value);
- no `=`/`<-` confusion (combinational vs clocked is a syntax distinction);
- no signed/unsigned mixing without an explicit cast;
- no `x & 1 == 0` precedence trap (Rust-style precedence, not C's).

Each rule has a stable `E`-code and a teaching message. See the
[cheat sheet](12-cheatsheet.md) for the full list, or run `mimz explain E0502`.

---

_Source of truth: the formal spec lives in [`../../spec/`](../../spec/); the
keyword words in [`../../lang/keywords.toml`](../../lang/keywords.toml). This guide
teaches; the spec defines._

Looking deeper into the compiler? See
[`docs/code/`](../code/) (maintainer docs) or
[`docs/source-guide/`](../source-guide/) (friendly code tour).
