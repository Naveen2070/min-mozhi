# 6 — Built-in Functions

Built-ins (and user-defined combinational functions) are the
call syntax in the language. Their names are **universal**: spelled the same
in every flavor. There are twelve built-ins.

## Width casts and conversions

| Call           | Does                                                             |
| -------------- | ---------------------------------------------------------------- |
| `extend(x, N)` | widen `x` to `N` bits (zero-extend `bits`, sign-extend `signed`) |
| `trunc(x, N)`  | keep the low `N` bits of `x`                                     |
| `signed(x)`    | reinterpret the bits of `x` as `signed` (pattern unchanged)      |
| `unsigned(x)`  | reinterpret the bits of `x` as unsigned                          |

`extend` makes a resize **visible** — widths never change implicitly, so when a
1-bit value has to join an 8-bit bus you say so:

```mimz
in  din: bit
reg sr:  bits[8] = 0

on rise(clk) {
  sr <- trunc(sr << 1, 8) | extend(din, 8)
}
```

`extend` only widens; trying to "extend" to a narrower width is an error
(`E0407`) — use `trunc` to narrow. `signed`/`unsigned` are how you cross the
signed/unsigned boundary on purpose:

```mimz
in a: bits[4]
in b: signed[4]
out y: signed[6]
y = signed(extend(a, 5)) + extend(b, 5)
```

## Arithmetic built-ins

| Call        | Does                                 | Result        |
| ----------- | ------------------------------------ | ------------- |
| `min(a, b)` | the smaller of two same-width values | same width    |
| `max(a, b)` | the larger of two same-width values  | same width    |
| `abs(x)`    | absolute value of a `signed` value   | `signed[N+1]` |

`abs` grows by one bit so that the magnitude of the most-negative value fits:
`abs` of `signed[4]`'s −8 is +8, which needs `signed[5]`. The compiler picks the
wider type for you:

```mimz
in  s:   signed[4]
out mag: signed[5]
mag = abs(s)
```

## Negated reductions

These are the negations of the `&`/`|`/`^` reduction operators, each returning a
single `bit`:

| Call      | Equivalent to | Meaning                      |
| --------- | ------------- | ---------------------------- |
| `nand(x)` | `~(&x)`       | not (all bits set)           |
| `nor(x)`  | `~(\|x)`      | not (any bit set)            |
| `xnor(x)` | `~(^x)`       | even parity (not odd parity) |

```mimz
in  bus:  bits[4]
out allz: bit
allz = nor(bus)      // 1 when bus is all zeros
```

A negated reduction on a `signed` value is rejected (`E0403`) — reductions are a
`bits` operation.

## Compile-time width builtin: `clog2`

`clog2(n)` folds to the ceiling of log2(n) — the number of bits needed to
address `n` items. Unlike the others above, `clog2` only makes sense in a
**compile-time** position: a width (`bits[clog2(DEPTH)]`), a `const`, or a
`repeat` bound.

```mimz
const DEPTH: int = 16
reg ptr: bits[clog2(DEPTH)] = 0   // clog2(16) = 4
```

