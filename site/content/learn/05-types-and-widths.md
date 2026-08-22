---
title: Types and widths
description: Why every signal has a width, how lossless arithmetic changes what you write, and what to watch for in the Guide's type chapter.
order: 5
---

# Types and widths

You met `bit` and `bits[WIDTH]` in the counter, and you met `+%` because
arithmetic grows. This step is about why that is, and what falls out of it.

In hardware a value is a bundle of physical wires. "How many bits" is not
bookkeeping — it is how much circuitry gets built. So widths are part of the type,
and the compiler tracks them everywhere.

## You should already be able to

- Read a module: parameters in the parentheses, ports in the body.
- Say why `count <- count +% 1` uses `+%` rather than `+`.

## What to notice

**Growth is the default, and it is the whole design.** `bits[8] + bits[8]` is
`bits[9]`, `bits[8] * bits[8]` is `bits[16]`. Nothing is ever dropped without you
saying so. Every width error you hit is this rule doing its job — the fix is
always to state which you meant: `+%` to wrap, `trunc()` to keep the low bits,
`extend()` to widen.

**`bits` and `signed` are different types, not a flag.** They never mix
implicitly. `signed(x)` and `unsigned(x)` cross between them, and both are pure
reinterpretations — no bits move, only the compiler's idea of what they mean.

**Conditions are `bit`, and nothing else.** There is no "non-zero is true". If
you want "any bit set", write the reduction `|bus` and say it. This feels
pedantic for about a day and then feels obviously right.

## Read it in the Guide

<a class="btn-primary" href="/guide/03-types-and-values">Guide 03 — Types and values &rarr;</a>

## Then come back for

Next: [operators](/learn/06-operators). If you want the width rules as a
reference rather than a lesson, the Handbook has
[types and widths](/handbook/04-types-and-widths).
