# `Seg7` — BCD → 7-segment decoder

A combinational lookup that turns a 4-bit BCD value into the seven segment
lines of a single display digit. The decimal digits **0–9** map to their glyph;
any other nibble (10–15) blanks the display. It is a pure `match` table with a
`_` wildcard, so it is exhaustive — the compiler proves every input is handled,
with no inferred latch.

Segment bit order is **gfedcba**, active high: `seg[0]` is segment _a_ (the top
bar) through `seg[6]` for _g_ (the middle bar).

## Source

- English: [`examples/english/std/seg7.mimz`](../../../examples/english/std/seg7.mimz)
- Also in `tanglish/`, `tamil/`, and `mixed/` flavors (byte-identical Verilog).
- Pure-Tamil twin (Tamil keywords **and** identifiers, natural SOV order):
  [`examples/tamil-pure/ennkaatti.mimz`](../../../examples/tamil-pure/ennkaatti.mimz)
  (`எண்காட்டி` — "digit display") — proven equivalent to the English module by
  canonical renaming.

## Interface

| Port    | Dir | Type      | Meaning                                 |
| ------- | --- | --------- | --------------------------------------- |
| `digit` | in  | `bits[4]` | the BCD value to show (0–9; else blank) |
| `seg`   | out | `bits[7]` | the segment lines, gfedcba, active high |

`Seg7` is purely combinational — no clock, no reset, no parameters.

## How it works

A single `match` over `digit` selects the glyph. The arms 0–9 carry the
hand-rolled segment patterns; the `_` arm blanks the display for any
non-decimal nibble, which is what makes the `match` exhaustive.

## Glyph table

`mimz sim examples/english/std/seg7.mimz --sweep "digit=0|1|2|3|4|5|6|7|8|9|10|15" --trace`:

```text
cycle | digit | seg (dec) | seg (hex) | segments lit
------+-------+-----------+-----------+--------------
    0 |     0 |        63 |      0x3F | a b c d e f
    1 |     1 |         6 |      0x06 | b c
    2 |     2 |        91 |      0x5B | a b d e g
    3 |     3 |        79 |      0x4F | a b c d g
    4 |     4 |       102 |      0x66 | b c f g
    5 |     5 |       109 |      0x6D | a c d f g
    6 |     6 |       125 |      0x7D | a c d e f g
    7 |     7 |         7 |      0x07 | a b c
    8 |     8 |       127 |      0x7F | a b c d e f g
    9 |     9 |       111 |      0x6F | a b c d f g
   10 |    10 |         0 |      0x00 | (blank)
   11 |    15 |         0 |      0x00 | (blank)
```

<!-- waveform screenshot slot: drop a GTKWave/playground PNG here as
     docs/guide/stdlib/img/seg7.png and link it below. -->

## Tests

The module ships with inline `test` blocks (run `mimz test
examples/english/std/seg7.mimz`): a representative digit decodes to its glyph,
`8` lights every segment, and a non-decimal nibble blanks the display. A
self-checking Icarus testbench (`tests/icarus/std_seg7_tb.v`) sweeps all 16
inputs against an independent glyph table for the bit-for-bit differential.
