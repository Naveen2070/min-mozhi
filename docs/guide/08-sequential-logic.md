# 8 — Sequential Logic

So far the circuits have been **combinational**: outputs are a pure function of
inputs, recomputed instantly. Real designs need **memory** — values that persist
across clock ticks. That is sequential logic, built from registers and clocks.

## The clocked block: `on rise`

A clocked block describes what happens on each rising edge of a clock. Register
updates (`<-`) live here and nowhere else:

```mimz
module Counter {
  clock clk
  reset rst
  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

Read `on rise(clk) { value <- value +% 1 }` as: "on each rising edge of `clk`,
the register `value` becomes its old value plus one (wrapping at 255)." The `+%`
is the wrapping add — a counter is exactly where you _want_ overflow to roll over.

The argument to `rise` must be a declared `clock` (`E0109`).

## Falling-edge: `on fall`

A register can also update on the **falling** edge with `on fall(clk)` (Verilog
`negedge`). Both edge blocks may appear in the same module on the same clock, and
their ordering within a cycle is observable — the rising-edge updates settle
before the falling-edge ones (matching Icarus semantics):

```mimz
on rise(clk) {
  a <- d              // capture d on the rising edge
}
on fall(clk) {
  b <- a              // capture a half a period later, on the falling edge
}
```

This is the building block for dual-edge (DDR-style) pipelines.

## Reset — synchronous (default) and asynchronous

Any module with registers must declare a `reset`. By default reset is
**synchronous and active-high**: on a rising edge, if reset is asserted, every
register returns to the value it declared at definition.

You do not write the reset logic by hand:

- the reset value on each `reg` _is_ the reset behavior;
- the emitter generates the `if (rst) … else …` for you.

That is why the reset value is mandatory: it is the known power-on state.

```mimz
reset rst              // synchronous (the default)
```

For an **asynchronous** reset — the register clears the instant `rst` is asserted,
without waiting for a clock edge — prefix `async`:

```mimz
async reset rst        // asynchronous, active-high
```

An async reset is added to the block's sensitivity list, lowering to Verilog
`always @(posedge clk or posedge rst)`; the sync default stays clock-only. Reach
for async reset when a power-on or brown-out reset must not wait for the clock.

## Registers hold their value

A register keeps its value unless something assigns it this cycle. That makes the
`else` optional on a statement-level `if` inside `on` (chapter 7):

```mimz
on rise(clk) {
  if enable {
    value <- value +% 1
  }
  // no else: when enable is 0, value simply holds
}
```

A register must be updated from exactly one `on` block; splitting it across two is
an error (`E0503`).

## Finite state machines

Put an `enum` register together with `match` and you have a clean, latch-free FSM.
This traffic light cycles Red → Green → Yellow on a timer:

```mimz
module TrafficLight {
  clock clk
  reset rst

  out red:    bit
  out yellow: bit
  out green:  bit

  enum State { Red, Green, Yellow }

  reg state: State   = State.Red
  reg timer: bits[8] = 0

  on rise(clk) {
    if timer == 0 {
      state <- match state {
        State.Red    => State.Green
        State.Green  => State.Yellow
        State.Yellow => State.Red
      }
      timer <- match state {
        State.Red    => 50
        State.Green  => 40
        State.Yellow => 10
      }
    } else {
      timer <- timer -% 1
    }
  }

  red    = state == State.Red
  yellow = state == State.Yellow
  green  = state == State.Green
}
```

Why this is safe by construction:

- `state` is a `reg` with a reset value (`State.Red`) — a known power-on state;
- the `match` over `State` is exhaustive — every state has a successor, so the
  compiler proves there is no forgotten transition;
- the outputs are plain combinational decodes of the current state.

## Clock domains

If a design has more than one clock, Min-Mozhi tracks which clock owns each
register and rejects reading a register from one domain inside another's logic
(`E0701`) — a real source of metastability bugs, caught at compile time.

Next: [modules and reuse](09-modules-and-reuse.md).
