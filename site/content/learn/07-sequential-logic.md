---
title: Sequential logic
description: Clocks, registers and memories — how time enters a design, why every register needs a reset, and the one-driver rule.
order: 7
---

# Sequential logic

Combinational logic has no memory: outputs follow inputs, continuously. A clock
is how a design gets memory and sequence — and it is the piece that makes real
hardware possible.

You already wrote one clocked block in the counter. This step is the rest.

## You should already be able to

- Write a module with `clock` and `reset` ports and one `on rise(clk)` block.
- Say why `reg value: bits[8] = 0` needs that `= 0`.

## What to notice

**`=` and `<-` are not two styles, they are two different things.** `=` drives
combinational logic, outside `on`. `<-` drives a register, inside `on`. Using the
wrong one is an error every time (`E1105`, `E1106`, `E0505`) rather than a
different meaning. This is deliberate: in Verilog the equivalent confusion
between blocking and non-blocking assignment is a convention nothing enforces,
and it produces bugs that survive simulation and appear in silicon.

**One `on` block owns each register.** Zero or several is `E0503`. If two pieces
of logic want to write the same register, they belong in the same block, choosing
between themselves with `if` or `match`.

**`default` is how you say "unless something else happens".** Inside an `on`
block, `default pulse <- 0` sets the fallback for the cycle, and a later
assignment overrides it. That is the clean way to write a one-cycle strobe.

**Reset is not optional.** A module with registers needs a reset port (`E0301`)
and each register needs a reset value (`E1104`). There is no uninitialized state
— which is the difference between a design that starts predictably and one that
starts as whatever the silicon felt like.

**`mem` is a register array.** Same rules, plus an init value.

## Read it in the Guide

<a class="btn-primary" href="/guide/08-sequential-logic">Guide 08 — Sequential logic &rarr;</a>

## Then come back for

Next: [functions and control flow](/learn/08-functions-and-control).

> **Practice this.** The [Lab](/lab/04-clocks-and-registers) has hands-on exercises for this chapter - graded by the compiler, in your browser.

