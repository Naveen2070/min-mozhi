# How the Compiler Works — A Beginner's Tour

This page explains, slowly and in plain words, what happens when you run
the `mimz` compiler — which files in `src/` do the work, in what order,
and what the data looks like at each step. It follows ONE real example
(`examples/english/counter.mimz`) all the way from text to Verilog.

If you only read one documentation page to understand this project,
read this one. The deeper per-module pages live in
[`docs/code/`](code/README.md) — links at the bottom.

## The one-sentence answer

A compiler is a program that reads text in one language and writes the same
meaning in another. `mimz` reads Min-Mozhi text and produces either **Verilog**
(to synthesize) or a **waveform** (to simulate). It shares one **front end** —
load, lex, parse, check — then takes one of two **back ends**:

```text
 your .mimz file
       |
       v
  [0] LOAD       src/project.rs        read the file, resolve its imports
       |
       v
  [1] LEX        src/lexer/            characters  ->  tokens (words)
       |
       v
  [2] PARSE      src/parser/           tokens      ->  AST (a tree)
       |
       v
  [3] CHECK      src/checker/          is the tree CORRECT and SAFE?
       |
       +-----------------------------+
       v                             v
  [4a] EMIT                      [4b] SIMULATE
  src/emit_verilog/              src/sim/
  tree -> Verilog text           tree -> elaborate -> run -> waveform
       |                             |
       v                             v
 your .v file                   VCD waveform / pass-fail tests
```

`mimz compile` takes the left branch; `mimz sim`, `mimz eval`, and `mimz test` take the right.
Each stage only talks to its neighbours — the lexer never sees the tree, the
emitter never sees raw text — which is why the code is split one folder per
stage.

