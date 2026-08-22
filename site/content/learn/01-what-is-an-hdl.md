---
title: What is an HDL?
description: A hardware description language describes a circuit, not a sequence of steps — what that difference means, what HDLs are used for, and who uses them.
order: 1
---

# What is an HDL?

An **HDL** — hardware description language — is a language for describing a
digital circuit.

That sounds like programming, and it looks like programming, and it is the
source of nearly every early misunderstanding. So start here: **an HDL program
does not run.** It describes something that gets built.

## Software runs. Hardware exists.

A normal program is a list of steps. The processor does step one, then step two.
Time passes because the machine works through your instructions.

An HDL description is a **wiring diagram written in text**. When you write:

```mimz
y = a & b
```

you are not saying "compute a AND b and store it in y". You are saying "there is
an AND gate; its inputs are `a` and `b`; its output is called `y`". That gate is
physically there, all the time, continuously. It does not wait its turn.

The consequences are immediate:

- **Everything happens at once.** Two lines of description are two pieces of
  circuitry sitting side by side, both live. Order on the page is not order in
  time.
- **You cannot "call" a piece of hardware.** You can wire one up and use its
  output. That is not a function call; the thing is always there, always
  computing.
- **A variable is a wire.** It does not hold a history. It has whatever value its
  driver is putting on it right now.

The one place time enters is a **clock** — a signal that ticks, and circuits that
only change on a tick. That is how a circuit gets memory and sequence, and it is
a deliberate construction rather than a free background assumption.

## What HDLs are used for

- **ASICs** — chips manufactured for one purpose. An HDL description is the
  input to the process that eventually produces silicon.
- **FPGAs** — chips full of generic logic blocks that can be rewired after
  manufacture. Your description is compiled into a configuration for that fabric.
  This is where most people start, because an FPGA board costs less than lunch
  and reprogramming it takes seconds.
- **Simulation and verification** — running the described circuit in software to
  check it behaves, long before any hardware exists. In practice this is where
  most of the engineering time goes.

## Who uses them

Chip designers, obviously. But also embedded engineers building custom
interfaces, researchers prototyping accelerators, people building retro computers
for fun, and students learning digital logic. An FPGA dev board and a free
toolchain are enough to start.

## Why they feel strange at first

Most HDLs were designed decades ago, in a different era of language design. The
two dominant ones — Verilog and VHDL — began life as **simulation** languages.
Describing hardware you could actually build came later, and was retrofitted.

That history left a mark. Both languages will happily accept a description that
simulates fine and cannot be built, or that builds into something subtly
different from what you simulated. The classic traps — an accidental latch, a
signal silently truncated, two things driving one wire — are legal code that
produces broken hardware.

Newer languages, Min-Mozhi among them, take the position that those should be
compile errors. That is the through-line for the rest of this section.

## Where to go next

Next: [the history](/learn/02-history), which explains why the tools look the way
they do. If you would rather see code now, jump to
[Verilog in a nutshell](/learn/03-verilog-in-a-nutshell).
