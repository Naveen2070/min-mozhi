---
title: Operators
description: Rust-style precedence, the three reductions that turn a bus into a bit, and comparisons that chain.
order: 3
chapter: /learn/06-operators
module: Ops
---

Min-Mozhi's operator table is Rust's, not C's — and two operators C has do not
exist here at all. Three exercises: precedence you can feel, reductions that
turn a bus into a bit, and a range check that reads like mathematics.

## Step 1 - Precedence you can feel

Write the "is `x` even?" detector in one line. In C, `x & 1 == 0` silently
parses as `x & (1 == 0)`; here bitwise binds tighter than comparison, so the
obvious reading is the correct one.

```mimz starter
module Ops {
  in x: bits[8]
  out even: bit

  // your code here: 1 when x is even
}
```

```mimz solution
module Ops {
  in x: bits[8]
  out even: bit

  even = x & 1 == 0
}
```

```mimz verify
test "even detector" for Ops {
  x = 4
  expect even == 1

  x = 5
  expect even == 0

  x = 0
  expect even == 1
}
```

> hint: no parentheses needed. `&` binds tighter than `==`, exactly as it
> looks.

## Step 2 - The three reductions

`&x` is "all bits set", `|x` is "any bit set", `^x` is odd parity. They are
how a bus becomes something a condition can accept. Drive all three outputs:

```mimz starter
module Ops {
  in x: bits[8]
  out all_ones: bit
  out any_set: bit
  out parity: bit

  all_ones = &x

  // any_set and parity are still missing

}
```

```mimz solution
module Ops {
  in x: bits[8]
  out all_ones: bit
  out any_set: bit
  out parity: bit

  all_ones = &x
  any_set = |x
  parity = ^x
}
```

```mimz verify
test "reductions" for Ops {
  x = 0b00000000
  expect all_ones == 0
  expect any_set == 0
  expect parity == 0

  x = 0b00000101
  expect all_ones == 0
  expect any_set == 1
  expect parity == 0

  x = 0b00000111
  expect parity == 1
}
```

> hint: one character each. The reduction sits before its operand like a
> minus sign.

## Step 3 - A range check that reads like mathematics

Comparisons can chain when they point the same way: `lo <= x <= hi`. Build
the range detector with the chained form — and note there is no `&&` to
reach for.

```mimz starter
module Ops {
  in x: bits[8]
  in lo: bits[8]
  in hi: bits[8]
  out in_range: bit

  // your code here: 1 when lo <= x <= hi
}
```

```mimz solution
module Ops {
  in x: bits[8]
  in lo: bits[8]
  in hi: bits[8]
  out in_range: bit

  in_range = if lo <= x <= hi { 1 } else { 0 }
}
```

```mimz verify
test "in range" for Ops {
  lo = 1
  hi = 10

  x = 5
  expect in_range == 1

  x = 10
  expect in_range == 1

  x = 42
  expect in_range == 0
}
```

> hint: `if` is an expression here — both branches exist as circuitry and a
> multiplexer picks. And a comparison that reverses direction mid-chain,
> like `a < b > c`, is rejected outright (`E1109`).

