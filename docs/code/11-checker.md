# 11 — The Checker (`src/checker/`)

The semantic safety stage, between parse and emit. **Landed across
2026-06-11/12 in four slices**: symbols/names/consts/reg-reset
(E00xx–E03xx), the width/type pass (E04xx — the exact-widths promise,
signed/bits separation, literal fitting), the driver pass (E05xx —
single-driver, output coverage, combinational-cycle DAG, `=` vs `<-`),
and the completion slice (E0302 instantiation completeness, E0601/E0602
match exhaustiveness, E0701 clock-domain ownership). The "deferred"
table below is the honest status of what remains.

## File layout

| File                 | Owns                                                                           |
| -------------------- | ------------------------------------------------------------------------------ |
| `mod.rs`             | `check()` entry, the `Checker` state, the `err()` plumbing                     |
| `symbols.rs`         | Pass 1 — project-wide tables (modules, enums, funcs) + E0001/E0002/E0801/E0802 |
| `consteval.rs`       | Pass 2 — file consts + the `eval()` engine for const positions                 |
| `names.rs`           | Pass 3 — module scopes, name resolution, structure rules, E0302                |
| `widths/mod.rs`      | Pass 4 — the `Ty` model, `Wcx`, config worklist, module walk                   |
| `widths/expr.rs`     | Pass 4 — bidirectional typing engine (check/infer, lvalues)                    |
| `widths/ops.rs`      | Pass 4 — operators, shifts, concat, the four builtins                          |
| `widths/insts.rs`    | Pass 4 — instantiation bindings + connection widths                            |
| `widths/patterns.rs` | Pass 4 — `match` patterns + exhaustiveness (E0601/E0602)                       |
| `funcs.rs`           | Pass (after 1) — call-graph cycle detection, E0805                             |
| `drivers.rs`         | Pass 5 — single-driver, coverage, comb-cycle (DAG), `=` vs `<-`                |
| `clocks.rs`          | Pass 6 — clock-domain ownership, cross-domain reads (E0701)                    |
| `tests.rs`           | Unit tests — one per error code, plus clean-pass cases                         |

Same module pattern as the parser (03): `mod.rs` owns the struct and the
diagnostic plumbing; each pass is an `impl` block in its own file behind
`pub(super)`. Pass 4 outgrew the ~600-line rule (07-decisions) and split
into its own directory module on 2026-06-12 — same pattern one level
down: `widths/mod.rs` owns the shared `Ty`/`Wcx` state, siblings hold
one concern each. Pass 3 stores each module's scope on the `Checker`
(`scopes`), and passes 4 and 5 resolve against those same tables instead
of rebuilding them (pass 6 works straight off the AST — domain coloring
needs only reg/wire/drive structure).

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
tombstone row here. Each code is exercised two ways: in-process by
`src/checker/tests.rs`, and **end-to-end** by a broken fixture under
`tests/fixtures/errors/` that the real binary must reject with this code
(`tests/errors.rs` — a completeness guard fails if any code lacks one).

