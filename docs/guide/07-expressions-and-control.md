# 7 — Expressions and Control Flow

In Min-Mozhi, choosing between values is done with **expressions** — `if` and
`match` both produce a value. This is the expression-oriented style of Rust and
modern languages, and it is what makes "no inferred latch" a guarantee rather
than a hope: an expression must always produce a value, so every case is covered
by construction.

## `if` expressions

An `if` used to drive a value **must** have an `else` — there is no "leave it
unchanged" default for combinational logic (that would be a latch):

```mimz
out y: bits[8]
y = if sel { a } else { b }
```

Both arms must have the same type and width. Omitting the `else` is a teaching
error pointing straight at the latch trap:

```mimz
// y = if sel { a }          // ERROR E1108: needs an else (would infer a latch)
```

Mismatched arms are caught too:

```mimz
// y = if sel { a } else { c }   // ERROR E0408 if a and c differ in width/type
```

## `match` expressions

`match` selects a value by comparing a scrutinee against patterns. It must be
**exhaustive** — every possible value handled — which is exactly what stops a mux
or FSM from accidentally latching:

```mimz
out y: bits[WIDTH]
y = match sel {
  0b00 => a
  0b01 => b
  0b10 => c
  0b11 => d
}
```

Patterns can be integers, booleans, enum variants, binary don't-care patterns
(below), or the wildcard `_` which matches anything left over:

```mimz
y = match op {
  0    => zero_case
  1    => one_case
  _    => default_case
}
```

### Don't-care patterns

A **binary** pattern may use `?` for a don't-care bit, matching any value at that
position — the `casez` idiom, ideal for priority decoders:

```mimz
grant = match req {
  0b1?? => 0b11      // any value whose high bit is 1: 100, 101, 110, 111
  0b01? => 0b10      // 010 or 011
  0b001 => 0b01
  _     => 0b00      // required — see below
}
```

- A masked pattern must be **binary** (`0b…`) and match the scrutinee's width
  exactly.
- Don't-care patterns **do not earn exhaustiveness on their own** — the compiler
  cannot prove they cover every value, so keep a `_` arm (or literal coverage) or
  you get a non-exhaustive error (`E0601`).

Rules the compiler enforces:

- **exhaustive or it fails** — a missing case is `E0601` (add the case or a `_`);
- **no unreachable arm** — an arm after a `_`, or a duplicate value, is `E0602`;
- you cannot `match` on a `signed` value (`E0409`);
- **OR-arm binding intersection** — when one arm lists multiple patterns
  separated by `,` (e.g. `Op.Add(a, b), Op.Sub(a, b) => a + b`), every
  alternative must bind the **same names with the same types**. A missing name
  or a width mismatch across alternatives is `E0808`. A `_` wildcard does not
  satisfy a binding requirement.

Matching over an `enum` is the idiomatic state machine — see the FSM in
[chapter 8](08-sequential-logic.md).

## Iteration: `loop`

Min-Mozhi supports a simple `loop` construct that evaluates iteratively. Since hardware does not have a "while loop" instruction, the compiler statically unrolls this loop at build time.

- Inside a combinational `fn`, the loop unrolls into parallel combinational logic.
- Inside a clocked `on` block, the loop's register updates unroll into a single cycle's next-state logic.

```mimz
on rise(clk) {
  loop i: 0..4 {
    regs[i] <- data[i]
  }
}
```

Note `loop`'s bound uses `:` (`loop i: 0..4`), not `in` — `in` is
`foreach`'s spelling (see below), a different, newer construct with
different semantics (its bound comes from an array's own length in the
element form, not a hand-written range).

_(Note: For loops that take multiple actual clock cycles to execute, use a `sync loop`. See [chapter 8](08-sequential-logic.md).)_

## Iteration sugar: `foreach`

`foreach` is a thin desugaring over `repeat`/`loop` for the two most common
shapes — walking a range, or walking every element of an array or `mem` — so
you don't hand-write the bound yourself:

```mimz
// index-range form: same unroll as `loop i in 0..4`
foreach i in 0..4 {
  lamps[i * 8 + 7 : i * 8] = i * 2
}
```

```mimz
// element form: iterates every element of an array-typed value —
// the count comes from the array's own declared length, so it can
// never drift out of sync with it
fn sum8(values: bits[8][8], acc: bits[11]) -> bits[11] {
  foreach v in values {
    let acc = acc +% extend(v, 11)
  }
  acc
}
```

The element form requires the source to be array- or `mem`-typed
(`E0417` otherwise). Like `loop`, `foreach` unrolls fully at compile time —
it is sugar, not a new execution model.

> **Provisional keyword:** the Tanglish/Tamil spellings (`ovvondraga` /
> ஒவ்வொன்றாக) are dev/testing placeholders pending native-speaker review —
> expect them to change before they're finalized.

## Statement-level `if` / `else`

Inside a clocked `on` block you also write `if`/`else` as **statements** that
choose what a register does this cycle. Here the `else` is optional — a register
naturally holds its value when nothing assigns it:

```mimz
on rise(clk) {
  if timer == 0 {
    state <- next_state
  } else {
    timer <- timer -% 1
  }
}
```

The difference is the context:

- driving a `wire`/`out` value → `if` is an **expression**, `else` required;
- updating a `reg` inside `on` → `if` is a **statement**, `else` optional (the
  register holds otherwise).

Next: [sequential logic](08-sequential-logic.md).
