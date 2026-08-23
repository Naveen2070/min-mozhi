---
title: Widths that grow
description: Why a + b is not always 8 bits, and the four ways to say what you meant — wrap, truncate, widen, reduce.
order: 2
chapter: /learn/05-types-and-widths
module: Grow
---

A value in hardware is a bundle of wires, so "how many bits" is how much
circuitry exists. Min-Mozhi tracks that width everywhere, and arithmetic
grows by design: `bits[8] + bits[8]` is `bits[9]`. This lesson makes you say
what you meant, four different ways.

## Step 1 - Let it grow

The honest adder keeps every carry bit. Widen the output port so `a + b`
fits — no operator change, just tell the truth about the width.

```mimz starter fails E0401
module Grow {
  in a: bits[8]
  in b: bits[8]
  out sum: bits[8]

  sum = a + b
}
```

```mimz solution
module Grow {
  in a: bits[8]
  in b: bits[8]
  out sum: bits[9]

  sum = a + b
}
```

```mimz verify
test "255 plus 1 fits in nine bits" for Grow {
  a = 255
  b = 1
  expect sum == 256
}
```

> hint: only the port declaration changes. `bits[8] + bits[8]` is already
> `bits[9]` — the wire just has to admit it.

## Step 2 - Wrap on purpose

When the register really is 8 wires, drop the carry explicitly. Rewrite the
adder with the wrapping operator `+%`, which adds without growing.

```mimz starter fails E0401
module Grow {
  in a: bits[8]
  in b: bits[8]
  out lo: bits[8]

  lo = a + b
}
```

```mimz solution
module Grow {
  in a: bits[8]
  in b: bits[8]
  out lo: bits[8]

  lo = a +% b
}
```

```mimz verify
test "carry is dropped, loudly allowed" for Grow {
  a = 200
  b = 100
  expect lo == 44

  a = 255
  b = 1
  expect lo == 0
}
```

> hint: `%+`'s sibling `+%` is the one you want — read it as "plus, wrap".
> The checker's error was this rule doing its job.

## Step 3 - Truncate, deliberately

`trunc(x, w)` keeps the low `w` bits of `x` and says so at the call site.
Take an 8-bit input down to its low nibble.

```mimz starter fails E0401
module Grow {
  in x: bits[8]
  out nibble: bits[4]

  nibble = x
}
```

```mimz solution
module Grow {
  in x: bits[8]
  out nibble: bits[4]

  nibble = trunc(x, 4)
}
```

```mimz verify
test "low four bits" for Grow {
  x = 0b10110011
  expect nibble == 0b0011

  x = 255
  expect nibble == 15
}
```

> hint: `trunc` takes two arguments — the value and the width you keep.
> There is also `extend(x, w)`, which widens; same rule, opposite direction.

## Step 4 - Multiplication grows twice

Adding grows by one bit; multiplying grows to the sum of the widths:
`bits[8] * bits[8]` is `bits[16]`. Widen the result port to fit.

```mimz starter fails E0401
module Grow {
  in a: bits[8]
  in b: bits[8]
  out product: bits[8]

  product = a * b
}
```

```mimz solution
module Grow {
  in a: bits[8]
  in b: bits[8]
  out product: bits[16]

  product = a * b
}
```

```mimz verify
test "twelve twelves" for Grow {
  a = 12
  b = 12
  expect product == 144
}
```

> hint: 8 + 8 = 16 bits for the widest possible product. The width rules are
> mechanical, which is why the compiler can enforce them — see
> [Learn 05](/learn/05-types-and-widths).

