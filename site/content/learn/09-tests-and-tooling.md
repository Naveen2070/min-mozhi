---
title: Tests, tooling and errors
description: Writing tests in the source file, the standard library, reading diagnostics, and the trilingual keyword flavors — the last step before you are on your own.
order: 9
---

# Tests, tooling and errors

The last step. You can describe hardware; this is about working on it — checking
it behaves, reusing what exists, and reading the compiler when it disagrees.

## You should already be able to

- Build a design out of several modules.
- Use `match` over an enum and know why it must be exhaustive.

## What to notice

**Tests live in the source file.** A `test` block names a module, drives its
inputs, advances the clock and checks results:

```mimz
test "counter counts" for Counter(WIDTH: 8) {
  rst = 1
  tick(clk)
  rst = 0
  tick(clk, 3)
  expect count == 3
}
```

The name is a quoted string and `tick` is a call — `tick(clk)` or `tick(clk, N)`.
Run them with `mimz test`.

**The standard library is embedded.** `import std.fifo` and similar give you a
FIFO, a debouncer, PWM, a seven-segment driver and a UART transmitter. Reading
their source is one of the better ways to see idiomatic Min-Mozhi. If you need to
modify one, `mimz eject` writes it out for vendoring.

**Diagnostics are meant to be read.** Every code has a long-form explanation
written as teaching text — what is wrong, why it is unsafe in hardware, how to
fix it:

```console
$ mimz explain E0401
$ mimz explain --list
```

**Keywords come in three flavors.** The same grammar takes English, Tanglish or
Tamil keywords, and the emitted Verilog is byte-identical. `mimz fmt --to
<flavor>` normalizes a file; `mimz translate` converts between flavors and can
switch to the `thamizh` word order, where a clause head trails its operand
(`x enil` rather than `if x`). Same grammar, different word order.

## Read it in the Guide

<a class="btn-primary" href="/guide/11-toolchain">Guide 11 — Toolchain &rarr;</a>

<a class="btn-primary" href="/guide/stdlib/readme">Guide — Standard library &rarr;</a>

Word order has its own chapter:
[Guide 10 — Word order](/guide/10-word-order-thamizh). If you want to drive real
hardware, [Guide 13 — Hardware emulation](/guide/13-hardware-emulation).

## You are done

That is the arc. From here:

- **[The Handbook](/handbook)** — the reference. Every keyword, operator, quirk,
  diagnostic and command.
- **[The Guide](/guide)** — the full text, including chapters this path did not
  route you through.
- **[The Spec](/spec)** — the normative definition, when you need the exact rule.
- **[The Playground](/playground)** — compile and simulate in the browser.
