---
title: Operators
description: Rust-style precedence instead of C's, the wrapping arithmetic family, reductions, comparison chaining, and the two operators that do not exist.
order: 6
---

# Operators

You have used `&`, `+%` and `==`. This step is the full set, and one decision
that will save you a debugging session.

## You should already be able to

- Explain why `+` and `+%` are different operators.
- Say what type an `if` condition has to be.

## What to notice

**Precedence is Rust-style, not C-style.** This is the important one:

```mimz
x & 1 == 0        // parses as (x & 1) == 0
```

In C that same line parses as `x & (1 == 0)` and silently computes nonsense.
Here, bitwise binds tighter than comparison, so the obvious reading is the
correct one. If you carry C habits, this is the trap that stops biting you.

**Reductions collapse a bus to one bit.** `&x` is "all bits set", `|x` is "any
bit set", `^x` is odd parity. They are how a multi-bit value becomes something an
`if` will accept.

**Comparisons can chain, but only monotonically.** `lo <= x <= hi` is valid and
means what it looks like. `a < b > c` is rejected (`E1109`) — because it reads
like mathematics and would mean something else entirely.

**There is no `/` and no `%`.** Division and modulo do not exist in the language.
Each gets a dedicated error rather than a confusing parse failure. For powers of
two, shift.

## Read it in the Guide

<a class="btn-primary" href="/guide/05-operators">Guide 05 — Operators &rarr;</a>

The builtins — `extend`, `trunc`, `min`, `max`, `abs`, `clog2` and friends — have
their own chapter: [Guide 06 — Builtins](/guide/06-builtins).

## Then come back for

Next: [sequential logic](/learn/07-sequential-logic). The exact precedence
ladder, with numbers, is in the Handbook's
[operators](/handbook/03-operators) chapter.

> **Practice this.** The [Lab](/lab/03-operators) has hands-on exercises for this chapter - graded by the compiler, in your browser.

