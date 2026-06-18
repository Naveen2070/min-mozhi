# 5 — Operators

Operators combine values into new values. The defining idea in Min-Mozhi is that
**width is never lost silently**: the lossless operators grow their result to fit,
and if you want the cheaper fixed-width wrap you ask for it explicitly.

## Arithmetic — lossless vs wrapping

| Lossless | Result width        | Wrapping | Result width |
| -------- | ------------------- | -------- | ------------ |
| `a + b`  | one bit wider       | `a +% b` | same as `a`  |
| `a - b`  | one bit wider       | `a -% b` | same as `a`  |
| `a * b`  | sum of input widths | `a *% b` | same as `a`  |

```mimz
in a: bits[4]
in b: bits[4]

out sum:  bits[5]   // a + b grows 4 -> 5 bits, so the carry is never lost
out prod: bits[8]   // a * b grows 4+4 -> 8 bits, the full product
out wrap: bits[4]   // a +% b stays 4 bits and wraps on overflow

sum  = a + b
prod = a * b
wrap = a +% b
```

Assigning a lossless result into a too-narrow target is an error — that is the
language telling you a bit would be dropped:

```mimz
// out s: bits[4] = a + b    // ERROR E0401: a + b is bits[5]
```

Use `+%` to wrap on purpose, or widen the target. (A counter that should roll
over from 255 to 0 uses `+%`.)

The wrapping and lossless forms emit the same Verilog operator; the _declared
width_ is what makes one lossless and the other wrap.

## Shifts

```mimz
a << 1     // shift left (zeros in from the right)
a >> 1     // shift right
```

A shift keeps the width of its left operand.

## Bitwise operators

```mimz
a & b      // and
a | b      // or
a ^ b      // xor
~a         // not (complement)
```

These work bit-for-bit on equal-width operands.

## Reductions — collapse a bus to one bit

A reduction applies an operator across _all_ the bits of a single value and
yields one `bit`:

```mimz
&a         // 1 if every bit of a is 1
|a         // 1 if any bit of a is 1
^a         // parity (xor of all bits)
```

(The negated forms `nand`/`nor`/`xnor` are built-in functions — see
[chapter 6](06-builtins.md).)

## Comparisons

```mimz
a == b   a != b
a <  b   a <= b   a >  b   a >= b
```

Each yields a `bit`. Comparisons work on `bits` and on `signed` (and `signed`
compares with sign), but never across the two without a cast.

### Chained comparisons

A genuinely nice touch: you can write a monotonic range check the way maths does,
and it desugars to the safe `&&` form:

```mimz
lo <= value <= hi      // means (lo <= value) && (value <= hi)
0  <  x     <  100
```

The chain must point one direction. A confusing mixed chain like `a < b > c`, or
chaining `==`, is rejected (`E1109`).

## Logical operators (on `bit` only)

```mimz
a && b     a and b      // logical and
a || b     a or  b      // logical or
!a         not a        // logical not
```

`and`, `or`, `not` are keyword spellings of `&&`, `||`, `!` — fully
interchangeable. Logical operators require `bit` operands; using them on a
multi-bit bus is an error (`E0404`) — reach for a reduction or a comparison
instead.

## Concatenation, indexing, slicing

Build and take apart buses:

```mimz
{a, b}        // concatenation: a is the high half, b the low half
{N{x}}        // replication: x repeated N times (N is compile-time)
data[3]       // index: a single bit
data[7:4]     // slice: bits 7 down to 4 (both bounds inclusive)
```

A slice's bounds are inclusive, so `data[7:4]` is four bits wide. An
out-of-range index or a reversed slice (`data[4:7]`) is caught (`E0406`).

**Replication** `{N{x}}` is concatenation's shorthand: `{2{a}}` is exactly
`{a, a}`, and `{4{a}}` is `a` four times over. The count `N` is compile-time, so
the result width is `N` times the width of `x` — `{2{a}}` on a `bits[4]` value is
`bits[8]`. Nest it inside a wider concat just like any other piece, e.g.
`{2{a}, b}`.

## Precedence — the C trap is disarmed

Min-Mozhi uses Rust-style precedence, not C's. In C, `x & 1 == 0` parses as
`x & (1 == 0)` — a famous bug. Here it parses the way you expect:

```mimz
x & 1 == 0      // parses as (x & 1) == 0
```

When in doubt, parenthesize. The emitter parenthesizes everything it generates,
so the Verilog is always unambiguous.

Next: [built-in functions](06-builtins.md).
