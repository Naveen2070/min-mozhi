# 11 — The Toolchain

The `mimz` CLI is how you check, build, run, and reshape your code. Every command
takes a `.mimz` file; run them through `cargo run --` until `mimz` is on your
`PATH`.

## `mimz --version` — compiler and language edition

`mimz` has **two version axes** (like `rustc 1.x` versus the Rust `2021` edition):
the compiler's own version and the language edition. `--version` prints both,
with the edition's codename on top:

```text
Wingless Butterfly
mimz    0.1.0                       (compiler)
edition wingless-butterfly-2026-1   (language)
```

The compiler version comes from the crate; the edition (`variant-year-code`)
tracks the language itself. See
[`../../spec/06-editions.md`](../../spec/06-editions.md) for what the two axes
mean and how editions evolve.

## `mimz init <name>` — scaffold a new project

Generate a ready-to-use project directory with a documented `mimz.toml` and a
starter counter module (with a passing `test` block), so `mimz test` and
`mimz compile` work immediately:

```text
mimz init my_project
cd my_project
mimz test my_project.mimz    # passes right away
mimz compile my_project.mimz # emits my_project.v
```

The module name is derived from the directory name (`my_project` →
`MyProject`). The command refuses to overwrite a non-empty directory.

## `mimz check` — lex, parse, and verify

The workhorse. Runs the front end and the full safety checker; writes nothing.

```text
mimz check counter.mimz                       # single check
mimz check counter.mimz --tokens              # also dump the token stream (debugging)
mimz check counter.mimz --json                # machine-readable diagnostics
mimz check counter.mimz --watch               # re-check on every save until Ctrl-C
```

With `--watch`, the process stays alive and re-runs whenever any file in the
project changes (entry + transitive imports) — useful for a tight edit–check
loop. A clean file prints `OK: <path> — <N> module(s), <M> test(s), <K> file(s).`;
a broken one prints `E`-coded diagnostics.

## `mimz compile` — emit Verilog

Runs the whole pipeline and writes synthesizable Verilog. Resolves imports.

```text
mimz compile counter.mimz                 # writes counter.v
mimz compile counter.mimz -o build/c.v    # choose the output path
mimz compile counter.mimz --emit-testbench # also writes counter_tb.v from inline tests
mimz compile counter.mimz --json          # machine-readable diagnostics
```

The command prints its output paths to `stdout`:

- **Normal:** `compiled <in> -> <out>`
- **With testbench:** a second line `compiled <in> -> <out> (testbench)`

`--emit-testbench` writes `<output>_tb.v` alongside the Verilog. If the source
has no `test` blocks it prints a note on `stderr` and writes only the `.v`; the testbench is
built before either file is written, so an emission error leaves no stray output.

## `mimz eval` — run combinational logic

Evaluate a purely combinational module's outputs for a set of inputs, without
running the clock — a quick one-shot. (For clocked designs, registers, and
waveforms use `mimz sim`.)

```text
mimz eval adder.mimz --in a=3,b=5
mimz eval alu.mimz --module Alu --in a=10,b=4,op=1 --param WIDTH=8
```

## `mimz sim` — simulate and write a waveform

Run a design under a default stimulus (reset asserted the first cycle, inputs
held, the clock toggled) and capture a waveform. Clocked **and** combinational
modules work; `-o` writes a VCD, `--trace` prints a per-cycle table.

```text
mimz sim counter.mimz --cycles 16 -o counter.vcd   # waveform → counter.vcd
mimz sim counter.mimz --cycles 8 --trace           # per-cycle table to stdout
mimz sim adder.mimz --in a=3,b=5                    # combinational: one frame
mimz sim adder.mimz --sweep a=1|2|3 --in b=10      # a frame per input combo
```

### Viewing the VCD

`mimz` emits a standard IEEE-1364 VCD that any waveform viewer opens:

- **Web, no install:** open <https://app.surfer-project.org> (Surfer, runs in the
  browser) and drag the `.vcd` in — the file stays local. Alt: <https://vc.drom.io>.
- **Desktop:** GTKWave — `winget install GTKWave` (or `scoop install gtkwave`),
  then `gtkwave counter.vcd`.
- **VS Code:** the Surfer / VaporView / WaveTrace extension opens `.vcd` in-editor.

For a full check → test → sim → view-waveform walkthrough on a real design (an
accumulator CPU exercising instances, imports, `repeat`, enum state, and
`match`-as-ROM), see [`../../demo/`](../../demo/).

## `mimz test` — run `test` blocks

Run a file's `test "…" for M(…) { … }` blocks (`tick`/`expect`), reporting
pass/fail with teaching messages; exits non-zero if any test fails.

```text
mimz test counter.mimz
mimz test counter.mimz --filter "counts up"   # only matching tests
mimz test counter.mimz --trace                # waveform table per test
```

## `mimz lint` — style and hygiene warnings

Separate from `check` (which is about correctness). `mimz lint` checks naming
conventions, unused signals, and other style rules. All diagnostics are
warnings — the command never fails the build:

```text
mimz lint counter.mimz
mimz lint counter.mimz --json    # machine-readable output
```

`lint` runs import resolution and analyses the whole project; load/lex failures
are the only things that make it exit non-zero.

