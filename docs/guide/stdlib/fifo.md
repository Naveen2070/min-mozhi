# `Fifo` — synchronous FIFO queue

A first-in-first-out buffer built on a `mem` (a ring buffer). Bytes pushed in
come out in the same order. Writes land on the clock; the datum at the head
reads out combinationally. A push when full and a pop when empty are both
**ignored**, so the queue never over- or underflows — it is always consistent.

## Source

- English: [`examples/english/std/fifo.mimz`](../../../examples/english/std/fifo.mimz)
- Also in `tanglish/`, `tamil/`, and `mixed/` flavors (byte-identical Verilog).
- Pure-Tamil twin (Tamil keywords **and** identifiers, natural SOV order):
  [`examples/tamil-pure/varisai.mimz`](../../../examples/tamil-pure/varisai.mimz)
  (`வரிசை` — "queue") — proven equivalent to the English module by canonical
  renaming.

## Interface

| Port    | Dir | Type          | Meaning                                      |
| ------- | --- | ------------- | -------------------------------------------- |
| `clk`   | in  | clock         | the queue clock                              |
| `rst`   | in  | reset         | sync reset — empties the queue               |
| `push`  | in  | `bit`         | enqueue `din` this cycle (ignored when full) |
| `pop`   | in  | `bit`         | dequeue this cycle (ignored when empty)      |
| `din`   | in  | `bits[WIDTH]` | the datum to enqueue                         |
| `full`  | out | `bit`         | no room left                                 |
| `empty` | out | `bit`         | nothing buffered                             |
| `dout`  | out | `bits[WIDTH]` | the datum at the head (valid when not empty) |

| Param   | Default | Meaning                                      |
| ------- | ------- | -------------------------------------------- |
| `WIDTH` | `8`     | datum width in bits                          |
| `AW`    | `2`     | pointer width; the ring holds `2^AW` entries |
| `DEPTH` | `4`     | number of entries — **must equal `2^AW`**    |

> `DEPTH` and `AW` are separate parameters because the language has no `clog2`.
> Keep the contract `DEPTH = 2^AW`; the defaults (4 and 2) satisfy it.

## How it works

1. `data` is a `mem` of `DEPTH` cells. `head` and `tail` are `AW`-bit pointers
   that wrap around the ring with `+%`.
2. `count` carries one extra bit so it can represent a completely full ring
   (`DEPTH`). `full = count == DEPTH`, `empty = count == 0`.
3. On each clock: a valid push writes `din` at `tail` and advances it; a valid
   pop advances `head`. The occupancy `count` rises on a push-only, falls on a
   pop-only, and holds steady when a push and pop happen together.

## Waveform

`mimz sim examples/english/std/fifo.mimz --in push=1,din=171 --cycles 6 --trace --verbose`
(`push` held with `din=171`; the queue fills one entry per clock, `dout` shows
the head datum, and `full` asserts once `count` reaches `DEPTH=4`):

```text
cycle | clk | din | pop | push | rst | count | head | tail | dout | empty | full
------+-----+-----+-----+------+-----+-------+------+------+------+-------+-----
    0 |   1 | 171 |   0 |    1 |   1 |     0 |    0 |    0 |    0 |     1 |    0
    1 |   1 | 171 |   0 |    1 |   0 |     1 |    0 |    1 |  171 |     0 |    0
    2 |   1 | 171 |   0 |    1 |   0 |     2 |    0 |    2 |  171 |     0 |    0
    3 |   1 | 171 |   0 |    1 |   0 |     3 |    0 |    3 |  171 |     0 |    0
    4 |   1 | 171 |   0 |    1 |   0 |     4 |    0 |    0 |  171 |     0 |    1
    5 |   1 | 171 |   0 |    1 |   0 |     4 |    0 |    0 |  171 |     0 |    1
```

<!-- waveform screenshot slot: drop a GTKWave/playground PNG here as
     docs/guide/stdlib/img/fifo.png and link it below. -->

## Tests

The module ships with inline `test` blocks (run `mimz test
examples/english/std/fifo.mimz`): a freshly reset queue is empty, one pushed
byte round-trips at the head, and pushing `DEPTH` entries fills the ring. A
self-checking Icarus testbench (`tests/icarus/std_fifo_tb.v`) checks FIFO
ordering across pushes/pops and that an overflow push is ignored, for the
bit-for-bit differential.