`clog2(1)` = `clog2(2)` = 1 (Min-Mozhi has no zero-width signal, so it
floors at 1, one bit more than Verilog's `$clog2(1) = 0`), `clog2(3)` =
`clog2(4)` = 2, `clog2(8)` = 3, `clog2(9)` = 4. The argument must
const-evaluate to `>= 1` (`E0202` otherwise). It's the same width formula
the checker already uses internally for enum tag widths.

`clog2(PARAM)` works in a module **body** width — it lowers to an injected
Verilog constant function, so the width still tracks an instantiation-time
parameter override. `clog2(PARAM)` in a **port** width is a compile error
(`E0407`) — a port's width has to be known before the body exists to inject
anything into. `clog2` of a plain literal always folds at compile time in
either position.

## Enum→bits: `encoding`

An `enum` is a symbolic type — you can `match` on it, but you can't otherwise
treat it as a number. `encoding(e)` is the one deliberate escape hatch: it
reads out an enum value's stable on-wire bit pattern as plain unsigned
`bits[N]`, the same bits the compiler already assigns as `localparam`s
internally.

The most common reason to reach for it is a debug or bring-up port — showing
an FSM's current state on LEDs or a logic-analyzer header without attaching a
full debugger:

```mimz
enum Light { Red, Green, Blue }

reg state: Light = Light.Red
out state_bits: bits[2]

state_bits = encoding(state)
```

For a plain, tag-only enum like `Light` (3 variants), `N` is just the tag
width, `clog2(3) = 2`. For a **tagged union** with payload fields, `N` is the
enum's FULL width — tag plus the largest payload — the exact same total
`inferred_total_width` the emitter already sizes the signal at. There's no
way to get just the tag out of a payload-carrying enum via `encoding` alone;
slice the result yourself (`encoding(pkt)[hi:lo]`) if that's what you need.

There is deliberately **no reverse cast** (`bits` → `enum`). An unchecked
integer-to-enum conversion would let an arbitrary bit pattern claim to be a
declared enum value — exactly the invalid-state class the enum type exists
to rule out at compile time.

Every OTHER built-in on this page rejects an enum argument (`E0403`/`E0407`);
`encoding` is the one built-in that requires one — anything else is `E0418`.

See the full runnable example: `examples/english/enum_encoding.mimz`.

## Combinational functions: `fn`

A `fn` is pure, stateless combinational logic that isn't worth its own
module — inlined at the call site during emission, so recursion isn't
allowed and there's no instantiation overhead:

```mimz
fn max3(a: bits[8], b: bits[8], c: bits[8]) -> bits[8] {
  return max(max(a, b), c)
}

module Top {
  in  x: bits[8]
  in  y: bits[8]
  in  z: bits[8]
  out biggest: bits[8]

  biggest = max3(x, y, z)
}
```

`fn` bodies can use `if`/`match`, `repeat`/`loop` unrolling, and other
built-ins (as above) — anything combinational. Function names are
project-wide unique (`E0801`) and are never namespace-qualified, unlike
module/enum/bundle names (chapter 9).

## Array-typed `fn` parameters

A `fn` parameter can be array-typed — `bits[8][4]` is an array of four
`bits[8]` elements. This isn't real Verilog array hardware: it's sugar over
N independent scalar ports, so the size must be fixed at compile time and
known from the type itself (the `foreach` element form in
[chapter 7](07-expressions-and-control.md) relies on exactly this — the
iteration count comes from the array type's own length):

```mimz
fn find_index(vals: bits[8][4], target: bits[8]) -> signed[4] {
  loop i: 0..4 {
    if vals[i] == target { return i }
  }
  -1
}
```

Indexing behaves differently depending on whether the index is known at
compile time:

- a **constant** index (a `loop`/`repeat` variable, a literal) folds
  directly to the matching scalar — `vals[i]` inside the `loop` above just
  becomes `vals_0`, `vals_1`, … at that unrolled position;
- a **runtime** index (an ordinary signal) can't select a real array
  element, so the emitter generates a ternary-chain mux over every element
  instead: `fn pick(vals: bits[8][4], idx: bits[3]) -> bits[8] { vals[idx] }`
  compiles to a chain of `idx == 0 ? vals_0 : idx == 1 ? vals_1 : …`. An
  out-of-range runtime index (more index values than elements — `idx` is
  3 bits here but the array only has 4 elements) falls through to the last
  element rather than erroring, since the mux chain must cover every
  possible bit pattern.

## Worked example

The `datapath` example in [`../../examples/`](../../examples/) exercises the
multiply/shift/concat/slice/`trunc` family; `bitops` exercises
`min`/`max`/`abs`/`nand`/`nor`/`xnor`. Both have self-checking testbenches, so
they double as runnable specs for these built-ins.

Next: [expressions and control flow](07-expressions-and-control.md).
