---
title: Functions and control flow
description: if and match as expressions, exhaustiveness, functions with their tail-expression rule, enums and bundles, and building designs out of modules.
order: 8
---

# Functions and control flow

Everything so far has been one module at a time. This step is about structuring
the logic inside a module, and then structuring a design out of modules.

## You should already be able to

- Drive a register inside `on rise(clk)` and a wire outside it.
- Say why an `if` that produces a value needs an `else`.

## What to notice

**`if` and `match` are expressions.** They produce a value rather than performing
a jump — which is the only thing that makes sense when the "branches" are both
physically present as circuitry, with a multiplexer choosing between them.

**Exhaustiveness is enforced.** A `match` must cover every case (`E0601`), and
arms must agree on type and width (`E0408`). A `match` over an enum that misses a
variant is a compile error, so adding a variant later forces you to revisit every
place that handles it. That is the feature.

**A function body ends in exactly one tail expression** — no `return`, no
semicolon, just the value. `return` exists for early exit and anything after it
is `E0812`. Functions are combinational only and cannot recurse (`E0805`): each
call is circuitry that gets built, and recursion would mean infinite hardware.

**Watch the return width.** `fn add3(a, b, c: bits[8]) -> ?` is `bits[10]`, not
`bits[8]`, because addition grows twice. Declaring the narrow type is `E0804` —
the width rules from chapter 5 apply here exactly as everywhere else.

**Enums use a dot, and bundles group signals.** `State.Idle`, not `State::Idle`.
A `bundle` is a named group of signals that travels as one thing — a handshake, a
coordinate pair — and `T?` is the built-in "may not be valid this cycle" wrapper,
with `??` supplying the fallback.

**Instantiation is parameters in parentheses, connections in braces:**
`let add = Adder(WIDTH: 8) { a: x, b: y }`, then read `add.sum`.

## Read it in the Guide

<a class="btn-primary" href="/guide/07-expressions-and-control">Guide 07 — Expressions and control &rarr;</a>

Then, for building designs out of modules:

<a class="btn-primary" href="/guide/09-modules-and-reuse">Guide 09 — Modules and reuse &rarr;</a>

## Then come back for

Next: [tests, tooling and errors](/learn/09-tests-and-tooling) — the last step.
