---
title: Learn
description: Start from zero — what a hardware description language is, where the field came from, and how to write your first Min-Mozhi module.
---

# Learn

A path into hardware description languages, and then into Min-Mozhi.

This section assumes **nothing**. If you have never written a line of Verilog — or
never heard of it — start at chapter 01 and keep going. If you already design
hardware and only want the language, skip straight to the
[Guide](/guide/01-getting-started).

## The path

**Background** — general HDL material, the part to verify before relying on it.

| #   | Chapter                                                         | What it covers                               |
| --- | --------------------------------------------------------------- | -------------------------------------------- |
| 01  | [What is an HDL?](/learn/01-what-is-an-hdl)                      | Why hardware is not a sequence of steps      |
| 02  | [A short history](/learn/02-history)                             | VHDL, Verilog, SystemVerilog, and what's new |
| 03  | [Verilog in a nutshell](/learn/03-verilog-in-a-nutshell)         | Enough to read it — and its four classic traps |

**Min-Mozhi** — written from the compiler, and accurate.

| #   | Chapter                                                          | What it covers                              |
| --- | ---------------------------------------------------------------- | ------------------------------------------- |
| 04  | [Your first module](/learn/04-your-first-module)                  | Install, an AND gate, a counter, 4 commands |
| 05  | [Types and widths](/learn/05-types-and-widths)                    | Why arithmetic grows                        |
| 06  | [Operators](/learn/06-operators)                                  | Rust-style precedence, reductions, chaining |
| 07  | [Sequential logic](/learn/07-sequential-logic)                    | Clocks, registers, reset, one-driver rule   |
| 08  | [Functions and control](/learn/08-functions-and-control)          | Expressions, exhaustiveness, modules        |
| 09  | [Tests, tooling and errors](/learn/09-tests-and-tooling)          | Tests, stdlib, diagnostics, flavors         |

From chapter 05 on, each step sets up a [Guide](/guide) chapter and hands you
over to it — Learn tells you what to look for, the Guide teaches it. Nothing is
taught twice.

## Practice while you read

Every Min-Mozhi chapter has a matching [Lab](/lab) lesson — short exercises in
the browser, graded by the compiler itself. Start at
[Verilog vs Min-Mozhi](/lab/00-verilog-vs-min-mozhi), or jump straight to
[Your first module](/lab/01-your-first-module).

## How this fits with the rest of the site

| Section                  | What it is                                                           |
| ------------------------ | -------------------------------------------------------------------- |
| **Learn** (you are here) | The on-ramp. Background first, then a guided route through Min-Mozhi. |
| [Guide](/guide)          | The full Min-Mozhi text, chapter by chapter.                         |
| [Handbook](/handbook)    | The reference — every keyword, operator, quirk and command.          |
| [Spec](/spec)            | The normative language specification.                                |

> **Beta.** This section is new. Corrections to the general HDL chapters are
> especially welcome.
