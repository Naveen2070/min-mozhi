# 1 — Getting Started

## Install

There are two paths: download a prebuilt binary, or build from source.

### Option A — download a prebuilt binary (no Rust needed)

Each tagged release attaches a `mimz` binary for every platform to the GitHub
Release page. Pick the archive for your OS/arch:

| Platform                   | Archive                                           |
| -------------------------- | ------------------------------------------------- |
| Linux (any distro, static) | `mimz-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Windows (x86-64)           | `mimz-<version>-x86_64-pc-windows-msvc.zip`       |
| macOS (Intel)              | `mimz-<version>-x86_64-apple-darwin.tar.gz`       |
| macOS (Apple Silicon)      | `mimz-<version>-aarch64-apple-darwin.tar.gz`      |

Unpack it, then put `mimz` on your `PATH`. Each release also ships a `SHA256SUMS`
file — verify your download first with `shasum -a 256 -c SHA256SUMS --ignore-missing`
(macOS/Linux) or `Get-FileHash <archive> -Algorithm SHA256` and compare to the
matching line in `SHA256SUMS` (Windows).

> **The binaries are UNSIGNED for v0.1.0** (code signing is deferred — see the
> `UNSIGNED.txt` in each archive). They are safe; the OS just doesn't recognise
> the (absent) signature on first run:
>
> - **macOS** — Gatekeeper may block it. Clear the quarantine once with
>   `xattr -d com.apple.quarantine ./mimz`, or right-click the binary → **Open**.
> - **Windows** — SmartScreen may warn. Click **More info → Run anyway**.

### Option B — build from source

Min-Mozhi is a Rust program. With [Rust](https://rustup.rs) (stable ≥ 1.85):

```text
git clone <the-repo>
cd min-mozhi
cargo build            # builds the compiler -> target/debug/mimz
```

Every command below is written as `mimz …`. Until you install the binary on your
`PATH`, run it through Cargo instead: `cargo run -- <args>`. For example,
`mimz check foo.mimz` becomes `cargo run -- check foo.mimz`.

Source files use the `.mimz` extension. Confirm your install with
`mimz --version` — it prints the compiler version and the language edition.

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

That completes the core loop: write → `check` → `compile`. Once a design is clocked you
can also `mimz sim` it to a waveform and `mimz test` it against self-checking
testbenches ([chapter 11](11-toolchain.md)); the [`../../demo/`](../../demo/)
folder walks that full loop on a worked example. The next chapters fill in the
language itself, starting with [the lexical basics](02-lexical-basics.md).
