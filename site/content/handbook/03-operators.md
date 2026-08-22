---
title: Operators
description: The full Min-Mozhi operator set and its exact precedence ladder, taken from the parser — plus the comparison-chaining rule and the wrapping arithmetic operators.
order: 3
---

# Operators

## Precedence ladder

Highest binds tightest. These numbers are the parser's own, from
`crates/mimz-core/src/parser/expr.rs`:

| Prec  | Operators                          | Group                    |
| ----- | ---------------------------------- | ------------------------ |
| 10    | `~` `!` `-` `&x` `\|x` `^x`        | unary and reduction      |
| 9     | `*` `*%`                           | multiply                 |
| 8     | `+` `-` `+%` `-%`                  | add / subtract           |
| 7     | `<<` `>>`                          | shift                    |
| 6     | `&`                                | bitwise and              |
| 5     | `^`                                | bitwise xor              |
| 4     | `\|`                               | bitwise or               |
| 3     | `==` `!=` `<` `<=` `>` `>=`        | comparison               |
| 2     | `&&` / `and`                       | logical and              |
| 1     | `\|\|` / `or`                      | logical or               |
| 0     | `??`                               | coalesce                 |

This is **Rust-style, not C-style**. The practical consequence is the one that
bites C programmers:

```mimz
x & 1 == 0        // parses as (x & 1) == 0
```

In C that would parse as `x & (1 == 0)` and silently compute the wrong thing.
Bitwise binds tighter than comparison here, so the obvious reading is the
correct one.

## Arithmetic

| Lossless | Wrapping | Meaning                          |
| -------- | -------- | -------------------------------- |
| `+`      | `+%`     | add                              |
| `-`      | `-%`     | subtract                         |
| `*`      | `*%`     | multiply                         |

The plain operators are **lossless**: the result grows wide enough to hold every
possible value, so `bits[8] + bits[8]` is `bits[9]`. That is why a free-running
counter needs `+%`, not `+` — you are explicitly asking for the wrap.

Assigning a grown result back into a narrow signal is `E0401`, not a silent
truncation. Use `trunc(x, N)` if you meant to drop the high bits.

There is **no `/` and no `%`**. Division and modulo do not exist in the
language, and using them gives a dedicated error rather than a parse failure:
`E1006` for `/`, `E1007` for `%`.

## Bitwise and reduction

```mimz
a & b     a | b     a ^ b     ~a        // bitwise, width-preserving
&a        |a        ^a                  // reduction: a whole bus -> one bit
```

A reduction collapses a bus into a single bit: `&a` is "all bits set", `|a` is
"any bit set", `^a` is odd parity. The negated forms are builtins rather than
operators — `nand(x)`, `nor(x)`, `xnor(x)`.

## Comparison and chaining

```mimz
lo <= x && x <= hi        // always fine
lo <= x <= hi             // also fine — a monotonic chain
```

Min-Mozhi allows a **monotonic** comparison chain: every link pointing the same
direction. Mixing directions or chaining equality is rejected with `E1109`,
because `a < b > c` and `a == b == c` read as mathematics but mean something
else entirely in C-family languages.

Comparisons produce a `bit`.

## Logical

```mimz
a && b        a and b
a || b        a or b
!a            not a
```

The word forms and the symbol forms are the same operator. Logical operators and
conditions require a `bit` — applying one to a multi-bit value is `E0404`,
rather than the C convention of "non-zero is true".

## Build and select

| Form         | Meaning                                     |
| ------------ | ------------------------------------------- |
| `{a, b}`     | concatenate                                 |
| `{N{x}}`     | replicate `x` `N` times                     |
| `x[i]`       | index one bit or one array element          |
| `x[hi:lo]`   | slice, high index first                     |
| `lhs ?? rhs` | coalesce a valid-bundle with a fallback     |

Index and slice bounds are checked at compile time — out of range or reversed is
`E0406`. Indexing an array literal directly is `E0419`: bind it to a name first.

## Signed and unsigned

`bits[N]` and `signed[N]` do not mix. Combining them without a cast is `E0403`.
Convert explicitly with `signed(x)` and `unsigned(x)` — both are
reinterpretations, not conversions, so the bit pattern is unchanged.

## Builtins

| Call           | Result                                                            |
| -------------- | ----------------------------------------------------------------- |
| `extend(x, N)` | widen to `N` bits — zero-extends `bits`, sign-extends `signed`    |
| `trunc(x, N)`  | keep the low `N` bits                                             |
| `signed(x)`    | reinterpret as signed                                             |
| `unsigned(x)`  | reinterpret as unsigned                                           |
| `min(a, b)`    | smaller of two same-width values                                  |
| `max(a, b)`    | larger of two same-width values                                   |
| `abs(x)`       | magnitude of a signed value, as `signed[N+1]`                     |
| `nand(x)`      | `~(&x)` — one bit                                                 |
| `nor(x)`       | `~(\|x)` — one bit                                                |
| `xnor(x)`      | `~(^x)` — one bit, even parity                                    |
| `encoding(e)`  | an enum value's on-wire bit pattern, as `bits[N]`                 |
| `clog2(n)`     | bits needed to address `n` items (compile-time)                   |

Misuse — `abs` of an unsigned value, `extend` that would narrow — is `E0407`.
Wrong arity is `E1110`. `clog2` has one placement restriction worth knowing
about: see [quirks](/handbook/06-quirks).
