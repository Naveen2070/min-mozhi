---
title: A short history
description: How hardware description languages got here — VHDL and Verilog in the 1980s, SystemVerilog, the modern wave of Chisel, Spade, Veryl and Amaranth, and where Min-Mozhi fits.
order: 2
---

# A short history

Why the tools look the way they do. Dates and standard numbers below are the
researched part of this section — worth double-checking against a primary source
if you are going to rely on them.

## Before HDLs

Digital design was schematics. You drew gates and wires, by hand and later in a
CAD tool. This works until the design has thousands of gates, at which point
nobody can see the whole thing, and a change means redrawing.

The pressure was the same one that produced programming languages: designs
outgrew the notation.

## VHDL (early 1980s)

**VHDL** came out of a US Department of Defense programme called VHSIC — Very
High Speed Integrated Circuit. The goal was partly documentation: the DoD wanted
a vendor-neutral way to *describe* the chips it was buying, so a design would
still be readable when the original vendor was gone.

It was standardised by the IEEE in 1987 and revised repeatedly since. VHDL is
verbose, strongly typed, and Ada-flavoured — unsurprising given who commissioned
it. That verbosity is a real cost and a real benefit: it is hard to write VHDL
that accidentally means something else.

## Verilog (1984)

**Verilog** arrived from the commercial side, created at Gateway Design
Automation. Gateway was acquired by Cadence, and Verilog was later opened and
standardised as IEEE 1364.

Where VHDL is Ada-like, Verilog is C-like: terser, looser, faster to write. It
became dominant in much of the industry, and if you have seen any HDL code, it
was probably Verilog.

Both languages were **simulation languages first**. The subset you can actually
build hardware from — the "synthesizable subset" — is a convention layered on
top, not a language boundary the compiler enforces. This is the origin of the
traps in [chapter 1](/learn/01-what-is-an-hdl): the language cannot tell you that
what you wrote is unbuildable, because describing unbuildable things was a
legitimate use case.

## SystemVerilog (2005 onward)

**SystemVerilog** extends Verilog with a large verification apparatus — classes,
constrained random stimulus, assertions, coverage — plus design-side conveniences
like `always_ff` and `always_comb`, which finally let you *say* which kind of
circuit you meant instead of leaving it to inference.

It is standardised as IEEE 1800 and is the industry mainstream today. It is also
enormous. The verification half is essentially a separate object-oriented
language sharing a file extension with the design half.

## The modern wave

Since roughly 2010, a set of projects has taken the position that the old
languages' permissiveness is a bug, and that a modern type system should rule out
broken hardware at compile time.

| Project      | Idea                                                              |
| ------------ | ----------------------------------------------------------------- |
| **Chisel**   | An embedded DSL in Scala — generate hardware with a real language |
| **Amaranth** | The same idea in Python, with a focus on approachability          |
| **Spade**    | A standalone language with strong types and pattern matching      |
| **Veryl**    | A modernised SystemVerilog that transpiles back to it             |

Two broad strategies show up here. The **embedded DSL** approach (Chisel,
Amaranth) gives you a full host language for generating hardware, at the cost of
error messages that talk about the host language. The **standalone language**
approach (Spade, Veryl) gives you a compiler that understands hardware
natively — and has to build its own ecosystem.

Almost all of them emit Verilog in the end, because that is what the existing
synthesis tools consume. Verilog has become the assembly language of hardware.

## Where Min-Mozhi fits

Min-Mozhi is in the standalone-language group, with two distinguishing
commitments:

**Safety is not advice.** The classic footguns are compile errors with codes and
explanations: an inferred latch, a silent truncation, two drivers on a wire, an
uninitialized register, a combinational loop. You can read the whole set with
`mimz explain --list`.

**Keywords are trilingual.** The same grammar accepts English, Tanglish
(romanized Tamil), or Tamil-script keywords, and the emitted Verilog is identical
whichever you use. That is unusual, and it is the point: a language for learning
digital design should not also demand a second language first.

It emits Verilog-2005, so it plugs into the toolchains everything else uses.

## Where to go next

Next: [Verilog in a nutshell](/learn/03-verilog-in-a-nutshell) — enough of the
incumbent to read it and to see what Min-Mozhi changes.
