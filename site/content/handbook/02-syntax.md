---
title: Syntax
description: Every Min-Mozhi construct with one canonical example each — modules, ports, signals, clocked blocks, functions, enums, bundles, tests and sim blocks.
order: 2
---

# Syntax

Every construct, one canonical example each. Every example on this page was
compiled with `mimz check` before it was written down. Examples are
English-flavor; the [keyword table](/handbook/01-keywords) gives the Tanglish and
Tamil spellings.

## Module

The parentheses after a module name hold **parameters**, not ports. Ports are
declared in the body.

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

A module with no parameters needs no parentheses at all:

```mimz
module And2 {
  in a: bit
  in b: bit
  out y: bit

  y = a & b
}
```

## Ports

| Form               | Meaning                 |
| ------------------ | ----------------------- |
| `in name: T`       | input                   |
| `out name: T`      | output                  |
| `clock name`       | clock input             |
| `reset name`       | synchronous reset input |
| `async reset name` | asynchronous reset input |

Ports may not be array-typed — array types are `fn`-parameter-only (`E0416`).

## Signals

```mimz
wire w: bits[8] = a & b     // combinational, driven with `=`
reg  r: bits[8] = 0         // clocked, driven with `<-`, needs a reset value
mem  m: bits[8][16] = 0     // register array, needs an init value
```

`reg` and `mem` **must** carry a reset/init value in their declaration —
otherwise `E1104`. A module holding regs but with no `reset` port is `E0301`.

## Assignment

| Operator | Drives         | Where                            |
| -------- | -------------- | -------------------------------- |
| `=`      | `wire`, `out`  | combinational — outside `on`     |
| `<-`     | `reg`, `mem`   | clocked — inside an `on` block   |

Using the wrong one is a compile error, not a subtle bug: `<-` outside `on` is
`E1105`, `=` inside `on` is `E1106`, and the wrong kind for the target is
`E0505`.

## Clocked blocks

```mimz
on rise(clk) {
  value <- value +% 1
}

on fall(clk) {
  sampled <- data_in
}
```

A `reg` may be assigned from exactly one `on` block — zero or several is
`E0503`.

### `default`

```mimz
on rise(clk) {
  default pulse <- 0        // holds unless something below overrides it
  if fire { pulse <- 1 }
}
```

One `default` per reg per block (`E0810`), and the target must be a reg
(`E0809`).

## Conditionals

```mimz
wire y: bits[8] = if sel { a } else { b }
```

An `if` that **drives a value** must have an `else` — otherwise the hardware
would need to remember the previous value, which is a latch. That is `E1108`.

## Match

Arms are separated by newlines. There are **no commas** — one statement per
line applies here too.

```mimz
y = match op {
  0b00 => a +% b
  0b01 => a -% b
  0b10 => a & b
  0b11 => a | b
}
```

`match` must be exhaustive (`E0601`); an unreachable arm is `E0602`. Arms must
agree on type and width (`E0408`).

## Enums

Variants are reached with a **dot**, not `::`.

```mimz
enum State { Idle, Busy, Done }
enum Msg { Ping, Data(v: bits[8]) }      // payload fields are NAMED

nxt = match s {
  State.Idle => State.Busy
  State.Busy => State.Done
  State.Done => State.Idle
}
```

Read an enum value's on-wire bit pattern with `encoding(e)`. Payload field types
must be concrete bit-vectors (`E0807`).

## Bundles

A bundle declaration takes optional parameters, and its fields may be written on
separate lines or comma-separated on one:

```mimz
bundle Handshake(W: int = 8) {
  valid: bit
  data:  bits[W]
}

bundle Point { x: bits[8], y: bits[8] }
```

A bundle **literal** is a plain brace list — the type name is not repeated:

```mimz
module BundlePassthrough(W: int = 8) {
  in  req: Handshake(W: W)
  out rsp: Handshake(W: W)

  rsp = { valid: req.valid, data: req.data }
}
```

A literal must supply every field (`E0901`) and may not name a field that does
not exist (`E0902`).

