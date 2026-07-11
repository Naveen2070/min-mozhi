# 9 — Modules and Reuse

A module is the unit of reuse — you build bigger circuits by **instantiating**
smaller modules and, since v0.2.14, by writing **combinational functions** (`fn`)
for pure combinational logic that doesn't need its own module. This chapter covers parameters,
instances, imports, and the compile-time generation features that let one
description produce many sizes of hardware.

## Parameters (generics)

A module can take compile-time parameters — typically widths — with defaults:

```mimz
module Reg(WIDTH: int = 8) {
  in  d: bits[WIDTH]
  out q: bits[WIDTH]
  q = d
}
```

Parameters are `int`/`bool` compile-time values (chapter 3). They fold into
widths and disappear; they are not wires.

## Conditional elaboration: `const if`

`const if` includes or excludes whole module-body items — ports, wires, regs,
instances, anything — at elaboration time, based on a compile-time condition
(usually a parameter):

```mimz
module Adder(WIDTH: int = 8) {
  in a: bits[WIDTH]
  in b: bits[WIDTH]
  out sum: bits[WIDTH]

  const if (WIDTH > 8) {
    out overflow: bit
    overflow = a[WIDTH-1] & b[WIDTH-1]
  }
}
```

The losing branch is discarded completely — not type-checked, not
name-resolved, not emitted, so it can reference ports/wires that only exist
in that branch. `COND` may use parameters, module-level `const`s, and
arithmetic/comparison operators; if it can't be resolved at compile time,
that's `E0811`. `const if` blocks may nest, and an `else { ... }` is
optional. It's module-body only — no file-level conditional items.

## Instances: `let`

You place a child module with `let name = Child(params) { connections }`:

```mimz
module Top {
  in  x:     bits[8]
  in  y:     bits[8]
  out total: bits[9]

  let add = Adder(WIDTH: 8) { a: x, b: y }
  total = add.sum
}
```

- `Adder(WIDTH: 8)` passes parameters;
- `{ a: x, b: y }` connects the child's **inputs** to signals in this module;
- `add.sum` reads the child's **output** by name.

Rules: every child input must be connected exactly once (`E0302`), you connect
inputs not outputs (`E0107`), and you read outputs not inputs (`E0104`).

## Imports across files

Split designs across files and bring modules into scope with `import` (or its
English alias `include`). A dotted path maps to a subfolder:

```mimz
import adder              // adder.mimz next to this file -> module Adder
include lib.full_adder    // lib/full_adder.mimz -> module FullAdder
```

`include` and `import` are the exact same keyword. A path that does not resolve to
a file is `E1201`.

## Packages: disambiguating same-named modules

Two files can declare a module (or enum, or bundle) with the same name — this
is legal as long as you never reference the name ambiguously. If you do
import two files that both declare `Fifo`, qualify the reference with the
exact import path you wrote, dot-joined:

```mimz
import mine.fifo    // declares module Fifo
import std.fifo      // ALSO declares module Fifo

module Top {
  let a = mine.fifo.Fifo(DEPTH: 4) { ... }
  let b = std.fifo.Fifo(DEPTH: 4) { ... }
}
```

The qualifier is exactly the import path as written in this file's own
`import` — there's no separate "package name" declaration. A bare,
unqualified name still works as long as it's unambiguous, so this is fully
additive. `E0110` fires if a bare name resolves to two or more declarations;
`E0111` fires if the qualifier doesn't match any import in the file.
Functions (`fn`) stay project-wide unique and are never qualified.

## Compile-time loops: `repeat`

`repeat` unrolls at build time — it is hardware generation, not a runtime loop.
The range is half-open (`lo..hi`):

```mimz
repeat i: 0..4 {
  // body is generated four times, with i = 0,1,2,3
}
```

You cannot _declare_ ports, wires, regs, clocks, etc. inside a `repeat` (those are
module structure, not repeatable bodies) — doing so is `E0303`. What you generate
inside is instances and drives.

## Instance arrays + `const`: a ripple-carry adder

Putting `const`, `repeat`, an instance array, and a dotted import together gives a
width-parameterized adder where the width is a single knob:

```mimz
include lib.full_adder

module RippleAdder {
  const WIDTH: int = 4

  in  a:   bits[WIDTH]
  in  b:   bits[WIDTH]
  in  cin: bit

  out sum:  bits[WIDTH]
  out cout: bit

  repeat i: 0..WIDTH {
    let fa[i] = FullAdder() { a: a[i], b: b[i], cin: if i == 0 { cin } else { fa[i - 1].cout } }
    sum[i] = fa[i].sum
  }

  cout = fa[WIDTH - 1].cout
}
```

What is happening:

- `const WIDTH: int = 4` — change this one line and the whole adder regrows;
- `let fa[i] = …` — an **instance array**: one `FullAdder` per bit;
- `fa[i - 1].cout` — the carry chains from each stage to the next; the index
  `i - 1` folds to a literal at compile time;
- the `if i == 0 { cin } else { … }` is evaluated _during unrolling_, so bit 0
  takes the module carry-in and no dead `fa[-1]` is ever emitted.

This is the heart of Min-Mozhi's generation model: ordinary-looking code that the
compiler expands into concrete, fixed hardware.

Next: [natural Tamil word order](10-word-order-thamizh.md).
