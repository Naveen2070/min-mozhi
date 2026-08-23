---
title: Clocks and registers
description: How time enters a design — the <- operator, the reset that is not optional, and the one-driver rule.
order: 4
chapter: /learn/07-sequential-logic
module: Counter
---

A clock is how a design gets memory. Registers advance on `rise(clk)` and are
driven with `<-`, never `=` — two different things, not two styles, and the
checker enforces the difference every time.

## Step 1 - The increment

The counter below has its ports, its register and its `on rise(clk)` block —
but the register never moves. Add the missing line inside the `else`: drive
`value` with itself plus one, using `<-` and `+%`.

```mimz starter
module Counter {
  clock clk
  reset rst

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    // your line here
  }

  count = value
}
```

```mimz solution
module Counter {
  clock clk
  reset rst

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

```mimz verify
test "counts up" for Counter {
  rst = 1
  tick(clk)
  rst = 0

  tick(clk)
  expect count == 1
  tick(clk)
  expect count == 2
}
```

> hint: `<-` is the register arrow; `+%` adds without growing the width.
> Plain `=` outside `on` drives wires — inside, it is an error (`E1105`).

## Step 2 - Reset is not optional

This module counts, but it has no reset port at all — the compiler refuses
it, because a design with registers must say how it starts. Add the missing
declaration. Notice what you do NOT write: no branch, no condition. Declaring
the port is the whole contract — the toolchain clears every register while
`rst` is high, and tests drive it like any input.

```mimz starter fails E0301
module Counter {
  clock clk

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

```mimz solution
module Counter {
  clock clk
  reset rst

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

```mimz verify
test "reset clears mid-flight" for Counter {
  rst = 1
  tick(clk)
  rst = 0

  tick(clk)
  tick(clk)
  expect count == 2

  rst = 1
  tick(clk)
  expect count == 0
}
```

> hint: two things are missing — the port declaration (`reset rst`) next to
> `clock clk`, and the clearing branch inside the block. There is no
> uninitialized state in Min-Mozhi; this error is why.

## Step 3 - A light that toggles

Wrap-and-toggle: when `cnt` reaches `LIMIT`, clear it and flip `led`. This is
the LED blinker from [Learn 04](/learn/04-your-first-module), and the pattern
behind every divider chain.

```mimz starter
module Blinker(LIMIT: int = 3) {
  clock clk
  reset rst

  out led: bit

  reg cnt:   bits[8] = 0
  reg state: bit      = 0

  on rise(clk) {
    // wrap cnt at LIMIT, flip state on the wrap
  }

  led = state
}
```

```mimz solution
module Blinker(LIMIT: int = 3) {
  clock clk
  reset rst

  out led: bit

  reg cnt:   bits[8] = 0
  reg state: bit      = 0

  on rise(clk) {
    if cnt == LIMIT - 1 {
      cnt <- 0
      state <- state ^ 1
    } else {
      cnt <- cnt +% 1
    }
  }

  led = state
}
```

```mimz verify
test "toggles once per period" for Blinker(LIMIT: 3) {
  rst = 1
  tick(clk)
  rst = 0

  tick(clk)
  expect led == 0
  tick(clk)
  expect led == 0
  tick(clk)
  expect led == 1

  tick(clk, 3)
  expect led == 0
}
```

> hint: two registers, one block. `^` is XOR — flipping a bit is
> `state <- state ^ 1`.

## Step 4 - One driver per register

Two pieces of logic want to write `v`. Verilog would let both connect and
produce `X`; here the compiler refuses the file outright (`E0503`). Look at
what the second block actually adds — then leave exactly one driver:

```mimz starter fails E0503
module Flipper {
  clock clk
  reset rst

  out q: bit

  reg v: bit = 0

  on rise(clk) {
    v <- v ^ 1
  }

  on rise(clk) {
    if v == 0 {
      v <- 1
    }
  }

  q = v
}
```

```mimz solution
module Flipper {
  clock clk
  reset rst

  out q: bit

  reg v: bit = 0

  on rise(clk) {
    v <- v ^ 1
  }

  q = v
}
```

```mimz verify
test "flips, respects reset" for Flipper {
  rst = 0

  tick(clk)
  expect q == 1
  tick(clk)
  expect q == 0

  rst = 1
  tick(clk)
  expect q == 0
}
```

> hint: one `on rise(clk)` block owns each register (`E0503`) — if logic
> wants the same register under different conditions, it belongs in one
> block, separated by `if`.