Destructuring binds fields under **their own names**; renaming is not supported
(`E0904`). Use dot access for an alias:

```mimz
let { x, y } = p            // ok
wire px = p.x               // the way to rename
```

### Valid-bundles (`T?`)

`T?` is sugar for `{ valid: bit, data: T }`. The `??` operator supplies a
fallback when `valid` is low:

```mimz
module Opt {
  in m: bits[8]?
  out y: bits[8]
  y = m ?? 0
}
```

## Functions

```mimz
fn add3(a: bits[8], b: bits[8], c: bits[8]) -> bits[10] {
  a + b + c
}
```

Note the return type. Addition is lossless, so `bits[8] + bits[8] + bits[8]` is
`bits[10]` — declaring `-> bits[8]` here is `E0804`.

Functions are **combinational only** and may not recurse (`E0805`). The body's
last expression is the return value; `return` exits early, and anything after it
is `E0812`.

## Module instantiation

Parameters in parentheses, port connections in **braces**. Read the outputs back
off the instance name.

```mimz
let add = Adder(WIDTH: 8) { a: x, b: y }
total = add.sum
```

Every input must be connected exactly once — unconnected or doubly-connected is
`E0302`.

## Constants and parameters

```mimz
const WIDTH: int = 8              // file level
module Fifo(DEPTH: int = 16) { }  // module parameter — no `const` keyword
```

Parameters and consts are `int` or `bool` only (`E1111`), and their values must
be compile-time constant (`E0201`).

## Loops

```mimz
repeat i: 0..4 {            // compile-time unroll, `:` and a range
  leds[i] = accr[i]
}

foreach x in arr { }        // over an array or `mem` — `in`, not `:`
foreach i in 0..4 { }       // index-range form
```

`repeat` and `foreach` are the same compile-time unroll with different spellings
of the binder. A declaration inside `repeat` is `E0303`; `foreach` over
something that is not an array or `mem` is `E0417`.

The `sync loop` form carries an accumulator across the unroll:

```mimz
sync loop frame_cnt on rise(clk) (i: 0..16) -> result: bits[4] = 0 {
  if h_cnt == H_TOTAL - 1 { result <- i }
}
```

## Tests

A test name is a **double-quoted string** (`E1107` otherwise), and `tick` is a
call.

```mimz
test "counter counts" for Counter(WIDTH: 8) {
  rst = 1
  tick(clk)
  rst = 0
  tick(clk, 3)
  expect count == 3
}
```

Drive inputs with `=`, advance the clock with `tick(clk)` or `tick(clk, N)`, and
check with `expect`. Run them with [`mimz test`](/handbook/07-cli).

## Sim blocks

A `sim` block lives **inside a test**, not at file level:

```mimz
test "blinker emulation" for Blinker(LIMIT: 1000000) {
  sim {
    speed mhz(50)
    bind led -> led(color: red)
  }
  rst = 1
  tick(clk)
  rst = 0
  tick(clk, 3000000)
}
```

`speed` sets the emulated clock rate as a call — `mhz(50)`, `khz(...)` — and
`bind` wires a port to a peripheral. What `--emulate` actually changes is
covered in [quirks](/handbook/06-quirks).

## Extern modules

Declares the port shape of a Verilog module Min-Mozhi does not compile.
Parameters in parentheses, ports in the body, and an optional `doc:` string:

```mimz
extern module Pll(MULT: int = 2) {
  doc: "50MHz input, 100MHz output, ~10us lock time"
  clock clk_in
  out clk_out: bit
  out locked: bit
}
```

If the real Verilog module has a different name, give it after `=`:

```mimz
extern module Pll = "PLL_HARD_IP_v2" {
  clock clk_in
  out clk_out: bit
}
```

Ports must be plain scalars — `bit`, `bits[N]` or `signed[N]` (`E1302`). A name
declared twice in one file is `E1301`.

## Imports

```mimz
import "lib/adder.mimz"
include "lib/adder.mimz"        // `include` is an alias for `import`
import std.fifo                 // the embedded standard library
```

A missing file is `E1201`; a bad standard-library import is `E1202`.