> The example below follows the **compile** branch end to end; a short
> [Station 5 — Simulate](#station-5--simulate-srcsim) section at the end picks up
> the same checked tree and runs it instead.

## The example we will follow

`examples/english/counter.mimz` — a counter that adds 1 on every clock
tick:

```text
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

Compile it with:

```text
mimz compile examples/english/counter.mimz
```

The sections below trace what happens, station by station.

## Where it all starts: `src/main.rs` (and `src/lib.rs`)

The compiler itself is a **library** — `src/lib.rs` lists its modules
(the pipeline stages plus shared tools like `translate`, `sim`, and
`config`).

`main.rs` is the thin front door over it:

- it reads the command line (`check`, `compile`, `sim`, `test`, `eval`, `translate`, `fmt`, `explain`, `eject`, `lsp`, …);
- it dispatches to a per-subcommand handler in `src/commands/`, which calls
  the stations in order;
- it renders whatever comes back (human carets, or one JSON array with
  `--json`).

The `compile` handler (`src/commands/compile.rs`) is literally the pipeline
written out:

```text
load_project(path)            // station 0
checker::check(&asts)         // stations 1+2 ran inside load; this is 3
transliterate(&mut asts)      // Tamil names -> ASCII (விளக்கு -> villakku)
emit_verilog::emit(...)       // station 4
std::fs::write(out_path, ...) // save the .v file
```

`main.rs` contains NO language logic. If you ever wonder "what is the
true order of the stages?", read the `compile` handler in
`src/commands/compile.rs` — it cannot lie, it IS the order. (The same
library also powers `mimz lsp`, the language server behind the VS Code
squiggles, and the simulator `mimz sim`/`mimz eval`/`mimz test` — same
front-end stations, then a different back end.)

## Station 0 — Load (`src/project.rs`)

**Job: turn a file path into source text, and pull in every imported
file too.**

- `read_source()` reads the file and normalizes the text to NFC. (Tamil
  letters can be encoded in more than one byte sequence; NFC makes the
  same-looking text always compare equal. English-only files are
  untouched.)
- `load_project()` keeps a to-do list of files. Starting with your entry
  file, for each file it: parses it (stations 1 and 2 happen here, per
  file), looks at its `import` lines, then turns `import lib.full_adder`
  into the path `lib/full_adder.mimz` next to the importing file and adds
  that file to the to-do list. A "visited" set stops infinite loops if two
  files import each other.
- When an import resolves to a **stdlib namespace** (`std`, `nuulagam`, or
  `நூலகம்`), the project loader calls `stdlib::resolve()` instead of reading
  from disk — the five stdlib modules (`seg7`, `pwm`, `fifo`, `uart_tx`,
  `debouncer`) are baked into the binary at compile time via `include_str!`.
  This is why `mimz` works in WASM and in bare-binary installs with no data
  files. `mimz eject std` extracts them to disk when you need to vendor or
  customise them.
- The result is a `Vec<LoadedFile>` — every file's path, its source
  text, and its parsed tree, with your entry file first.

The counter example has no imports, so the result is a single `LoadedFile`.

## Station 1 — Lex (`src/lexer/`)

**Job: chop a stream of characters into a stream of _tokens_ — the
"words" of the language.**

The lexer does not understand programs. It only recognizes spellings.
Take one line from the counter:

```text
value <- value +% 1
```

The lexer walks left to right and produces:

```text
Ident("value")   LArrow   Ident("value")   PlusPct   Int { value: 1, raw: "1" }   Newline
```

Five details stand out:

- `<-` became ONE token (`LArrow`), not `<` then `-`. The lexer always
  prefers the longest match — that is also how it tells `<-` from `<=`
  and `<<` (locked by the `larrow_vs_comparison` test).
- `value` became `Ident(...)` — an identifier, a name YOU chose.
- `module` (on line 1) became `Kw(Module)` — a keyword, a word the
  LANGUAGE owns. How does the lexer know `module` is a keyword but
  `value` is not? It looks the word up in a table — see "the trilingual
  trick" below.
- The line ends with a `Newline` token. Min-Mozhi uses newlines as
  statement ends (like Go), so newlines are real tokens — except after
  an operator, where the line obviously continues, and the lexer drops
  them.
- Every token carries a `Span` — "I came from bytes 217..222 of the
  source". Spans ride along through every later station so that an
  error found at station 3 can still point at the exact source
  characters. `src/span.rs` is that tiny type.

Files in this folder:

| File                | Does what                                                          |
| ------------------- | ------------------------------------------------------------------ |
| `lexer/mod.rs`      | the character-walking loop itself (`lex()`)                        |
| `lexer/token.rs`    | the `Token` and `TokKind` types — the full vocabulary, listed once |
| `lexer/keywords.rs` | loads `lang/keywords.toml` into a lookup table at startup          |
| `lexer/tests.rs`    | unit tests for tricky cases (`<-` vs `<=`, Tamil identifiers, …)   |

### The trilingual trick (`lang/keywords.toml`)

The repo root has `lang/keywords.toml` — a plain data file:

```toml
[keywords.module]
en = "module"
tanglish = "thoguthi"
tamil = "தொகுதி"
```

At startup, `lexer/keywords.rs` reads this into one big map:
`"module" -> Kw::Module`, `"thoguthi" -> Kw::Module`,
`"தொகுதி" -> Kw::Module`. All three spellings become the **identical
token**. From station 2 onward the compiler physically cannot tell
which language you wrote — that is the whole design. Adding or fixing a
spelling is a data edit, not a code edit. (`include` works the same
way: it is listed as an `en_aliases` entry of `import`, so it lexes to
`Kw::Import`.)

### Natural Tamil word order (the Grammar Engine)

Trilingual keywords are only half the language story. A file that opens with
`syntax thamizh` may also be written in **natural Tamil (subject-object-verb)
word order** — `on rise(clk)` becomes `rise(clk) on`, `if c { }` becomes
`c if { }`. This is the **Grammar Engine** (`spec/04`, Phase 1.8): a parser
_profile_, not a second grammar. It rearranges the very same rules, so station 2
produces the **identical AST** whichever order you wrote — every station after
the parser is none the wiser. Code-order is the default; no directive needed.

## Station 2 — Parse (`src/parser/`)

**Job: turn the flat list of tokens into a _tree_ that mirrors the
structure of the program. The tree is called the AST — Abstract Syntax
Tree.**

"Abstract" just means "details like spelling, spacing and comments are
gone — only structure remains."

For the counter, the tree looks like this (sketch, not exact Rust):

```text
File
└── Module "Counter"
    ├── param WIDTH: int = 8
    ├── clock clk
    ├── reset rst
    ├── out count: bits[WIDTH]
    ├── reg value: bits[WIDTH], reset value 0
    ├── on rise(clk)
    │   └── value <- (value +% 1)
    └── count = value
```

Note `value +% 1` is itself a small tree (an operator with two
children). That nesting is what makes precedence real: `x & 1 == 0`
parses as `(x & 1) == 0` because the parser builds the `&` node first
(Rust's precedence rules — a deliberate safety choice).

The parser is "recursive descent": one function per grammar rule, each
function eats the tokens its rule allows and calls other rule-functions
for the parts inside. Every parser function carries its grammar rule as
a doc comment, so the code and the spec read side by side.

| File              | Does what                                                           |
| ----------------- | ------------------------------------------------------------------- |
| `parser/mod.rs`   | the `Parser` state (current position, collected errors), `parse()`  |
| `parser/items/`   | big structures: modules, ports, regs, `on` blocks, imports, tests   |
| `parser/expr.rs`  | expressions: operators, precedence, calls, `if`/`match` expressions |
| `parser/tests.rs` | unit tests for tree shapes and teaching errors                      |

The tree types themselves live in `src/ast/` (`ast/mod.rs` for
file/module/item shapes, `ast/expr.rs` for expression shapes). The AST
files contain almost no logic — they are the shared "shape vocabulary"
that parser, checker, and emitter all agree on.

## Station 3 — Check (`src/checker/`)

**Job: the tree has the right SHAPE — but does it MAKE SENSE? Find
every mistake, explain each one, never stop at the first.**

The parser would happily accept `count = valu` — it is a perfectly
shaped assignment. Only the checker knows there is nothing named
`valu`. It runs six passes, in order, each in its own file (or folder):

1. **`symbols.rs`** — walk every file, collect all module names and
   enum names into project-wide tables. Two modules with one name?
   Error E0001.
2. **`consteval.rs`** — compute every `const` to an actual number, top
   to bottom, so later passes can use the values (for example as
   `repeat` bounds). Overflow is an error, never a silent wrap — the
   checker obeys the language's own honesty rule.
3. **`names.rs`** — for every module: build a scope (every declared
   name and what it is — port, wire, reg, clock, const, instance…),
   then walk every expression and assignment and ask "does this name
   exist, and is this use legal?" Assigning to an input, clocking on a
   non-clock, leaving an instance input unconnected — all caught here.
   It also enforces structure rules like "a module with regs must
   declare a `reset`" (E0301).
4. **`widths/`** — the exact-widths promise: every assignment,
   operand, and connection has the width its context needs; `signed`
   and `bits` never mix silently; a `match` must cover every value.
5. **`drivers.rs`** — every wire and output driven exactly once, every
   reg owned by exactly one `on` block, and no combinational loops
   (the wire graph must be a DAG).
6. **`clocks.rs`** — every reg belongs to one clock, and nothing reads
   across clock domains (that needs the explicit `sync` of Phase 2).

When something is wrong, the checker never panics and never stops
early — it collects diagnostics and keeps checking, so you see ALL your
mistakes in one run. Change `count = value` to `count = valu` and you
get:

```text
error[E0101]: unknown name `valu`
  --> examples/english/counter.mimz:17:11
   |
 17|   count = valu
   |           ^^^^
   = help: nothing with this name is declared in this module — check the
     spelling, or declare it as a port, wire, reg, or const
```

Three parts, all mandatory by design: a **stable code** (`E0101` —
tests and future translations key off it), the **exact source
location** (that's the `Span` riding along since station 1), and a
**help line** that teaches. The full code catalog is in
[`docs/code/11-checker.md`](code/11-checker.md).

Only when the checker finds zero errors does the pipeline continue.

## Station 4 — Emit (`src/emit_verilog/`)

**Job: walk the (now trusted) tree and print Verilog text.**

The emitter is a tree-to-text printer. It first builds a small project
table (`Project::from_files` — which modules exist, their ports), then
walks each module and writes Verilog line by line:

- `module Counter(WIDTH: int = 8)` → a Verilog module with a
  `parameter`
- `clock clk` / `reset rst` → plain `input wire` ports (the TYPES were
  for the checker's benefit; Verilog has no clock type)
- the `on rise(clk)` block → an `always @(posedge clk)` block, with the
  reset branch **generated for you** from the reg's `= 0` reset value
- `value <- value +% 1` → `value <= (value + 1);` — wrapping is what
  plain `+` already does in Verilog at fixed width; the `+%` spelling
  exists so the WRITER says it on purpose

The actual output for the counter:

```text
// Generated by mimz 0.1.0 (edition wingless-butterfly-2026-1) — Min-Mozhi (மின்மொழி). Do not edit.

module Counter #(
    parameter WIDTH = 8
) (
    input wire clk,
    input wire rst,
    output wire [(WIDTH)-1:0] count
);
    reg [(WIDTH)-1:0] value;
    assign count = value;
    always @(posedge clk) begin
        if (rst) begin
            value <= 0;
        end else begin
            value <= (value + 1);
        end
    end
