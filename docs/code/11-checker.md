# 11 — The Checker (`src/checker/`)

The semantic safety stage, between parse and emit. **First slice landed
2026-06-11** (symbol tables + duplicates, name resolution, const
evaluation, reg-requires-reset); the **width/type slice landed the same
day** (E04xx — the exact-widths promise, signed/bits separation,
literal fitting). Single-driver, exhaustiveness, and clock ownership
are later slices; the "deferred" table below is the honest status.

## File layout

| File           | Owns                                                           |
| -------------- | -------------------------------------------------------------- |
| `mod.rs`       | `check()` entry, the `Checker` state, the `err()` plumbing     |
| `symbols.rs`   | Pass 1 — project-wide tables (modules, enums) + E0001/E0002    |
| `consteval.rs` | Pass 2 — file consts + the `eval()` engine for const positions |
| `names.rs`     | Pass 3 — module scopes, name resolution, structure rules       |
| `widths.rs`    | Pass 4 — width/type rules under concrete parameter bindings    |
| `tests.rs`     | Unit tests — one per error code, plus clean-pass cases         |

Same module pattern as the parser (03): `mod.rs` owns the struct and the
diagnostic plumbing; each pass is an `impl` block in its own file behind
`pub(super)`. Pass 3 stores each module's scope on the `Checker`
(`scopes`), and pass 4 resolves against those same tables instead of
rebuilding them.

## The contract

- `checker::check(&[ast::File]) -> Result<(), Vec<Diag>>` — runs after
  `load_project`, before the emitter, in BOTH `mimz check` and
  `mimz compile` (`check` loads imports too, so cross-file names resolve).
- Every checker diagnostic carries **a stable code** (`E0101`), **a file
  index** (multi-file rendering via `project::render_diags`), and **a
  help line**. None of the three is optional — `Checker::err()` makes it
  structurally impossible to skip them.
- The checker never stops early: all errors in one run, like every other
  stage (errors-as-values, docs/code/06). The width pass adds the
  anti-cascade rule: a sub-expression that already errored types as
  `Unknown`, which absorbs every operation silently — one mistake, one
  diagnostic.
- The emitter still builds its own `Project` table and keeps its own
  duplicate-module error — it stays usable standalone (in tests). The
  checker fires first in the CLI, so users see the coded error.

## How the width pass handles parameters (no symbolic algebra)

`bits[WIDTH]` cannot be checked symbolically, so every module is checked
under a **concrete parameter binding**:

- Seed: every module whose params all have defaults is checked once,
  bound to those defaults (defaults may use earlier params, left to
  right).
- Every instantiation found while walking re-evaluates the CHILD's port
  types under the instance's actual arguments (explicit args evaluate in
  the parent's env; omitted ones take their defaults) and checks each
  connection — the checker-side mirror of the emitter's `width_subst`.
- Each distinct `(module, binding)` configuration is then checked once
  (memoized; capped at 1000 configurations to terminate pathological
  recursive instantiation). A module whose params lack defaults is
  therefore checked exactly as instantiated; if it is never
  instantiated, its internals are skipped (passes 1–3 still ran).

Compile-time integers (literals, consts, params, `repeat` vars) are
**polymorphic** (`Ty::CtInt`): they adapt to any sized context they fit
(spec/02 section 1.8) — `value +% 1` works because `1` takes `value`'s
width, and `cnt == LIMIT` works because the const takes `cnt`'s. A value
that does not fit is E0405, never a silent wrap.

## Error-code catalog

Codes are a **stable contract**: tests assert on them, and future docs/
translations key off them. Never renumber; retire codes by leaving a
tombstone row here.