| Code  | Meaning                                                                                   | Typical fix the help teaches                                    |
| ----- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| E0001 | duplicate module name (project-wide)                                                      | rename — module names are project-unique                        |
| E0002 | duplicate file-level enum name (project-wide)                                             | rename — enums travel with `import`                             |
| E0003 | name declared twice inside one module                                                     | rename; the message says what holds the name                    |
| E0004 | duplicate file-level `const`                                                              | rename within the file                                          |
| E0101 | unknown name in an expression                                                             | check spelling / declare it                                     |
| E0102 | unknown module (instantiation or test header)                                             | check spelling / add the missing `import`                       |
| E0103 | unknown enum, variant, or named type                                                      | lists the enum's real variants                                  |
| E0104 | reading a non-output of an instance (`inst.x`)                                            | lists the module's outputs; inputs connect at `let`             |
| E0105 | `.field` on something that has no fields                                                  | `.` is for `Enum.Variant` / `inst.output` only                  |
| E0106 | unknown parameter in instantiation or test header                                         | lists the module's parameters                                   |
| E0107 | bad connection port (unknown, or an output)                                               | outputs are read with `.`, not connected                        |
| E0108 | assigning to a non-signal (input, const, clock, …)                                        | only out ports, wires, regs are assignable                      |
| E0109 | `on rise(x)` where `x` is not a clock                                                     | declare `clock clk`                                             |
| E0201 | expression is not a compile-time constant                                                 | what IS allowed in const positions                              |
| E0202 | constant evaluation overflow (i128 range)                                                 | —                                                               |
| E0301 | module has regs but no `reset` declaration                                                | add `reset rst`                                                 |
| E0302 | instance input unconnected, or connected twice                                            | connect every input exactly once; clock/reset connect by name   |
| E0303 | declaration (port/`wire`/`reg`/`clock`/`reset`/`const`/`enum`/`on`) inside `repeat`       | declare once outside; `repeat` only generates hardware          |
| E0401 | assignment/connection width mismatch (`=`, `<-`, init, conns)                             | `extend`/`trunc`/slice; `+` into same width teaches `+%`        |
| E0402 | operand width mismatch (`+%` family, `& \| ^`, comparisons)                               | `extend` the narrow side                                        |
| E0403 | kind mixing: signed↔bits, enums as numbers, clock/reset as data                           | the visible casts `signed()`/`unsigned()`                       |
| E0404 | logical op / condition on a non-`bit`                                                     | compare (`x != 0`) or reduce (`\|x`)                            |
| E0405 | compile-time value does not fit, or has no width to adopt                                 | the value, the width, and the max that fits                     |
| E0406 | index/slice out of range, reversed bounds, base not indexable                             | indices `0..=N-1`; slices `[hi:lo]` msb first                   |
| E0407 | builtin/unary misuse (`extend` narrowing, `-` on bits, …)                                 | what the builtin is FOR; `0 -% x` for wrap-negate               |
| E0408 | `if`/`match` arms disagree on type/width                                                  | every arm becomes the same wire                                 |
| E0409 | pattern errors (match on signed, wrong enum, too-wide value)                              | what the scrutinee's type admits                                |
| E0410 | width expression invalid (zero, negative, absurd)                                         | hardware needs at least one bit                                 |
| E0501 | more than one driver (2nd drive, drive-to-wire, overlapping bit ranges)                   | one `=` per signal; `if`/`match` exprs choose; disjoint bits OK |
| E0502 | output never driven, or driven on only some bits                                          | drive it; names the first undriven bit                          |
| E0503 | reg assigned from zero or several `on` blocks, or memory written from several `on` blocks | exactly one `on` block owns each reg or memory                  |
| E0504 | combinational cycle (path shown, incl. through instances)                                 | every feedback loop passes through a `reg`                      |
| E0505 | wrong assignment kind: `<-` to wire/out, `=` to reg                                       | `<-` = registers in `on`; `=` = combinational                   |
| E0601 | `match` not exhaustive (names a missing value/variant)                                    | add the missing arms, or end with `_ =>`                        |
| E0602 | unreachable `match` arm (after `_`, or a duplicate value)                                 | move `_` last / delete the duplicate                            |
| E0701 | cross-clock-domain read, or a wire mixing two domains                                     | one domain per signal; `sync` (Phase 2) will allow crossings    |
| E0801 | duplicate user-defined function name (project-wide)                                       | rename — function names are project-unique                      |
| E0802 | function name collides with a builtin (`extend`, `trunc`, `min`, …)                       | choose a different name                                         |
| E0803 | wrong number of arguments in a `fn` call (expected N, got M)                              | pass exactly the number of arguments the function declares      |
| E0804 | function body width doesn't match the declared return type                                 | `extend`/`trunc`/slice the body, or fix the `->` type           |
| E0805 | recursive function call (direct or mutual cycle in the call graph)                        | replace recursion with fixed-size repetition or a `repeat` loop |

Numbering scheme:

- E00xx — structure/duplicates;
- E01xx — name resolution;
- E02xx — const evaluation;
- E03xx — module structure rules;
- E04xx — width/type rules;
- E05xx — drivers/cycles;
- E06xx — exhaustiveness;
- E07xx — clock domains;
- E08xx — user-defined functions.

(Lexer E10xx, parser E11xx, and loader E12xx codes live in
docs/code/06 — retrofit completed 2026-06-12.) Claim a block when a new
pass lands, and add the rows in the same commit.

