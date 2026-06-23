# `UartTx` — UART transmitter (8-N-1)

Serializes a byte onto a single wire at a fixed baud rate: one low **start**
bit, eight **data** bits least-significant-first, one high **stop** bit. The
line idles high. Assert `start` for one clock with `data` valid to begin; `busy`
stays high until the whole frame has shifted out.

It is a four-state Moore FSM (Idle → Start → Data → Stop). `tx` and `busy` are
pure functions of the state, so the line is glitch-free.

## Source

- English: [`examples/english/std/uart_tx.mimz`](../../../examples/english/std/uart_tx.mimz)
- Also in `tanglish/`, `tamil/`, and `mixed/` flavors (byte-identical Verilog).
- Pure-Tamil twin (Tamil keywords **and** identifiers, natural SOV order):
  [`examples/tamil-pure/anuppi.mimz`](../../../examples/tamil-pure/anuppi.mimz)
  (`அனுப்பி` — "transmitter") — proven equivalent to the English module by
  canonical renaming.

## Interface

| Port    | Dir | Type      | Meaning                                   |
| ------- | --- | --------- | ----------------------------------------- |
| `clk`   | in  | clock     | the baud clock                            |
| `rst`   | in  | reset     | sync reset — returns to Idle              |
| `start` | in  | `bit`     | pulse high (one clock) to begin a frame   |
| `data`  | in  | `bits[8]` | the byte to send (sampled at frame start) |
| `tx`    | out | `bit`     | the serial line (idles high)              |
| `busy`  | out | `bit`     | high while a frame is in flight           |

| Param          | Default | Meaning                                      |
| -------------- | ------- | -------------------------------------------- |
| `CLKS_PER_BIT` | `4`     | clock cycles per serial bit = `f_clk / baud` |

## How it works

1. **Idle** — the line is high. When `start` is seen, the byte is latched into
   `shift` and the FSM moves to **Start**.
2. **Start** — drives the line low (the start bit) for `CLKS_PER_BIT` clocks,
   counted by the `clk_count` baud divider.
3. **Data** — drives `shift[0]` for `CLKS_PER_BIT` clocks per bit, shifting
   right after each so the byte goes out LSB-first; after 8 bits, → **Stop**.
4. **Stop** — drives the line high for one bit period, then back to **Idle**.

## Waveform

`mimz sim examples/english/std/uart_tx.mimz --param CLKS_PER_BIT=2 --in start=1,data=75 --cycles 22 --trace`
(`data=75` = `0x4B` = `0100_1011`; with `start` held, watch `tx`: start bit (0)
at cycles 1–2, then the eight data bits LSB-first — `1 1 0 1 0 0 1 0` — two
clocks each, then the stop bit (1) at cycles 19–20, then idle):

```text
cycle | start | data | tx | busy | state | clk_count | bit_index | shift
------+-------+------+----+------+-------+-----------+-----------+------
    0 |     1 |   75 |  1 |    0 |  Idle |         0 |         0 |     0
    1 |     1 |   75 |  0 |    1 | Start |         0 |         0 |    75
    2 |     1 |   75 |  0 |    1 | Start |         1 |         0 |    75
    3 |     1 |   75 |  1 |    1 |  Data |         0 |         0 |    75
    5 |     1 |   75 |  1 |    1 |  Data |         0 |         1 |    37
    7 |     1 |   75 |  0 |    1 |  Data |         0 |         2 |    18
    9 |     1 |   75 |  1 |    1 |  Data |         0 |         3 |     9
   11 |     1 |   75 |  0 |    1 |  Data |         0 |         4 |     4
   13 |     1 |   75 |  0 |    1 |  Data |         0 |         5 |     2
   15 |     1 |   75 |  1 |    1 |  Data |         0 |         6 |     1
   17 |     1 |   75 |  0 |    1 |  Data |         0 |         7 |     0
   19 |     1 |   75 |  1 |    1 |  Stop |         0 |         0 |     0
   21 |     1 |   75 |  1 |    0 |  Idle |         0 |         0 |     0
```

(The `state` column shows the enum by name; the raw trace prints its index
0–3. Every other clock is elided for brevity — each data bit spans two.)

<div class="live-waveform" data-module="uart_tx" data-cycles="22" data-inputs="start=1,data=75"></div>

## Tests

The module ships with inline `test` blocks (run `mimz test
examples/english/std/uart_tx.mimz`): the line idles high, asserting `start`
raises `busy` and drives the start bit, and the line returns to idle after a
full frame. A self-checking Icarus testbench (`tests/icarus/std_uart_tx_tb.v`)
reconstructs the serialized frame for `0x4B` and verifies every bit, for the
bit-for-bit differential.