endmodule
```

Compare it with the source — every line of output is traceable to a
line of input. Keeping the Verilog readable like this is a project
rule (the prior-art doc explains why, using Chisel as the cautionary
tale).

| File                       | Does what                                                 |
| -------------------------- | --------------------------------------------------------- |
| `emit_verilog/mod.rs`      | `emit()` entry, the project table, output assembly        |
| `emit_verilog/module.rs`   | one module → one Verilog module (ports, always blocks)    |
| `emit_verilog/expr.rs`     | one expression tree → one Verilog expression string       |
| `emit_verilog/translit.rs` | Tamil identifiers → readable ASCII (விளக்கு → `villakku`) |

Two emitter tricks worth knowing even on day one: `repeat i: 0..4` is
**unrolled** at compile time (four copies of the hardware, the loop
variable folded into every index — there is no loop in the Verilog),
and `signed[N]` signals are declared `wire signed`, so two's-complement
math behaves exactly as the spec promises.

This station also lowers the RTL-parity constructs the language has since grown:
`on fall(clk)` becomes a `negedge` block, `async reset` widens the sensitivity
list to `@(posedge clk or posedge rst)`, a `mem` becomes a Verilog reg array
(`reg [W-1:0] m [0:DEPTH-1]`) with a power-on `initial` seed, and `{N{x}}`
becomes Verilog replication. The full per-construct detail is in
[`docs/code/05-emit-verilog.md`](code/05-emit-verilog.md).

The same source in `examples/tanglish/counter.mimz` or
`examples/tamil/counter.mimz` produces **byte-identical** output —
remember, the flavors stopped existing at station 1. A CI test
(`all_four_flavors_compile_to_identical_verilog`) asserts this for
every example in the repo.

## Station 5 — Simulate (`src/sim/`)

`mimz compile` stops at station 4. But the same checked tree can take the OTHER
back end: `mimz sim` (write a waveform) and `mimz test` (run `tick`/`expect`
checks) hand the tree to `src/sim/` instead of the Verilog emitter. Three steps:

1. **Elaborate** (`elaborate.rs`) — flatten the design into a plain list of
   signals and registers a machine can step: module instances are inlined
   (`inst.port` becomes a real wire), `repeat` is unrolled, enums become integer
   codes, memories get a cell store. The result is a `Design` that matches, gate
   for gate, what the emitter would have written.
2. **Run** (`run.rs` + `kernel.rs`) — drive the design under a default stimulus
   (reset asserted the first cycle, inputs held, the clock toggled) and step it
   cycle by cycle. The kernel is **edge-aware**: within one period it samples on
   the rising edge, then the falling edge — so an `on fall` register updates at
   the right moment. A clockless design settles one frame per input vector
   instead (`comb.rs`). Each step records every signal into a `Timeline`.
3. **Write** (`vcd.rs` / `trace.rs`) — turn the `Timeline` into a standard
   IEEE-1364 **VCD** waveform (any viewer opens it) or a per-cycle text table
   (`--trace`).

The expression math (`+`, `match`, slicing, replication, …) is evaluated by
**one shared file, `value.rs`** — the same evaluator behind `mimz eval` — so the
simulator and the generated Verilog cannot drift apart in what an expression
means.

Why trust it? A three-way differential
(`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) runs every example
through our kernel, reconstructs the values from the VCD we wrote, AND runs the
same design through **Icarus Verilog** — all three must agree bit-for-bit, every
cycle. The moment our simulator disagreed with real Verilog, that test goes red.

