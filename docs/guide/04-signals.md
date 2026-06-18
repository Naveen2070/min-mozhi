# 4 — Signals: Ports, Wires, and Registers

A module is made of **signals**. There are a handful of kinds, and the difference
between two of them — `wire` and `reg` — is the single most important distinction
in hardware design.

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

## `mem`: memories (register arrays)

A `mem` is an addressable array of registers — a RAM or register file. Declare it
with an element type, a depth, and a mandatory power-on init value:

```mimz
mem m: bits[8][4] = 0      // 4 cells of 8 bits, every cell seeded to 0
```

The type reads `bits[W][DEPTH]`: each cell is `bits[8]` and there are `4` of them.
Like a `reg`, a memory must declare its init value — leaving it off is an error
(`E1104`) — because every cell powers up in a known state; a memory needs no
separate reset line.

Access a `mem` by index. Writes happen on the clock (with `<-`, inside an `on`
block); reads are combinational (with `=`):

```mimz
on rise(clk) {
  if we {
    m[waddr] <- wdata      // clocked write to one cell
  }
}

rdata = m[raddr]           // combinational read
```

You index into a memory to read or write a cell — you cannot assign the whole
memory at once, and the usual assignment-kind rule applies (`<-` to write a cell
on the clock, `=` only in a combinational read context; misuse is `E0505`). A
given memory must be written from exactly one `on` block (`E0503`).

## `clock` and `reset`

Sequential modules declare a clock, and any module with registers needs a reset:

```mimz
clock clk
reset rst
```

`clock` is its own type — you cannot read it as data (`E0403`), and `on rise(x)`
requires `x` to actually be a clock (`E0109`). `reset` is **synchronous and
active-high by default**: on a rising clock edge, if reset is asserted, every
register snaps back to its declared reset value. Prefix `async` —
`async reset rst` — for an asynchronous reset that clears the registers the
instant it is asserted, without waiting for the clock (see
[chapter 8](08-sequential-logic.md)).

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
| memory   | `mem`        | **yes**      | `<-` (cell) | **required** |
| clock    | `clock`      | —            | —           | —            |
| reset    | `reset`      | —            | —           | —            |

Next: [operators](05-operators.md).