## `mimz repl` — interactive combinational evaluator

Parses a `.mimz` file once, then reads input bindings from stdin line by line.
Each line is evaluated immediately and the module's outputs are printed:

```text
mimz repl adder.mimz
Min-Mozhi REPL  —  module `Adder`  (Ctrl-C or :quit to exit)

mimz> a=3, b=5
sum = 8  (bits[9])
```

Internal commands: `:quit` / `:q` to exit, `:help` for usage. `--param` and
`--module` work as in `mimz eval`.

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
reversible, pass `-o`:

- a sidecar **`<out>.names.json`** (here `k.mimz.names.json`) is written next to
  the output, recording `romanized → original Tamil`;
- a reverse run restores the exact Tamil names;
- the sidecar is **found automatically**, so no flag is needed:

```text
mimz translate k.mimz --to tamil          # auto-loads k.mimz.names.json
mimz translate k.mimz --to tamil --names-map other.json   # or point at one
mimz translate k.mimz --to tamil --no-names-map           # keep the Latin names
```

The map carries a version; a map this `mimz` doesn't understand is rejected with
a clear error rather than mis-restoring.

One edge to know: Tamil script can be the _only_ separator between a number and a
following name (e.g. `42கணக்கி`, written with no space).

- Romanizing to Latin would glue them into an unlexable `42kannakki`, so the
  reskin inserts a single separating space there.
- Such input round-trips **token-equivalent** (it gains that space), not
  byte-for-byte.
- Normal whitespace-separated code is unaffected.

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

## `mimz doctor` — toolchain health check

Prints the compiler version, platform info, runs an in-memory compile smoke
test, and probes for optional external tools (iverilog, verilator, gtkwave):

```text
mimz doctor              # user toolchain
mimz doctor --dev        # also check Rust, wasm-pack, test tools (contributors)
```

Missing external tools are warnings, not failures — the runtime CLI is entirely
in-process. The command exits non-zero only on a real problem (broken pipeline,
unwritable temp dir, invalid `mimz.toml`). Aliased as `mimz env`.

## `mimz completions <shell>` — shell tab-completion

Prints a shell tab-completion script to stdout, generated straight from the
clap command tree (always matches the real subcommands and flags):

```text
mimz completions bash > /etc/bash_completion.d/mimz
mimz completions powershell >> $PROFILE        # then reload-profile
mimz completions zsh > /usr/local/share/zsh/site-functions/_mimz
```

Supports bash, zsh, fish, powershell, and elvish.

## `mimz eject std` — vendor the standard library

Writes the embedded standard library to a local directory so a project can
vendor and customise it (then point `mimz.toml [lib] std` at the directory):

```text
mimz eject std --to ./my-std          # English canonical
mimz eject std --to ./my-std --flavor tamil   # pure-Tamil twins
mimz eject std --to ./my-std --force  # overwrite existing files
```

See the [standard-library gallery](stdlib/README.md) for what ships inside.

## Project defaults: `mimz.toml`

Tired of retyping the same flags? Drop a `mimz.toml` at your project root and
`mimz` reads its defaults. The file is found in one of two ways:

- discovered by walking **up** from the input file (like
  `Cargo.toml`/`rustfmt.toml`);
- or pointed at explicitly with a global `--config <path>`.

Precedence is **command-line flag › `mimz.toml` › built-in default**, so a
one-off flag always wins.

```toml
lang = "tamil"          # default diagnostics flavor for check/compile/eval/sim/test

[compile]
emit_testbench = true   # always emit _tb.v on compile

[translate]
to = "tanglish"         # default --to
order = "code"          # default --order
romanize_names = false  # default --romanize-names
names_map = "auto"      # "auto" = auto-load the sidecar; "off" = don't

[fmt]
to = "tamil"            # default fmt --to
strict = true           # default fmt --strict

[lib]
std = "./vendor/std"    # override the embedded standard library (mimz eject std)
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

`check`, `compile`, `eval`, `sim`, `test`, `lint`, and `repl` render
diagnostics in the flavor your file mostly uses, and `--lang` overrides:

```text
mimz check counter_tamil.mimz --lang tamil
mimz lint counter.mimz --lang tamil
```

(Localized error messages are rolling out catalog entry by catalog entry; codes
without a localized template fall back to clear English. The machine-readable
`--json` output always stays English.)

## In your editor: `mimz lsp`

`mimz lsp` is a Language Server: live red squiggles as you type (in the same
flavor as the CLI), plus hover, go-to-definition, and completion — hover a
signal to see its declared type, jump straight to where a name is defined
(even across files, for a cross-file module instantiation), and get
in-scope identifiers plus your file's majority-flavor keywords as you type.
Completion never mixes flavors: a Tamil-flavored file offers Tamil
keywords, never English ones. The VS Code client lives in
[`../../editors/vscode/`](../../editors/vscode/), which also provides syntax
highlighting for all three flavors.

## Before you commit (for contributors)

```text
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
npx prettier --write "**/*.md" && npx markdownlint-cli2
```

Curious how the compiler implements all this under the hood? See
[`../code/`](../code/) (maintainer docs) or
[`../source-guide/`](../source-guide/) (friendly code tour).

Next: [the cheat sheet](12-cheatsheet.md).
