# `Debouncer` — switch / button debouncer

A mechanical switch **bounces**: when pressed or released its raw line flickers
between 0 and 1 for a few milliseconds before settling. Feeding that raw line
straight into your logic causes phantom double-presses. `Debouncer` filters it:
it only changes its output once the (synchronized) input has held a new value
for `STABLE` consecutive clock samples.

It also runs the raw line through a **two-flip-flop synchronizer** first, so an
asynchronous switch can't push a metastable value into your design.

## Source

- English: [`examples/english/std/debouncer.mimz`](../../../examples/english/std/debouncer.mimz)
- Also in `tanglish/`, `tamil/`, and `mixed/` flavors (byte-identical Verilog).
- Pure-Tamil twin (Tamil keywords **and** identifiers, natural SOV order):
  [`examples/tamil-pure/nilaippaduthi.mimz`](../../../examples/tamil-pure/nilaippaduthi.mimz)
  — proven equivalent to the English module by canonical renaming.

## Interface

| Port     | Dir | Type  | Meaning                             |
| -------- | --- | ----- | ----------------------------------- |
| `clk`    | in  | clock | sample clock                        |
| `rst`    | in  | reset | sync reset — clears the output to 0 |
| `raw`    | in  | `bit` | the noisy switch / button line      |
| `stable` | out | `bit` | the debounced, glitch-free output   |

| Param    | Default | Meaning                                                             |
| -------- | ------- | ------------------------------------------------------------------- |
| `WIDTH`  | `3`     | bit width of the internal sample counter                            |
| `STABLE` | `4`     | steady samples required before a change is accepted (≤ 2^WIDTH − 1) |

Pick `STABLE` so that `STABLE × clock_period` comfortably exceeds your switch's
bounce time (a few ms is typical), and `WIDTH` so the counter can hold `STABLE`.

## How it works

1. `sync0`/`sync1` register `raw` twice — crossing into the clock domain safely.
2. While the synchronized value `sync1` **disagrees** with the current output,
   `cnt` counts up. The moment it agrees again, `cnt` resets — the input must be
   _continuously_ different to win.
3. When `cnt` reaches `STABLE`, the new value is accepted into `out_q` and the
   counter resets.

## Waveform

`mimz sim examples/english/std/debouncer.mimz --in raw=1 --cycles 8 --trace --verbose`
(reset is asserted on cycle 0; `stable` flips to 1 on cycle 7 — two cycles of
synchronizer delay plus `STABLE` counts):

```text
cycle | clk | raw | rst | cnt | out_q | sync0 | sync1 | stable
------+-----+-----+-----+-----+-------+-------+-------+-------
    0 |   1 |   1 |   1 |   0 |     0 |     0 |     0 |      0
    1 |   1 |   1 |   0 |   0 |     0 |     1 |     0 |      0
    2 |   1 |   1 |   0 |   0 |     0 |     1 |     1 |      0
    3 |   1 |   1 |   0 |   1 |     0 |     1 |     1 |      0
    4 |   1 |   1 |   0 |   2 |     0 |     1 |     1 |      0
    5 |   1 |   1 |   0 |   3 |     0 |     1 |     1 |      0
    6 |   1 |   1 |   0 |   4 |     0 |     1 |     1 |      0
    7 |   1 |   1 |   0 |   0 |     1 |     1 |     1 |      1
```

<!-- waveform screenshot slot: drop a GTKWave/playground PNG here as
     docs/guide/stdlib/img/debouncer.png and link it below. -->

## Tests

The module ships with inline `test` blocks (run `mimz test
examples/english/std/debouncer.mimz`): one asserts a sustained input settles to
`1`, the other that a glitch shorter than `STABLE` is rejected. A self-checking
Icarus testbench (`tests/icarus/std_debouncer_tb.v`) encodes the same semantics
for the bit-for-bit differential.
