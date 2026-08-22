---
title: Your first module
description: Install Min-Mozhi, write an AND gate and a counter, and learn the four commands you will actually use — check, compile, test and explain.
order: 4
---

# Your first module

From here on, everything is Min-Mozhi-specific — the accurate half.

## Install

Two paths.

**Prebuilt binary.** The easiest route is the
[Downloads page](/downloads) — it has the current release for Linux, Windows and
macOS (Intel and Apple Silicon), with its checksums, and a
[full release history](/downloads/releases) if you need to pin an older version.
The same files are attached to each GitHub Release.

Unpack the archive and put `mimz` on your `PATH`. Each release also ships
`SHA256SUMS`; verify before running. The binaries are **unsigned**, so macOS
Gatekeeper and Windows SmartScreen will both complain the first time.

**From source.** Min-Mozhi is a Rust program. With
[Rust](https://rustup.rs) (stable 1.85 or newer):

```console
$ git clone https://github.com/Naveen2070/min-mozhi
$ cd min-mozhi
$ cargo build
```

That produces `target/debug/mimz`. Until it is on your `PATH`, run it through
Cargo: `cargo run -- check foo.mimz` instead of `mimz check foo.mimz`.

Confirm with `mimz --version`, which prints the compiler version and the language
edition. Source files use the `.mimz` extension.

## An AND gate

Create `and2.mimz`:

```mimz
module And2 {
  in a: bit
  in b: bit
  out y: bit

  y = a & b
}
```

Read it as a circuit, not a procedure: two one-bit inputs, one output, and `y` is
wired to the AND of the two. There is no `main` and nothing that "runs". A module
is a box with ports and the logic driving its outputs.

Note where things live: **ports go in the body**. The parentheses after a module
name — which this module does not need — hold *parameters*, not ports.

## Check it

```console
$ mimz check and2.mimz
```

`check` runs the lexer, parser and the full safety checker. A clean file prints
an `OK:` line. This is the command you will run most.

Try breaking it. Delete the `y = a & b` line and check again:

```text
error[E0502]: output 'y' is never driven
```

Not a warning. An output that nothing drives is not a circuit, so it is an error
with a code you can look up.

## A counter

An AND gate has no memory. Adding a clock is what makes it interesting. Create
`counter.mimz`:

```mimz
module Counter(WIDTH: int = 8) {
  clock clk
  reset rst

  out count: bits[WIDTH]

  reg value: bits[WIDTH] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

Four new things:

- **`(WIDTH: int = 8)`** — a parameter. It configures the hardware at compile
  time; it never becomes a wire.
- **`clock clk` / `reset rst`** — a clock input and a reset input. A module with
  registers must have a reset (`E0301`).
- **`reg value: bits[WIDTH] = 0`** — a register, with its reset value. The `= 0`
  is mandatory (`E1104`): there is no such thing as an uninitialized register.
- **`on rise(clk)`** — this block happens on the rising edge of `clk`. Inside it
  you drive registers with `<-`. Outside, you drive wires and outputs with `=`.

### Why `+%` and not `+`

Because arithmetic is lossless. `bits[8] + 1` is `bits[9]` — wide enough for the
carry. Assigning that back into a `bits[8]` register would lose a bit, so the
compiler stops you with `E0401`.

`+%` is **wrapping** add: same width, wraps on overflow, on purpose and visibly.
A counter is exactly the case where you want that. Say so and the compiler agrees.

This one surprise catches almost everybody on day one, and it is the language
working correctly.

## Compile it

```console
$ mimz compile counter.mimz -o counter.v
```

That emits Verilog-2005, ready for a synthesis or simulation toolchain. Open it —
reading the output is a good way to check your mental model against what is
actually built.

## Test it

Tests live in the source file:

```mimz
test "counter counts" for Counter(WIDTH: 8) {
  rst = 1
  tick(clk)
  rst = 0
  tick(clk, 3)
  expect count == 3
}
```

Drive inputs with `=`, advance the clock with `tick(clk)` or `tick(clk, N)`,
and check with `expect`. Then:

```console
$ mimz test counter.mimz
```

Note the test name is a quoted string, and `tick` is a call.

## When something breaks

Every diagnostic has a code, and every code has a long-form explanation:

```console
$ mimz explain E0401
$ mimz explain --list      # every code, one line each
```

These are written as teaching text: what is wrong, why it is unsafe in hardware,
and how to fix it.

## The four commands

| Command   | For                                       |
| --------- | ----------------------------------------- |
| `check`   | is this valid? — the fast inner loop      |
| `compile` | produce Verilog                           |
| `test`    | run the `test` blocks in a file           |
| `explain` | what does this error code mean?           |

There is **no `build` and no `run`**. The full set is in the
[CLI reference](/handbook/07-cli).

## Where to go next

You have the shape of the language. The rest of this section walks you through
the [Guide](/guide), one topic at a time, telling you what to look for.

Next: [types and widths](/learn/05-types-and-widths).
