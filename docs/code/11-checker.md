# 11 — The Checker (`src/checker/`)

The semantic safety stage, between parse and emit. **First slice landed
2026-06-11**: symbol tables + duplicates, name resolution, const
evaluation, reg-requires-reset — with stable error codes. The heavier
rules (widths, single-driver, exhaustiveness, clock ownership) are
later slices; the "deferred" table below is the honest status.

## File layout

| File           | Owns                                                           |
| -------------- | -------------------------------------------------------------- |
| `mod.rs`       | `check()` entry, the `Checker` state, the `err()` plumbing     |
| `symbols.rs`   | Pass 1 — project-wide tables (modules, enums) + E0001/E0002    |
| `consteval.rs` | Pass 2 — file consts + the `eval()` engine for const positions |
| `names.rs`     | Pass 3 — module scopes, name resolution, structure rules       |
| `tests.rs`     | Unit tests — one per error code, plus clean-pass cases         |

Same module pattern as the parser (03): `mod.rs` owns the struct and the
diagnostic plumbing; each pass is an `impl` block in its own file behind
`pub(super)`.

## The contract

- `checker::check(&[ast::File]) -> Result<(), Vec<Diag>>` — runs after
  `load_project`, before the emitter, in BOTH `mimz check` and
  `mimz compile` (`check` loads imports too, so cross-file names resolve).
- Every checker diagnostic carries **a stable code** (`E0101`), **a file
  index** (multi-file rendering via `project::render_diags`), and **a
  help line**. None of the three is optional — `Checker::err()` makes it
  structurally impossible to skip them.
- The checker never stops early: all errors in one run, like every other
  stage (errors-as-values, docs/code/06).
- The emitter still builds its own `Project` table and keeps its own
  duplicate-module error — it stays usable standalone (in tests). The
  checker fires first in the CLI, so users see the coded error.

## Error-code catalog

Codes are a **stable contract**: tests assert on them, and future docs/
translations key off them. Never renumber; retire codes by leaving a
tombstone row here.

| Code  | Meaning                                            | Typical fix the help teaches                        |
| ----- | -------------------------------------------------- | --------------------------------------------------- |
| E0001 | duplicate module name (project-wide)               | rename — module names are project-unique            |
| E0002 | duplicate file-level enum name (project-wide)      | rename — enums travel with `import`                 |
| E0003 | name declared twice inside one module              | rename; the message says what holds the name        |
| E0004 | duplicate file-level `const`                       | rename within the file                              |
| E0101 | unknown name in an expression                      | check spelling / declare it                         |
| E0102 | unknown module (instantiation or test header)      | check spelling / add the missing `import`           |
| E0103 | unknown enum, variant, or named type               | lists the enum's real variants                      |
| E0104 | reading a non-output of an instance (`inst.x`)     | lists the module's outputs; inputs connect at `let` |
| E0105 | `.field` on something that has no fields           | `.` is for `Enum.Variant` / `inst.output` only      |
| E0106 | unknown parameter in instantiation or test header  | lists the module's parameters                       |
| E0107 | bad connection port (unknown, or an output)        | outputs are read with `.`, not connected            |
| E0108 | assigning to a non-signal (input, const, clock, …) | only out ports, wires, regs are assignable          |
| E0109 | `on rise(x)` where `x` is not a clock              | declare `clock clk`                                 |
| E0201 | expression is not a compile-time constant          | what IS allowed in const positions                  |
| E0202 | constant evaluation overflow (i128 range)          | —                                                   |
| E0301 | module has regs but no `reset` declaration         | add `reset rst`                                     |

Numbering scheme: E00xx structure/duplicates, E01xx name resolution,
E02xx const evaluation, E03xx module structure rules. Width rules will
take E04xx, drivers E05xx — claim a block when a new pass lands, and add
the rows in the same commit.

## What const-eval accepts (and why the rest errors)

`consteval::eval` works on `i128` values: literals, named consts (file
consts top-to-bottom, then module consts), `repeat` variables, `+ - *`,
shifts, comparisons, `&& || !`, `if/else`. Deliberately NOT accepted
(E0201, each with its own explanation): signal names, wrapping operators
(`+%` has no meaning without a bit width), `match`, concat/index/slice,
builtins. Overflow is E0202, never a silent wrap — the checker holds
itself to the language's own honesty rule.

## Deferred to later slices (the honest list)

| Rule                                                | Blocked on / planned with                        |
| --------------------------------------------------- | ------------------------------------------------ |
| Width checking (`+` grows, `+%` exact, assignments) | next slice — takes E04xx                         |
| Single-driver + combinational-cycle (DAG) check     | after widths — takes E05xx                       |
| `match` exhaustiveness / wire-`if` analysis         | with widths (needs value ranges)                 |
| Clock ownership (one clock per reg)                 | with the multi-clock design (Phase 2 `sync`)     |
| `repeat` unrolling (elaboration)                    | const-eval EXISTS now; unrolling is emitter work |
| Instantiation completeness (all inputs connected)   | next slice (names are checked today)             |
| Test BODY checking (drives/`tick`/`expect`)         | simulator, Phase 1.5                             |
| E-codes on lexer/parser errors                      | retrofit pass, Phase 1                           |
| Did-you-mean suggestions on E0101                   | nice-to-have; needs edit distance                |

## How to add a checker rule

1. Pick the pass file (or add a new sibling for a new pass — wire it in
   `mod.rs::check()`).
2. Claim the next code in the right E-block; add the catalog row above.
3. Write the error with `self.err(file, span, code, msg, help)` — the
   help line is the teaching moment, write it for the spec/01 persona.
4. Add a unit test in `tests.rs` asserting the CODE and a message
   substring (loose on wording, tight on contract).
5. If the rule rejects something an example does — fix the example or
   the rule; `every_example_compiles` decides who wins.
