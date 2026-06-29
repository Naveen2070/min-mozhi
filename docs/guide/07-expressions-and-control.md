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
