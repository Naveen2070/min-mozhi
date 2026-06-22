# 01 — The Pipeline, End to End

> Before diving in: if you want a friendlier, file-by-file introduction,
> start with [`../source-guide/`](../source-guide/) instead. This page
> assumes you already know roughly what each module does.

What actually happens when you run:

```text
mimz compile examples/english/alu.mimz -o alu.v
```

## Step 0 — CLI dispatch (`src/main.rs`)

`main.rs` is **CLI-only** on purpose: clap parsing, subcommand dispatch
(`check`, `compile`, `fmt`, `translate`, `eval`, `sim`, `test`, `explain`,
`lsp`), config resolution, and exit codes. Each subcommand's handler lives in its own file
under `src/commands/` (`check.rs`, `compile.rs`, …) — `main.rs` only parses
args and calls them. No compilation logic lives there, so the pipeline can later
be reused by other frontends (LSP, web playground) without untangling it from
the terminal.

- `mimz check file` → lex + parse one file, print diagnostics, exit code.
- `mimz check file --tokens` → stop after the lexer, dump the token stream
  (the standard way to debug lexer issues).
- `mimz compile file [-o out]` → the full pipeline below.

## Step 1 — Load the project (`src/project.rs`)

`load_project(entry)` performs a worklist traversal:

1. `read_source` reads the file and **NFC-normalizes** it
   (`unicode-normalization`). Everything downstream — spans, keyword
   lookups, identifier comparisons — is defined over NFC text, so Tamil
   combining marks always compare consistently.
2. `parse_file` runs lexer + parser (steps 2–3) on that one file.
3. Each `import a.b` becomes a path **relative to the importing file**:
   `a/b.mimz`. A missing file is an error pointing at the `import` line.
   (`include` is an English alias of `import` — identical token by the
   time it reaches the parser, so this step never sees the difference.)
4. A `visited` set of canonicalized paths makes duplicate imports and
   cycles harmless — each file is parsed exactly once.

Output: `Vec<LoadedFile>` (path + source text + AST), entry file first.
The source text is kept because diagnostics render spans against it.

## Step 2 — Lex (`src/lexer/`)

Source text → `Vec<Token>`. Each token = kind + span + (for keywords)
which language flavor spelled it. Details in [`02-lexer.md`](02-lexer.md).

## Step 3 — Parse (`src/parser/`)

Tokens → `ast::File`. Recursive descent with statement-level error
recovery, so one bad line doesn't hide the next error. The pipeline uses
the **strict** `parse` (any error → no tree → no codegen); tools that want
a best-effort tree from broken input use `parse_recover` (returns the tree
with `Error` placeholder nodes). Details in [`03-parser.md`](03-parser.md).

## Step 4 — Check (`src/checker/`)

Between parse and emit, `checker::check` runs over all loaded files (in
BOTH `mimz check` and `mimz compile`). It performs:

- project-wide duplicates;
- name resolution (every name points at a declaration — signals,
  modules, enums/variants, instance ports, parameters);
- const evaluation;
- the reg-requires-reset rule;
- the **width/type pass** (exact widths, lossless growth, signed/bits
  separation, literal fitting — checked under concrete parameter
  bindings);
- the **driver pass** (single-driver per signal/bit, output coverage,
  reg-per-`on`-block, combinational-cycle DAG incl. through-instance
  paths, `=` vs `<-`);
- **match exhaustiveness** (every value/variant covered, unreachable
  arms rejected);
- **instantiation completeness** (every input connected exactly once);
- the **clock-domain pass** (per-reg clock ownership, cross-domain reads
  rejected until Phase 2's `sync`).

Every checker error carries a stable code (`E0101`) — catalog and
details in [`11-checker.md`](11-checker.md).

## Step 5 — Build the project symbol table (`src/emit_verilog/mod.rs`)

`Project::from_files` collects **every module and enum across all files**
into name → node maps. This is what lets `let u = Adder(...)` find
`Adder` no matter which imported file defines it. Duplicate module names
are rejected here (module names are project-unique, spec/02 section 1.5).

> This table lives in `emit_verilog` because the emitter is its only
> consumer; the checker (which landed 2026-06-11/12) does its own
> project-wide name resolution in `src/checker/symbols.rs` rather than
> sharing this one. See
> [`07-decisions-and-evolution.md`](07-decisions-and-evolution.md).

## Step 6 — Emit Verilog (`src/emit_verilog/`)

ASTs + symbol table → one Verilog-2005 source string, written to the
output path. Details in [`05-emit-verilog.md`](05-emit-verilog.md).

After the checker, `emit_verilog::transliterate` rewrites Tamil
identifiers to readable ASCII (விளக்கு → `villakku`), and the emitter
unrolls `repeat` at compile time. If `--emit-testbench` is passed, it
also extracts inline `test` blocks and emits a standard Verilog `_tb.v`. **Phase 1 is complete** — every plan
item in `docs/plan/phase-1-verilog-backend.md` is ticked. The Phase 1.8
grammar engine (the `thamizh-order` parser profile, all five clause flips)
and the Phase 1.5 simulator (`mimz sim` / `mimz test`, full parity) have
since landed too; the next pipeline work is the Phase 2 IR.

## Error flow

Every stage returns `Result<T, Vec<Diag>>`:

```text
lex      → Err(all lex errors)        — keeps scanning after an error
parse    → Err(all parse errors)      — recovers at statement boundaries
check    → Err(all checker errors)    — all passes run, E-coded
from_files → Err(duplicate modules)
emit     → Err(all emit errors)       — keeps emitting other modules
```

The CLI renders whichever `Vec<Diag>` it receives with `diag::render`
(caret output) and exits non-zero. No stage prints anything itself —
see [`06-diagnostics.md`](06-diagnostics.md).
