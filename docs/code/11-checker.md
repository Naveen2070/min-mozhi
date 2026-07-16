# 11 — The Checker (`crates/mimz-core/src/checker/`)

The semantic safety stage, between parse and emit. **Landed across
2026-06-11/12 in four slices**: symbols/names/consts/reg-reset
(E00xx–E03xx), the width/type pass (E04xx — the exact-widths promise,
signed/bits separation, literal fitting), the driver pass (E05xx —
single-driver, output coverage, combinational-cycle DAG, `=` vs `<-`),
and the completion slice (E0302 instantiation completeness, E0601/E0602
match exhaustiveness, E0701 clock-domain ownership). The "deferred"
table below is the honest status of what remains.

## File layout

| File                 | Owns                                                                                            |
| -------------------- | ----------------------------------------------------------------------------------------------- |
| `mod.rs`             | `check()` entry, the `Checker` state, the `err()` plumbing                                      |
| `symbols.rs`         | Pass 1 — per-file module/enum/bundle tables, project-wide funcs + E0001/E0002/E0801/E0802/E0909 |
| `funcs.rs`           | Pass 2 — call-graph cycle detection (E0805), unreachable-after-return (E0812)                   |
| `consteval.rs`       | Pass 3 — file consts + the `eval()` engine for const positions                                  |
| `names.rs`           | Pass 4 — module scopes, name resolution, structure rules, E0302                                 |
| `extern_module.rs`   | `extern module` port-type validation — scalar-only ports (E1302)                                |
| `widths/mod.rs`      | Pass 5 — the `Ty` model, `Wcx`, config worklist, module walk                                    |
| `widths/expr.rs`     | Pass 5 — bidirectional typing engine (check/infer, lvalues)                                     |
| `widths/ops.rs`      | Pass 5 — operators, shifts, concat, the four builtins                                           |
| `widths/insts.rs`    | Pass 5 — instantiation bindings + connection widths                                             |
| `widths/patterns.rs` | Pass 5 — `match` patterns + exhaustiveness (E0601/E0602)                                        |
| `drivers.rs`         | Pass 6 — single-driver, coverage, comb-cycle (DAG), `=` vs `<-`                                 |
| `clocks.rs`          | Pass 7 — clock-domain ownership, cross-domain reads (E0701)                                     |
| `tests.rs`           | Unit tests — one per error code, plus clean-pass cases                                          |

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
`crates/mimz-core/src/checker/tests.rs`, and **end-to-end** by a broken fixture under
`tests/fixtures/errors/` that the real binary must reject with this code
(`tests/errors.rs` — a completeness guard fails if any code lacks one).

