---
title: Verilog in a nutshell
description: Enough Verilog to read it — modules, wire vs reg, assign vs always, blocking vs non-blocking, testbenches — and the traps that motivated safer languages.
order: 3
---

# Verilog in a nutshell

Verilog is the language everything else compiles down to. You do not need to be
fluent, but being able to read it makes the rest of this section — and the output
of `mimz compile` — legible.

This chapter is general HDL background, researched rather than authored from
first-hand expertise. Verify specifics against a Verilog reference before relying
on them.

## A module

```verilog
module and2(
  input  wire a,
  input  wire b,
  output wire y
);
  assign y = a & b;
endmodule
```

A module is a box with ports and contents. It is the only unit of structure —
there is no namespace, no class, no file scope. Modules instantiate other
modules, and that hierarchy is the design.

## `wire` vs `reg`

The single most misleading pair of names in the language.

| Type   | What it actually means                                              |
| ------ | ------------------------------------------------------------------- |
| `wire` | driven continuously by something else — a connection                |
| `reg`  | assigned inside a procedural block — **not** necessarily a register |

A `reg` is not a flip-flop. It is a variable you are allowed to assign inside an
`always` block. Whether it becomes a flip-flop, a latch, or plain combinational
logic depends entirely on *how* you wrote the block. The name has misled
beginners for forty years.

## `assign` vs `always`

```verilog
assign y = a & b;              // continuous: y is always this

always @(*) begin              // combinational block
  y = a & b;
end

always @(posedge clk) begin    // clocked block
  q <= d;
end
```

`assign` drives a `wire` continuously. `always @(*)` describes combinational
logic. `always @(posedge clk)` describes something that updates on a clock edge —
a register.

## Blocking vs non-blocking

```verilog
a = b;      // blocking     — takes effect immediately, in order
a <= b;     // non-blocking — all right-hand sides sampled, then all assigned
```

The rule everyone is taught: **blocking (`=`) in combinational blocks,
non-blocking (`<=`) in clocked blocks.** Mixing them up produces code that
simulates one way and synthesizes another — a bug that survives testing and
appears in hardware.

Nothing in the language enforces this. It is a convention held together by code
review.

## A testbench

```verilog
module tb;
  reg a, b;
  wire y;

  and2 dut(.a(a), .b(b), .y(y));

  initial begin
    a = 0; b = 0; #10;
    a = 1; b = 1; #10;
    $display("y = %b", y);
    $finish;
  end
endmodule
```

`initial` runs once at time zero. `#10` waits ten time units. `$display` prints.
None of this is synthesizable — it exists purely to drive a simulation. This is
the "simulation language first" heritage in plain view.

## The traps

Four that motivated the newer languages:

**Inferred latches.** An `always @(*)` block that does not assign an output on
every path creates a latch to remember the old value. It is legal, silent, and
almost never what you meant.

**Silent truncation.** Assign an 8-bit value to a 4-bit signal and the top bits
are dropped, quietly.

**Multiple drivers.** Two `assign` statements on one wire produce `X` (unknown)
rather than an error.

**Uninitialized state.** A register with no reset starts as `X` in simulation and
as whatever the silicon felt like in hardware.

Every one of these is legal Verilog that compiles cleanly.

## What Min-Mozhi does about it

The same four situations, in Min-Mozhi:

| Verilog                        | Min-Mozhi                                            |
| ------------------------------ | ---------------------------------------------------- |
| inferred latch                 | `E1108` — a value-driving `if` must have an `else`   |
| silent truncation              | `E0401` — say `+%` or `trunc()` if you meant it      |
| multiple drivers               | `E0501` — one driver per signal                      |
| uninitialized register         | `E1104` / `E0301` — regs need a reset value          |

And the `wire`/`reg` confusion is designed out: `wire` is combinational and
driven with `=`, `reg` is clocked and driven with `<-`. Using the wrong operator
is an error (`E1105`, `E1106`, `E0505`) rather than a different meaning.

You will still see Verilog — it is what `mimz compile` emits, and reading it is a
good way to check your mental model against what actually gets built.

## Where to go next

Enough background. Next:
[your first Min-Mozhi module](/learn/04-your-first-module).