| Code  | Meaning                                                         | Typical fix the help teaches                             |
| ----- | --------------------------------------------------------------- | -------------------------------------------------------- |
| E0001 | duplicate module name (project-wide)                            | rename — module names are project-unique                 |
| E0002 | duplicate file-level enum name (project-wide)                   | rename — enums travel with `import`                      |
| E0003 | name declared twice inside one module                           | rename; the message says what holds the name             |
| E0004 | duplicate file-level `const`                                    | rename within the file                                   |
| E0101 | unknown name in an expression                                   | check spelling / declare it                              |
| E0102 | unknown module (instantiation or test header)                   | check spelling / add the missing `import`                |
| E0103 | unknown enum, variant, or named type                            | lists the enum's real variants                           |
| E0104 | reading a non-output of an instance (`inst.x`)                  | lists the module's outputs; inputs connect at `let`      |
| E0105 | `.field` on something that has no fields                        | `.` is for `Enum.Variant` / `inst.output` only           |
| E0106 | unknown parameter in instantiation or test header               | lists the module's parameters                            |
| E0107 | bad connection port (unknown, or an output)                     | outputs are read with `.`, not connected                 |
| E0108 | assigning to a non-signal (input, const, clock, …)              | only out ports, wires, regs are assignable               |
| E0109 | `on rise(x)` where `x` is not a clock                           | declare `clock clk`                                      |
| E0201 | expression is not a compile-time constant                       | what IS allowed in const positions                       |
| E0202 | constant evaluation overflow (i128 range)                       | —                                                        |
| E0301 | module has regs but no `reset` declaration                      | add `reset rst`                                          |
| E0401 | assignment/connection width mismatch (`=`, `<-`, init, conns)   | `extend`/`trunc`/slice; `+` into same width teaches `+%` |
| E0402 | operand width mismatch (`+%` family, `& \| ^`, comparisons)     | `extend` the narrow side                                 |
| E0403 | kind mixing: signed↔bits, enums as numbers, clock/reset as data | the visible casts `signed()`/`unsigned()`                |
| E0404 | logical op / condition on a non-`bit`                           | compare (`x != 0`) or reduce (`\|x`)                     |
| E0405 | compile-time value does not fit, or has no width to adopt       | the value, the width, and the max that fits              |
| E0406 | index/slice out of range, reversed bounds, base not indexable   | indices `0..=N-1`; slices `[hi:lo]` msb first            |
| E0407 | builtin/unary misuse (`extend` narrowing, `-` on bits, …)       | what the builtin is FOR; `0 -% x` for wrap-negate        |
| E0408 | `if`/`match` arms disagree on type/width                        | every arm becomes the same wire                          |
| E0409 | pattern errors (match on signed, wrong enum, too-wide value)    | what the scrutinee's type admits                         |
| E0410 | width expression invalid (zero, negative, absurd)               | hardware needs at least one bit                          |

Numbering scheme: E00xx structure/duplicates, E01xx name resolution,
E02xx const evaluation, E03xx module structure rules, E04xx width/type
rules. Drivers take E05xx — claim a block when a new pass lands, and add
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

| Rule                                              | Blocked on / planned with                             |
| ------------------------------------------------- | ----------------------------------------------------- |
| Single-driver + combinational-cycle (DAG) check   | next slice — takes E05xx                              |
| `match` exhaustiveness / wire-`if` analysis       | widths exist now; needs value-set analysis            |
| Clock ownership (one clock per reg)               | with the multi-clock design (Phase 2 `sync`)          |
| `repeat` unrolling (elaboration)                  | widths check per-iteration; unrolling is emitter work |
| Instantiation completeness (all inputs connected) | next slice (names + widths are checked today)         |
| Instance-array output widths via the `repeat` var | read outside the loop falls back to param defaults    |
| Defaultless-param module never instantiated       | internals skipped silently (passes 1–3 still ran)     |
| Test BODY checking (drives/`tick`/`expect`)       | simulator, Phase 1.5                                  |
| E-codes on lexer/parser errors                    | retrofit pass, Phase 1                                |
| Did-you-mean suggestions on E0101                 | nice-to-have; needs edit distance                     |

## How to add a checker rule

1. Pick the pass file (or add a new sibling for a new pass — wire it in
   `mod.rs::check()`).
2. Claim the next code in the right E-block; add the catalog row above.
3. Write the error with `self.err(file, span, code, msg, help)` — the
   help line is the teaching moment, write it for the spec/01 persona.
4. Add a unit test in `tests.rs` asserting the CODE and a message
   substring (loose on wording, tight on contract).
5. If the rule rejects something an example does — fix the example or
   the rule; `every_example_compiles` decides who wins. (The width slice
   did exactly this: `shift_register.mimz` now writes
   `extend(din, WIDTH)` because the rule won.)
