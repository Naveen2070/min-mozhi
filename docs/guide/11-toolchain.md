# 11 — The Toolchain

The `mimz` CLI is how you check, build, run, and reshape your code. Every command
takes a `.mimz` file; run them through `cargo run --` until `mimz` is on your
`PATH`.

## `mimz check` — lex, parse, and verify

The workhorse. Runs the front end and the full safety checker; writes nothing.

```text
mimz check counter.mimz
mimz check counter.mimz --tokens    # also dump the token stream (debugging)
mimz check counter.mimz --json      # machine-readable diagnostics
```

A clean file prints `OK:`; a broken one prints `E`-coded diagnostics.

## `mimz compile` — emit Verilog

Runs the whole pipeline and writes synthesizable Verilog. Resolves imports.

```text
mimz compile counter.mimz                 # writes counter.v
mimz compile counter.mimz -o build/c.v    # choose the output path
```

## `mimz eval` — run combinational logic

Evaluate a purely combinational module's outputs for a set of inputs, without a
simulator. (No clock, `reg`, instances, or `repeat` — those need the full
simulator, which is a later phase.)

```text
mimz eval adder.mimz --in a=3,b=5
mimz eval alu.mimz --module Alu --in a=10,b=4,op=1 --param WIDTH=8
```

## `mimz explain` — the long-form error book

Every `E`-code has a classroom explanation: the rule, why silicon needs it, and
the fix.

```text
mimz explain E0502
mimz explain e0403     # case-insensitive
```

## `mimz translate` — change flavor and/or word order

Convert a file between keyword flavors and between code/thamizh word order.

```text
mimz translate counter.mimz --to tamil               # keywords only (lossless)
mimz translate counter.mimz --order thamizh          # natural Tamil word order
mimz translate counter.mimz --order thamizh --to tamil
```

Two important differences:

- `--to` (flavor only) is a **lossless** keyword reskin — comments and layout
  survive.
- `--order` re-emits from the AST, so it **reformats and drops comments** (the
  result still compiles to byte-identical Verilog and re-parses identically).

## `mimz fmt` — normalize to one flavor

The `gofmt` of Min-Mozhi: rewrite a file in place so every keyword is one flavor.
It rides the lossless `translate` path, so comments and layout are preserved.

```text
mimz fmt messy.mimz                  # normalize to the file's majority flavor
mimz fmt messy.mimz --to english     # force a flavor
mimz fmt messy.mimz --strict         # warn + exit non-zero if flavors are mixed
mimz fmt messy.mimz -o clean.mimz    # write elsewhere, leave the input alone
```

`fmt` only normalizes _flavor_; word-order reformatting stays with
`translate --order` (because that one is not lossless).

## Diagnostics in your language: `--lang`

`check`, `compile`, and `eval` render diagnostics in the flavor your file mostly
uses, and `--lang` overrides:

```text
mimz check counter_tamil.mimz --lang tamil
```

(Localized error messages are rolling out catalog entry by catalog entry; codes
without a localized template fall back to clear English. The machine-readable
`--json` output always stays English.)

## In your editor: `mimz lsp`

`mimz lsp` is a Language Server (diagnostics-only v0) — live red squiggles as you
type, in the same flavor as the CLI. The VS Code client lives in
[`../../editors/vscode/`](../../editors/vscode/), which also provides syntax
highlighting for all three flavors.

## Before you commit (for contributors)

```text
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
npx prettier --write "**/*.md" && npx markdownlint-cli2
```

Next: [the cheat sheet](12-cheatsheet.md).
