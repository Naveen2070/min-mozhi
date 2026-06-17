# 1 — Getting Started

## Install

Min-Mozhi is a Rust program. With [Rust](https://rustup.rs) (stable ≥ 1.85):

```text
git clone <the-repo>
cd min-mozhi
cargo build            # builds the compiler -> target/debug/mimz
```

Every command below is written as `mimz …`. Until you install the binary on your
`PATH`, run it through Cargo instead: `cargo run -- <args>`. For example,
`mimz check foo.mimz` becomes `cargo run -- check foo.mimz`.

Source files use the `.mimz` extension.

## Your first module

Create `blink.mimz`:

```mimz
module And2 {
  in a: bit
  in b: bit
  out y: bit

  y = a & b
}
```

Read it as a circuit: two one-bit inputs `a` and `b`, one output `y`, and `y` is
wired to the AND of the two. That is the whole module — no `main`, no statements
that "run". A module is a box with ports and the logic that drives the outputs.

## Check it

```text
mimz check blink.mimz
```

`check` runs the lexer, parser, and the full safety checker. A clean file prints
an `OK:` line. A broken one prints a diagnostic with a stable code:

```text
error[E0502]: output 'y' is never driven
 --> blink.mimz:4:7
```

Every diagnostic has an `E`-code you can look up: `mimz explain E0502`.

## Compile to Verilog

```text
mimz compile blink.mimz -o blink.v
```

This produces standard Verilog-2005 you can feed to any simulator or synthesis
tool (the project tests every example against [Icarus Verilog](http://iverilog.icarus.com/)):

```verilog
module And2 (
    input wire a,
    input wire b,
    output wire y
);
    assign y = (a & b);
endmodule
```

## The mental model: a five-station pipeline

When you run `mimz compile`, your file flows through five stations:

1. **Lexer** — text becomes tokens; keywords in any of the three flavors resolve
   to the same token here.
2. **Parser** — tokens become an AST (the tree shape of your module).
3. **Checker** — six passes enforce every safety rule (names, widths, drivers,
   exhaustiveness, clock domains). This is where you get teaching errors.
4. **Emitter** — the AST becomes Verilog; `repeat` loops unroll, Tamil
   identifiers transliterate to ASCII.
5. **Output** — a `.v` file.

`mimz check` stops after station 3 (no file written); `mimz compile` runs all
five. For the deep tour of the pipeline on a real example, see
[`../how-the-compiler-works.md`](../how-the-compiler-works.md).

## Where to go next

You now have the loop: write → `check` → `compile`. Once a design is clocked you
can also `mimz sim` it to a waveform and `mimz test` it against self-checking
testbenches ([chapter 11](11-toolchain.md)); the [`../../demo/`](../../demo/)
folder walks that full loop on a worked example. The next chapters fill in the
language itself, starting with [the lexical basics](02-lexical-basics.md).
