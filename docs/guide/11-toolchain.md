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

### Romanizing Tamil names: `--romanize-names`

By default `translate` swaps only keywords and keeps identifiers verbatim — a
fully-Tamil program (see `examples/tamil-pure/`) keeps its Tamil _names_ even
when you switch keyword flavor. Add `--romanize-names` to also rewrite Tamil
identifiers to readable Latin (`கணக்கி` → `kannakki`), using the same scheme the
Verilog backend uses:

```text
mimz translate tamil-pure/kanakki.mimz --to tanglish --romanize-names -o k.mimz
```

Romanization is **one-way** on its own (the rule can't be inverted). To make it
reversible, pass `-o`: a sidecar **`<out>.names.json`** (here `k.mimz.names.json`)
is written next to the output, recording `romanized → original Tamil`. A reverse
run restores the exact Tamil names — and the sidecar is **found automatically**,
so no flag is needed:

```text
mimz translate k.mimz --to tamil          # auto-loads k.mimz.names.json
mimz translate k.mimz --to tamil --names-map other.json   # or point at one
mimz translate k.mimz --to tamil --no-names-map           # keep the Latin names
```

The map carries a version; a map this `mimz` doesn't understand is rejected with
a clear error rather than mis-restoring.

One edge to know: Tamil script can be the _only_ separator between a number and a
following name (e.g. `42கணக்கி`, written with no space). Romanizing to Latin would
glue them into an unlexable `42kannakki`, so the reskin inserts a single
separating space there. Such input round-trips **token-equivalent** (it gains
that space), not byte-for-byte — normal whitespace-separated code is unaffected.

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

## Project defaults: `mimz.toml`

Tired of retyping the same flags? Drop a `mimz.toml` at your project root and
`mimz` reads its defaults — discovered by walking **up** from the input file (like
`Cargo.toml`/`rustfmt.toml`), or pointed at explicitly with a global
`--config <path>`. Precedence is **command-line flag › `mimz.toml` › built-in
default**, so a one-off flag always wins.

```toml
lang = "tamil"          # default diagnostics flavor for check/compile/eval

[translate]
to = "tanglish"         # default --to
order = "code"          # default --order
romanize_names = false  # default --romanize-names
names_map = "auto"      # "auto" = auto-load the sidecar; "off" = don't

[fmt]
to = "tamil"            # default fmt --to
strict = true           # default fmt --strict
```

Every key is optional; an unknown key is reported as an error (a typo never
silently does nothing).

## A non-fatal lint: `W0001`

Mixing **Tamil** keywords with English/Tanglish ones in one file triggers a
non-fatal warning (`W0001`) on `check`/`compile`/`eval` and in the editor — Tamil
reads in a different word order, so one language per file reads best. It never
fails the build; `mimz fmt` normalizes the mix away. (English + Tanglish share
code order, so mixing those two stays clean.)

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
