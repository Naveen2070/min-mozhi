---
title: Quirks
description: The surprising corners of Min-Mozhi — layout rules, the function tail expression, clog2 in port widths, what --emulate actually does, and why the emitted Verilog contains an odd-looking delay.
order: 6
---

# Quirks

Things that are correct, deliberate, and still surprise people. Each one is
verified against the compiler, not folklore.

## One statement per line, and no semicolons

Min-Mozhi is brace-delimited, but a statement ends at its **newline**. Two
statements cannot share a line, even inside braces. You never write a semicolon.

The part that catches people is the exception: an expression **may** continue
onto the next line after an operator. These are the same expression:

```mimz
y = a +
    b
```

```mimz
y = a + b
```

So the line break is a statement terminator only where a statement could
actually end.

## A function body ends in exactly one tail expression

A `fn` body is statements followed by **exactly one tail expression** — the
guaranteed fallthrough value. It is not optional and it is not a `return`:

```mimz
fn add3(a: bits[8], b: bits[8], c: bits[8]) -> bits[10] {
  a + b + c        // <- tail expression, no `return`, no semicolon
}
```

`return` exists for early exit. Code after it is unreachable and is rejected
(`E0812`) rather than quietly ignored.

## `clog2` cannot size a port

`clog2(n)` works in a module body but **not in a port width** — that is `E0420`.

The cause is real Verilog, not a Min-Mozhi limitation. A port's width is written
into the Verilog port list, and Verilog-2005 port range expressions may only use
constants and parameters. They cannot call the `clog2` constant function, which
lives in the module body where the port list cannot reach it.

Two fixes: size a body `wire`/`reg` with `clog2(param)` instead, or pass the
computed width in as its own `int` parameter.

## An array literal cannot be indexed in place

```mimz
[a, b, c][0]        // E0419
```

This builds an array out of thin air and reads one element back in the same
breath — there is no named signal for the compiler to hold it in, so nothing
here is addressable hardware. Bind it first, then index the name.

## `--emulate` does not decide whether `sim` runs

A natural assumption, and wrong. The emulation host is constructed
**unconditionally** so that `bind` validation always runs, even headless.

What `--emulate` (or `--step`) actually gates is **live pacing and redraw** — the
real-time peripheral view. And it needs one more thing: stdout must be a real
terminal. Piped output and CI never go live, even with the flag, so a
`mimz test --emulate` in a script behaves exactly like one without it, minus the
animation.

## The emitted Verilog has an `initial #0`

If you read the generated Verilog you will find an `initial` block deferred by
`#0`. That is deliberate: without it, `iverilog` schedules a hoisted operand read
before its driver has settled and the read returns X/Z. The `#0` defers the block
past that point. Other simulators schedule differently; the delay is harmless
there.

## Standard-library imports are trilingual in both halves

```mimz
import std.fifo
import நூலகம்.வரிசை
```

The **namespace** is trilingual — `std` / `nuulagam` / `நூலகம்` — and so is the
**module name**: either the English stem (`fifo`) or its pure-Tamil twin
(`வரிசை` / `varisai`). A std import is exactly two segments; anything else is
`E1202`.

## There is no `/` and no `%`

Division and modulo do not exist. Rather than a confusing parse error, each gets
a dedicated diagnostic: `E1006` for `/`, `E1007` for `%`.

## `+` grows, so counters need `+%`

Arithmetic is lossless — `bits[8] + bits[8]` is `bits[9]`. A free-running counter
assigning back into its own `bits[8]` therefore needs the **wrapping** operator:

```mimz
count <- count +% 1
```

Plain `+` there is `E0401`, not a silent truncation. This is the single most
common first-day surprise, and it is the language working as intended.

## 16 keyword spellings are still provisional

The English keyword column is frozen. Sixteen of the Tanglish/Tamil spellings are
dev/testing placeholders pending native-speaker review and may change in a future
keyword-set version — they are marked in the
[keyword table](/handbook/01-keywords). Only the Tanglish and Tamil words are
affected; your program keeps lexing the same way and the emitted Verilog is
unchanged.

## Three emitted codes have no `explain` entry

`mimz explain` covers 100 E-codes, but three codes the compiler really emits are
missing from it and come back as "unknown": **`E0904`** (bundle destructure field
rename), **`E1112`** (`syntax` names an unknown grammar profile) and **`E1113`**
(expression nested too deeply to parse, or empty parens on a tag-only enum
variant). Each is covered by a test in the compiler's suite, so they are real
diagnostics — just undocumented ones.

## Bundle destructure cannot rename a field

```mimz
let { x, y } = p            // ok — fields bind under their own names
let { y: alias } = p        // E0904
```

Renaming in a destructure is not supported. Give the wire its own name with dot
access instead: `wire alias = p.y`.
