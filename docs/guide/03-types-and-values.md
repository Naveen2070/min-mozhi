# 3 — Types and Values

Every signal in Min-Mozhi has a **type**, and the type carries a **width** (how
many bits the wire is). Widths are checked everywhere — the compiler never
silently widens or truncates a value. Getting comfortable with widths is most of
learning the language.

## The three hardware types

| Type        | Meaning                     | Example values            |
| ----------- | --------------------------- | ------------------------- |
| `bit`       | a single wire, one bit      | `true`, `false`, `0`, `1` |
| `bits[N]`   | `N` wires, unsigned integer | `bits[8]` holds 0..255    |
| `signed[N]` | `N` wires, two's-complement | `signed[4]` holds −8..7   |

```mimz
in  flag:  bit
in  data:  bits[8]
in  delta: signed[4]
```

`N` is a **width expression** — usually a literal, but it can be a parameter or a
`const`, which the compiler folds at build time:

```mimz
module Reg(WIDTH: int = 8) {
  in  d: bits[WIDTH]
  out q: bits[WIDTH]
  q = d
}
```

A width must be a positive compile-time integer. `bits[0]` and a negative width
are rejected (`E0410`).

## `bit` vs `bits[1]`

`bit` is the boolean type: it is what conditions and logic operators produce and
consume. It is distinct from `bits[1]`; do not expect them to interchange. Use
`bit` for flags and control, `bits[N]` for data.

## Booleans

`true` and `false` are the two `bit` literals:

```mimz
out ready: bit
ready = true
```

## Signed vs unsigned, and why they never mix

`bits[N]` is unsigned; `signed[N]` is two's-complement. Min-Mozhi will **not**
let you combine them in one operation without an explicit cast — silent
sign-confusion is a notorious bug source:

```mimz
in a: bits[4]
in b: signed[4]
// out y: ... = a + b      // ERROR E0403: kind mixing
```

Convert deliberately with the cast built-ins (chapter 6): `signed(a)` reinterprets
the bits as signed, `unsigned(b)` the other way. Neither changes the bit pattern;
they change how the _next_ operator treats it.

## Number literals and their width

A bare number like `7` is a compile-time integer with no fixed width yet — it
adapts to the context it is used in, as long as it fits. Assigning a literal that
does not fit its target is rejected:

```mimz
out small: bits[2]
small = 3      // ok: 3 fits in 2 bits
// small = 9   // ERROR E0405: 9 does not fit in bits[2]
```

For a `signed` target, a negative literal is fine; a negative literal into an
unsigned `bits[N]` is rejected (`E0405`).

## Enums: named states

An `enum` is a set of named values, perfect for state machines. The compiler
assigns each variant a bit pattern and computes the width for you:

```mimz
enum State { Red, Green, Yellow }
```

You refer to a variant with `Enum.Variant`:

```mimz
reg s: State = State.Red
```

`enum` needs at least one variant. Enums shine with `match`, which forces you to
handle every variant (chapter 7).

### Tagged unions (enums with payloads)

Since v0.2.15, enum variants can carry **payload fields** — data that differs per
variant:

```mimz
enum Packet {
  Data(data: bits[8]),
  Ctrl(kind: bits[2], seq: bits[4]),
  Empty
}
```

`Data` carries an 8-bit value; `Ctrl` carries two fields (`kind` and `seq`);
`Empty` carries nothing. The compiler sizes the tag to fit the variant count and
pads the payload to the widest variant — `Ctrl`'s 6 bits (`kind` + `seq`) in
this case, so the total width is `tag_bits + 6`.

Match on a tagged union must unpack the payload:

```mimz
match pkt {
  Packet.Data(d)      => out <- d
  Packet.Ctrl(k, _)   => out <- {k, 0b0000}
  Packet.Empty        => out <- 0b0000_0000
}
```

The payload bindings (`d`, `k`) are available inside the arm. Wrong binding
count or incompatible types across OR-arms are caught at compile time (E0806,
E0808).

## Bundles (Structs / Interfaces)

A `bundle` is a named group of signals, used to cleanly pass multiple related wires or registers together. This eliminates boilerplate when connecting modules.

```mimz
bundle AxiStream {
  valid: bit
  ready: bit
  data:  bits[8]
}

module Node {
  in  bus_in:  AxiStream
  out bus_out: AxiStream
}
```

The compiler automatically flattens a bundle into individual signals (`bus_in_valid`, `bus_in_ready`, etc.) during Verilog emission, meaning bundles have zero runtime overhead and generate clean, synthesis-safe hardware.

Build a bundle-typed value with a **literal**, `{ field: value, ... }` — every
field must be given (a missing field is `E0901`, an unknown one is rejected
too):

```mimz
bundle Hs { valid: bit, data: bits[8] }

out dst: Hs
dst = { valid: 1, data: 0 }
```

You can also **destructure** a bundle-typed value into individual bindings
with `let { field, ... } = expr`:

```mimz
let { valid, data } = bus_in
out y: bits[8]
y = valid ? data : 0
```

A partial destructure (naming only some fields) is fine — you don't have to
bind every field. Duplicate binding names and field-rename syntax
(`{ f: alias }`) are both rejected at parse time; a destructured field keeps
its own name.

A bundle can also be a `fn` parameter or return type. Field access on a
bundle-typed parameter works exactly like field access on a module port:

```mimz
bundle Handshake(W: int = 8) {
  valid: bit
  data:  bits[W]
}

fn get_valid(h: Handshake(W: 8)) -> bit {
  h.valid
}
```

## Compile-time types: `int` and `bool`

Parameters and `const`s are compile-time values, not hardware. They use the
compile-time types `int` and `bool` — never `bits[N]`:

```mimz
const LANES: int = 4
const DEBUG: bool = false

module Bus(WIDTH: int = 8) { /* ... */ }
```

These exist only during compilation; they fold into widths and unrolled hardware
and never become wires themselves. More on parameters and `const` in
[chapter 9](09-modules-and-reuse.md).

Next: [signals — ports, wires, and registers](04-signals.md).
