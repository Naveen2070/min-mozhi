# `Pwm` — pulse-width modulator

A free-running counter sweeps `0 → 2^WIDTH-1` and wraps. The output is high
while the counter is **below** `duty` and low above it, so the fraction of each
period spent high is `duty / 2^WIDTH` — a duty cycle you set at runtime. Drive
an LED with it to dim the LED; drive a motor to set its speed.

## Source

- English: [`examples/english/std/pwm.mimz`](../../../examples/english/std/pwm.mimz)
- Also in `tanglish/`, `tamil/`, and `mixed/` flavors (byte-identical Verilog).
- Pure-Tamil twin (Tamil keywords **and** identifiers, natural SOV order):
  [`examples/tamil-pure/minukki.mimz`](../../../examples/tamil-pure/minukki.mimz)
  (`மினுக்கி` — "dimmer") — proven equivalent to the English module by canonical
  renaming.

## Interface

| Port   | Dir | Type          | Meaning                                   |
| ------ | --- | ------------- | ----------------------------------------- |
| `clk`  | in  | clock         | the period clock                          |
| `rst`  | in  | reset         | sync reset — clears the counter to 0      |
| `duty` | in  | `bits[WIDTH]` | threshold; high time per period is `duty` |
| `pwm`  | out | `bit`         | the modulated output                      |

| Param   | Default | Meaning                                          |
| ------- | ------- | ------------------------------------------------ |
| `WIDTH` | `8`     | counter width; the resolution is `2^WIDTH` steps |

`duty = 0` holds the output low forever; the maximum `duty` (`2^WIDTH-1`) holds
it high for all but the last count.

## How it works

1. `counter` increments every clock and wraps with `+%` — a free-running ramp.
2. `pwm = counter < duty` is a pure comparison: high for the first `duty` counts
   of each period, low for the rest. The high fraction is exactly `duty / 2^WIDTH`.

## Waveform

`mimz sim examples/english/std/pwm.mimz --param WIDTH=4 --in duty=10 --cycles 16 --trace --verbose`
(WIDTH=4 → a 16-count period; with `duty=10` the output is high for ten counts
and low for six, a 10/16 = 62.5% duty cycle):

```text
cycle | clk | duty | rst | counter | pwm
------+-----+------+-----+---------+----
    0 |   1 |   10 |   1 |       0 |   1
    1 |   1 |   10 |   0 |       1 |   1
    2 |   1 |   10 |   0 |       2 |   1
    3 |   1 |   10 |   0 |       3 |   1
    4 |   1 |   10 |   0 |       4 |   1
    5 |   1 |   10 |   0 |       5 |   1
    6 |   1 |   10 |   0 |       6 |   1
    7 |   1 |   10 |   0 |       7 |   1
    8 |   1 |   10 |   0 |       8 |   1
    9 |   1 |   10 |   0 |       9 |   1
   10 |   1 |   10 |   0 |      10 |   0
   11 |   1 |   10 |   0 |      11 |   0
   12 |   1 |   10 |   0 |      12 |   0
   13 |   1 |   10 |   0 |      13 |   0
   14 |   1 |   10 |   0 |      14 |   0
   15 |   1 |   10 |   0 |      15 |   0
```

<div class="live-waveform" data-module="pwm" data-cycles="16" data-inputs="duty=10,rst=0"></div>

## Tests

The module ships with inline `test` blocks (run `mimz test
examples/english/std/pwm.mimz`): a zero duty cycle never drives the output high,
and a non-zero duty leads the period high out of reset. A self-checking Icarus
testbench (`tests/icarus/std_pwm_tb.v`) confirms exactly `duty` of every 16
cycles are high — the duty-cycle guarantee — for the bit-for-bit differential.
