---
title: Types and widths
description: Min-Mozhi's type set, the lossless width-growth rules, signed/unsigned separation, arrays, bundles and compile-time types.
order: 4
---

# Types and widths

## The type set

| Type        | Meaning                                        |
| ----------- | ---------------------------------------------- |
| `bit`       | one bit — the only type conditions accept      |
| `bits[N]`   | `N`-bit unsigned                               |
| `signed[N]` | `N`-bit two's complement                       |
| `T[N]`      | fixed-size array of `N` elements of `T`        |
| `T?`        | valid-bundle sugar: `{ valid: bit, data: T }`  |
| `Bundle`    | a named bundle declared with `bundle`          |
| `int`       | compile-time integer — parameters and `const`  |
| `bool`      | compile-time boolean — parameters and `const`  |

`int` and `bool` exist **only at compile time**. They size and configure
hardware; they never become wires. A parameter or `const` of any other type is
`E1111`.

## Widths grow, they never silently shrink

This is the rule the rest of the chapter follows from. Arithmetic is lossless:
the result is wide enough for every value it could take.

| Expression              | Result width |
| ----------------------- | ------------ |
| `bits[8] + bits[8]`     | `bits[9]`    |
| `bits[8] * bits[8]`     | `bits[16]`   |
| `bits[8] & bits[8]`     | `bits[8]`    |
| `&bits[8]` (reduction)  | `bit`        |
| `abs(signed[8])`        | `signed[9]`  |

So a sum that could carry gets its carry bit, automatically. What you cannot do
is put that `bits[9]` back into a `bits[8]` and pretend nothing happened — that
is `E0401`.

Two ways to say what you actually meant:

```mimz
count <- count +% 1          // wrapping: I want modular arithmetic
sum   =  trunc(a + b, 8)     // truncating: I want the low 8 bits
```

Both are explicit. Neither can happen by accident.

## Operand widths must match

Binary operators need both sides at the same width — mismatched is `E0402`.
Widen the narrow side with `extend`:

```mimz
wide = a + extend(b, 16)
```

`extend` zero-extends a `bits` value and sign-extends a `signed` one, which is
almost always what you want and is the reason it is one builtin rather than two.
`extend` may only widen; asking it to narrow is `E0407`.

## Signed and unsigned are separate types

`bits[N]` and `signed[N]` never mix implicitly — `E0403`. Cross over explicitly:

```mimz
wire s: signed[8] = signed(u)
wire u2: bits[8]  = unsigned(s)
```

Both are pure reinterpretations. The bits do not move; only the compiler's idea
of what they mean changes.

## Conditions are `bit`, not "non-zero"

`if`, `&&`, `||`, `!` and their word forms all require a `bit`. Handing them a
`bits[8]` is `E0404`. If you meant "any bit set", say so with a reduction:

```mimz
if |bus { ... }
```

## Literals

A literal must fit its target — `E0405` if it does not. Tamil digits inside a
literal are `E1003`, and a malformed number is `E1004`.

## Arrays

```mimz
mem m: bits[8][16] = 0
wire e: bits[8] = m[3]
```

- The element type must be valid (`E0411`) and the length sane (`E0412`).
- Index out of range is `E0415`; array literal elements that disagree are
  `E0414`; a length mismatch when passing an array is `E0413`.
- Module-level array _signals_ are not supported (`E0416`) — arrays live in
  `mem`, in functions, and as arguments.
- An array literal cannot be indexed in place (`E0419`) — bind it first.

## Bundles

```mimz
bundle Point { x: bits[8], y: bits[8] }
```

Bundles are checked structurally as well as by name: passing a bundle that is
missing a field the destination needs is `E0910`, and a field whose type does
not line up is `E0907`. A duplicate declaration is `E0909`, and an unknown
bundle name or wrong parameter count is `E0906`.

## Valid-bundles

`T?` expands to `{ valid: bit, data: T }` — the standard "this value may not be
here this cycle" shape, with a type behind it.

```mimz
wire got: bits[8] = maybe ?? 0
```

`??` requires a `T?` on the left (`E0911`, or `E1115` at parse time) and a
right-hand side matching the left's `data` type (`E0912`).

## `clog2` and parameter-sized ports

`clog2(n)` gives the number of bits needed to address `n` items, at compile
time. It works in the module body but **not in a port width** — that restriction
is `E0420` and has a real Verilog cause, covered in
[quirks](/handbook/06-quirks).
