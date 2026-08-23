---
title: Functions and control
description: match as a multiplexer, functions with a width contract, and an enum state machine.
order: 5
chapter: /learn/08-functions-and-control
module: Sel
---

`if` and `match` are expressions — both branches exist as circuitry with a
multiplexer choosing. Functions are combinational blocks with a tail
expression, and enums make impossible states unrepresentable.

## Step 1 - The missing arm

A `match` must cover every value of what it matches on. This multiplexer is
missing its last arm — the compiler refuses it. Complete the pattern:

```mimz starter fails E0601
module Sel {
  in sel: bits[2]
  in a: bits[4]
  in b: bits[4]
  in c: bits[4]
  in d: bits[4]
  out y: bits[4]

  y = match sel {
    0b00 => a
    0b01 => b
    0b10 => c
  }
}
```

```mimz solution
module Sel {
  in sel: bits[2]
  in a: bits[4]
  in b: bits[4]
  in c: bits[4]
  in d: bits[4]
  out y: bits[4]

  y = match sel {
    0b00 => a
    0b01 => b
    0b10 => c
    0b11 => d
  }
}
```

```mimz verify
test "all four ways" for Sel {
  sel = 0b00
  expect y == a

  sel = 0b01
  expect y == b

  sel = 0b10
  expect y == c

  sel = 0b11
  expect y == d
}
```

> hint: arms are `pattern => value`, no commas, no default-and-inferred-latch.
> Exhaustiveness (`E0601`) is the feature: no missing case becomes hardware.

## Step 2 - A function with a width contract

This function multiplies two 8-bit values but promises only 8 bits of result
— the checker knows the product needs 16 and refuses the lie. Fix the
return type:

```mimz starter fails E0804
fn mac(a: bits[8], b: bits[8]) -> bits[8] {
  a * b
}

module Mac {
  in a: bits[8]
  in b: bits[8]
  out result: bits[16]

  result = mac(a, b)
}
```

```mimz solution
fn mac(a: bits[8], b: bits[8]) -> bits[16] {
  a * b
}

module Mac {
  in a: bits[8]
  in b: bits[8]
  out result: bits[16]

  result = mac(a, b)
}
```

```mimz verify
test "multiply-accumulate core" for Mac {
  a = 12
  b = 12
  expect result == 144

  a = 200
  b = 100
  expect result == 20000
}
```

> hint: the body is one tail expression — no `return`, no semicolon. The
> width rules from [widths that grow](/lab/02-widths-that-grow) apply to
> function returns exactly as to wires.

## Step 3 - An enum state machine

Two states, one wire output, transitions on the clock. The enum makes every
other value unrepresentable, and the `match` over it must stay exhaustive.

```mimz starter
module Lamp {
  clock clk
  reset rst

  out go: bit

  enum Light { Red, Green }

  reg state: Light = Light.Red

  on rise(clk) {
    state <- match state {
      Light.Red   => Light.Green
      Light.Green => Light.Red
    }
  }

  // your line here: go is 1 exactly when state is Green
}
```

```mimz solution
module Lamp {
  clock clk
  reset rst

  out go: bit

  enum Light { Red, Green }

  reg state: Light = Light.Red

  on rise(clk) {
    state <- match state {
      Light.Red   => Light.Green
      Light.Green => Light.Red
    }
  }

  go = match state {
    Light.Green => 1
    Light.Red   => 0
  }
}
```

```mimz verify
test "red green red" for Lamp {
  rst = 1
  tick(clk)
  rst = 0
  expect go == 0

  tick(clk)
  expect go == 1

  tick(clk)
  expect go == 0
}
```

> hint: enums use a dot — `Light.Green`, not `::`. Arms take payloads in
> parentheses when variants carry them; bare names otherwise.

