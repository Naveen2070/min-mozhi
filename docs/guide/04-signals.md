# 4 — Signals: Ports, Wires, and Registers

A module is made of **signals**. There are five kinds, and the difference between
two of them — `wire` and `reg` — is the single most important distinction in
hardware design.

## Ports: `in` and `out`

Ports are a module's connection to the outside world.

```mimz
module Half {
  in  a: bit
  in  b: bit
  out sum:   bit
  out carry: bit

  sum   = a ^ b
  carry = a & b
}
```

Inputs are readable; outputs must be **driven** exactly once. An undriven output
is an error (`E0502`), and so is one driven twice (`E0501`).

## `wire`: combinational signals

A `wire` is a named piece of combinational logic — a value that is always equal
to its driving expression, recomputed continuously. Declare it with a type and a
driver:

```mimz
wire doubled: bits[9] = x + x
```

Or declare and drive separately:

```mimz
wire t: bits[8]
t = a & b
```

Either way a `wire` has exactly one driver. Combinational logic must not form a
loop — `wire w = w + 1` is rejected as a cycle (`E0504`).

## `reg`: registers (memory)

A `reg` is a register: it **remembers** a value between clock edges. Because real
flip-flops power up in a known state, every `reg` must declare a **reset value**
right where it is defined:

```mimz
reg count: bits[8] = 0
```

Leaving the reset value off is an error (`E0301` / `E1104`). A `reg` is only ever
updated inside a clocked block, with the `<-` operator (next section, and
[chapter 8](08-sequential-logic.md)).

## `clock` and `reset`

Sequential modules declare a clock, and any module with registers needs a reset:

```mimz
clock clk
reset rst
```

`clock` is its own type — you cannot read it as data (`E0403`), and `on rise(x)`
requires `x` to actually be a clock (`E0109`). `reset` in v0.2 is synchronous and
active-high: on a rising clock edge, if reset is asserted, every register snaps
back to its declared reset value.

## The two assignment operators: `=` vs `<-`

This is the rule that prevents the classic Verilog blocking/non-blocking bug.
Min-Mozhi makes the distinction _syntactic_:

| Operator | Used for                  | Where                        |
| -------- | ------------------------- | ---------------------------- |
| `=`      | driving a `wire` or `out` | combinational (outside `on`) |
| `<-`     | updating a `reg`          | clocked (inside `on rise`)   |

```mimz
module Counter {
  clock clk
  reset rst
  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    value <- value +% 1   // <- updates a register on the clock edge
  }

  count = value           // = drives an output combinationally
}
```

Using the wrong one is caught and explained:

- `=` on a register, or `<-` on a wire → `E0505`;
- `<-` outside an `on` block → `E1105`;
- `=` inside an `on` block → `E1106`.

You physically cannot write the blocking/non-blocking mix-up that bites Verilog
beginners.

## Quick reference

| Kind     | Keyword (en) | Holds state? | Driven with | Reset value? |
| -------- | ------------ | ------------ | ----------- | ------------ |
| input    | `in`         | no           | (external)  | —            |
| output   | `out`        | no           | `=`         | —            |
| wire     | `wire`       | no           | `=`         | —            |
| register | `reg`        | **yes**      | `<-`        | **required** |
| clock    | `clock`      | —            | —           | —            |
| reset    | `reset`      | —            | —           | —            |

Next: [operators](05-operators.md).