| Code  | Meaning                                                                                                                          | Typical fix the help teaches                                                                                                                              |
| ----- | -------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| E0001 | duplicate module name (per-file)                                                                                                 | rename — module names are unique within one file; a different file may reuse the name, qualify by import path if it becomes ambiguous (spec/02 §1.5b)     |
| E0002 | duplicate file-level enum name (per-file)                                                                                        | rename — enum names are unique within one file; a different file may reuse the name, qualify by import path if it becomes ambiguous (spec/02 §1.5b)       |
| E0003 | name declared twice inside one module                                                                                            | rename; the message says what holds the name                                                                                                              |
| E0004 | duplicate file-level `const`                                                                                                     | rename within the file                                                                                                                                    |
| E0101 | unknown name in an expression                                                                                                    | check spelling / declare it                                                                                                                               |
| E0102 | unknown module (instantiation or test header)                                                                                    | check spelling / add the missing `import`                                                                                                                 |
| E0103 | unknown enum, variant, or named type                                                                                             | lists the enum's real variants                                                                                                                            |
| E0104 | reading a non-output of an instance (`inst.x`)                                                                                   | lists the module's outputs; inputs connect at `let`                                                                                                       |
| E0105 | `.field` on something that has no fields                                                                                         | `.` is for `Enum.Variant` / `inst.output` only                                                                                                            |
| E0106 | unknown parameter in instantiation or test header                                                                                | lists the module's parameters                                                                                                                             |
| E0107 | bad connection port (unknown, or an output)                                                                                      | outputs are read with `.`, not connected                                                                                                                  |
| E0108 | assigning to a non-signal (input, const, clock, …)                                                                               | only out ports, wires, regs are assignable                                                                                                                |
| E0109 | `on rise(x)` where `x` is not a clock                                                                                            | declare `clock clk`                                                                                                                                       |
| E0110 | Ambiguous reference — bare name resolves to 2+ declarations across different files                                               | qualify with the import path (`a.b.Name`); the message lists the candidate files                                                                          |
| E0111 | Qualified reference's path doesn't match any `import` written in this file                                                       | check the import path segments, or drop the qualifier if the bare name is unambiguous                                                                     |
| E0201 | expression is not a compile-time constant                                                                                        | what IS allowed in const positions                                                                                                                        |
| E0202 | constant evaluation overflow (i128 range)                                                                                        | —                                                                                                                                                         |
| E0301 | module has regs but no `reset` declaration                                                                                       | add `reset rst`                                                                                                                                           |
| E0302 | instance input unconnected, or connected twice                                                                                   | connect every input exactly once; clock/reset connect by name                                                                                             |
| E0303 | declaration (port/`wire`/`reg`/`clock`/`reset`/`const`/`enum`/`on`) inside `repeat`                                              | declare once outside; `repeat` only generates hardware                                                                                                    |
| E0401 | assignment/connection width mismatch (`=`, `<-`, init, conns)                                                                    | `extend`/`trunc`/slice; `+` into same width teaches `+%`                                                                                                  |
| E0402 | operand width mismatch (`+%` family, `& \| ^`, comparisons)                                                                      | `extend` the narrow side                                                                                                                                  |
| E0403 | kind mixing: signed↔bits, enums as numbers, clock/reset as data                                                                  | the visible casts `signed()`/`unsigned()`                                                                                                                 |
| E0404 | logical op / condition on a non-`bit`                                                                                            | compare (`x != 0`) or reduce (`\|x`)                                                                                                                      |
| E0405 | compile-time value does not fit, or has no width to adopt                                                                        | the value, the width, and the max that fits                                                                                                               |
| E0406 | index/slice out of range, reversed bounds, base not indexable                                                                    | indices `0..=N-1`; slices `[hi:lo]` msb first                                                                                                             |
| E0407 | builtin/unary misuse (`extend` narrowing, `-` on bits, …)                                                                        | what the builtin is FOR; `0 -% x` for wrap-negate                                                                                                         |
| E0408 | `if`/`match` arms disagree on type/width                                                                                         | every arm becomes the same wire                                                                                                                           |
| E0409 | pattern errors (match on signed, wrong enum, too-wide value)                                                                     | what the scrutinee's type admits                                                                                                                          |
| E0410 | width expression invalid (zero, negative, absurd)                                                                                | hardware needs at least one bit                                                                                                                           |
| E0411 | invalid array element type (nested array/enum/bundle element)                                                                    | array elements are `bit`, `bits[N]`, or `signed[N]`                                                                                                       |
| E0412 | invalid array length (zero, negative, absurd)                                                                                    | an array needs at least one element                                                                                                                       |
| E0413 | array literal argument's length disagrees with the parameter's                                                                   | the argument must have exactly as many elements as declared                                                                                               |
| E0414 | array literal elements disagree in width/signedness                                                                              | every element shares one type — `extend` a narrower one to match                                                                                          |
| E0415 | compile-time array index out of range                                                                                            | indices run `0..=len-1`; a runtime index passes unchecked                                                                                                 |
| E0416 | port/wire/register declared with an array type                                                                                   | arrays are only supported for `fn` parameters in v0.2                                                                                                     |
| E0417 | `foreach` element-form source is not an array or `mem` type                                                                      | `y` in `foreach x in y` must be a declared array/mem signal; use `foreach i in lo..hi` for a range instead                                                |
| E0501 | more than one driver (2nd drive, drive-to-wire, overlapping bit ranges)                                                          | one `=` per signal; `if`/`match` exprs choose; disjoint bits OK                                                                                           |
| E0502 | output never driven, or driven on only some bits                                                                                 | drive it; names the first undriven bit                                                                                                                    |
| E0503 | reg assigned from zero or several `on` blocks, or memory written from several `on` blocks                                        | exactly one `on` block owns each reg or memory                                                                                                            |
| E0504 | combinational cycle (path shown, incl. through instances)                                                                        | every feedback loop passes through a `reg`                                                                                                                |
| E0505 | wrong assignment kind: `<-` to wire/out, `=` to reg                                                                              | `<-` = registers in `on`; `=` = combinational                                                                                                             |
| E0601 | `match` not exhaustive (names a missing value/variant)                                                                           | add the missing arms, or end with `_ =>`                                                                                                                  |
| E0602 | unreachable `match` arm (after `_`, or a duplicate value)                                                                        | move `_` last / delete the duplicate                                                                                                                      |
| E0701 | cross-clock-domain read, or a wire mixing two domains                                                                            | one domain per signal; `sync` (Phase 2) will allow crossings                                                                                              |
| E0801 | duplicate user-defined function name (project-wide)                                                                              | rename — function names are project-unique                                                                                                                |
| E0802 | function name collides with a builtin (`extend`, `trunc`, `min`, …)                                                              | choose a different name                                                                                                                                   |
| E0803 | wrong number of arguments in a `fn` call (expected N, got M)                                                                     | pass exactly the number of arguments the function declares                                                                                                |
| E0804 | function body width doesn't match the declared return type                                                                       | `extend`/`trunc`/slice the body, or fix the `->` type                                                                                                     |
| E0805 | recursive function call (direct or mutual cycle in the call graph)                                                               | replace recursion with fixed-size repetition or a `repeat` loop                                                                                           |
| E0806 | wrong number of payload bindings in a match pattern, or arguments to `Enum.Variant(...)` construction (got M, expected N fields) | list the exact bindings/arguments, or use fewer/more                                                                                                      |
| E0807 | payload field has a non-concrete type (enum or named type used as payload)                                                       | use `bit`, `bits[N]`, or `signed[N]`; nested enums deferred                                                                                               |
| E0808 | OR-pattern alternatives must expose the same binding interface                                                                   | ensure every alternative binds identical names with identical types, or split into separate arms                                                          |
| E0809 | `default` assignment target is not a `reg`                                                                                       | only `reg` signals can have sequential default assignments; drive wires combinationally                                                                   |
| E0810 | duplicate `default` for the same reg in one `on` block                                                                           | each reg may have at most one `default` per `on` block; merge into a conditional expression                                                               |
| E0811 | `const if` condition is not a compile-time constant                                                                              | use only module parameters, `const` values, literals, and arithmetic/comparison on those                                                                  |
| E0812 | unreachable code after `return` in the same statement list                                                                       | remove the dead statement(s), or move `return` inside an `if` if it was meant to be conditional                                                           |
| E0813 | `fn`-body `let` shadows an existing name (an earlier `let`, or a param) at a different width                                     | rename the binding — shadowing at the SAME width (a fold/accumulator pattern) is fine; a different width can't share one Verilog `reg` declaration        |
| E0901 | Bundle literal missing a required field; or (v0.2.24) a bundle-typed `fn` call argument or `return` value has the wrong shape    | list all fields in the bundle literal; the field is named in the error — or match the argument/return expression's shape to the declared bundle type      |
| E0902 | Bundle literal references an unknown field name                                                                                  | check spelling against the bundle definition                                                                                                              |
| E0903 | Duplicate binding name in `let { }` destructure                                                                                  | each name may appear at most once in the binding list                                                                                                     |
| E0904 | Field rename `{ f: alias }` in `let { }` destructure is not supported (reserved for parser)                                      | use dot access `expr.f` instead of renaming in the destructure                                                                                            |
| E0906 | Bundle type reference: unknown bundle name or wrong param count                                                                  | declare the bundle at file level or import the file that does; parameter count must match                                                                 |
| E0907 | Bundle field type mismatch (structural — a shared field's type differs)                                                          | make the field's type match exactly; width/type never coerce implicitly                                                                                   |
| E0909 | Bundle declared more than once (per-file name collision)                                                                         | rename one — bundle names are unique within one file; a different file may reuse the name, qualify by import path if it becomes ambiguous (spec/02 §1.5b) |
| E0910 | Bundle is missing a required field (structural — extra fields are fine, missing ones are not)                                    | add the missing field to the provided bundle, or connect/assign one that has it                                                                           |
| E1301 | `extern module` name reused more than once in this file                                                                          | rename one — extern module names are unique within one file, same rule as `module` (spec/02 §1.5b)                                                        |
| E1302 | `extern module` port has a non-scalar type (bundle/array)                                                                        | flatten to `bit`/`bits[N]`/`signed[N]` — a real Verilog module's port list is always flat wires (spec/02 §1.5c)                                           |

Numbering scheme:

- E00xx — structure/duplicates;
- E01xx — name resolution;
- E02xx — const evaluation;
- E03xx — module structure rules;
- E04xx — width/type rules;
- E05xx — drivers/cycles;
- E06xx — exhaustiveness;
- E07xx — clock domains;
- E08xx — user-defined functions;
- E09xx — bundles (Phase 2);
- E13xx — `extern module` / Verilog FFI.

(Lexer E10xx, parser E11xx, and loader E12xx codes live in
docs/code/06 — retrofit completed 2026-06-12.) Claim a block when a new
pass lands, and add the rows in the same commit.

## How the OR-arm binding intersection pass works (pass 3)

When a match arm lists multiple patterns separated by `,` (OR-patterns), each
sub-pattern may introduce payload bindings. After the individual sub-patterns are
resolved, `names.rs` runs a five-phase intersection check:

1. **Collect** the binding map for each alternative (name → type).
2. **Short-circuit** on E0806 — if any alternative already drew a payload-count
   error, the intersection is skipped (one diagnostic, no cascade).
3. **Name check** — for each name present in any alternative, verify it appears
   in every alternative; missing names are E0808.
4. **Width check** — for names present in all alternatives, verify every
   alternative has the same type width; mismatches are E0808.
5. **Inject** — for clean arms, bind the agreed-upon names into the arm's value
   expression scope.

`_` wildcards contribute an empty binding map; `A(x), _ => x` is E0808 because
the wildcard alternative satisfies no binding. The pass lives entirely in
`crates/mimz-core/src/checker/names.rs` and runs during Pass 3 (module scope / name resolution),
after individual OR-sub-pattern binding but before the width pass sees the arm
body.

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

| Rule                                                  | Blocked on / planned with                                                                                                                                                                                                                                    |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `repeat` unrolling (elaboration)                      | widths/drivers check per-iteration; unrolling is emitter work                                                                                                                                                                                                |
| Cross-INSTANCE clock-domain tracking                  | pass 7 (`clocks.rs`) is module-local; instance outputs carry no domain                                                                                                                                                                                       |
| Cross-clock reads allowed via `sync`                  | the Phase 2 multi-clock construct relaxes E0701 explicitly                                                                                                                                                                                                   |
| Instance-array output widths via the `repeat` var     | read outside the loop falls back to param defaults                                                                                                                                                                                                           |
| Defaultless-param module never instantiated           | internals skipped silently (passes 1–3 still ran)                                                                                                                                                                                                            |
| Driver COVERAGE under non-default bindings            | upgrade path: reuse widths' per-instantiation config set                                                                                                                                                                                                     |
| Recursive instantiation                               | comb summary comes back empty (no through-paths seen); no cycle invented                                                                                                                                                                                     |
| Unevaluable instance-array index in a read            | the comb edge is skipped (under-approximation; elaboration closes it)                                                                                                                                                                                        |
| Test BODY checking (drives/`tick`/`expect`)           | ✅ delivered in Phase 1.5 (`crates/mimz-sim/src/sim/harness.rs`)                                                                                                                                                                                             |
| E-codes on lexer/parser errors                        | ✅ delivered — E10xx/E11xx/E12xx all retrofitted (2026-06-12)                                                                                                                                                                                                |
| Did-you-mean suggestions on E0101                     | nice-to-have; needs edit distance                                                                                                                                                                                                                            |
| `count_clocks`/`collect` walk both `ConstIf` branches | deferred — no const-eval env plumbed into these free functions; overcounts harmless in practice (over-approximate clocks, extra reg drive registrations). Fix: fold `count_clocks` into the walk that already has env, or thread `&HashMap<…, i128>` through |

## How `loop`/`suzhal` and `sync loop` get checked

Neither construct claims its own E-code block — both are checked by
routing through existing passes and existing codes, the same way `foreach`
routes to `E0417` plus whatever the lowered `Repeat`/`Loop` triggers:

- **`SeqStmt::Loop`/`FnStmt::Loop`** (`loop`/`suzhal`, inside an `on` block
  or `fn` body): pass 3 (`names.rs`) const-evaluates the bounds the same
  way `Repeat`'s are (non-const bounds are `E0201`, same as `Repeat`);
  pass 5 (`widths/mod.rs`) width-checks the loop body per iteration —
  ordinary `E04xx` codes fire on a bad drive/expression inside, exactly as
  they would outside a loop. The loop variable is scoped strictly to the
  body in both passes (leaks are a bug, not a diagnosable user error).
- **`ModuleItem::SyncLoop`** (`sync loop`) is checked as the RAW AST node —
  the checker runs before `ast::sync_loop_lower`'s desugaring into
  `Port`/`Reg`/`On`/`Drive` primitives (see
  [`docs/source-guide/05-ast.md`](../source-guide/05-ast.md)):
  - `names.rs` (pass 4) declares the loop's 4 generated signals
    (`<name>_start`/`_done`/`_result`/the counter) so a collision with a
    user-declared signal reuses **E0003**; its `on rise(...)`/`on
fall(...)` clock clause reuses **E0109** if the name isn't a real
    clock (`b.what()` names what it actually is).
  - `widths/mod.rs` (pass 5) width-checks the `result_ty`/`result_init`
    pair and the `lo`/`hi` bounds through the same machinery as any other
    typed declaration and const position — no new codes.
  - `drivers.rs` (pass 6) treats the sync loop's body exactly like an
    `on`-block body (`self.on_block(dcx, sl.span.start, &sl.body)`), so
    single-driver/coverage/`=`-vs-`<-` rules (**E0501–E0505**) apply
    unchanged.
  - `clocks.rs` (pass 7) colors the generated result register with the
    sync loop's own clock, exactly like a normal `reg` — **E0701**
    applies if it's read across a domain elsewhere.

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