## How exhaustiveness works (in pass 4)

`check_patterns` already holds the scrutinee's type and every validated
pattern, so coverage is counted in the same walk: enum matches must name
every variant, `bit`/`bits[N]` matches must cover all `2^N` values —
or end with `_`. Spec ruling (v0.2.3, 2026-06-12): **full enum coverage
needs no `_`** (the Rust rule), and a `_` AFTER full coverage is legal,
not unreachable — it is the documented defense against non-enum
encodings after a bit flip (the emitter's ternary chain makes the last
arm the Verilog default either way). E0602 fires only for arms after a
`_` arm and for duplicate values. Exhaustiveness is skipped when a
pattern already drew a type error (one mistake, one diagnostic), and
`wire`-driving `if` needs no checker rule — the parser already refuses
an expression `if` without `else`.

## How the clock pass works (pass 6)

Modules with fewer than two clocks skip instantly. Otherwise: each reg
is colored with its `on` block's clock; each wire/out gets the UNION of
domains its driving expressions reach (through wire chains, memoized;
comb cycles already died as E0504). A read inside `on rise(clkB)` whose
domain set contains any other clock is E0701, as is a wire whose own
set holds two domains. Instance outputs contribute no domain —
cross-instance tracking is a deferred row.

## How the driver pass works (pass 5)

One analysis per module (driver structure is parameter-independent; the
default binding supplies constant index values). Each drive records an
**extent** — the whole signal, a constant bit range, `Dynamic` (runtime
index — conflicts with everything), or `Unknown` (unevaluable with no
binding — never conflicts, so no false positives). Disjoint constant
ranges are legal: `repeat i: 0..8 { led[i] = ... }` is eight drivers for
eight different bits, and full coverage.

The cycle check builds one combinational graph per module: wires, outs,
ins, and **per-index instance-output pseudo-nodes** (`fa[0].cout` and
`fa[1].cout` are different nodes — merging them would call the legal
ripple-carry chain a loop). `inst.out` depends on whatever the
connection expressions of the child's relevant inputs read, where
"relevant" comes from the child's **combinational summary** (out → ins
reachability over the child's own graph, memoized). Registers break
paths — sequential assignment creates no edges, which is exactly why a
reg is the fix the E0504 help teaches.

## What const-eval accepts (and why the rest errors)

`consteval::eval` works on `i128` values: literals, named consts (file
consts top-to-bottom, then module consts), `repeat` variables, `+ - *`,
shifts, comparisons, `&& || !`, `if/else`. Deliberately NOT accepted
(E0201, each with its own explanation): signal names, wrapping operators
(`+%` has no meaning without a bit width), `match`, concat/index/slice,
builtins. Overflow is E0202, never a silent wrap — the checker holds
itself to the language's own honesty rule.

## Deferred to later slices (the honest list)

| Rule                                              | Blocked on / planned with                                                |
| ------------------------------------------------- | ------------------------------------------------------------------------ |
| `repeat` unrolling (elaboration)                  | widths/drivers check per-iteration; unrolling is emitter work            |
| Cross-INSTANCE clock-domain tracking              | pass 6 is module-local; instance outputs carry no domain                 |
| Cross-clock reads allowed via `sync`              | the Phase 2 multi-clock construct relaxes E0701 explicitly               |
| Instance-array output widths via the `repeat` var | read outside the loop falls back to param defaults                       |
| Defaultless-param module never instantiated       | internals skipped silently (passes 1–3 still ran)                        |
| Driver COVERAGE under non-default bindings        | upgrade path: reuse widths' per-instantiation config set                 |
| Recursive instantiation                           | comb summary comes back empty (no through-paths seen); no cycle invented |
| Unevaluable instance-array index in a read        | the comb edge is skipped (under-approximation; elaboration closes it)    |
| Test BODY checking (drives/`tick`/`expect`)       | simulator, Phase 1.5                                                     |
| E-codes on lexer/parser errors                    | retrofit pass, Phase 1 (E10xx/E11xx reserved)                            |
| Did-you-mean suggestions on E0101                 | nice-to-have; needs edit distance                                        |

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
