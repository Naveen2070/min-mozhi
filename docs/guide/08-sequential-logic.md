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

## Explicit fallback: `default`

Holding is implicit — nothing written means nothing changes. `default` is the
opposite: an EXPLICIT fallback value, for when you want a register to fall
back to something specific unless a later condition overrides it, without
writing an `else` on every branch:

```mimz
on rise(clk) {
  default done <- 0        // fallback: cleared unless a branch below sets it
  if count == LIMIT {
    done <- 1
  }
}
```

A `default` always applies FIRST, so any later conditional `<-` to the same
register in the same block overrides it — write it once at the top and forget
about it, rather than repeating `<- 0` in every unhandled branch. Rules the
compiler enforces:

- the target must be a `reg` (`E0809`) — a `wire`'s value is never held, so a
  "default" for one is meaningless;
- at most one `default` per register per `on` block (`E0810`) — two fallback
  values for the same register is a contradiction, not a priority order.

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

## Synchronous Loops (`sync loop`)

Sometimes you need a hardware block to process data iteratively over multiple clock cycles. Writing manual state machines and counters for this is tedious and error-prone. The `sync loop` construct is a lightweight high-level synthesis (HLS) feature that automatically generates a finite state machine, an internal counter, and a busy flag for you.

```mimz
module SerialScanner {
  clock clk
  reset rst

  // The sync loop automatically tracks iteration state
  sync loop scan(clk, rst) i in 0..12 -> result: bits[8] = 0 {
    // This executes once per clock cycle
    result <- result +% 1
  }
}
```

Behind the scenes, Min-Mozhi lowers the `sync loop` directly into primitive `reg` and `on` blocks. It safely manages the counter widths (using `clog2(hi)` to save logic elements) and handles all the state-transition condition checks for you, ensuring that the hardware generated is both efficient and completely safe.

## Clock domains

If a design has more than one clock, Min-Mozhi tracks which clock owns each
register and rejects reading a register from one domain inside another's logic
(`E0701`) — a real source of metastability bugs, caught at compile time.

Why it matters: a flip-flop needs its input to be stable for a short window
around the clock edge. A signal arriving from a DIFFERENT clock has no such
guarantee — it can change exactly at the edge, and the flip-flop can settle to
neither 0 nor 1 for a while. That is **metastability**, and it produces bugs
that appear once an hour on real silicon and never in simulation. So the
compiler refuses the read rather than letting you find out the hard way.

## Crossing a clock domain on purpose: `sync.*`

Sometimes you genuinely need a signal to travel between domains. Min-Mozhi
gives you two built-in synchronizers, and they are the only sanctioned way
across:

### `sync.double_flop` — carry a LEVEL across

Use this when the signal is a steady state ("the button is held", "the FIFO
is empty"). It passes the value through two registers clocked by the
destination clock, which gives any metastable state a full cycle to settle:

```mimz
module Crossing {
  clock clk_fast
  clock clk_slow
  reset rst

  in  flag_fast: bit
  out flag_slow: bit

  reg src:  bit = 0
  reg dest: bit = 0

  on rise(clk_fast) {
    src <- flag_fast
  }

  on rise(clk_slow) {
    dest <- sync.double_flop(src, clk_fast, clk_slow)
  }

  flag_slow = dest
}
```

Read the call as: "take `src`, which belongs to `clk_fast`, and make it safe
to use in `clk_slow`." The value shows up in the destination domain **two
destination-clock cycles later**.

### `sync.pulse` — carry an EVENT across

Use this when the signal is a one-cycle strobe ("a byte arrived"). A level
synchronizer would miss a fast pulse entirely, or stretch it; `sync.pulse`
converts it to a toggle, crosses that, and rebuilds a single one-cycle pulse
on the far side:

```mimz
wire got_it: bit = sync.pulse(tick_fast, clk_fast, clk_slow)
```

### The rules the compiler enforces

Both primitives lower to ordinary registers — there is no magic Verilog
construct behind them — but they only work if used exactly right, so the
checker is strict about it:

| Rule                                                                                                                                       | Code    |
| ------------------------------------------------------------------------------------------------------------------------------------------ | ------- |
| the two clocks must be two DIFFERENT declared `clock`s                                                                                     | `E0702` |
| the signal must be exactly 1 bit — multi-bit crossing is not provided yet                                                                  | `E0703` |
| the signal must really belong to the source clock's domain                                                                                 | `E0704` |
| `double_flop` must be the direct `<-` right-hand side in the destination clock's `on` block; `pulse` must be a `wire`'s direct initializer | `E0705` |

The last rule looks fussy but is the point: a synchronizer buried inside a
larger expression is not a synchronizer, it is a race condition with extra
steps.

> **Still crossing?** These two cover control signals. Multi-bit data across
> domains wants a FIFO — see [`std.fifo`](stdlib/fifo.md), where the crossing
> is handled inside a tested module instead of by hand.

## Guarding invariants: `assert`

A register that should never exceed a bound, an input that should never be
zero, a state that should never repeat — `assert` checks a hard invariant
and teaches it at the exact moment it breaks, instead of hiding in a
comment:

```mimz
module Divider {
  in a: bits[8]
  in b: bits[8]
  out q: bits[8]

  assert(b != 0, "division by zero")
  q = a
}
```

`assert(cond)` / `assert(cond, "msg")` works in two places:

- **In the module body** — checked every settled combinational state, like
  `Divider`'s above.
- **Inside `on rise(clk) { }`** — checked once per triggering edge:

```mimz
on rise(clk) {
  assert(count_r < 15, "counter must never exceed its declared width")
  count_r <- count_r +% 1
}
```

`cond` must be a single `bit` (the same rule `if`/`&&` follow); `msg` is a
plain string, not a general expression — it falls back to `cond`'s own
source text when omitted.

`assert` never reaches real hardware: the emitted Verilog wraps it in an
`` `ifndef SYNTHESIS `` guard, so a synthesis tool strips it entirely.
It **does** run in `mimz sim`/`mimz test`/the playground — a failing
assert stops the run immediately with the reason, right where it broke.

> `assume`/`cover`/SVA-style sequence assertions aren't in the language
> yet — `assert` is the first piece of a larger verification story.

Next: [modules and reuse](09-modules-and-reuse.md).