| File                      | Does what                                                                    |
| ------------------------- | ---------------------------------------------------------------------------- |
| `sim/elaborate.rs`        | flatten the AST into a steppable `Design` (instances, `repeat`, enums, mems) |
| `sim/kernel.rs`           | the edge-aware cycle engine (rise → sample → fall)                           |
| `sim/comb.rs`             | settle a clockless (combinational) design, one frame per vector              |
| `sim/value.rs`            | the shared expression evaluator (also behind `mimz eval`)                    |
| `sim/vcd.rs` / `trace.rs` | `Timeline` → VCD waveform / per-cycle table                                  |

The simulator's design notes live in
[`docs/code/13-tooling.md`](code/13-tooling.md) (the `sim` section).

## The two files every station uses

| File          | Does what                                                                                                                                             |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/span.rs` | `Span` — "bytes 217..222 of the source". Created at station 1, carried through every tree node, spent at error-render time to draw the `^^^^` carets. |
| `src/diag.rs` | `Diag` — one error: span, message, optional code, optional help, optional file index. Plus `render()`, which draws the error block you saw above.     |

Every station returns `Result<_, Vec<Diag>>` — errors are ordinary
values, never exceptions, never process aborts. `main.rs` renders
whatever comes back and sets the exit code.

## How the tests keep this picture true

`cargo test` runs several layers (**476 tests** today; the full ledger,
per-binary breakdown, and "what a failure means" notes are in
[`docs/code/10-test-map.md`](code/10-test-map.md)):

- **Unit tests** live next to the code they test (`lexer/tests.rs`,
  `parser/tests.rs`, `checker/tests.rs`) — token shapes, tree shapes,
  one test per checker error code.
- **Integration tests** (`tests/examples.rs`) run the real pipeline
  over every file in `examples/` — every example must check clean,
  compile, and match its three sibling flavors byte-for-byte.
- **The Icarus differential** (`tests/icarus.rs`) runs every example through our
  own simulator AND through Icarus Verilog under the same stimulus and asserts
  they agree bit-for-bit — the independent judge that keeps station 5 honest.
- **Docs-sync tests** (`tests/docs_sync.rs`) mechanically verify that
  the docs' structural claims (module lists, file tables) match the
  real `src/` tree — so this very page's neighbours can't silently rot.
- **Grammar-sync tests** (`tests/grammar_sync.rs`) verify the VS Code
  extension's grammar lists every spelling in `lang/keywords.toml`.

## "Where do I look when…" cheat sheet

| You want to…                                | Look in                                              |
| ------------------------------------------- | ---------------------------------------------------- |
| change/add a keyword spelling               | `lang/keywords.toml` (data only — no code)           |
| see why `<-` lexes as one token             | `src/lexer/mod.rs` + `lexer/tests.rs`                |
| change what a construct LOOKS like (syntax) | `src/parser/items/` or `parser/expr.rs`              |
| change what the tree STORES                 | `src/ast/` (then fix parser + checker + emitter)     |
| add a new error / safety rule               | `src/checker/` — recipe in `docs/code/11-checker.md` |
| change the generated Verilog                | `src/emit_verilog/module.rs` or `expr.rs`            |
| change how a design is simulated            | `src/sim/` (elaborate / kernel / value)              |
| change how errors are printed               | `src/diag.rs` (`render`)                             |
| change import resolution / file loading     | `src/project.rs`                                     |
| add a CLI flag or subcommand                | `src/main.rs` (clap) + a handler in `src/commands/`  |
| vendor the stdlib modules to disk           | `mimz eject std [--to <dir>] [--flavor tamil]`       |
| see errors as squiggles in VS Code          | `editors/vscode` (the extension runs `mimz lsp`)     |
| debug "what tokens does this file produce?" | `mimz check file.mimz --tokens`                      |

## Going deeper

Each station has a full maintainer page with the real function names
and the design decisions behind them:

| Station                                       | Deep-dive page                                                     |
| --------------------------------------------- | ------------------------------------------------------------------ |
| overview                                      | [`code/01-pipeline.md`](code/01-pipeline.md)                       |
| lexer                                         | [`code/02-lexer.md`](code/02-lexer.md)                             |
| parser                                        | [`code/03-parser.md`](code/03-parser.md)                           |
| AST                                           | [`code/04-ast.md`](code/04-ast.md)                                 |
| emitter                                       | [`code/05-emit-verilog.md`](code/05-emit-verilog.md)               |
| diagnostics                                   | [`code/06-diagnostics.md`](code/06-diagnostics.md)                 |
| checker                                       | [`code/11-checker.md`](code/11-checker.md)                         |
| simulator, `translate`/`fmt`, version/edition | [`code/13-tooling.md`](code/13-tooling.md)                         |
| one real run, token offsets and all           | [`code/09-walkthrough-counter.md`](code/09-walkthrough-counter.md) |
| in-depth intro (every Rust file, friendly)    | [`source-guide/README.md`](source-guide/README.md)                 |
