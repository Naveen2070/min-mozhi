---
title: Keywords
description: All 44 Min-Mozhi keywords in English, Tanglish and Tamil, plus aliases, reserved words and provisional spellings — generated from the compiler's own keyword table.
order: 1
generated: keywords
---

# Keywords

Min-Mozhi has **one grammar and three keyword flavors**. The same program can be
written with English, Tanglish (romanized Tamil) or Tamil-script keywords, and
the emitted Verilog is byte-identical. Flavors are a skin over the words, not a
dialect of the language.

## How to read the table

- **English** is the canonical column and is frozen.
- **Tanglish** is romanized Tamil — ASCII, so it types on any keyboard.
- **Tamil** is Tamil script.
- A keyword may carry an **alias**: a second accepted spelling that lexes to the
  same token.

You can mix flavors within a file, but mixing Tamil-script keywords with
English/Tanglish ones raises `W0001` — a warning, not an error. To rewrite a file
into one flavor, use [`mimz fmt --to <flavor>`](/handbook/07-cli); to convert
between flavors while keeping names, use `mimz translate`.

## Word order

Two of these keywords change how the parser reads a file rather than what a
construct means:

- `syntax` introduces a grammar directive.
- `thamizh` selects the **thamizh word-order profile**, in which a clause head
  trails its operand — `x enil` rather than `if x`. This is a different word
  order for the same grammar, not different keywords.

Everything below is generated from `lang/keywords.toml` at build time, so it
cannot drift from the compiler.
