# Architectural, language & process gaps

Findings that are **not defects** - nothing here is wrong behavior against the
current spec. These are structural limits, missing capabilities, and missing test
oracles that constrain what the project can safely become. Filed here so they are
trackable and rankable rather than living only inside a review narrative.

Split from the other audit files deliberately:

| File                           | What belongs there                                                               |
| ------------------------------ | -------------------------------------------------------------------------------- |
| [`bugs.md`](bugs.md)           | Wrong behavior against the spec - a program does the wrong thing                 |
| [`security.md`](security.md)   | Input-triggered crashes, overflow, memory safety                                 |
| [`hardening.md`](hardening.md) | Preventive measures added, and what was checked and found safe                   |
| **`gaps.md`** (this file)      | Correct-but-limited: architecture debt, absent language features, absent oracles |

Each entry states: **what**, **why it matters**, **evidence**, and the
**recommended direction**. New gaps append here; nothing is deleted. When a gap
is closed, its status is edited in place (same convention as `bugs.md`).

Source: [`review-2026-08-02.md`](review-2026-08-02.md).

## Index

| ID                                                                                                                                                   | Gap                                                                              | Severity   | Status |
| ---------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- | ---------- | ------ |
| [GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times)                                                               | No IR; width/kind semantics implemented three times                              | HIGH       | OPEN   |
| [GAP-2](#gap-2-medium---simulator-is-2-state-with-a-whole-value-unknown-flag-no-xz-no-tri-state)                                                     | 2-state simulator; no X/Z, no tri-state/`inout`                                  | MEDIUM     | OPEN   |
| [GAP-3](#gap-3-medium---parser-violates-the-projects-own-mandatory-help-contract)                                                                    | Parser violates the mandatory-help contract (14/60 sites)                        | MEDIUM     | CLOSED |
| [GAP-4](#gap-4-lowmedium---string-keyed-name-resolution-throughout-no-interning)                                                                     | String-keyed name resolution; no interning                                       | LOW→MEDIUM | OPEN   |
| [GAP-5](#gap-5-high-testing---no-declared-type-vs-produced-value-oracle-self-determined-positions-ungenerated)                                       | No declared-type-vs-value oracle; self-determined positions ungenerated          | HIGH       | CLOSED |
| [GAP-6](#gap-6-medium-language---no-assertions-assertassumecover)                                                                                    | No assertions (`assert`/`assume`/`cover`)                                        | MEDIUM     | CLOSED |
| [GAP-7](#gap-7-medium-language---no-enumbits-cast)                                                                                                   | No enum↔bits cast                                                                | MEDIUM     | CLOSED |
| [GAP-8](#gap-8-medium-language---surface-gaps-division-attributes-pipelines-type-generics)                                                           | Surface gaps: division, attributes, pipelines, type generics                     | MEDIUM     | OPEN   |
| [GAP-9](#gap-9-medium-dx---lsp-feature-set-and-missing-fix-it-spans)                                                                                 | LSP feature set + missing fix-it spans                                           | MEDIUM     | OPEN   |
| [GAP-10](#gap-10-low-process---no-coverage-measurement-checker-and-emitter-unfuzzed)                                                                 | No coverage measurement; checker and emitter unfuzzed                            | LOW        | OPEN   |
| [GAP-11](#gap-11-medium-testing---the-width-conformance-oracle-is-vacuous-and-ci-fuzzes-at-a-depth-that-finds-nothing)                               | Width-conformance oracle vacuous; CI fuzzes 20 seeds                             | MEDIUM     | CLOSED |
| [GAP-12](#gap-12-medium-performance---mimz-compile-is-superlinear-in-module-size)                                                                    | `mimz compile` is superlinear in module size                                     | MEDIUM     | CLOSED |
| [GAP-13](#gap-13-medium-testing---the-position-matrix-has-no-exprkind-axis-and-the-only-structural-coverage-assertion-was-deleted)                   | Position matrix has no `ExprKind` axis; deleted coverage assert                  | MEDIUM     | CLOSED |
| [GAP-14](#gap-14-medium-process---the-release-gate-is-scored-at-a-shallower-fuzz-depth-than-the-projects-own-ci-runs)                                | Release gate scored at 400 seeds while CI is configured for 5000                 | MEDIUM     | CLOSED |
| [GAP-15](#gap-15-medium-process---the-per-arm-reasoning-audit-has-no-independent-party-and-cannot-have-one)                                          | Per-arm reasoning audit has no independent party (single-author repo)            | MEDIUM     | CLOSED |
| [GAP-16](#gap-16-high-architectural-closed-2026-08-16---the-self-determined-hoist-machinery-is-scoped-to-module-bodies-and-nothing-states-the-scope) | Self-determined hoist machinery scoped to module bodies, unstated                | HIGH       | CLOSED |
| [GAP-17](#gap-17-medium-process-closed-2026-08-17---rule-a-audits-the-arms-the-defects-are-at-the-call-sites)                                        | Rule (a′) audits arms; the defects are at call sites                             | MEDIUM     | CLOSED |
| [GAP-18](#gap-18-high-architectural-closed-2026-08-19---the-hoist-buffers-flush-point-is-a-second-scoping-axis-and-nothing-watches-it)               | Hoist buffer's flush point is a second, unwatched scoping axis                   | HIGH       | CLOSED |
| [GAP-19](#gap-19-medium-testing-closed-2026-08-18---wasm_parity-skips-silently-and-ci-never-builds-the-artifact-it-needs)                            | `wasm_parity` skips silently; CI never builds the artifact it needs              | MEDIUM     | CLOSED |
| [GAP-20](#gap-20-high-testing-open---the-three-pre-declaration-render-sites-are-outside-every-oracle-and-no-test-elaborates-the-corpus-it-ships)     | Three pre-declaration render sites outside every oracle; corpus never elaborated | HIGH       | OPEN   |
| [GAP-21](#gap-21-low-language-open---clog2param-cannot-size-a-port-verilog-2005-port-list-scoping)                                                   | `clog2(PARAM)` cannot size a port (Verilog-2005 port-list scoping)               | LOW        | OPEN   |

---

## GAP-1 (HIGH, architectural) - No IR; width/kind semantics implemented three times

**Status:** OPEN. Filed 2026-08-02.

**What.** The pipeline is `lex → parse → AST → (AST→AST lowerings) → check →
emit-text`. There is no intermediate representation, no netlist, no elaborated
graph, and no optimization pass. Three independent consumers each carry their own
type model:

| Consumer  | Type model                             | Location                                     |
| --------- | -------------------------------------- | -------------------------------------------- |
| Checker   | `Ty<'a>`                               | `crates/mimz-core/src/checker/widths/`       |
| Emitter   | `Kind` via `infer_kind`                | `crates/mimz-core/src/emit_verilog/kinds.rs` |
| Simulator | `Val { bits, width, signed, unknown }` | `crates/mimz-sim/src/sim/value/`             |

**Why it matters.** This is the documented structural cause of the project's
largest recurring bug family. BUG-11, 18, 19, 20, 21, 22, 23, 24 in
[`bugs.md`](bugs.md) are all _"two implementations of one width rule disagreed."_
So are BUG-28/29 (F-1), BUG-30 (F-2), and BUG-34 (a fused shift chain's
context-widening rule, found 2026-08-03 while validating GAP-5's own
width-conformance oracle - the ninth instance of this exact family since
BUG-11). Every future operator or builtin re-opens the same three-way drift
surface.

**Count as of 2026-08-09: seventeen.** BUG-11, 18–24, 28, 29, 30, 34, 35, 36,
41–44, 47. The v0.2 remediation added four of them (41–44 were found by it,
47 by the oracle it built), which is the argument for this gap rather than
against it: the count is climbing because detection improved, not because the
code got worse.

BUG-47 also surfaced a **third failure mode** for this family, distinct from
the usual two. The first two are a missing case (BUG-41: a gate that never ran
the classifier) and a wrong case (BUG-42: `min`/`max` classified "no mismatch
possible"). BUG-47 was neither - it was a **correct case whose justification had
been deleted**: `Builtin::Extend`'s `allow_shift: false` was right when written
and documented in detail, and BUG-34's own rework removed the simulator function
(`eval_ctx`) the whole comment depended on, leaving a guard that no longer
guarded anything. Exhaustiveness checking cannot see this: the match was
exhaustive, the arm reachable, the comment precise, and every word of it about
code that no longer existed. A typed IR removes the failure mode entirely by
making the width rule singular; short of that, the only detector is an external
oracle, which is what found it.

**Evidence.** `emit_verilog/kinds.rs:6` states the duplication is deliberate -
`foreach`/`sync_loop` lowering produces fresh `Expr` trees at ~6 call sites, so an
annotation set by one pass cannot reach another. The constraint is real; the
workaround (recompute from AST, share the rules via `width_rules.rs`) treats the
symptom. `width_rules.rs:14` is explicit that the unification is partial:

> Deliberately narrow: only `shift_result`/`slice_result` so far … the
> `same_width`/`lossless`/`concat` families remain in `checker/widths/ops.rs`,
> unconverted.

**Direction.** Introduce a minimal **typed elaborated IR** between check and
emit:

```rust
// One node, one resolved type, one owner. Names are interned ids, not Strings.
pub struct Net { pub id: NetId, pub kind: Kind, pub clock: Option<ClockId>, pub span: Span }

pub enum Op {
    Const(Bits),
    Ref(NetId),
    Bin(BinOp, NetId, NetId),
    Mux(NetId, NetId, NetId),
    Slice(NetId, u32, u32),
    Concat(Vec<NetId>),
    Ext { src: NetId, to: u32, signed: bool },
    Reg { d: NetId, clk: ClockId, rst: Option<(NetId, Bits)>, edge: Edge },
    // …
}
```

Run `foreach` / `sync_loop` / `bundle` lowering **into** the IR, have the checker
annotate it, and have both the emitter and the simulator consume the _same_ IR.
Width is then computed exactly once, by construction.

`Ext` becomes an explicit node the emitter **must** materialize - which makes
BUG-28 structurally impossible rather than a case-table entry.

Also unlocks: constant folding, dead-signal elimination, `--emit-netlist` for
debugging, per-module parallel emit, and any future non-Verilog backend (which
the Constitution's _"Verilog interop forever, even after native backends exist"_
clause already presumes).

**Related.** [GAP-4](#gap-4-lowmedium---string-keyed-name-resolution-throughout-no-interning)
should land as part of this, not separately.

### Sub-gap (2026-09-03, narrowed 2026-09-04): `ir::exec` and the AST kernel disagreed on `<<`/`>>` — 2 of 3 issues fixed

~~Three-fold divergence~~ Was three-fold; issues 1 and 2 (truncation, and
the missing `const_amount` making growth worst-case instead of exact) are
fixed as of 2026-09-04 (the 2026-09-04 GAP-1 fix round, Task 5):
`lower_binop` now sizes `Shl`'s `out` pin via `width_rules::shift_result`
at worst-case growth (matching the AST evaluator's own fallback when no
compile-time shift amount is known), instead of truncating at `a.width()`.
Worst-case growth (`2^b.width()-1`) is always `>=` any real constant
growth, so the IR's `out` pin can never be narrower than the true shifted
value — the extra high bits are simply zero, which any comparison at the
SOURCE-declared output width (e.g. the differential test's own
`bits_to_limbs(..., declared_width)`) masks away identically either way.
This makes issue 1 (truncation) unconditionally fixed.

**RESOLVED 2026-09-05 (residual-fix Task 1) — worst-case sizing now agrees
with the checker's exact per-constant sizing.** `ir::lower`'s `ExprKind::
Binary` arm const-evals the source `rhs` `Expr` (`crate::value::
const_eval` against `design.consts`) before it's lowered to `Bits`, and
threads the result through `lower_binop`'s new `shl_const_amount: Option<
u128>` parameter — mirroring the checker's own `shift_ty`
(`checker/widths/ops/mod.rs`) and the AST evaluator's `eval_shift_chain`
(`value/binary.rs`), both of which resolve the constant the same way. A
non-constant (genuinely RUNTIME) shift amount still passes `None` and
falls back to worst-case growth, unchanged. `(a << 2) & c` now validates
cleanly (regression test: `crates/mimz-core/src/ir/tests/lower_binops.rs`,
`shl_with_a_compile_time_constant_amount_sizes_exactly_not_worst_case`).

**RESOLVED 2026-09-05 (residual-fix Task 2) — this corner is no longer
SILENT; it is now a loud `WidthMismatch`-shaped `ValidationError`.** An
over-wide `Shl` result that reaches a module OUTPUT PORT directly — or via
`extend`'s `target <= base.width()` no-op branch, e.g.
`out y: bits[10] = extend(a << 2, 10)` — used to give `y` 11 real nets
instead of the 10 the source declared with nothing in `validate()`
reporting it, because `ir::Module` carried no record of a port's
originally-DECLARED width once lowering was done, only the actual `Bits`
width the lowering produced. Fixed by giving `ir::Module` a new
`port_declared_widths: BTreeMap<String, u32>` field, populated by
`lower()` from `design.outputs`'s own `Signal.width.bits` (same v1 scope
boundary as `extern_decls`/`signals`: not round-tripped by the text
format, so a hand-parsed IR fixture has no entry and `validate()` skips
the check gracefully rather than treating a missing entry as a
violation — same pattern as the black-box-port-shape check). `validate()`
gained a sixth check comparing each OUTPUT port's lowered `Bits::width()`
against its `port_declared_widths` entry when present, reporting a new
`ValidationError::PortWidthMismatch { port, declared, found }`. It was
benign in practice only because every consumer masked back to the
source-declared width externally (the differential test harness's
`bits_to_limbs(..., declared_width)` is the example), not because the IR
itself enforced anything — now it does.

**A checker-legal program can panic in lowering.** `bits[8] << 999000`
type-checks (the checker's exact-constant growth, `999008` bits, is under
`MAX_WIDTH`), but `ir::lower`'s worst-case formula computes growth via the
literal's OWN natural width (`natural_width(999000) = 20` bits `->`
`2^20-1` growth), which exceeds `MAX_WIDTH` and panics via `.expect()`.
Judged acceptable as a PANIC (not a silent miscompile) since `ir::lower`
has no production caller today (only tests/the fuzz differential leg
reach it) and every other out-of-scope IR construct already panics the
same way — but the `.expect()` message text claimed the checker
"guarantees" this never happens, which is false for the worst-case-growth
formula specifically; corrected as part of Task 8.

**RESOLVED 2026-09-05 (residual-fix Task 3) — `validate.rs`'s copy of
this same `.expect()` is gone.** This used to be a sharper concern than
`lower.rs`'s: `validate()` exists specifically to REPORT malformed IR
rather than crash on it, so a hand-written IR **text** fixture
(`tests/ir_validation.rs`'s whole mechanism) declaring an oversized
`Shl.b` pin panicked `validate()` instead of yielding a `ValidationError`.
Fixed by changing `expected_widths`'s return type to
`Result<Vec<(&'static str, u32)>, (u32, u32)>`, with the `Shl` arm
matching `width_rules::shift_result`'s own `Ok`/`Err` instead of
`.expect()`-ing it; the checks-2&3 loop in `validate()` now turns an
`Err((lhs_width, amount_width))` into a new
`ValidationError::ShiftGrowthTooWide { cell_index, lhs_width,
amount_width }` instead of panicking (regression test:
`tests/ir_validation.rs`'s `shift_growth_too_wide_fixture_is_rejected`,
fixture `tests/fixtures/ir_errors/shift_growth_too_wide.ir`).

**RESOLVED 2026-09-05 (residual-fix Task 4) — closed by Task 1 alone, no
lowering-side fusion needed.** `value::binary::eval_shift_chain` evaluates
`(p2 >> 4) << 7` as ONE unit at a single fixed width, while `lower` still
emits two independent cells with a materialized intermediate — but for an
UNSIGNED chain that no longer produces a different NUMBER, because Task 1
made every cell's local width formula exact (not worst-case) whenever its
shift amount is a compile-time constant: the running width `ir::lower`
arrives at one cell at a time is now identical, step for step, to the
running width `eval_shift_chain` folds explicitly, so a "locally sized"
unfused intermediate is exactly as wide as a "chain-final-width" fused one
would be — extra zero bits above an unsigned value never change its value.
Confirmed empirically (not just by this reasoning), per BUG-34's own
provenance as an external-fuzz-found bug: `crates/mimz-core/src/ir/tests/
lower_binops.rs`'s `shift_chains_lowered_per_node_match_the_ast_kernels_
fused_evaluation` lowers BUG-34's exact repro shape, its mirror, and a
3-step chain through `ir::lower` + `ir::exec`, exhaustively over all 256
values of an 8-bit input, and diffs every result against `eval_shift_chain`
directly — zero divergences.

### Sub-gap (2026-09-03, narrowed 2026-09-04): `ir::lower` could not lower any builtin call — 11 of 14 now lowered

Fixed as of 2026-09-04 (the 2026-09-04 GAP-1 fix round, Tasks 1-4):
`extend`/`trunc` (for provably-unsigned arguments), `signed`/`unsigned`/
`encoding` (pure identity casts — `ir::Bits` carries no signed bit, so a
cast changes nothing about the netlist), and `nand`/`nor`/`xnor`
(composed from the existing `RedAnd`/`RedOr`/`RedXor` + `LogicNot` cells,
no new `CellKind`) all lower now. `clog2`/`sync.double_flop`/`sync.pulse`
are `unreachable!()` (never survive to a checked `Design`, same guarantee
`value::fn_eval::call`'s own matching arm already relies on).

**`min`/`max`/`abs` — RESOLVED 2026-09-18/19 (IR-base-gap-closure Task 6 +
its fix round).** They were refused loudly (`unimplemented!()`) while
`ir::Bits`'s v1 schema could not express signedness. `Bits` now carries a
per-value `signed: bool` (the `Kind`-style pair this paragraph used to call
a separately-scoped follow-up), so all three lower for real as `Lt` + `Mux`
over existing cell kinds — no new `CellKind`, and `extend`'s sign-dependent
branch reads the same flag instead of a best-effort `Expr`-shape heuristic.
See the dedicated sub-gap "`ir::Bits` has no signed bit in v1" below for
what the flag is computed from and what stays out of scope.

**Making `signed(x)` lowerable widens an already-existing signed-comparison
hole, not a new one — RESOLVED 2026-09-05 for the ORDERING comparisons
(GAP-1 residual Task 5).** Comparison and arithmetic `CellKind`s (`Lt`/`Le`/
`Gt`/`Ge`/`Add`/`Sub`/`Mul`/etc.) used to carry no signedness at all — so
`signed(a) < signed(b)` lowered to an UNSIGNED compare in the IR, while
`emit_verilog` renders the same source as a genuinely SIGNED comparison,
divergent for negative operand values (`signed(-1) < signed(1)` answered
FALSE, `-1`'s bit pattern being the largest unsigned value). That predated
Task 2's `signed`/`unsigned`/`encoding` lowering: a bare `signed[8]`-typed
input already reached `lower_binop` with no guard and no cast needed.

What changed, precisely:

- **Schema (additive, not a new variant).** `CellKind::{Lt,Le,Gt,Ge}` gained
  a `signed: bool` field. `ir::Bits` was UNCHANGED at the time — it carried
  no sign bit; signedness was scoped to exactly these four cell kinds rather
  than threaded through `lower_expr`'s return type. **Superseded 2026-09-18
  (Task 6):** `ir::Bits` now DOES carry `signed`, and it is what the four
  cell kinds' flag is computed from; see the sub-gap below.
- **`Eq`/`Ne` deliberately untouched**, still unit variants: two's-complement
  equality compares the same bit patterns under either interpretation.
- **Arithmetic (`Add`/`Sub`/`Mul`/`*Wrap`/`Shl`/`Shr`) untouched** — those
  `CellKind`s still carry no `signed` field of their own. **Amended
  2026-09-19:** their RESULT `Bits` does now carry one (`a.signed ||
b.signed` for the lossless/wrapping/bitwise families, `a.signed` for the
  shifts — the same rule `width_rules::lossless_result`/`matched_result`/
  `shift_result` give the checker), so a derived value's signedness reaches
  `extend`/`min`/`max`/`abs`/unary `-`/comparison lowering correctly. Only
  `CellKind::Mul` still has no `signed` flag, so a genuinely signed multiply
  executes unsigned in `ir::exec` — pre-existing, separately scoped.
- **`Lt`/`Le` are the ones with real new BEHAVIOUR.** They are the only
  ordering comparisons `ir::lower` produces; `BinOp::Gt`/`Ge` still fall into
  `lower_binop`'s `unimplemented!()` catch-all, unchanged and out of scope
  here. `CellKind::Gt`/`Ge` took the same field for schema consistency (they
  are reachable from hand-written IR text and already dispatched by
  `ir::exec`), so they are **schema-consistent and sign-aware when executed,
  but not independently lowered** — a `Gt`/`Ge` cell can only enter the IR
  through `parse_line`, never through `lower`.
- **Text format round-trips it**, unlike the v1 scope-boundary fields
  (`Dff::clock`, `Mem::init`): the flag changes what a cell COMPUTES, so
  losing it would silently change behaviour. Unsigned stays the bare `$lt`
  spelling (every pre-existing IR text still means what it did); signed is
  `$lt[signed]`, in the same bracket style as `$dff[Rise]`. An unrecognized
  bracket argument is a parse error, never a silent fall back to unsigned.
- **Signedness detection** is `lower.rs`'s new `expr_is_definitely_signed`.
  It recognizes exactly two shapes: a bare `Ident` naming a module-level
  signal declared signed, and `signed(<Ident>)` (GAP-1's headline case — a
  free reinterpret over two UNSIGNED-declared signals). Everything else is
  conservatively `false`, i.e. an unsigned comparison — today's behaviour, so
  an unrecognized shape is never a regression. Deliberately NOT a general
  `infer_kind`-style walk; see the invariant below for why.

**The guard's actual invariant (corrected 2026-09-05 after review).**
`lower_binop` marks a comparison signed only when its two operand pins came
out the SAME width. That guard is only sound if a `true` from
`expr_is_definitely_signed` implies **"this pin's lowered width IS the width
the CHECKER types the expression at."** The first cut of this work stated the
weaker claim that "a width mismatch means the narrow side is a bare literal"
and leaned on its converse, which is false: wherever `ir::lower`'s width
formula and the checker's disagree, a literal on the other side can match the
LOWERED width while the checker sized it from the (different) TYPE width, and
reinterpreting those bits as two's complement silently changes the answer.
Two such divergences exist today, and both are why the helper is restricted
to declared-signal shapes rather than walking the expression tree:

- **`UnOp::Neg`** — see the separate residual below. `-a < 200` for
  `a: signed[8]` was empirically wrong for 128 of 256 inputs under the first
  cut. Pinned by `a_negated_operand_keeps_the_comparison_unsigned`.
- **`BinOp::Mul` with a literal operand** — `lower_binop` sizes it
  `a.width() + b.width()` from the literal's NATURAL width, while the checker
  first adapts the literal to the sized side's type: `a * 2` for
  `a: signed[8]` is 10 bits lowered but `Signed(16)` typed, so a 10-bit
  literal on the other side (`a * 2 < 600`) would satisfy the guard while
  meaning something else.

**Narrowed residual — a natural-width literal operand keeps the comparison
unsigned.** For two non-literal operands the width guard costs nothing:
`checker::widths::ops::matched_ty` rejects a genuinely mixed comparison
outright (E0403 "cannot mix X and Y … convert visibly with
`signed(x)`/`unsigned(x)`"), so they always share a type and hence a width.
A width MISMATCH means the narrow side is a bare literal, which the checker
types as untyped `Ty::CtInt` inheriting the sized side's type — but
`lower_expr`'s `Int` arm sizes it at its own NATURAL width (`5` → 3 bits),
and reinterpreting those 3 bits as two's complement would read `5` as `-3`,
flipping `1 < 5` from true to false. So `x < 5` for a signed `x` stays an
unsigned cell: still not what `emit_verilog` renders, but unchanged rather
than newly and differently wrong. The NEGATED-literal form (`x < -5`) is
benign by a different mechanism — `-5` lowers to a `Neg` cell at the
literal's own narrow width, so it too fails the width guard — but it is
benign by accident, not by design, and the `Neg` residual below is what
would need closing to reason about it properly. Sizing a literal from its
comparison context is the separate follow-up that would close all of this —
**RESOLVED 2026-09-08, GAP-1 residual Task 3** (see the new sub-gap
immediately below this one for the general fix, which closes a strictly
broader class than just ordering comparisons). The test that used to pin
this boundary,
`a_natural_width_literal_operand_keeps_the_comparison_unsigned`
(`crates/mimz-core/src/ir/tests/lower_binops.rs`), was renamed
`a_literal_operand_is_sized_to_its_signed_siblings_width_and_the_comparison_is_signed`
and its assertions flipped, exactly as its own prior comment anticipated.

**Fuzz corpus:** `tests/differential_fuzz.rs`'s `gen_ir_clocked_module`
(Task 18's narrowed generator) is UNCHANGED by this round — it still never
emits a builtin call at all, by construction. Widening it to use the newly
lowerable builtins (or folding the IR leg back into the main clocked
generator, per this sub-gap's original note) is follow-up work: the main
generator's width machinery renders `extend`/`signed`/`unsigned` "at
essentially every node" (Task 18's own measurement), and a meaningful
fraction of those are genuinely signed or hit `min`/`max`/`abs` — folding
the leg back in today would still skip most seeds on the newly-narrower
but still-real residual gap above.

### Sub-gap (2026-09-08, RESOLVED 2026-09-08 — GAP-1 residual Task 3): a bare literal or const/param identifier lowered at its own natural width instead of its use-context width

**What.** `ExprKind::Int`'s arm sized EVERY bare literal at
`crate::bits::natural_width(value).max(1)` — its own minimal width — with no
notion of the width its use-context actually requires. A module parameter or
file-level `const` referenced as a plain identifier (resolved by GAP-1
residual Task 2, the sub-gap immediately above) had the same problem: it
sized to its own natural width, never the sized sibling/target the checker
promotes it to (`matched_ty`/`concat_ty`, the same `Ty::CtInt` promotion the
signed-comparison sub-gap above describes). This is strictly BROADER than
that sub-gap: it also hits plain `Eq`/`Ne` (sign-agnostic, no comparison
"signedness" involved at all) and output/wire-port assignment positions, not
just signed ordering comparisons. This plan's investigation dumped
`traffic_light.mimz`'s actual lowered IR to confirm it: `state == State.Red`
lowered `state[0:2]` (2 bits) against `State.Red`'s tag `{12}` sized to 1 bit,
tripping `ir::validate::validate`'s `WidthMismatch` on a real shipped
example, not a hypothetical — `State.Green`'s tag hit the same bug,
`State.Yellow`'s happened to need exactly 2 bits naturally so it didn't
trip. `debug_wrapper.mimz`'s `PortWidthMismatch { port: "dbg_out", declared:
8, found: 1 }` was the same root cause in a different position: `dbg_out =
0` inside a `const if` branch, sizing `0` to 1 bit instead of the declared
8-bit port. `sync_loop_search.mimz` turned out to be a THIRD, distinct
manifestation, not "one more instance of the first shape" as first assumed:
`sync_loop_lower.rs`'s desugared loop-done check (`find_first_cnt == hi - 1`,
by design emitted as a real `BinOp::Sub` AST node — `hi - 1` is never
pre-folded to a bare literal, so as to keep `ast` free of a `checker`
dependency) lowers `hi - 1` through `lower_binop`'s ordinary lossless-Sub
growth formula (`in_width + 1`) regardless of both operands being literals,
coming out WIDER (5 bits) than the 3-bit counter it's compared against —
the checker, by contrast, treats an all-compile-time-constant subexpression
as untyped `Ty::CtInt` sized by context, same as a bare literal. This is the
opposite direction from the other two cases (there, the constant side was
NARROWER and needed widening).

**Fix.** A new `lower_expr_sized` helper (`crates/mimz-core/src/ir/lower.rs`)
re-lowers a compile-time-constant-shaped `Expr` at an explicit target width,
falling through to plain `lower_expr` unchanged for anything else (a real
signal's width is already reconciled by the checker, so re-sizing it would
silently mis-widen or truncate a value that isn't constant). Three shapes
count as "constant": a bare `ExprKind::Int` (reuses its own arbitrary-width
`bits::Bits` directly, no precision loss); a `design.consts` `Ident` (module
parameter/file-level `const`, guarded against a `locals`-shadowed name of the
same spelling); and — added once `sync_loop_search.mimz`'s real shape was
understood — anything else `crate::value::const_eval` can fold, gated to
`locals.is_none()` (see below). A sibling `is_const_foldable` predicate
answers the same question without committing to a width, used by
`ExprKind::Binary`'s arm to pick WHICH side to re-lower: deliberately NOT
"whichever side is narrower" (that heuristic is exactly the assumption that
made `sync_loop_search.mimz` look like a repeat of the first shape when it
isn't — a constant EXPRESSION can come out wider than its sibling, not just
narrower), gated on a new `requires_matched_ab(op: BinOp) -> bool` free
function that mirrors `validate.rs`'s own `CellKind`-keyed function of the
same name (deliberately DUPLICATED, not extracted into one shared list —
`CellKind` doesn't retain which `BinOp` produced it, so unifying the two
would need a translation layer neither file has today for what is an 11-arm
boolean lookup, not a numeric formula like `Shl`'s growth, which both files
already share through `width_rules::shift_result`; see `expected_widths`'s
own doc comment on `Shl` for the same "must never drift" principle, which it
also accepts a duplicated formula for rather than sharing). `resolve()`
(the wire/output driver path) now looks up the signal's declared width from
`design.outputs`/`design.wires` itself before lowering its comb driver,
sizing a constant driver to the PORT's declared width — the fix
`debug_wrapper.mimz` needed, which the `ExprKind::Binary` change alone does
not reach.

**Review fix (2026-09-08): the generalized (non-literal, non-Ident)
fallback arm originally routed through `crate::value::const_eval`, which
narrows to `i128` via `to_i128_saturating` — and that function does NOT
error past `i128`'s range, it silently returns `i128::MAX`/`MIN`
(`ConstVal::to_i128_saturating`'s own doc comment).** Code review found this
more reachable than the first cut's doc comment claimed, and the actual
failure mode worse than "truncation" — a checker-legal program
(`bits[200] w = 1 << 190;`, `Bits::Wide` exists exactly for a value like
this, BUG-13 layer 2) would silently lower a WRONG constant with no error
anywhere. Fixed by routing the fallback (and the matching `is_const_foldable`
predicate, so the two never disagree on what folds) through
`crate::value::const_eval_wide` instead — its own doc says it's for exactly
this case, "the one place a compile-time value's own MAGNITUDE (not just a
structural size) matters" — then resizing to `target_width` via
`crate::value::from_const_at_width`, the SAME sign-aware extend/truncate
`ir::exec`'s own `Const`-cell evaluation uses. This is the same reasoning
already correctly applied to keep `ExprKind::Int` off plain `const_eval` in
the same helper (see above), just not originally carried to the fallback
arm. The `locals.is_none()` gate is unaffected and still needed —
`const_eval_wide` has no notion of `locals` either, for the same reason
`const_eval` doesn't (see `lower_expr_sized`'s doc comment for the full
reasoning); every real call site today already passes `locals: None`.

New test pinning this specifically:
`a_wide_compile_time_constant_expression_lowers_exactly_not_saturated_to_i128_max`
(`crates/mimz-core/src/ir/tests/lower_binops.rs`) — `1 << 190` driving a
`bits[200]` output, asserting the folded `Const` cell's value has its
highest bit at position 190 (not a corrupted/saturated placeholder), is not
the old bug's specific wrong output (`i128::MAX` as bits), and matches an
independently-recomputed oracle (`const_eval_wide` + `from_const_at_width`,
the same two functions the production path now calls).

If neither side of a `Binary` op is const-foldable, both sides come back
unchanged and the mismatch reaches `lower_binop`/`validate` untouched — a
genuine checker/lowering disagreement surfaced by `ir::validate`'s existing
`WidthMismatch` check, not a silent truncation path.

New tests (`crates/mimz-core/src/ir/tests/lower_binops.rs`):
`debug_wrapper_shaped_bare_literal_in_a_wire_driver_sizes_to_the_declared_output_width`,
`traffic_light_shaped_enum_match_arms_size_their_tags_to_the_scrutinees_width`,
`sync_loop_search_shaped_compile_time_subtraction_sizes_down_to_the_narrower_counter_width`
(pins the direction-reversal fix `is_const_foldable` needed), and
`a_wide_compile_time_constant_expression_lowers_exactly_not_saturated_to_i128_max`
(the review-fix regression above), each asserting `ir::validate::validate`
returns `[]` where it used to return a specific
`PortWidthMismatch`/`WidthMismatch` (the fourth also asserts the exact
folded value, not just a width/validation check).

Verification: `cargo test -p mimz-core ir::` clean (94 passed); `cargo test
--workspace` clean (1428 passed); `cargo clippy --workspace --all-targets --
-D warnings` clean; this plan's coverage probe re-run confirms
`debug_wrapper`/`traffic_light`/`sync_loop_search`/`blinker` (all flavors:
english/tanglish/tamil/mixed, plus tamil-pure for `sync_loop_search`) now
validate cleanly.

### Sub-gap (2026-09-05, RESOLVED 2026-09-18 — Task 6): `ir::lower` did not grow `UnOp::Neg`, but the checker does

Pre-existing and independent of the signed-comparison work above, surfaced
while reviewing it. `checker/widths/ops/mod.rs` types negation losslessly —
`UnOp::Neg => match t { Ty::Signed(n) => Ty::Signed(n + 1), .. }`, gaining
the carry bit, so `-a` for `a: signed[8]` is `Signed(9)`. `ir::lower`'s
`UnOp::Neg` arm keeps the operand's width instead:
`UnOp::Neg => (CellKind::Neg, a.width())`, i.e. 8 bits.

Consequences: `-a` overflows in the IR for `a = -128` (`Signed(8)`'s most
negative value, whose negation genuinely needs 9 bits), and any lowered width
computed downstream of a negation is one bit short of the checker's. This is
the divergence that made the signed-comparison guard unsound in review; the
guard now sidesteps it by refusing to call a negated expression signed
(`a_negated_operand_keeps_the_comparison_unsigned` pins that), but the
underlying width bug is untouched and belongs in its own task — it changes
pin widths, so it interacts with `validate`'s matched-`a`/`b` checks and with
`emit_verilog` parity, unlike the comparison flag, which is metadata-only.

**Fixed 2026-09-18 (Task 6).** `UnOp::Neg`'s `out` pin is now
`a.width() + 1` when `a.signed`, matching the checker exactly, and the
result `Bits` inherits `a.signed`. The heuristic the guard used to sidestep
it with is deleted;
`a_negated_operand_keeps_the_comparison_unsigned` became
`a_negated_operand_now_makes_the_comparison_signed`.

### Sub-gap (2026-09-18, RESOLVED 2026-09-20 — Task 6 + four fix rounds): `ir::Bits` has no signed bit in v1

**What.** `ir::Bits` was a bare `Vec<NetId>`, so a lowered value carried no
signedness at all. Everything that needed it re-derived it from the SOURCE
`Expr` with a best-effort two-shape proof (`expr_is_definitely_signed`,
`arg_is_definitely_unsigned`, `unshadowed_signal_signed`), which is why
`min`/`max`/`abs` were refused outright and `extend` panicked whenever it
could not prove its argument unsigned.

**Fix.** `Bits { nets, signed }`, computed once at construction and never
re-derived downstream. The invariant is exact and narrow: **a value's
`Bits::signed` equals the signedness of the `Ty` the checker gives that same
expression** — nothing weaker ("it might be negative") and nothing looser
("it came from something signed"). Concretely: stamped from the declared
`Width.signed` for input ports, register Qs and extern-instance outputs;
flipped by `signed(x)`/`unsigned(x)`; carried by `trunc` (the checker keeps
the operand's kind); and — the fix round's correction, 2026-09-19 —
**computed for DERIVED values too**. Without that last piece the flag was
`false` for every
computed expression, so all four Task 6 features silently reverted to their
unsigned reading the moment an operand was an expression rather than a bare
signed port (`extend(a - b, 16)` zero-extended, `min(a - b, c)` compared
unsigned, `abs(a - b)`/`-(a - b)` came out one bit short). `lower_binop` now
gives its result the signedness the checker's own rules give it
(`a.signed || b.signed` for `+`/`-`/`*`/the `+%` family/bitwise — `||` not
`&&`, because a re-sized literal's `ConstVal` is always built unsigned;
`a.signed` for the shifts; always unsigned for comparisons and `&&`/`||`),
`~x` inherits its operand's flag (the checker's `BitNot` returns the operand
type unchanged), `abs` is `signed` like the checker's `Signed(n+1)`, and
`min`/`max` give a literal operand the same `lower_expr_sized` resize +
sign-inheritance that `ExprKind::Binary`'s comparison path already had (so
`min(x, 3)` and the `max(x, 0)` clamp idiom lower to a valid module instead
of a `WidthMismatch`). Pinned by four Executor-level regression tests in
`crates/mimz-core/src/ir/tests/lower_builtins.rs`, all over computed
operands.

**Second fix round, 2026-09-19 — the shapes that "inherit" got wrong.** A
re-review found the same disease at three more sites, all of them a
`Bits`-producing lowering that guessed instead of following the checker's own
rule for that expression kind:

- A **slice** and a **bit-select** (constant OR runtime index) are
  UNCONDITIONALLY unsigned — `width_rules::slice_result` returns
  `signed: false` whatever the base was (the single shared rule that makes
  BUG-21 structurally unreintroducible), and `checker/widths/expr/lvalue.rs`
  types a bit-select as `Ty::Bit`. They used to inherit the base's flag, so
  `extend(a[7:4], 8)` sign-extended garbage and `min(a[7:4], b[7:4])` compared
  signed. The runtime bit-select needed its own fix: it rides on a `Shr`,
  which DOES keep its left operand's signedness, so the 1-bit result has to
  drop that flag explicitly.
- An **`if`-expression** and a **`match`** result now carry their
  branches'/arms' signedness. Both were built straight from
  `Module::alloc_bits` (always unsigned), so `extend(if s { a } else { b },
16)` zero-extended and `-(if …)`/`abs(if …)` came out a bit short. The
  checker unifies every branch to one `Ty` (`emit_verilog/kinds.rs` derives
  the mux's `Kind` from its branches the same way), so there is exactly one
  flag to take: all three inline `Mux` literals now go through
  `push_mux_cell`, which holds that shared flag.

Pinned by five more Executor-level regression tests in the same file, over
slice, bit-select, runtime-bit-select, `if`-expression and `match` operands.

**Third fix round, 2026-09-19 — the constant-operand family.** A third review
found five more sites, and every one of them was the SAME shape: a value whose
checker `Ty` is signed lost the flag because one side/branch/operand of it was
a compile-time constant. The checker gives a constant NO signedness of its own
(`Ty::CtInt` simply becomes its context's `Ty` — `matched_ty`/`adapt_lossless`,
`crates/mimz-core/src/checker/widths/ops/mod.rs`) and `lower_const` therefore
builds every constant's `Bits` unsigned. So any merge that asked both sides to
be signed (`&&`) let that constant veto a genuinely signed sibling, and any
context that had no sibling at all never stamped anything.

Fixed with one rule stated two ways, rather than five independent patches:

- **Where the context is a sibling value, merge with `||`, not `&&`.** For two
  REAL operands the two are equivalent — `width_rules::matched_result` demands
  an identical `Kind` and `lossless_result` demands identical signedness (the
  checker rejects a mixed comparison/arithmetic outright, E0403), and the
  checker unifies every mux branch/arm to one `Ty`. They differ only on a
  constant operand, where `||` is exactly "the sibling's sign wins", which is
  what `Ty::CtInt` does. Applied to `cmp_signed` for `<`/`<=`/`>`/`>=` (the
  literal-resize stamp beside it only ran on a width MISMATCH, so `a < -128`
  over a `signed[8]` — literal already 8 bits — compared unsigned) and to
  `push_mux_cell` (so `if c { x } else { 0 }`, `match s { 0 => 0  1 => a }` and
  `min(a, 0)`/`max(a, 0)` keep their signedness). `||` cannot invent a sign a
  checker-valid program does not have: a positive literal that `fit`s in
  `signed[N]` always has natural width `< N`, so it reaches the resize-and-
  stamp path instead.
- **Where the context is a DECLARED type, stamp it directly** — there is no
  sibling to merge with. A `fn` call's result is its declared return type (so
  `-> signed[8] { … return -1 }` is signed), a `fn` argument is its
  parameter's declared type (so `ext(-1)` binds a `signed[8]` `-1`, not a
  1-bit `1`), and a memory read is the memory's element type
  (`checker/widths/expr/lvalue.rs` types `m[addr]` as `Ty::Signed(width)` for
  a `mem m: signed[N][D]`, and `emit_verilog/kinds.rs` reads the same element
  `Kind`).

Two more things fell out of the same round. `lower_expr_sized`'s blanket
`locals.is_none()` gate refused to const-fold ANY compound constant inside an
inlined `fn` body, so a signed fn's `return -1` (an `Unary{Neg}`, not a bare
`Int`) lowered at its operand's own 1-bit natural width and returned `1`; the
gate is now `visible_consts(locals)`, which hides only the design-level
constants a local name actually shadows — the real hazard the gate existed
for — and lets everything else fold. And `lower_seq_stmts`'s branch-merge, the
fourth and last inline `Mux` literal, now goes through `push_mux_cell` too.

Pinned by nine more Executor-level regression tests in the same file, over a
same-width literal comparison, an `if`/`match` with a literal branch, a signed
fn returning a literal, a signed memory read, a literal fn argument, a nested
literal-branch mux, an array-element read and the `min`/`max` clamp idiom.

**Fourth fix round, 2026-09-20 — the same disease on the WIDTH axis.** Rounds
1-3 only ever touched the SIGN axis. A `Ty::CtInt` has no width of its own
either, and `lower_expr` sizes a constant at its own natural/arithmetic-growth
width unless the consuming site explicitly re-sizes it through
`lower_expr_sized`. Every site rounds 1-3 taught to apply a SIGN context was
therefore still applying only half of that context. The review that found this
reproduced six divergences through the full pipeline, all pre-existing (none a
regression of rounds 1-3); the systematic `lower_expr(` call-site pass this
round ran on top of them found three more:

- `LowerCtx::resolve` read a wire/output's declared `width.bits` but never its
  `width.signed`, so `wire w: signed[8] = -1` lowered unsigned (F1).
- An `if`-expression's branches and a `match`'s arms were lowered with plain
  `lower_expr`, so a constant branch kept its natural width and the mux
  returned the raw literal — `if s { a } else { -1 }` gave `1` (F2).
- A sequential assignment's RHS (`SeqStmt::Default`, `Target::Signal`,
  `Target::MemWrite`'s data) was never sized or signed to the register's /
  memory element's declared type (F3).
- A register's RESET value was lowered at the folded `ConstVal`'s own natural
  width — `elaborate::module`'s `const_eval_wide` stores it un-resized — so
  `reg q: signed[8] = -1` reset to a 1-bit `1` (F4).
- `Mul`'s literal-resize special case matched only a bare `ExprKind::Int`,
  missing `a * -1` and `a * K`; both hit `PortWidthMismatch` at `validate` on
  a checker-valid program (F5).
- The const-foldable resize was gated on `requires_matched_ab`, which
  deliberately excludes the lossless ops, so a constant operand of `+`/`-`/`*`
  (and of the `+%` family, also absent from that set) was never sized or
  signed to its sibling at all, although `adapt_lossless`/`matched_ty` give it
  exactly that type (F6).
- Systematic pass: a slice write's RHS (`q[7:4] <- 3`) was lowered at the
  literal's natural width and then spliced by `clone_from_slice`, which
  PANICKED on the length mismatch — `lower_bitselect_write` now takes the RHS
  as an `Expr` and sizes it to the slot. A memory read/write ADDRESS is the
  same constant-context position and now adopts the memory's `clog2(depth)`
  width. And a `fn`-body `let` bound to a constant (`let m = 1  v & m`) is an
  untyped `Ty::CtInt` at the checker but an already-lowered `Bits` in the IR,
  invisible to `is_const_foldable`; `LowerCtx::local_consts` now keeps the
  folded value visible through `visible_consts` for the rest of that body.

The rule is unchanged from round 3, just applied to both axes at once: where
the context is a SIBLING value, the constant adopts that sibling's width and
sign; where the context is a DECLARED type, both are stamped from the
declaration. Two supporting changes made that observable rather than merely
representable. `ir::exec`'s arithmetic cells (`Add`/`Sub`/`Mul` and the `+%`
family) now read their operand pins' own `Bits::signed` (`||`, same rule as
`lower_binop`'s `out_signed`) instead of evaluating every operand as unsigned,
so a lossless op sign-extends into its growth bit the way `value::binary`'s
`as_i128` and Verilog's `$signed` do — this is also what closes the
"`CellKind::Mul` executes unsigned in `ir::exec`" note this section used to
carry. And `validate`'s `CellKind::Mux` rule, which checked only the `sel`
pin, now checks BOTH data pins against `out`: F2/F3/F4 were all SILENT
precisely because it did not. That rule made `abs`'s own mux (whose `-x` arm
is a bit wider than its `x` arm) a true positive, so `Builtin::Abs` now
sign-extends the untouched arm explicitly — semantically a no-op, since that
arm is only selected when `x >= 0`.

Pinned by fifteen Executor-level regression tests in
`crates/mimz-core/src/ir/tests/lower_ct_width.rs`, each RED against the
pre-round-4 tree. `tests/golden/ir/async_reset.ir` was regenerated: its
`+% 1` literal and its reset `0` are now 8-bit constants instead of 1-bit
ones, with no change in cell count.

**Honest limit of this claim.** The sign axis and the width axis are now both
applied at every site the systematic `lower_expr(` call-site pass could
justify — that pass asked, for all 41 sites, whether the position's
sibling/declared context can require a width or sign a bare `lower_expr` would
miss. The sites deliberately left alone are recorded below. That is a
stronger statement than round 3's, but it is still a statement about the sites
this codebase has today, not a proof: four rounds have each found the same
disease at sites the previous round did not think to look at. The `Mux`
`a`/`b`-vs-`out` validation added here is the structural part — it turns this
class from silent to loud for every future mux — but `Concat`, `Replicate`,
`Slice` bases and shift amounts have no equivalent guard.

**Deliberately still out of scope.** `Concat`/`Replicate` results default to
unsigned — they merge multiple sources, so there is no single flag to inherit,
and the checker types both unsigned anyway (it rejects a `signed` operand of
`{…}` outright, E0403, so nothing signed can reach them). Their PARTS need no
resize either: E0405 ("a bare literal has no width inside `{…}`") rejects an
untyped constant part outright. A shift AMOUNT is deliberately not resized to
its left operand's width — `width_rules::shift_result` derives `<<`'s growth
from that width, so widening a constant amount would inflate the result — and
`BinOp::Shl`/`Shr` are excluded from `adapts_const_operand` for that reason.
`lower_const` stays context-free and unsigned, which is what `Ty::CtInt` is;
the context is applied by the caller, either by `||` against a sibling or by
stamping a declared type — see the third fix round above for why that is the
rule rather than a to-do.

### Sub-gap (2026-09-04, RESOLVED 2026-09-04): `ir::validate`'s driven-set seeding was direction-blind

Fixed as of 2026-09-04 (the 2026-09-04 GAP-1 fix round, Task 6): the
seeding loop now reads the `Dir` it previously destructured and discarded,
seeding `driven` only from `in`-direction ports. A new regression fixture,
`tests/fixtures/ir_errors/undriven_output_port.ir`, pins the case this
used to miss (a declared `out` port with zero driving cells).

### Sub-gap (2026-09-04, RESOLVED 2026-09-07 — GAP-1 residual Task 6): `ir::lower`'s single memory read port

`lower_expr`'s memory arm (`crates/mimz-core/src/ir/lower.rs`) used to
panic when one memory was read at two different addresses (comparing
LOWERED nets, so two DISTINCT read sites whose indices were textually
identical also tripped it).

**Fix — a new field, not numbered pins.** `CellKind::Mem` gains
`read_ports: Vec<(Bits, Bits)>`, one `(raddr, rdata)` entry per DISTINCT
lowered read address, populated by `LowerCtx::mem_read`
(`HashMap<String, Vec<(Bits, Bits)>>` — was one bare pair per memory). A
read at an existing port's address reuses it for free; a genuinely
different address grows a new entry instead of panicking. Deliberately
NOT numbered pins on `Cell::pins` (that map's keys are `&'static str`, so
numbered names would need either a fragile pre-interned cap or changing
the key type everywhere) — the single write port (`waddr`/`wdata`/`wen`,
plus `clock` when present) stays an ordinary pin set, unaffected.

**Every consumer updated.** `exec.rs`'s `Mem` arm loops `read_ports`,
publishing each port's `rdata` independently off its own `raddr` against
the same underlying storage. `validate.rs`'s driven-net check registers
each port's `rdata` as driven by its cell (no longer visible via the
generic `"out"|"q"|"rdata"` pin scan, since read ports aren't in `pins`).
Text format (`print_line`/`parse_line`/`print_sexpr`) round-trips ports as
numbered `raddrN`/`rdataN` entries — parsed generically into `pins` like
any other pin, then pulled back out into `read_ports` by `parse_cell_kind`
(now takes `pins` by `&mut` so it can `.remove()` them).

**The comb-cycle design question, resolved.** `find_combinational_cycle`
used to skip `Mem` entirely (treated as fully sequential, like `Dff`).
But `exec.rs`'s own `Mem` arm reads `rdata` as a pure function of `raddr`
and the pre-tick array — genuinely combinational, unlike `Dff`'s `d`->`q`,
which a clock edge breaks. So each read port's `raddr`->`rdata` edge is
now modeled as a real combinational edge; the write side
(`waddr`/`wdata`/`wen`/`clock`) stays excluded, mirroring `Dff`'s
treatment — those pins only affect state at `tick()`.

**`Module::signals`'s per-memory entry** (keyed by memory NAME, no way to
pick a port) is only populated when a memory has exactly one read port —
sound for every existing use, and the only case that was ever
unambiguous; a multi-port memory's individual reads simply aren't
addressable by name today (nothing in this codebase needs that).

New tests (`crates/mimz-core/src/ir/tests/lower_mem.rs`):
`a_second_read_at_a_different_address_grows_a_second_port` (replaces the
old `#[should_panic]` test — now asserts two independent ports, addresses,
and `rdata` nets) and `a_second_read_at_the_same_address_reuses_the_port`
(the pre-existing "same address" behavior, pinned as a regression).
`tests/golden/ir/regfile.ir` regenerated for the new `raddrN`/`rdataN`
text syntax (`MIMZ_UPDATE_GOLDENS=1 cargo test --test ir_golden`).

Not extended: `tests/differential_fuzz.rs`'s `gen_ir_clocked_module` (the
IR-vs-kernel differential leg) deliberately excludes `mem` by design (a
documented generator scope boundary, not a lowering gap — see its own
"given up" list) — wiring multi-port memory into that generator is a
separate, out-of-scope change, not required to close this residual.

Verification: `cargo test -p mimz-core ir::` (84 passed) clean; `cargo
test --workspace` clean (1418 passed); `cargo clippy --workspace
--all-targets -- -D warnings` clean.

### Sub-gap (2026-09-08, RESOLVED 2026-09-08 — GAP-1 residual Task 2): `ir::lower` panicked on any module parameter or file-level `const` referenced as a plain identifier

**What.** `lower_expr`'s `ExprKind::Ident` arm only ever checked `locals`
(inlined `fn`-call args) before falling to `resolve()`, which does
`self.design.comb.get(name)` and panics
(``no driver recorded for signal `{name}` (checker should have caught
this)``) if the name isn't found. A module parameter
(`module Blinker(LIMIT: int = 50000000)`) or a file-level `const` (`const
SCALE: int = 3`) is neither a `locals` binding nor a comb-driven signal —
the elaborator folds both into `design.consts: BTreeMap<String, i128>`
(`crates/mimz-core/src/elaborate/module.rs`), which `ir::lower` never
consulted at all. This plan's coverage probe hit this exact panic on 35 of
95 real failures — the single largest cause — across `blinker.mimz`
(`LIMIT`), `fn_with_const.mimz` (`SCALE`), `std/debouncer.mimz`
(`STABLE`), `std/fifo.mimz` (`DEPTH`), and their Tamil-pure equivalents.

**Fix.** `ExprKind::Ident`'s arm now tries `design.consts.get(name)`
between the `locals` check and the `resolve()` fallback, folding a hit
straight to a `Const` cell via `lower_const`. The `i128 -> ConstVal`
conversion reuses `checker::consteval::ConstVal::from_i128` — already the
established convention for exactly this "folded `i128` consts/params map"
shape (`value::build_env`'s doc comment names it explicitly; the same
constructor is used by every checker/emitter site that consumes
`design.consts`/`design.params`) — rather than hand-rolling the
two's-complement/width logic for a negative value a second time.

New tests (`crates/mimz-core/src/ir/tests/lower_consts.rs`):
`fn_body_references_file_level_const_as_shift_amount` (mirrors
`fn_with_const.mimz`'s `const SCALE: int = 3; fn scaled(a) { a >> SCALE }`
shape — asserts `lower()` no longer panics and the `Shr`'s shift-amount
pin traces to a `Const` cell holding `3`) and
`module_body_references_module_parameter_directly` (mirrors `blinker.mimz`'s
`if cnt == LIMIT` shape — a module parameter referenced directly in a
module body, asserting the `Eq`'s rhs traces to a `Const` cell holding
`1_000_000`).

Verification: `cargo test -p mimz-core ir::` clean; `cargo clippy
--workspace --all-targets -- -D warnings` clean.

### Sub-gap (2026-09-08, RESOLVED 2026-09-08 — Task 8): `lower_binop`'s catch-all can still be reached by `BinOp::Coalesce`, which is actually always-unreachable dead code

Found while reviewing the 2026-09-08 round's Task 1 (`Gt`/`Ge`/`LogicAnd`/
`LogicOr` production wiring): `BinOp` (`crates/mimz-core/src/ast/expr.rs:225`)
has 20 variants; after Task 1, `lower_binop`'s match handles 19 of them. The
last, `BinOp::Coalesce` (the `??` operator from the valid-bundle-sugar
feature, `bit?`/`bits[N]?` → `{ valid: bit, data: T }`, shipped 2026-07-17 —
spec/02 section 1.12a: unwrap `T? ?? T -> T` or OR-mux `T? ?? T? -> T?`,
decided by the right operand's shape), still falls to `lower_binop`'s
catch-all `unimplemented!("binop not yet lowered: {other:?}")`.

**First read as "no bundle/interface support in `ir::lower` at all" — traced
further and that read is WRONG; corrected here rather than left standing.**
Both of `??`'s forms are fully eliminated by `elaborate::rewrite` BEFORE a
`Design` exists, same discipline as `Pattern::Variant`→`Int`/`IntMask` and
`Clog2`/`SyncDoubleFlop`/`SyncPulse` (all three already `unreachable!()` in
`ir::lower`, not `unimplemented!()`):

- The **unwrap** form (`raw ?? 0`, scalar result) is rewritten by
  `Rw::expr`'s dedicated `ExprKind::Binary { op: BinOp::Coalesce, .. }` arm
  (`crates/mimz-core/src/elaborate/rewrite.rs:58-91`) into an ordinary
  `IfExpr` (`if raw.valid { raw.data } else { 0 }`), recursed into
  immediately — the rewritten tree contains no `Coalesce` node at all.
- The **OR-mux** form (`x ?? y`, both sides and the result stay bundle-typed)
  "never reaches this generic scalar-expression rewrite" (the rewrite's own
  comment, same lines) — it's intercepted earlier, at bundle-typed
  signal-declaration time, via `bundle_field_expr`
  (`crates/mimz-core/src/elaborate/bundle.rs:39-91`, itself referencing a
  prior "Task 10"), which decomposes a bundle-typed initializer into one
  scalar expression PER FIELD before any generic expression rewrite ever
  sees a bare `Coalesce` node.

So `BinOp::Coalesce` reaching `ir::lower` is unreachable in practice, exactly
like the three cases already marked so in the same file's `ExprKind::Call`
match. The real, narrow fix is a one-line `unreachable!()` arm in
`lower_binop`, mirroring that existing precedent — NOT a bundle/interface
lowering project. (`ir::lower`'s separate, general "expression form not yet
lowered... field access" catch-all wording is ALSO worth a second look for
the same reason — this investigation didn't find a probe failure that
actually hits `ExprKind::Field`, only ones hitting `Index`/`Replicate`,
which the message's parenthetical lumps in alongside a "field access" case
that may be similarly stale/unreachable rather than a live gap. Not chased
further here — flagged for whoever next touches that arm.)

**Not found by this round's own coverage probe** (129/224, later improved by
Task 1) because no file in `examples/` uses `?`/`??` — the probe can only
surface what real example programs exercise, and this was invisible to it.
Found only by reading `ast::BinOp`'s full definition after a reviewer's
finding named `Coalesce` as the sole remaining unhandled variant, then
corrected again by reading `elaborate::rewrite`/`elaborate::bundle` directly
rather than trusting the first (wrong) inference from the panic message's
own wording.

Tracked as its own small task
(`docs/superpowers/plans/2026-09-08-ir-remaining-gaps.local.md`'s Task 9) —
unlike Task 7 (general signed IR values, a genuine schema-level decision),
this one is NOT flagged as deferred: it's a same-shape, low-risk fix to the
existing `Clog2`/`SyncDoubleFlop`/`SyncPulse` precedent and belongs in this
plan's normal completion gate.

**Resolved (Task 8) — scoped to MODULE-level `??` use.** Re-verified this
entry's own citations directly rather than trusting them: `rewrite.rs`'s
`Binary { op: BinOp::Coalesce, .. }` arm is still exactly lines 58-91
(unchanged); `bundle.rs`'s `bundle_field_expr` Coalesce-interception block is
lines 47-92 (fn signature at 47, its Coalesce-handling `if let` at 63-92 —
the prior "39-91" citation started from the doc comment and undershot the
closing brace by one line; both now cited precisely). Then went further than
re-reading citations: added `lower_coalesce_is_unreachable_for_both_source_forms`
(`crates/mimz-core/src/ir/tests/lower_binops.rs`), which runs the REAL lex ->
parse -> check -> elaborate_project -> lower pipeline over both of `??`'s
forms AT MODULE LEVEL (the two `checker::tests::bundles` fixtures this
entry's own probe never reached — a wire declaration + assignment for each
form) and asserts both that `lower()` does not panic and that the elaborated
`Design` contains no `Coalesce` node at all (`{design:?}` scan) — positive,
empirical confirmation, not just re-reading the two files' source.
`lower_binop` now has an explicit `BinOp::Coalesce => unreachable!(...)` arm
(`crates/mimz-core/src/ir/lower.rs`, in the `BinOp` match) mirroring the
`Clog2`/`SyncDoubleFlop`/`SyncPulse` precedent's justification-comment style
— its comment and message are careful to scope the "never reaches
`ir::lower`" guarantee to module-level use (wire/reg decl, assignment,
instance connection, fn-call argument), NOT to a bundle-typed `fn` parameter
referenced bare inside its own body — see the new sub-gap immediately below,
found by this same fix round's own review and NOT covered by the guarantee
above.

This entry's own parenthetical about `lower_expr`'s separate "field access"
catch-all wording was also re-checked (not chased further, per that
parenthetical's own hedge): `elaborate::rewrite::Rw::field` has a genuine
fallback arm (`rewrite.rs`, "Some other field access — keep the shape") that
re-emits a bare `ExprKind::Field` node whenever `base` isn't a plain `Ident`
naming an enum/instance/bundle-signal — so a "field access can't reach
`ir::lower`" claim is NOT confirmable by a quick trace the way `Coalesce`
was; it stays an open question for whoever next touches that arm, not
resolved here.

Verification: `cargo test -p mimz-core ir::` clean; `cargo clippy
--workspace --all-targets -- -D warnings` clean.

### Sub-gap (2026-09-08, RESOLVED 2026-09-15 for the unwrap form): `??` against a bare bundle-typed `fn` parameter bypasses elaborate's Coalesce elimination entirely, and currently crashes via a separate, undocumented `Ident`-resolution panic

Found by Task 8's own fix-round review, re-checking the "Resolved" claim
above against a shape neither this entry's original investigation nor
Task 8's test covered: a bundle-typed `fn` PARAMETER referenced BARE (not
via `.field`) inside that fn's own body, combined with `??`:

```
bundle Handshake(W: int = 8) { valid: bit  data: bits[W] }
fn get_or(h: Handshake(W: 8)) -> bits[8] { h ?? 0 }
module M { in c: bit  in d: bits[8]  out o: bits[8]
  o = get_or({ valid: c, data: d }) }
```

`checker::check` accepts this (empirically confirmed). The elaborated
`Design`'s `funcs["get_or"].tail` is a genuine, un-eliminated
`Binary { op: Coalesce, lhs: Ident("h"), rhs: Int(0) }` node — `elaborate`'s
usual Coalesce-elimination path never runs on it. Root cause:
`flatten_bundle_refs_expr` (`crates/mimz-core/src/elaborate/bundle.rs:234-260`)
— the fn-body counterpart of `Rw::field`'s bundle handling, used ONLY for
`design.funcs` bodies (never the general `Rw::expr`) — guards its rewrite on
`ExprKind::Field { base: Ident, .. }` (line 235-240): it only rewrites
`param.field` reads. Its `Binary` arm (line 256-260) just recurses into
`lhs`/`rhs` with no Coalesce-specific desugaring, so a bare `Ident("h")`
sitting inside `h ?? 0` is structurally untouched and passed through as-is.

This does NOT currently reach Task 8's new `unreachable!()` arm — verified
empirically (`cargo test`, ad hoc probe): lowering this exact program panics
one step EARLIER, at `ir::lower`'s `resolve()` ("no driver recorded for
signal `h` (checker should have caught this)"), because the bare `Ident`
`h` was never flattened to any of `get_or`'s real (per-field) parameter
names and so isn't found among `locals` or `design`'s signals either. That
earlier panic is itself a separate, pre-existing, undocumented bug in
bare-bundle-param `Ident` resolution — NOT something Task 8 introduced or is
scoped to fix. The two bugs currently mask each other (the `Ident` panic
fires first, so the `Coalesce` gap is invisible in practice) but are
independent: fixing the `Ident`-resolution bug without also teaching
`flatten_bundle_refs_expr` about `Coalesce` would very plausibly turn this
into a live hit on Task 8's `unreachable!()` arm for a checker-legal
program.

Not fixed here — out of scope for Task 8 (which only owns `lower_binop`'s
`Coalesce` arm and its own justification), left for whoever next touches
`flatten_bundle_refs_expr` or fn-body bundle-parameter handling generally.

**Resolved 2026-09-15, for the unwrap form only.** Root-caused as two
independent, currently-masking-each-other bugs, both in
`flatten_bundle_refs_expr` (`elaborate/bundle.rs`), re-verified against live
source rather than trusting this entry's own citations (line numbers had
drifted since 2026-09-08):

1. **The real bug.** `flatten_bundle_refs_expr`'s only special case was
   `Field { base: Ident(p), field }` → `Ident("{p}_{field}")`
   (`bundle.rs:235-240`); its `Binary` arm (`bundle.rs:256-260`) just
   recursed into `lhs`/`rhs` generically, with no `BinOp::Coalesce` case at
   all — unlike `Rw::expr`'s module-level counterpart
   (`elaborate/rewrite.rs:58-91`), which already desugars the unwrap form
   to `IfExpr{cond: lhs.valid, then: lhs.data, els: rhs}`.
   `flatten_bundle_refs_expr` simply never learned the same trick, so a
   bare `Ident("h")` inside `h ?? 0` walked through unchanged.
2. **The masking bug (separate, still open).** Because bug 1 left an
   unflattened `Ident("h")` inside a surviving `Coalesce` node, and the
   flattened `fn`'s param list no longer has a param named `h` (only
   `h_valid`/`h_data`), `ir::lower`'s `ExprKind::FnCall` inlining panicked
   resolving `Ident("h")` — ``"no driver recorded for signal `h`"`` —
   before `lower_binop` ever saw the `Coalesce` node. This is why the
   panic this entry originally reported was the `Ident`-resolution one,
   not Task 8's `unreachable!()` arm. Fixing bug 1 alone removes the
   `Coalesce` node entirely (same elimination mechanism as every other
   working case), so bug 2 never gets a chance to fire for THIS shape —
   confirmed by tracing the resulting AST, not assumed. Bug 2 itself
   (`ir::lower`'s `resolve()`/`Ident` handling has no notion of a bundle
   param at all) is unfixed and would still bite a DIFFERENT bare-bundle-
   param shape that has no `??` involved at all, e.g. a `fn` returning a
   whole bundle by identity (`fn f(h: Handshake) -> Handshake { h }`) —
   not reproduced or scoped here, left as a distinct residual for whoever
   next touches bare (non-`.field`, non-`??`) bundle-param `Ident`
   resolution.

**Fix.** `flatten_bundle_refs_expr` gained its own `Binary{Coalesce}` case,
mirroring `Rw::expr`'s desugaring exactly (`.valid`/`.data` `Field` access
on `lhs`, `IfExpr{cond, then, els: rhs}`), then recursing the rewritten
node back through `flatten_bundle_refs_expr` itself so the `Field {
base: Ident(h), .. }` nodes it just produced get flattened to
`Ident("h_valid")`/`Ident("h_data")` by the function's own pre-existing
check — no new mechanism, just one more call site for an already-proven
one. `ir::lower`'s `BinOp::Coalesce => unreachable!()` arm (Task 8) needed
no code change, only a comment correction: it used to explicitly disclaim
this shape ("NOT proven for a bundle-typed fn parameter referenced bare");
that sentence is now stale and was corrected in the same change, same
discipline Task 8 itself used when it corrected this entry's own first
(wrong) hypothesis.

**Scope: unwrap form only.** The OR-mux form (`x ?? y`, both sides and the
result stay bundle-typed) is still open.

**Re-scoped 2026-09-16 — this is NOT a narrow Coalesce corner, it's an
instance of a broader, pre-existing gap: bundle-typed `fn`-call RESULTS are
entirely unlowerable in `ir::lower`, independent of `??` altogether.**
Empirically probed (two throwaway tests, not committed — reproducible from
the shapes below) rather than assumed from reading alone, because the first
write of this note (2026-09-15) guessed the boundary wrong:

- **Baseline probe, zero `??` involved:** `fn make_handshake(v: bit) ->
Handshake(W: 8) { { valid: v, data: 0 } }` (a plain `BundleLit` tail, the
  same shape `checker::widths::mod::bundle_literal_tail_return_is_shape_checked`
  already type-checks) called from `wire req: Handshake(W: 8) =
make_handshake(a)`. Checker accepts it; `elaborate_project` succeeds and
  produces `design.comb["req_valid"] = Field{base: FnCall{make_handshake,
[a]}, field: "valid"}` (and the matching `"req_data"` entry) — an
  UNRESOLVED `Field`-on-`FnCall` node, because `bundle_field_expr`
  (`bundle.rs:47-114`) only special-cases `Coalesce`/`BundleLit`/`Ident`;
  anything else (including a bundle-returning `FnCall`) falls to its
  generic "keep the shape" fallback, and `Rw::field`'s own fallback
  (`rewrite.rs:385-392`) does the same — neither inlines the callee. This
  `Field` node then hits `ir::lower`'s existing generic catch-all
  (`lower.rs`, "expression form not yet lowered... field access") and
  panics — with NO `Coalesce` anywhere in the program.
- **The OR-mux probe** (`fn combine(a: Handshake(W: 8), b: Handshake(W: 8))
-> Handshake(W: 8) { a ?? b }`, called from a bundle-typed wire the same
  way) panics at the EXACT SAME `Field`-on-`FnCall` site, for the exact
  same reason — the call-site field access on `combine(...)`'s result
  fails before `ir::lower` ever gets far enough to inline `combine`'s own
  body and reach the `Coalesce`-turned-`IfExpr` tail at all.

So "the OR-mux form for a bundle-typed fn TAIL" was the wrong frame: it's
unreachable not because `flatten_bundle_refs_expr` lacks an OR-mux case
(true, but moot), but because NO bundle-returning `fn` call — `??`-based or
not — survives past its own call site's field access today.
`flatten_bundle_params_in_func` only ever flattens PARAMS (confirmed by
reading it, `bundle.rs:127-173`); nothing analogous exists for a `fn`'s
`ret`/tail, and nothing in `ir::lower` inlines a callee to answer "just
give me field `f` of what this call returns" the way it already does for
a plain scalar call (`ExprKind::FnCall`'s existing continuation-splicing
arm, `lower.rs:569-632`, always produces ONE flat `Bits` for the WHOLE
call, with no per-field slicing capability).

Closing this needs either (a) an elaboration-time inlining step that lets
`bundle_field_expr` recurse into a bundle-returning `FnCall`'s (param-
substituted) callee tail the same way it already recurses into `Coalesce`/
`BundleLit`, or (b) teaching `ir::lower` a genuine `ExprKind::Field`
handler that can inline the callee and slice out one field's `Bits` from
the result. Either is a real design decision (which layer owns bundle-
typed-fn-call inlining, and whether it composes with the existing scalar
`FnCall` continuation-splicing or replaces it for the bundle case) — same
weight class and same Decision-block gate as the two 2026-09-10 IR
Decisions (signed `Bits`, loop unrolling), not a fix-in-place residual.
**Not attempted here** — filed as its own, broader, still-open item;
the `??`-specific framing this entry used from 2026-09-08 through
2026-09-15 is retired in favor of this one.

New test (`crates/mimz-core/src/ir/tests/lower_binops.rs`):
`bare_bundle_typed_fn_param_coalesce_unwrap_is_eliminated` — runs the real
lex → parse → check → `elaborate_project` → `ir::lower` pipeline over
`fn get_or(h: Handshake(W: 8)) -> bits[8] { h ?? 0 }` called from a real
module, asserting `lower()` does not panic, the elaborated `Design`
contains no `Coalesce` node anywhere (`{design:?}` scan, same strength as
Task 8's own test), and the lowered module validates cleanly.

Verification: `cargo test -p mimz-core ir::` clean (106 passed); `cargo
test --workspace` clean (1440 passed); `cargo clippy --workspace
--all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean.

Plan: `docs/superpowers/plans/2026-09-15-bare-bundle-param-coalesce.local.md`
(deleted once this entry and `docs/log/2026-09-15.md` carried its content).

### Sub-gap (2026-09-08, RESOLVED 2026-09-08 — GAP-1 residual Task 4): `ir::lower` could not lower `ExprKind::Replicate`

`lower_expr`'s `ExprKind::Replicate` arm (`{N{x}}` replication, Verilog-style)
hit the catch-all `unimplemented!()` for expression forms not yet lowered. The
`count` parameter always const-folds (checker-enforced, same guarantee `Slice`
bounds and `Shl` constant amounts already rely on), and the result is pure
bit-vector reassembly: each part of `parts` lowers once (memoized via
`expr_memo` as usual), then that entire sequence repeats `count` times, with
no new cell allocation — exactly the "no cell, just re-point nets" strategy
`ExprKind::Concat` already uses one match arm above.

**Fix.** `lower_expr`'s new `ExprKind::Replicate` arm (`crates/mimz-core/
src/ir/lower.rs:559-572`) const-evals the `count` expression against
`design.consts` (reusing the same `crate::value::const_eval` call-site pattern
and error message as `Slice`'s `hi`/`lo` bounds), then iterates `count` times
over `parts.iter().rev()` (MSB-first ordering, identical to `Concat`'s
reversal), extending the output net vector once per part. The implementation
mirrors `Concat`'s exact style and ordering convention, ensuring nets are
reused without reallocation: three references to the same signal in
`{3{a}}` produce three `NetId` values all pointing to the same underlying net.

New tests (`crates/mimz-core/src/ir/tests/lower_unary_concat_slice.rs`):
`lowers_replicate_reuses_same_nets` (`{3{a}}` for 1-bit `a` produces a 3-bit
`Bits` where all three net references are identical `NetId` values, not fresh
allocations — asserts `rep_bits.0[0] == rep_bits.0[1] == rep_bits.0[2]`)
and `lowers_replicate_preserves_msb_first_ordering` (`{2{a,b}}` for 1-bit
signals produces 4-bit pattern `[b,a,b,a]` in LSB-first indexing, which reads
as `[a,b,a,b]` in source MSB-first order — asserts the correct net names at
each index).

Verification: `cargo test -p mimz-core ir::` clean (96 passed); `cargo
clippy --workspace --all-targets -- -D warnings` clean.

---

### Sub-gap (2026-09-08, RESOLVED 2026-09-08 — GAP-1 residual Task 5): `ir::lower` could not lower a plain-vector bit-select `v[i]`

`lower_expr`'s `ExprKind::Index` arm only handled a memory-word read
(`self.indexed_mem(base).is_some()`, `crates/mimz-core/src/ir/lower.rs:432`);
`v[i]` on anything else (a plain `bits[N]`/`signed[N]` value, not a
`design.mems` entry) fell through to the catch-all `unimplemented!()`.

**Preflight.** Unlike `Slice`'s `hi`/`lo` (which MUST const-fold —
`checker/widths/expr/lvalue.rs`'s `const_bound`, line 199, errors on
anything `consteval::eval` can't resolve), a plain-vector single-bit index
is NOT required to const-fold. `index_in_range`
(`crates/mimz-core/src/checker/widths/expr/lvalue.rs:72-102`) only range-
checks a `Ty::CtInt` index (line 75); a `Ty::Bit`/`Ty::Bits(_)` index — a
genuine runtime signal — passes through unchecked at line 86. So both a
constant index (`a[0]`, `fn_return_guard.mimz`/`uart_tx.mimz`'s real
failures) and a runtime index (`a[i]` for a signal `i`) are checker-legal,
and both needed lowering, not just the constant case the plan flagged as
"near-certain."

**Fix.** `lower_expr`'s new `ExprKind::Index` arm (no guard, sibling to the
memory-read arm above it; `crates/mimz-core/src/ir/lower.rs:454-487`)
lowers `base` once, then tries `crate::value::const_eval` on `index`:

- **Constant** (`Ok`): pure re-pointing, no cell — `Bits(vec![base_bits.0[i]])`,
  identical strategy to the existing `Slice` arm.
- **Runtime** (`Err`): composes the existing `BinOp::Shr` lowering (via
  `lower_binop`, unchanged) to compute `base >> index`, then takes net-index
  0 of THAT result. `Shr`'s output width is always `a.width()` regardless of
  the shift amount (`lower_binop`'s `BinOp::Shr` arm,
  `crates/mimz-core/src/ir/lower.rs:937`), so bit 0 of the shift's own
  freshly-allocated `Bits` is always net-index 0 — a constant slice despite
  `i` itself being runtime, needing no new bit-level indexing machinery.

The stale catch-all message (which used to name a bit-select on a plain
vector as still-missing "bit-level indexing machinery") was trimmed since
this arm now handles it.

New tests (`crates/mimz-core/src/ir/tests/lower_unary_concat_slice.rs`):
`lowers_constant_index_to_a_single_bit_repoint` (`a[0]` for 8-bit `a`
produces a 1-bit `Bits` whose single net is exactly `a`'s bit-0 `NetId`,
asserted directly — not just "doesn't panic" — and asserts `module.cells`
stays empty) and `lowers_runtime_index_via_shr_and_a_zero_slice` (`a[i]`
for a runtime 3-bit `i` produces exactly one `Shr` cell whose `out` pin
stays 8 bits wide, and the result `Bits` points at net-index 0 of that
`out`).

Verification: `cargo test -p mimz-core ir::` clean (98 passed); `cargo
clippy --workspace --all-targets -- -D warnings` clean.

---

### Sub-gap (2026-09-08, RESOLVED 2026-09-08 — GAP-1 residual Task 6): `ir::lower` could not lower an array-typed `fn` param

`lower_expr`'s `ExprKind::FnCall` arm unconditionally `unimplemented!()`d the
moment any callee param was `Type::Array { .. }` — `fn_array_search.mimz`,
`foreach_sum.mimz`, and Tamil-pure `kootu.mimz` all hit this.

**Preflight (ported, not designed).** `emit_verilog` and the AST/value
evaluator already agree on ONE convention for this, confirmed by direct
reading rather than assumed: an array-typed param of length N flattens to N
scalar bindings named `"{param}_{i}"` (`i` in `0..N`), each at the array's
element width.

- Emitter: `crates/mimz-core/src/emit_verilog/module/funcs.rs:59-67` builds
  an `arrays: ArrayScope` bookkeeping map per array-typed param/`let`;
  `funcs.rs:135-151` declares the N `input {ew}{param}_{i};` ports;
  `crates/mimz-core/src/emit_verilog/expr.rs:996-1007` expands a call-site
  `ArrayLit` argument element-by-element and a bare in-scope array `Ident`
  to its `"{n}_{i}"` scalars.
- AST/value evaluator (what `mimz-sim` runs on): `crates/mimz-core/src/
value/fn_eval.rs:56-104`, specifically line 100 —
  `locals.insert(format!("{}_{i}", param.name.name), ...)` — binds each
  array param element under the identical key, with its own comment stating
  "the SAME `<name>_<i>` convention the emitter uses for its scalar ports".

**A real, confirmed second gap, not just the `call_locals` one.** Neither
`emit_verilog` nor the evaluator resolves `vals[i]` as a plain bit-select —
both special-case a bare `Ident` naming an in-scope array FIRST
(`value::mod.rs:516-548`, specifically the `array_len(name)` check at
`524-528`, which resolves each element via `r.signal(&format!("{name}_{i}"))`
before ever falling through to the generic bit-select code below it).
`ir::lower`'s `ExprKind::Index` arm (Task 5, above) had no such branch and no
array-scope concept anywhere in `LowerCtx` — confirmed by grepping
`arrays|array_len|ArrayScope` across the whole file (zero hits) before this
task started. So Task 6 was originally dispatched scoped to `call_locals`
construction only; that preflight finding was reported BLOCKED (a flattened
`"vals_0".."vals_3"` `call_locals` map is unreachable from a real `fn` body,
since `vals[i]` would resolve `Ident("vals")` through `locals` → `design.
consts` → `self.resolve()`, and `resolve()` panics — there is no `"vals"`
key in this scheme, only its per-element scalars). The controller then
explicitly expanded Task 6's scope to cover both ends together, since they
are not separable in practice — this entry covers that expanded scope, not
the original brief's narrower one.

**Fix — two ends of the same change.**

1. **`call_locals`/`call_arrays` construction**
   (`crates/mimz-core/src/ir/lower.rs:564-623`, the `ExprKind::FnCall` arm):
   for each param, if it's `Type::Array { .. }`, its call-site argument MUST
   be an `ExprKind::ArrayLit` (the only shape possible — a module signal can
   never itself be array-typed, E0416, so there is no surface syntax for
   anything else); each element is lowered individually and bound under
   `"{param}_{i}"`, and `call_arrays` records the element count under the
   plain param name. A non-array param is unchanged: one `call_locals` entry
   under its own name.
2. **Array-scope threading + `ExprKind::Index`'s new array branch**
   (`crates/mimz-core/src/ir/lower.rs:461-560`): `arrays: Option<&HashMap<
String, u32>>` was added as `locals`'s sibling parameter everywhere
   `locals` already flowed (`lower_expr`, `lower_expr_sized`,
   `lower_fn_stmts`, `lower_match`), `Some` exactly when `locals` is (an
   array param's scope only ever exists alongside its call's own
   `call_locals`). `ExprKind::Index` now checks — mirroring `value::mod.rs`'s
   own dispatch order exactly — whether `base` is a bare `Ident` naming a key
   in `arrays` FIRST, before falling through to the untouched plain-vector
   bit-select logic:
   - **Constant index:** direct re-pointing to `locals["{name}_{i}"]` — no
     cell, same "pure re-pointing" shape as `Slice`/the plain-vector constant
     case, just at element width instead of 1 bit.
   - **Runtime index:** an `if idx==0 {name_0} else if idx==1 {name_1} else
... {name_{len-1}}` chain, folded from the LAST element backward
     (mirrors `lower_match`'s own reverse fold) using this file's existing
     `Eq` (`push_binary_cell`) and `Mux` cells — no new `CellKind`. An
     out-of-range runtime index falls through every `Eq` to the
     unconditional last element, matching the emitter's ternary-chain
     default (spec/02 §1.14) and `value::mod.rs`'s own clamp-to-last
     behaviour.

New tests (`crates/mimz-core/src/ir/tests/lower_array_fn_params.rs`, new
file), mirroring `fn_array_search.mimz`'s `pick(vals: bits[8][4], idx:
bits[3]) -> bits[8] { vals[idx] }` shape:
`lowers_constant_index_into_array_param_to_the_flattened_elements_bits`
(`fn third(vals: bits[8][4]) { vals[2] }` called `third([a,b,c,d])` —
asserts the result `Bits` equals `c`'s own port `Bits` exactly, and
`module.cells` stays empty) and
`lowers_runtime_index_into_array_param_via_eq_mux_chain` (`pick`'s own
shape, called `pick([a,b,c,d], idx)` — asserts exactly 3 `Eq` + 3 `Mux`
cells, walks the chain from the outermost `Mux` driving the output inward
through each `Eq`'s constant selector (0, then 1, then 2) via concrete
`NetId`/pin equality, and asserts the innermost `Mux`'s unconditional
default (`b` pin) is `d`'s own `Bits` with no further `Eq` — pinning the
clamp-to-last-element behaviour, not just "doesn't panic").

Also updated `docs/code/10-test-map.md` and `README.md`'s test-count badge
(1428 → 1434, `tests/docs_sync.rs`'s `test_count_matches_docs_and_badge`
mechanically enforces this) to account for this task's +2 together with
Task 4's and Task 5's +2 each, none of which had been folded in yet.

Verification: `cargo test -p mimz-core ir::` clean (100 passed, up from 98);
`cargo test --workspace` clean (1434 passed); `cargo clippy --workspace
--all-targets -- -D warnings` clean.

### Sub-gap (2026-09-09, OPEN, architectural — Task-7-adjacent): `fn`-body `foreach`/loop unrolling is not lowered by `ir::lower`

Found by Task 9's re-run of this plan's coverage probe (184/224, up from
129/224): `fn_array_search.mimz`, `foreach_sum.mimz`, and the Tamil-pure
`kootu.mimz` (10 of the 40 remaining failures, all flavors) still panic
after Task 6 landed, but on a DIFFERENT arm than the one Task 6 fixed —
`lower_fn_stmts`'s `FnStmt::Loop`/`FnStmt::ForEach` arm
(`crates/mimz-core/src/ir/lower.rs:942-948`):

```
not implemented: loop/foreach unrolling inside fn bodies not yet lowered by
Task 9 (needs const-var-substitution machinery, same gap as Task 8's
on-block Loop/ForEach); span: ...
```

**Distinct from Task 6, not a regression of it.** Task 6 (the sub-gap
immediately above) fixed array-typed `fn` PARAMETER flattening and
per-element indexing (`vals[idx]`) — its own fixture is a one-line
`pick(vals, idx) { vals[idx] }` body with no loop construct anywhere. This
gap is a `foreach`/`repeat`-style loop written INSIDE an `fn` body's own
statement list (`examples/*/foreach_sum.mimz`'s `fn sum8(values, acc) {
foreach v in values { let acc = acc +% extend(v, 11) } acc }`) — a
structurally separate AST shape (`FnStmt::Loop`/`FnStmt::ForEach`, not
`ExprKind::Index`/`ExprKind::FnCall`) that Task 6 never touched or claimed
to fix. Confirmed by grep: zero references to `FnStmt::Loop`/`FnStmt::ForEach`
anywhere in Task 6's fix or its tests. The three examples Task 6's own "Why"
section named as motivation clear Task 6's panic and immediately hit this
next, adjacent one — invisible to every prior probe because the array-param
panic fired first and masked it.

**Naming coincidence, not a self-reference.** The panic text's own "Task 8"/
"Task 9" citations are STALE — they refer to the OLDER 2026-09-05 GAP-1
residual round's own task numbering (this exact arm predates the current
2026-09-08 plan entirely; neither this plan's Task 8 nor its Task 9 wrote or
touched it). A future reader should not confuse this with the current plan's
own Task 9 (this entry) or Task 8 (the `Coalesce` sub-gap above).

**Why this is architectural, not a quick fix.** Per the panic message's own
diagnosis, unrolling a loop inside an `fn` body needs "const-var-substitution
machinery" — binding the loop variable to each successive constant and
re-lowering the body once per iteration, inline, inside a function-call
context that already threads `locals`/`arrays` through recursively. This is
a new lowering capability, not a missing cell or a resize, and it is the
same shape of decision this project's constitution requires a dated plan +
Decision-block for before code (same gate as Task 7, general signed IR
values) — not something to fold into a "docs sync" task's normal gate.

Not fixed here. Filed as a new dated plan candidate for whoever picks it up
next, same as Task 7.

### Sub-gap (2026-09-09, OPEN, narrow/fixable): `fn_return_guard.mimz` — the `if`/`return` mux-tree in `lower_fn_stmts`'s `FnStmt::If` arm sizes literals to their own natural width, not the function's declared return width

Found by the same Task 9 probe re-run: `fn_return_guard.mimz` (all 5
flavors) now validates with

```
PortWidthMismatch { port: "idx", declared: 4, found: 3 }
```

`fn find_first_set(a: bits[8]) -> signed[4] { if a[0]==1 { return 0 } ...
if a[7]==1 { return 7 } -1 }` lowers its `if`/`return` chain to a
right-nested `Mux` tree (`lower_fn_stmts`'s `FnStmt::If` arm,
`crates/mimz-core/src/ir/lower.rs`, `out_width = then_val.width().max(
else_val.width())`). Each `then_val` is a bare integer literal (`0`..`7`)
lowered at ITS OWN natural width (`crate::bits::natural_width`) — never
resized to `find_first_set`'s declared `signed[4]` (4-bit) return type. The
widest literal (`7`) naturally needs 3 bits, so the whole tree comes out
3 bits, one short of the port it ultimately drives.

**Same root-cause class as Task 3 (sub-gap above), a third call site Task 3
never reached.** Task 3 fixed exactly this "bare literal/const-ident sized
at natural width instead of use-context width" bug at two call sites
(`ExprKind::Binary`'s sibling-matching, and `resolve()`'s wire/output-driver
path, via the new `lower_expr_sized` helper). `lower_fn_stmts`'s
`FnStmt::If`/return-mux construction is a third, independent call site with
the identical bug, which Task 3's brief did not name and Task 3's
implementation did not touch.

**Why this was invisible until now.** Task 5 (plain-vector bit-select,
sub-gap above) named `fn_return_guard.mimz` as one of its two motivating
examples and asserted it "lowers cleanly" after that fix — verified via a
targeted unit fixture (`a[0]` in isolation), not the real example file
through the full pipeline. The bit-select panic Task 5 fixed was masking
this next-layer width bug the same way Task 6's panic masked the sub-gap
immediately above.

**Not fixed here** — out of Task 9's docs-only scope. A natural candidate
for whoever next extends `lower_expr_sized`'s reach (or an equivalent
sizing pass) to `lower_fn_stmts`'s return-value mux construction — narrow,
same shape as Task 3, not a new architectural question.

---

## GAP-2 (MEDIUM) - Simulator is 2-state with a whole-value unknown flag; no X/Z, no tri-state

**Status:** OPEN. Filed 2026-08-02.

**What.** `Val` (`crates/mimz-sim/src/sim/value/mod.rs:36`) carries
`unknown: bool` - a single flag for the entire vector, with `bits` documented as
_"MEANINGLESS when `unknown` is `true`."_ There is no per-bit X, no Z, and no
`inout`. A `grep` over `spec/` returns zero hits for `inout` / `tristate` /
`'bz`.

**Why it matters.**

- `{1'b0, x_value}` cannot be modeled - the known half is lost.
- Uninitialized-register detection (the number-one real-world use of X in RTL
  simulation) is impossible.
- Bidirectional buses, open-drain I²C, external memory DQ pins - all
  unrepresentable. This blocks most board-level IoT work, which the README lists
  as a target domain.
- X-optimism / X-pessimism mismatches against the emitted Verilog are
  undetectable, so an entire class of sim-vs-synthesis divergence has no oracle.

**Direction.** Two-plane representation, the standard encoding (Verilator,
CXXRTL):

```rust
pub struct Val { pub val: Bits, pub mask: Bits, pub width: u32, pub signed: bool }
// mask bit set = that bit is unknown
```

Costs one extra `Bits` per value; turns every operator's X-propagation into a
bitwise rule rather than a special case.

Ship `inout` / tri-state **after**, as a separate design - it needs
resolution-function semantics that directly contradict the current single-driver
rule (E0501).

**Sequencing.** Build on top of
[GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times),
not before it - otherwise the X rules get written twice.

---

## GAP-3 (MEDIUM) - Parser violates the project's own mandatory-help contract

**Status:** CLOSED 2026-08-04. Filed 2026-08-02.

**What.** `Checker::err` (`crates/mimz-core/src/checker/mod.rs:109`) takes `help`
as a **required** parameter - _"the teaching contract (spec/01 G1) is not
optional."_ The parser's `Parser::error`
(`crates/mimz-core/src/parser/mod.rs:164`) takes no help at all; `self.help()` is
a separate opt-in call.

**Evidence.** Counted across `crates/mimz-core/src/parser/`:

| File                     | `self.error(` | `self.help(` |
| ------------------------ | ------------- | ------------ |
| `expr.rs`                | 13            | 4            |
| `mod.rs`                 | 5             | 1            |
| `items/mod.rs`           | 6             | 2            |
| `items/module.rs`        | 8             | 3            |
| `items/seq.rs`           | 6             | 3            |
| `items/file.rs`          | 5             | 1            |
| `items/test.rs`          | 8             | 0            |
| `items/func.rs`          | 4             | 0            |
| `items/extern_module.rs` | 4             | 0            |
| `items/bundle.rs`        | 1             | 0            |
| `items/inst.rs`          | 0             | 0            |
| **Total**                | **60**        | **14**       |

Roughly **77% of syntax errors ship with no `= help:` line.** Observed:

```text
error[E1101]: expected a value here, found `}`
  --> p.mimz:1:56
   |
  1 | fn f(x: bits[4]) -> bits[8] { return extend({x, x}, 8) }
   |                                                        ^
```

No help. The real issue is that a `fn` needs a tail expression, not just
`return` - a beginner cannot recover from that message.

**Why it matters.** Syntax errors are the **first** errors a learner hits. This
is the highest-traffic diagnostic surface in the compiler and it is the least
covered. G1 ("beginner-first, measurably") is a stated constitutional goal.

**Direction.**

1. Change the signature so the decision cannot be skipped, mirroring the checker:

   ```rust
   fn error(&mut self, span: Span, code: &'static str,
            msg: impl Into<String>, help: impl Into<String>)
   ```

2. Fill in all 60 sites.
3. Add a test asserting every `E11xx` fixture renders a `= help:` line, the same
   way `tests/errors.rs` already guards the `E0xxx` catalog.

**Fix.** `Parser::error`'s signature now requires `help: impl Into<String>`
(`crates/mimz-core/src/parser/mod.rs`) - byte-for-byte the drafted signature
above, mirroring `Checker::err`. All 60 call sites across `mod.rs`, `expr.rs`,
and the 8 `items/*.rs` files filled with a construct-specific teaching help
line; the 14 sites with a separate opt-in `self.help(...)` had it folded into
the `error()` call, and `help()` itself deleted (no longer reachable).
Test: `every_parse_error_carries_a_help_line`
(`crates/mimz-core/src/parser/tests/safety_and_precedence.rs`) sweeps a
broken-source case per `E11xx` code (in-crate, mirroring the existing
`every_parse_error_carries_a_code` structural test, rather than a new
fixtures-directory + CLI subprocess suite - the same contract, smaller
surface). Watched it fail (`garbage here` / E1102 had no help) before the fix.

---

## GAP-4 (LOW→MEDIUM) - String-keyed name resolution throughout; no interning

**Status:** OPEN. Filed 2026-08-02.

**What.** `ExprKind::Ident(String)`, `HashMap<String, …>` symbol tables,
`HashMap<(usize, String), Rc<Scope>>` scopes, and ~203 `.clone()` calls in
non-test `mimz-core`. Every one of the nine checker passes re-resolves each name
by string hash; the emitter and the simulator then do it again. Only
`QualIdent.resolved_file` caches anything.

**Why it matters - and why it is not urgent.** Measured on a release build:

| Design                    | `mimz check` | `mimz compile` |
| ------------------------- | ------------ | -------------- |
| 4,008 lines / 2,000 regs  | 0.77 s       | 1.67 s         |
| 16,008 lines / 8,000 regs | 1.65 s       | -              |

Scaling is roughly linear (4× input → 2.1× wall, startup-dominated). No quadratic
blow-up in name resolution, driver analysis, or the combinational-cycle DAG
check - better than a `HashMap<String, _>` design would suggest.

But there is no interning, no `SymbolId`, and no arena. The ceiling arrives with
real designs (a soft CPU plus peripherals is 50–200k lines of generated RTL) and
it arrives as a wall, not a slope.

**Direction.** Intern identifiers to `Symbol(u32)` during parse, key everything on
the id, and give `Ident` a `Cell<Option<DefId>>` resolved once. Expect 3–10× on
the checker and a large drop in allocator pressure.

**Sequencing.** Do this as part of
[GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times),
**not** as a standalone refactor - otherwise the churn is paid twice.

---

## GAP-5 (HIGH, testing) - No declared-type-vs-produced-value oracle; self-determined positions ungenerated

**Status:** CLOSED 2026-08-04. Both directions' testing infrastructure
landed 2026-08-03 (static matrix, width-conformance property, randomized
position-aware generation), and everything that infrastructure found
(BUG-34, BUG-35, BUG-36) is now fixed - see `bugs.md` for each. The
oracle gap itself (the actual subject of this entry) is closed: the
fuzzer now asserts width-conformance on every run, and its generator now
reaches every self-determined position with random `Builtin`-wrapped
fragments, not just hand-picked ones.

**Update 2026-08-03 (branch `bug-33-gap-5-perf-and-width-oracle`).**
Direction 1's width-conformance assertion landed in
`tests/differential_fuzz.rs` (`assert_bits_fit_width`): after every kernel
evaluation (both the combinational and clocked differential tests), every
signal's produced `Bits` is checked against the width the SIMULATOR itself
resolved during elaboration (`comb::Output::width`, `Timeline::signals`) -
an independent authority from the fuzzer's own generator bookkeeping. No
generator change was needed, as GAP-5's own direction predicted. Running the
now-instrumented fuzzer at deeper `N` (validating the new assertion) found
zero width-conformance violations but surfaced an unrelated, real
kernel-vs-Icarus divergence at `N=100` - filed and fixed as
[BUG-34](bugs.md) (chained shifts on a signed operand).

**Update 2026-08-03 (branch `bug-34-chained-signed-shifts`, same day).**
Direction 2's fuzzer generator extension landed too - GAP-5 is now FULLY
addressed at the infrastructure level (both static matrix and randomized
generation exist; what remains is fixing what they find). `wrap_builtin`
(`tests/differential_fuzz.rs`) wraps a randomly generated fragment in a
randomly chosen `Builtin` call (`Extend`/`Trunc`/`SignedCast`/
`UnsignedCast`/`Abs`/`Min`/`Max`/`Nand`/`Nor`/`Xnor` - the same set
`tests/self_determined_regression.rs`'s static matrix classifies),
following the exact width/kind rule each builtin's `call_ty` uses. Wired
into `gen_expr`'s dispatch as a new combinator, it needed no separate
per-position wiring: `combine_concat`, `combine_same_width`'s comparison
operators, and `cast_to` already accept any composite fragment as an
operand, so a builtin-wrapped fragment reaches the concat-member,
comparison-operand, and `signed`/`unsigned`-argument self-determined
positions purely through the generator's existing composition. Running it
at `N=300` immediately found two NEW, real, previously-unknown bugs
(exactly what this direction was for) - filed as [BUG-35](bugs.md) (a
shift whose left operand is a builtin call isn't hoisted in a
self-determined position) and [BUG-36](bugs.md) (`trunc()` of a
non-identifier expression emits an invalid Verilog part-select - BUG-20's
own class, reopened through a different call site). Both left OPEN,
deliberately out of scope for the branch that found them.

**What.** The test architecture is strong - Icarus differential in two layers,
`REQUIRE_IVERILOG=1` so it can never silently skip, a 1003-line random-program
differential fuzzer, 4 libFuzzer targets, docs/grammar sync tests, WASM parity.
But **every oracle compares simulator vs. Verilog.** There is no oracle asserting
**declared type vs. produced value**.

**Why it matters.** This is exactly the shape of the two most serious findings in
[`review-2026-08-02.md`](review-2026-08-02.md):

- **BUG-28 / BUG-29** pass the simulator and fail the hardware. They survive the
  fuzzer because its generator is documented as checker-clean _"by construction -
  every combine step unifies operand widths via `extend()`"_
  (`tests/differential_fuzz.rs:8`), which keeps every `extend` in a
  **context-determined** position. The broken case only appears in a
  **self-determined** position, which the generator never produces.
- **BUG-30** fails _neither_ oracle: the simulator and Verilog agree with each
  other, and both disagree with the declared type.

**Direction - two additions, in priority order.**

1. **Width-conformance property.** After every simulator evaluation, assert the
   produced `Val` fits the checker's declared width for that expression. Wire it
   into the existing fuzzer - it needs no new generator, only a new assertion,
   and it catches the entire BUG-30 class.
2. **Position-aware generation.** Extend the fuzzer (and add a static matrix to
   `tests/self_determined_regression.rs`) placing every `Builtin` and every
   operator into each of the five self-determined positions:

   - concat member
   - replication body
   - replication count
   - comparison operand
   - `$signed` / `$unsigned` argument

   Drive the matrix off the `Builtin` enum so a newly added builtin **fails the
   build** until it is classified. This catches the entire BUG-28/29 class and
   prevents its recurrence.

   **Landed (static matrix half only).** `tests/self_determined_regression.rs`
   gained `matrix_shape` - an exhaustive match over `Builtin` (no wildcard; a
   14th variant is a compile error until classified there) plus one
   differential test per testable variant. Tracing the actual call graph while
   fixing BUG-29 found the "5 positions" framing overstates the real
   dimensionality: `verilog_self_determined_kind`, `kind_is_inferrable`, and
   `hoist_if_needed` are pure functions of the expression alone and are the
   _exact same three functions_ at every call site (`Concat`/`Replicate` share
   one code path byte-for-byte; a comparison operand and a `$signed`/
   `$unsigned` argument are two more, already exercised) - so one test per
   builtin at one position (a `Concat` member) exercises the whole shared
   mechanism, and a replication COUNT can never carry a runtime builtin call
   at all (`replicate_ty` requires it compile-time-constant, and it folds to
   a literal before emit ever reaches this code). The fuzzer's own generator
   extension (placing RANDOM programs, not hand-picked ones, into these
   positions) is not done - still open, and is where BUG-28/29-_shaped_ bugs
   in operators (not builtins) would be caught. Direction 1
   (width-conformance property, BUG-30's own oracle gap) is also still open.

---

## GAP-6 (MEDIUM, language) - No assertions (`assert`/`assume`/`cover`)

**Status:** CLOSED 2026-08-05. Filed 2026-08-02.

**What.** There is no `assert`, `assume`, or `cover` construct. A `grep` over
`spec/02-syntax-and-grammar.md` finds no assertion syntax; the only verification
surface is the `test` block, which checks port values at cycle boundaries.

**Why it matters.**

- **No property checking, no formal, no functional coverage.** Blocks
  safety-critical work entirely.
- **It is also the cheapest teaching tool not being used.**
  `assert(count < LIMIT)` teaches invariants better than any documentation
  chapter, and it teaches them at the moment the invariant breaks.
- Every peer HDL has it: SystemVerilog (SVA), Chisel, SpinalHDL, Clash; Spade has
  partial support.

**Direction.** Start small and synthesis-safe:

1. `assert(cond)` inside a module body and inside `on rise(clk)`.
2. Lower to `$fatal`/`$display` inside an `ifdef`-guarded block in the emitted
   Verilog so synthesis ignores it by default.
3. Evaluate in the simulator natively, reported through the existing `S03xx`
   runtime-diagnostic path.
4. `cover(cond)` next; SVA sequence emission and a formal bridge much later.

**Effort:** medium. **Value:** highest capability-per-effort of any item in this
file.

**Fix.** `assert(cond)` / `assert(cond, "msg")` landed exactly as directed,
1–3 above, in a 13-task branch: new keyword `assert`/`valiyuruthu`/
`வலியுறுத்து` (PROVISIONAL Tanglish/Tamil, pending native review); one
shared `AssertStmt` AST payload reused by `ModuleItem::Assert` and
`SeqStmt::Assert` (module-item level: checked every settled comb state;
inside `on rise(clk) { }`: checked once per triggering edge); the checker
reuses its existing `check_cond` (E0404, the same rule `if`/`&&`/`test`'s
`expect` already use) and classifies `assert` as read-only (no driver) in
every exhaustive-match pass; the emitter renders a
`` `ifndef SYNTHESIS ``-guarded `$display`/`$fatal(1)` block at both sites
(synthesis no-op by construction); the simulator evaluates it natively
from the two places state is known consistent (`Sim::tick_edge` for every
clocked entry point, `comb_run`'s per-vector loop for the clockless
path), failing immediately with a new **`S0501`** diagnostic (opening the
`S05xx` family, direction 3's ask - landed as its own dedicated code
rather than reusing `S03xx`, since it's a distinct failure category, not
a test-harness control-flow error). `cover(cond)`, `assume`, and SVA
sequence emission (direction 4) remain deliberately out of scope, as
directed.

**Test.** TDD throughout (13 tasks, each its own failing-test-first
checkpoint): lexer keyword test, pretty-printer round-trip, parser tests
(both grammar sites, both word orders), checker E0404 test + clean-pass
tests, emitter golden-string tests (both sites), a `mimz-sim` catalog
test (`S0501` in `ALL_SIM_CODES`), an elaborator collection test, kernel
tests (`assert` fires / never fires), a `comb_run` failure test, an
Icarus differential proving a fired clocked `assert` actually halts real
`iverilog`/`vvp` (`$fatal`'s non-zero exit code), and 10 new examples (5
flavors × comb/clocked). 1156 tests passing (workspace, `REQUIRE_IVERILOG=1`),
clippy clean. Two real, independent bugs were caught and fixed along the
way (both outside `assert` itself): T1's new keyword wasn't propagated to
`editors/vscode/syntaxes/mimz.tmLanguage.json` or
`spec/03-keywords-trilingual.md` (the standing `grammar_sync.rs` doc-sync
contract caught it); `comb_run`'s `Result<Timeline, String>` boundary
silently drops a `Diag`'s `.code`, discovered while writing T10's own test
(fixed the test's assertion to check message text, documented why).

---

## GAP-7 (MEDIUM, language) - No enum↔bits cast

**Status:** CLOSED 2026-08-07. Filed 2026-08-02.

**What.** There is no way to obtain an enum value's bit encoding.
`unsigned(enumval)` is rejected; an enum in a concat produces E0403.

**Why it matters.** The encoding already exists and is stable - the emitter
assigns it directly:

```verilog
localparam [1:0] STATE_RED = 0;
localparam [1:0] STATE_GREEN = 1;
localparam [1:0] STATE_YELLOW = 2;
```

It is simply unreachable from source. Debug ports, state-trace outputs,
one-hot exports, and any "expose the FSM state on a header pin" workflow all need
it. This is a routine need in FPGA bring-up.

**Direction.** Either extend `unsigned(x)` to accept an enum (result
`bits[clog2(variants)]`), or add an explicit `encoding(e)` builtin. The explicit
builtin reads better against the language's "conversions are visible" doctrine
and does not overload a cast that currently means "reinterpret, same width".

**Related.** The reverse direction (`bits` → enum) should stay unavailable, or be
gated behind an exhaustiveness-checked construct - an unchecked int→enum cast
would reintroduce the invalid-state class the enum type exists to prevent.

**See also.** [BUG-31](bugs.md) - the diagnostic a user currently hits when they
try this is actively misleading.

**Fix.** Landed the explicit `encoding(e)` builtin, exactly as this entry's
own Direction recommended over overloading `unsigned(x)`. `Builtin::Encoding`
joins the existing plain-identifier builtin table (`from_name`) - not a
keyword, so no `lang/keywords.toml`/trilingual work at all, unlike GAP-6.
The checker requires `Ty::Enum` and returns `bits(en.inferred_total_width)`

- the FULL tag+max-payload width for a tagged union, not just the tag;
  everything else is a brand-new **`E0418`** (next free `E04xx` slot), never a
  reuse of `E0407`, per this project's one-code-per-rule catalog policy. The
  emitter and simulator needed no enum-specific logic at all: an enum-typed
  signal is already tracked as a generic `Kind{width, signed}` at that layer,
  so `encoding` reuses `unsigned(x)`'s exact mechanism (self-determined-
  position classification, hoisting, `$unsigned(...)` codegen, runtime eval)
  byte-for-byte in every exhaustive `Builtin` table it joins. GAP-5's own
  position-matrix (`tests/self_determined_regression.rs`) gained `Encoding`'s
  classification plus two Icarus differential tests (tag-only and
  payload-carrying enum) - found and worked around two separate, narrow,
  pre-existing `mimz-sim` bugs along the way (`comb::eval_outputs` doesn't
  model any enum signal yet, filed as [BUG-38](bugs.md); a `reg`'s reset
  value can't be a payload-carrying `EnumConstruct` expression, filed as
  [BUG-39](bugs.md)), neither fixed here, both filed for a future branch.
  Five new examples (`enum_encoding.mimz` × english/tanglish/tamil/
  mixed/tamil-pure) exercise the motivating "expose FSM state on a debug port"
  use case this entry's own Why-it-matters described. The reverse direction
  (`bits` → `enum`) stays permanently unsupported, as this entry's own Related
  note specified - not deferred, rejected. Full workspace green throughout,
  `fmt`/`clippy -D warnings` clean.

---

## GAP-8 (MEDIUM, language) - Surface gaps: division, attributes, pipelines, type generics

**Status:** OPEN. Filed 2026-08-02.

Grouped because each is individually small to state and each blocks a specific
downstream domain.

### 8a - No `/` or `%`

`BinOp` (`crates/mimz-core/src/ast/expr.rs:225`) has no `Div` or `Mod`.
Defensible - dividers are expensive and should not be implicit - but there is no
`divmod`-by-constant either, so power-of-two division needs manual shifts with no
width-checking help and no diagnostic explaining why. **Direction:** allow
division by a compile-time constant power of two (folds to a shift, fully
checked), and give a teaching error for the general case that names the cost.

### 8b - No synthesis attributes

No `ram_style`, `keep`, `dont_touch`, `async_reg`, `max_fanout`. Once real FPGA
designs are generated these are needed and there is **no escape hatch at all** -
`extern module` covers instantiating foreign Verilog but not annotating
mimz-generated signals. **Direction:** an attribute syntax with an explicit
vendor-mapping table, so the mapping is data rather than emitter special cases.
Interacts with [BUG-32](bugs.md) (memory style - widened 2026-08-18 to cover
every `reg`'s own `initial`-seed lowering too, see BUG-65/BUG-69).

### 8c - No pipeline construct

Manual stage registers only. Given the teaching mission, and that pipelining is
_the_ concept students struggle with most, a `pipeline` / `stage` form would be
both high-value and differentiating - and it is a natural fit for a language that
already owns clock-domain information statically. **Direction:** design after
[GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times);
a pipeline construct wants an IR to lower into.

### 8d - No parameterized-type generics

Modules take `int`/`bool` parameters only. No type parameters, so no generic
FIFO-of-T. `bundle` softens this but does not replace it. Compounds with BUG-12
(`fn` cannot be parameterized by module scope), which is the bigger day-to-day
tax of the two. **Direction:** BUG-12 first; type generics are a v0.5-scale
language change and warrant an RFC.

---

## GAP-9 (MEDIUM, DX) - LSP feature set and missing fix-it spans

**Status:** OPEN. Filed 2026-08-02.

**What.** `src/lsp.rs` advertises `hover_provider`, `definition_provider`, and
`completion_provider`, plus push diagnostics. Missing: find-references, rename,
document symbols, formatting, semantic tokens, and code actions.

Separately, `JsonDiag` (`crates/mimz-core/src/diag.rs:282`) carries
`severity / code / message / help / path / line / col / span` - but **no
structured fix**. An editor therefore cannot offer a quick-fix even where the
help text describes an exact mechanical change.

**Why it matters.** The help lines already describe mechanical rewrites:

```text
error[E0401]: expected `bits[8]`, found `bits[9]`
    = help: `+`/`-` are lossless … For same-width wrap-around use `+%`/`-%`
```

`+` → `+%` is a one-token replacement at a known span. Shipping that as a
one-click code action would be the single most visible DX improvement available,
and it reinforces the teaching goal rather than bypassing it (the action still
shows what changed and why).

**Direction.**

1. Add `fix: Option<{ span, replacement, label }>` to `JsonDiag` and populate it
   for the mechanical cases first (`+`→`+%`, `=`→`<-` and vice versa, missing
   `extend`).
2. Wire `textDocument/codeAction` in `src/lsp.rs` off that field.
3. Then find-references and rename - `analysis.rs`'s symbol index is already
   structured to support both, so these are mostly plumbing.
4. Document symbols, semantic tokens, formatting (`mimz fmt` already exists;
   exposing it over LSP is small).

---

## GAP-10 (LOW, process) - No coverage measurement; checker and emitter unfuzzed

**Status:** OPEN. Filed 2026-08-02.

**What.** Three process gaps in an otherwise strong assurance story.

**10a - No coverage measurement in CI.** No `cargo llvm-cov` (or equivalent)
step, so untested paths are invisible. With 1114 tests the _absolute_ number is
reassuring, but nothing shows which of the nine checker passes, which emitter
branches, or which `Builtin` arms are actually exercised. BUG-28 lived in an
unexercised arm.

**10b - The checker and the emitter are unfuzzed.** The four libFuzzer targets
are `lex_parse_compile`, `lex_parse_eval`, `pretty_roundtrip`, and
`translate_roundtrip`. They cover lex/parse/eval and two round-trip properties.
The passes where BUG-28/29 live - the checker's width rules and the emitter's
self-determined-position logic - have no fuzz target, only hand-written tests and
the differential fuzzer (whose generator gap is
[GAP-5](#gap-5-high-testing---no-declared-type-vs-produced-value-oracle-self-determined-positions-ungenerated)).

**10c - The differential fuzzers' PRNG is biased in its low bits, so some
choices are unreachable at any depth.** Added 2026-09-03 (Task 18).
`tests/differential_fuzz.rs`'s `Rng::next_range(n)` is `next_u64() % n`, i.e.
the LCG's LOW bits. The update is `x = x * 2654435761 + 0x9E3779B9` — both
multiplier and increment odd — so bit 0 strictly ALTERNATES (`next_range(2)`
yields 0,1,0,1,…) and bit 1 has period 4. Interleaved with a generator's other
fixed-count calls, a small-power-of-two choice can land on a sub-cycle that
never selects one of its arms. Found live: a 4-way `next_range(4)` comparison
pick produced **zero `<=` across 500 generated programs** — one arm of four,
unreachable at any depth. This is the same failure class as the round-6
`nand` bias (`task9_reduction_fuzz_bias_reaches_both_bare_and_extended_operands`),
and it is a coverage gap that no amount of CI depth can close, which is why it
belongs here rather than with the (CLOSED) depth work of GAP-11.

Task 18 fixed it only inside its OWN generator (a local `pick()` helper reading
the high bits) and deliberately left `Rng` itself alone: changing `next_range`
renumbers every program `gen_module`/`gen_clocked_module` emit and therefore
invalidates BOTH regression corpora,
`tests/fixtures/fuzz-seeds/{comb,clocked}.txt`. So this is a
**deliberate-hit follow-up task, not a drive-by fix** — but it should not sit
forever: a corpus whose value depends on the generator's own reachability is
worth less than a correctly-random generator. Nobody has measured which
`next_range(2)`/`next_range(4)` choices in the two older generators are
currently degenerate.

**Direction.**

1. Add `cargo llvm-cov` to CI as a **reported, non-gating** number first; set a
   floor only once the baseline is known.
2. Add a fifth fuzz target driving `checker::check` → `emit_verilog::emit` on
   generated-but-plausible ASTs, asserting the two invariants that must always
   hold: _checker-accepted input never panics the emitter_, and _emitted Verilog
   always elaborates under `iverilog -t null`_.
3. Fix `Rng::next_range` to draw from the high bits, in one task that also
   re-derives both fuzz-seed corpora against the renumbered seed space (and
   re-confirms each recorded bug is still reproduced by its new seed).

**See also.** [HARD-9](hardening.md) tracks 10b as a recommended hardening item.

## GAP-11 (MEDIUM, testing) - The width-conformance oracle is vacuous, and CI fuzzes at a depth that finds nothing

**Status:** CLOSED 2026-08-09 - both halves. Filed 2026-08-07. Source:
[`review-2026-08-07.md`](review-2026-08-07.md).

**What.** Two separate holes in the oracle work that closed
[GAP-5](#gap-5-high-testing---no-declared-type-vs-produced-value-oracle-self-determined-positions-ungenerated)
on paper.

**(a) The oracle is nearly tautological.** The recommendation was: _after every
simulator evaluation, assert the produced `Val` fits the checker's declared width
**for that expression**._ What shipped (`assert_bits_fit_width`,
`tests/differential_fuzz.rs:97`) checks only top-level **signals** - ports,
registers, `Timeline::signals`. Its own doc comment is candid:

> `mimz-sim`'s kernel already masks every stored value to its signal's width by
> construction, so this should never fire today

It cannot catch the BUG-30 class it was written for: BUG-30's defect was an
_intermediate_ (`din << 2` typed `bits[4]`, valued 60) whose value lands in a
`bits[8]` output and fits. It equally cannot catch [BUG-43](bugs.md), whose
wrong value also fits. The oracle has to run at every sub-expression and compare
against the **checker's** `Ty`, not against the kernel's own resolved signal
width - two independent authorities is the entire point of an oracle.

**(b) CI fuzzes 20 seeds.** `MIMZ_DIFF_FUZZ_N` and `MIMZ_DIFF_FUZZ_CLOCKED_N`
default to 20 and CI does not raise them. At 400 each the run takes **55
seconds** and found two live miscompiles ([BUG-42](bugs.md),
[BUG-44](bugs.md)) - both since fixed, which is the argument for the depth,
not against it: neither was reachable at 20 seeds.

**Why it matters.** The single highest correctness-per-line-changed action
available to this project right now is one environment variable in `ci.yml`.

**Direction.**

1. Raise the per-PR default to a few hundred seeds; add a nightly run in the
   thousands, printing the failing seed.
2. Keep a `tests/fixtures/fuzz-seeds/` corpus of every seed that ever failed and
   replay it on every run - a fuzzer without a regression corpus re-finds the
   same bug and loses the old ones.
3. Rewrite `assert_bits_fit_width` to walk sub-expressions and compare against
   `checker::infer_ty`, not against `Timeline::signals`.

### (b) resolved 2026-08-09 - depth moved to CI, corpus made depth-independent

Directions 1 and 2 are done - direction 3 (the (a) half) is covered by the
`(a) resolved` section below this one.

**Depth now lives where a long run is free**, not in the source default:

| where                                                                                 | seeds per generator | cost                |
| ------------------------------------------------------------------------------------- | ------------------- | ------------------- |
| in-source `DEFAULT_FUZZ_N`                                                            | 20                  | 5.8 s (with corpus) |
| `ci.yml` → `check` (per-PR)                                                           | 400                 | 76.9 s, debug       |
| `ci.yml` → `fuzz-nightly` (nightly, was weekly - Task 5, `v0.2-class-closure-round3`) | 5000                | release             |

The first draft of this fix raised the **in-source** default to 400 and was
wrong: every seed shells out to `mimz compile` plus `iverilog` plus `vvp`, so
depth is paid in ~3 process spawns per seed per generator, and an in-source
default charges that to every unrelated local `cargo test`. "Per-PR" in
direction 1 means the CI job, not the constant. `fuzz-nightly` had to grow an
`iverilog` install - it previously ran libFuzzer targets only.

**The corpus is the depth-independent half.** `tests/fixtures/fuzz-seeds/`
holds every seed that has ever failed - `comb.txt` (7: BUG-18, 19 ×2, 23, 24,
34, 42) and `clocked.txt` (3: BUG-21, 23, 44) - replayed _before_ the fresh
seeds at every depth, including `N=0`. This matters more than the number in the
table: fresh seeds are `base + i`, so without a corpus, changing the depth
silently changes which historical bugs are still covered, and lowering it drops
them. Four of the ten sit past the per-PR depth and are reachable only from the
corpus. An empty or all-commented corpus file now asserts rather than passing
vacuously - silent coverage loss is the failure mode a corpus exists to prevent.

---

## GAP-12 (MEDIUM, performance) - `mimz compile` is superlinear in module size

**Status:** CLOSED 2026-08-09 (measured - see the `Fixed`/`Detection added`
sections below). Filed 2026-08-07. Source:
[`review-2026-08-07.md`](review-2026-08-07.md).

**What.** Measured on one module of N 8-bit registers, release build:

| regs  | source lines | `mimz compile` |
| ----- | ------------ | -------------- |
| 1,000 | 2,010        | 0.43 s         |
| 2,000 | 4,010        | 1.00 s         |
| 4,000 | 8,010        | 3.51 s         |
| 8,000 | 16,010       | 10.57 s        |

4x the input costs roughly **10x** the time. `mimz check` on the same inputs
stays linear (0.35 s -> 0.87 s for 4x input), so this is the **emitter**, not
the checker - which revises `review-2026-08-02.md`'s "scaling is roughly linear"
finding for the compile path specifically (that review measured `compile` at one
size only).

**Evidence.** `crates/mimz-core/src/emit_verilog/expr.rs` contains **22**
`self.cur_decls.clone()` calls, each a full `HashMap<String, Kind>` clone of
every declaration in the module, executed **per expression node visited**. With
8,000 declarations and 8,000 assignments that is on the order of 64M entry
clones. Most of these call sites were added by the hoisting work (BUG-19 through
BUG-36).

**Why it matters.** The stated ceiling for this project is a soft CPU plus
peripherals - 50-200k lines of generated RTL. At the current curve that is
minutes, not seconds, and it arrives as a wall.

**Direction.** Local fix, no IR dependency: hold `cur_decls` behind an `Rc`, or
restructure the borrow so the map is passed by reference rather than cloned per
node. Bundles naturally with
[GAP-4](#gap-4-lowmedium---string-keyed-name-resolution-throughout-no-interning)
(interning) but does not depend on it.

### Fixed 2026-08-09 - `cur_decls` is an `Rc<HashMap<String, Kind>>`

The field type changed; the 22 call sites became `Rc::clone(&self.cur_decls)`, an
O(1) refcount bump. Semantics are unchanged - the map is replaced wholesale per
module and never mutated in place, so a snapshot could never observe a later
write under either type. Every golden is byte-identical and the full suite is
green (1213 passed).

**Measured, same machine, best of 3 per point:**

| regs  | before | after  | speedup |
| ----- | ------ | ------ | ------- |
| 500   | 0.14 s | 0.04 s | 3.5x    |
| 1,000 | 0.37 s | 0.07 s | 5.3x    |
| 2,000 | 1.46 s | 0.13 s | 11x     |
| 4,000 | 7.59 s | 0.28 s | 27x     |

Cost per doubling went **2.71 / 3.90 / 5.20 → 1.63 / 1.85 / 2.09**: superlinear
(worse than quadratic, and degrading) to linear. The speedup grows with size,
which is the signature of removing a per-node O(declarations) copy.

**The original workload does not reproduce this gap, and that matters for
whoever re-measures.** A module of N registers wired `r_i <- r_{i-1}` compiles in
0.36 s at N=8,000 both before and after - a bare `Ident` right-hand side never
enters a hoist path, so it never reaches a `cur_decls` snapshot at all. The
numbers in the table above come from drives carrying `trunc(extend(r_i, 16) *
extend(3, 16), 8)`, where declaration count and hoisting expression count both
scale with N. The table at the top of this entry (0.43 → 10.57 s) therefore
measured something other than the 22 clone sites named as its own evidence; its
absolute figures were never reproducible here, and only the hoist-heavy shape
exhibits the curve the evidence describes.

### Detection added 2026-08-09 - `mimz-bench` now has a size axis

The second half. `mimz-bench` sampled exactly one module size, so no trend it
recorded could distinguish a complexity regression from a slower runner - which
is what let this ship in the first place.

`metrics/scaling.rs` emits one synthetic module at 250 / 500 / 1,000 registers
and records the **cost ratio per doubling**. The ratio, not the absolute time, is
the metric that trends: ~2.0 means linear on any machine. It lands in
`bench-history.jsonl` as `worst_doubling_ratio` (plus `scaling_ms` for the raw
points) and gets its own trend chart in the HTML report.

Validated by running the new section against the pre-`Rc` emitter - a detector
that never fires is the same mistake one layer up:

| emitter         | 250     | 500     | 1,000    | worst ratio |
| --------------- | ------- | ------- | -------- | ----------- |
| before (GAP-12) | 22.8 ms | 82.3 ms | 372.2 ms | **x4.52**   |
| after (`Rc`)    | 3.6 ms  | 7.7 ms  | 18.0 ms  | **x2.33**   |

Run-to-run spread is about ±0.3, so the separation is wide. Reported, never
gated - same reasoning as `MIMZ_PERF_GATE` (BUG-33): a hard threshold on a
shared runner would flap.

### (a) resolved 2026-08-09 - every intermediate gets a declared, checked home

The oracle could not walk sub-expressions because neither authority is reachable
from `tests/`: the checker's `Ty` and `infer_ty` are private by design (_"Lives
only inside this pass - the AST stays untyped"_) and the emitter's `infer_kind`
is `pub(crate)`. Rather than punch a public hole in the checker for a test, the
fuzz generator **materializes** each internal node of every expression it builds
as its own `out` port, declared at the width and kind the generator constructed
it at (`MAX_SUB_OUTPUTS = 8` per module, both the combinational and clocked
generators). That yields the two independent authorities directly:

- the **checker** endorses each width by accepting the program at all - the
  generator is checker-clean by construction, so a width it would reject fails
  the 1,000-seed validity test immediately;
- the **simulator** and **Icarus** each produce a value per intermediate, now
  compared against each other and against the declared width.

Additive, never substitutive: `y` and every `<-` still render their whole
expression inline. BUG-30 was _"naming an intermediate changes the result"_, so
rewriting the root to reference these ports would test a different program than
the one the differential exists to cover.

**Validated by A/B, with BUG-44's fix reverted, over 400 combinational seeds:**

| configuration                  | result                                 |
| ------------------------------ | -------------------------------------- |
| sub-expression outputs **on**  | **FAILS** - seed 12648671, output `y3` |
| sub-expression outputs **off** | passes - blind                         |

The old root-only oracle cannot see a real, shipped miscompile that the new one
catches - and catches it in the _combinational_ generator, which never found
BUG-44 at all (it took the clocked generator at i=201). Seed 12648671 is pinned
in the corpus. `assert_bits_fit_width` is kept, now backed by an explicit
declared-vs-resolved width assertion; on its own it remains tautological, which
was the original finding.

**Status: CLOSED.** Both halves done - the cost removed, and the instrument that
would have caught it now exists.

**Residual, decided rather than omitted (round-3 class-closure plan, Task
9).** `assert_bits_fit_width`
staying tautological means the shape it can't catch - an intermediate whose
VALUE exceeds its DECLARED width where sim and Verilog agree with each other
and both disagree with the type (F-2's exact shape) - has no oracle.
Materializing such an intermediate as a declared-width port makes both
engines truncate it identically, so the differential agrees and the assert
passes; nothing structurally prevents the next BUG-30-shaped defect from
hiding the same way. Two options were on the table: (a) a `#[cfg(test)]`
accessor exposing the checker's private `infer_ty` for one expression, or
(b) fold it into [GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times) -
once there is one width per IR node, the oracle becomes "walk the IR."
**Decision: (b).** A test-only hole in the checker's own type boundary is
exactly the kind of narrow, single-purpose special case GAP-1's whole
argument is against - building it now would mean building it again,
differently, once the IR lands. Deferred, not dropped; this residual is the
concrete reason GAP-1's typed-IR oracle should include a width-conformance
walk on day one, not as a later addition.

---

## GAP-13 (MEDIUM, testing) - The position matrix has no `ExprKind` axis, and the only structural coverage assertion was deleted

**Status:** CLOSED 2026-08-09. Filed 2026-08-09. Source:
[`review-2026-08-09.md`](review-2026-08-09.md). All three directions done -
1 and 3 the same day it was filed, 2 (fuzz generator vocabulary) same day
too, round-3 class-closure plan Task 4.

**What.** [GAP-5](#gap-5-high-testing---no-declared-type-vs-produced-value-oracle-self-determined-positions-ungenerated)'s
position matrix covers every `Builtin`. It covers no `ExprKind`. The gate that
decides whether the hoist runs (`kinds::infer_kind`, since BUG-41 the only one)
matches on `ExprKind`, not on `Builtin`, and that match is **not** wildcard-free:
`_ => None` at `kinds.rs:191`, plus early `return None` inside the `Field` and
`Slice` arms. [BUG-48](bugs.md) and [BUG-49](bugs.md) are the two live
consequences.

**Cause.** Two decisions, each locally reasonable:

1. `matrix_shape` / `ALL_BUILTINS` were deleted in Task 4 of the v0.2
   remediation, on the correct reasoning that the real matches are already
   wildcard-free so the compiler enforces the `Builtin` axis by itself. That
   removed the file's only structural coverage assertion of any kind, and
   nothing replaced it for the axis where the match is still **not**
   wildcard-free.
2. `tests/self_determined_regression.rs:566-584`'s comment block - "one
   gate-and-classifier pair, not five, so shape coverage is redundant" - is
   about **positions** and is correct about positions. It is silent about
   **shapes**, and has been read as covering both for three rounds.

**Measurement.** Disabling the five `ExprKind` arms BUG-41's fix added
(`#[cfg(any())]`, falling through to `_ => None`) fails **exactly five tests, all
five of them the hand-written repro for that exact shape**. Nothing generalises;
a sixth shape is caught by nothing. Two such shapes are filed as BUG-48.

**Direction.**

1. An exhaustive `match` over `ExprKind` in `tests/self_determined_regression.rs`,
   each arm yielding either a `.mimz` source that puts that shape in a concat
   member, or an explicit `NotApplicable(reason)`. A new variant then fails the
   build until classified - what `matrix_shape` did for `Builtin`, aimed at the
   axis that still needs it.
2. Teach the differential-fuzz generator the shapes it cannot currently emit:
   `fn` call, instance port (plain **and** array), `if`/`match`, `mem` read, and
   a `const`-bounded slice. The generator's expression vocabulary - not its seed
   depth - is why 2,000 seeds cannot reach BUG-41 or BUG-48.
3. Add an `iverilog -t null` **elaboration** assertion to the trunc/slice-base
   tests, which today compare values only and so cannot fail on output that does
   not parse ([BUG-49](bugs.md)).

### Direction 1 resolved 2026-08-09 - the axis exists, and it found a live bug

`expr_kind_self_determined_coverage` (`tests/self_determined_regression.rs`)
is exactly the recommended shape: an exhaustive `match` over `ExprKind`, one
arm per variant, each naming either the differential test(s) that cover the
shape or a reasoned `NotApplicable` (a bare literal/identifier - nothing for
Verilog to differ on, matching `self_determined.rs`'s own `_ => None`; a
bundle/array/enum literal - checker-rejected upstream, never reaches a
self-determined position). No wildcard arm - a new `ExprKind` variant fails
to compile the test file until classified.

**The exercise of actually writing one differential per shape - not just
citing an existing test - found a genuine, previously-unknown CRITICAL
defect.** Every prior `Replicate` test used it as the OUTER self-determined
container; nothing had tried `Replicate` as a NESTED operand (mirroring
BUG-36's own `Concat`-as-nested-operand shape). `trunc({2{p0}}, N)` hoisted
into a wire declared at only `1/count` of the value's real width, reading
back partially `x` (undriven) - filed and fixed same day as
[BUG-50](bugs.md). This is direct evidence for the gap's own thesis: an axis
with a `CoveredBy("existing test name")` arm only proves what the cited test
actually exercises, and "a `Replicate` appears somewhere in this file" was
not the same claim as "a `Replicate` as a nested operand is correct."

`kinds::infer_kind`'s own match (the actual gate, not just the test file's
copy of it) was also made exhaustive over `ExprKind` in the same pass -
`_ => None` replaced with three explicit arms (`BundleLit`/`ArrayLit`/
`EnumConstruct`, each `None` with its own reason). A new `ExprKind` variant
now fails the BUILD, not just this one test file, matching what
`self_determined.rs`'s own exhaustive `Builtin` match already gives that
axis.

**Still open:** nothing - direction 3 (elaboration assertion) had already
landed, folded into [BUG-49](bugs.md)'s own fix rather than as a separate
step, since `differential`/`differential_clocked`'s existing `iverilog`
build-step assert already provided it.

### Direction 2 resolved 2026-08-09 - the generator can now emit every shape, and finds BUG-48 unassisted

`gen_special_leaves` (`tests/differential_fuzz.rs`) precomputes, once per
module exactly like `ports`/`regs` already are: a `fn`-call leaf, a
`const`-bounded slice leaf (both generators), and - clocked only, since
`comb::eval_outputs` does not elaborate instances at all - a plain
instance-port leaf, an array-instance-port leaf, and a `mem`-read leaf.
`if`/`match` landed as two new recursive combinator arms in
`gen_expr_collecting` itself (`combine_if`/`combine_match`), needing no
auxiliary declarations at all, unlike the other five shapes.

**Acceptance test, run for real (the plan's own "done when"): with
[BUG-48](bugs.md)'s fix reverted (`kinds.rs`'s `Field` array-instance arm
and `Slice`'s `slice_bound_fold`, the same two spots the fix itself
touches), the fuzzer catches it - at the very first fresh seed, in BOTH
generators, well inside the per-PR depth of 400:**

```text
comb    seed 12648435 (i=0): kernel y=890590     Icarus y=689886
        y4 = (signed(extend(6, 6)) +% extend(signed(p1[HI:0]), 6))
clocked seed 202427629 (i=0): kernel y=115539590 Icarus y=110100010111111111010000110
```

`p1[HI:0]` - a const-bounded slice inside a self-determined position - is
exactly BUG-48's own `Slice` shape, reached by random generation for the
first time. With the fix restored, both generators are green at `N=400`
(69 s), and the full workspace suite passes throughout.

**A real side effect worth recording, not swept under the "done" claim:**
extending `gen_leaf`/`gen_expr_collecting`'s own dispatch (a new
unconditional draw for the special-leaf pool, and the combinator-arm
divisor going from 7 to 9) changes which `rng.next_range` calls happen for
EVERY leaf and EVERY combinator choice - so a seed number that used to
generate one specific historical repro (e.g. `comb.txt`'s own
`12648435 # i=5 BUG-23`) now generates a **different program** under the
new generator, coincidentally the same number that surfaced BUG-48's shape
above. The regression corpus (`tests/fixtures/fuzz-seeds/`) still passes
in full - every entry's CURRENT program (whatever it is now) still matches
Icarus - but a seed's comment naming a specific historical bug is no
longer a live guarantee that replaying it still exercises that exact
shape after a generator vocabulary change. This is inherent to a
probabilistic generator, not a bug in this change, and not new: any prior
vocabulary change (GAP-5's `wrap_builtin`, v3's clocked leaves) had the
identical effect and was never flagged. Worth a maintainer's note the next
time the corpus is touched, not a fix here.

#### Vocabulary extended again 2026-08-18 - the nested `fn` call (round-7 plan Task 11)

Round 7 ([`review-2026-08-17.md`](review-2026-08-17.md), Part 7) found six
regions this direction still does not reach, and named the cheapest as the
highest-value: `gen_special_leaves` emitted exactly **one** `fn` per program,
so a `fn` calling another `fn` - [BUG-67](bugs.md)'s own shape - was
structurally unreachable at any depth. It now generates a second `fn`,
`inner{w}`, and offers `inner{w}(x)` to the outer `fn`'s body as an ordinary
`special` leaf, on the same footing as a port. The placement is the point: the
call lands in whatever concat member / cast operand / `match` arm the tree
builds around it, which is what BUG-67 needed (a self-determined position
inside a `fn` body), not merely a call. The callee is declared only when the
generated body actually kept the call, so no program carries an unused `fn`.

**Acceptance criterion, same shape as this direction's own:** with
[BUG-67](bugs.md)'s fix reverted and nothing else changed, 400/400 fails at
clocked seed `202427811` (`0xC10CCED + 182`) on
`1 => unsigned((signed(extend(54, 6)) *% signed(inner6(x))))`. 5000/5000 green
with the fix restored (5 passed, 1246.11 s).

**The seed-comment caveat above fires again, and this is the second time.**
Adding the inner-`fn` body generation and the new special-pool entry changes
which `rng.next_range` calls happen, so every `tests/fixtures/fuzz-seeds/`
entry now generates a **different program** than it did before 2026-08-18. The
corpus still replays clean in full at every depth (workspace green, 400/400 and
5000/5000 green), but a seed's `# BUG-nn` comment is again a historical label
rather than a live guarantee that replaying it exercises that exact shape.
Unchanged conclusion: inherent to a probabilistic generator, not a fix here.

**Still open after this** - the other five regions round 7 named: an instance
port connection with a hoisting expression, a non-constant `reg` reset, a `mem`
init over a signal ([BUG-66](bugs.md)), a `const if` ([BUG-68](bugs.md)), and
any emitted-testbench verdict. Each is a separate generator feature; none is
ship-blocking, and each bug is covered directly by its own differential.

---

## GAP-14 (MEDIUM, process) - The release gate is scored at a shallower fuzz depth than the project's own CI runs

**Status:** CLOSED 2026-08-13 (procedure enforced AND actually run clean).
Filed 2026-08-10. Source: [`review-2026-08-10.md`](review-2026-08-10.md).

**What.** The v0.2 release gate's "no new instance of the F-1/F-2 pattern" check
is scored from a differential-fuzz run at the **per-PR** depth
(`MIMZ_DIFF_FUZZ_N=400`, `MIMZ_DIFF_FUZZ_CLOCKED_N=400`), while
`.github/workflows/ci.yml`'s `fuzz-nightly` job is configured for **5000/5000**.
Two live CRITICALs sat at HEAD in the band between the two depths:

| bug               | generator | seed      | fresh index |
| ----------------- | --------- | --------- | ----------- |
| [BUG-52](bugs.md) | clocked   | 202428078 | **449**     |
| [BUG-55](bugs.md) | comb      | 12649355  | **925**     |

Both were found by a plain `MIMZ_DIFF_FUZZ_N=2000
MIMZ_DIFF_FUZZ_CLOCKED_N=2000` run during round 4. Neither is reachable at 400.

**Cause.** [GAP-13](#gap-13-medium-testing---the-position-matrix-has-no-exprkind-axis-and-the-only-structural-coverage-assertion-was-deleted)'s
Task 4 acceptance criterion is deliberately scoped to the per-PR depth ("with
Task 1 reverted, the fuzzer must find BUG-48 from a fresh seed within the per-PR
depth") - which is the right criterion for _that_ question, since it measures
whether the generator's vocabulary reaches the shape at all. It was then reused
as the _gate's_ evidence, where the question is different: "is anything live?"
That question is bounded by whatever depth the project is willing to run, and the
project had, in the very same plan (round-3 Task 5), just committed to 5000
nightly.

**Why it matters more than a procedural nit.** This is the second consecutive
round with the identical miss. Round 3 recorded it in its own words: _"BUG-47 was
found at i=642, past the per-PR depth of 400, by a manual gate run"_ - and used
that as the argument for making the deep job daily. Task 5 changed the cron and
nothing changed the gate procedure, so the next release-readiness pass scored
gate 5 at 400 again and shipped two CRITICALs into a review. The project's own
note - _"every deeper run on 2026-08-09 found something"_ - held on 2026-08-10
too, untested.

**Fix.**

1. The release-gate checklist runs the differential fuzz at the **nightly** depth
   (5000/5000), not the per-PR depth. Gate 5 must not be scorable from a 400-seed
   run.
2. Append every new find to the corpus, so a fixed bug past the per-PR depth is
   still covered at depth 0:

   ```text
   comb.txt     12649355   # i=925  BUG-55  signed >> inside a match arm escaped BUG-47's context hoist
   clocked.txt  202428078  # i=449  BUG-52  if-expression as a concat member skipped the hoist
   ```

3. Record the depth a gate was scored at, next to the result, so a future round
   can tell "clean at 5000" from "clean at 400" - the two currently read the same
   in every prior review's gate table.

**Not a criticism of Task 4.** The extended generator vocabulary is what made
both seeds reachable at all - `combine_if`/`combine_match`
(`tests/differential_fuzz.rs:992-1006`) are real combinators over arbitrary
generated sub-expressions, not fixed leaves, so they can and did produce a branch
that renders narrower than its mimz width. Round 3's generator could not have
emitted either program. The instrument works; the procedure did not use it.

**Closed (2026-08-13).** The procedure fix (item 1, 3 above) landed
2026-08-10 (`docs/audit/README.md`'s "Release-gate scoring convention").
The gate itself was not actually re-run at 5000/5000 until now - running
it for the first time at this depth found a **third** live CRITICAL,
[BUG-59](bugs.md) (comb seed `12650993`, index 2563: a fused shift chain
inside an `if`/`match` branch, un-hoisted as an outer growing shift's own
LHS, saw the wrong ambient width - a value mismatch despite BUG-52's own
width-mismatch check agreeing on both sides). This is exactly what the
gap predicted: a deeper run than the one that scored the gate finds
something. Fixed same day (`bugs.md`), corpus seed appended, and the gate
**re-run clean** at 5000/5000 after the fix (4/4 passed, ~1184s,
`REQUIRE_IVERILOG=1 MIMZ_DIFF_FUZZ_N=5000 MIMZ_DIFF_FUZZ_CLOCKED_N=5000
cargo test --release --test differential_fuzz`). That re-run - not the
procedure alone - is what actually closes this gap; a procedure that is
never exercised is not evidence.

**Mechanised (2026-08-13, round-5 plan Task 6).** The residual round 5's own
review named: the rule lived in prose only, and the project's own 3-day gap
between writing it and running it is the proof a prose rule survives exactly
until the next handoff that doesn't remember it. Two changes, both XS:
`tools/gate.sh` runs the fuzz at the mandated 5000/5000 and prints the depth
and a CLEAN/FAILED banner in its own output - smoke-tested end-to-end at a
reduced depth to confirm the pass/fail path and exit-code propagation both
work, then restored to 5000. `.github/PULL_REQUEST_TEMPLATE.md` gained a
"Release gate" section, scoped to PRs that restore/claim a release-readiness
note, asking for `tools/gate.sh`'s output or a `fuzz-nightly` run URL pasted
in - a checkbox with nothing to paste is visibly incomplete, which a prose
convention never was.

**Debug-reachable at the mandated depth (2026-08-13, round-5 plan Task 7).**
A separate residual, found by the round-5 review: `hoist_if_needed`'s two
`debug_assert!` self-checks (`emit_verilog/module/ports.rs:564`/`:587`,
round-4 plan Task 9) are inert in every `--release` build, and every
differential/gate run - including `fuzz-nightly`'s own 5000/5000 step and
this gap's own `tools/gate.sh` - used `--release`. Checked, not assumed,
whether dropping it would slow the deep run down: measured N=100 locally,
25.7s debug vs 29.5s release for the identical test - debug is dominated by
`iverilog`/`vvp` subprocess time, not `rustc`'s `-O`, matching `ci.yml`'s own
pre-existing comment. Dropped `--release` from both `fuzz-nightly`'s deep
step and `tools/gate.sh` - free, and it keeps the assert live at the one
depth that has actually found a CRITICAL past the per-PR depth (BUG-59,
index 2563). Proved the assert genuinely fires, not just compiles in:
injected a deliberate one-line `Kind` mismatch into `kinds.rs`'s `Ident`
arm, ran one debug-mode differential, got the exact panic -
`hoist_if_needed: \`b\` declared as Some(Kind { width: 4, signed: false })
but caller computed Kind { width: 5, signed: false }`at`ports.rs:564:13`

- then reverted, confirmed clean by diff. Workspace 1259/1259 unchanged
  (CI/tooling-only), fmt/clippy clean.

---

## GAP-15 (MEDIUM, process) - The per-arm reasoning audit has no independent party, and cannot have one

**Status:** CLOSED 2026-08-13 (round-5 plan Task 5). Filed 2026-08-13. Source:
[`review-2026-08-13.md`](review-2026-08-13.md) (round 5).

**What.** [Round 4](review-2026-08-10.md)'s stated closure condition for the
self-determined-width class was, verbatim, one of:

- **(a)** every arm of both matches, on both axes, carries a written reason that
  has been checked against real Icarus **by someone other than its author** - a
  bounded, one-time job of roughly 60 arms; or
- **(b)** [GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times)
  lands and the question stops existing.

Round-4 plan Task 4 executed (a)'s _work_ across eight batches, thoroughly and
in good faith - it produced [BUG-56](bugs.md), [BUG-57](bugs.md) and
[BUG-58](bugs.md), corrected four miscalibrated citations, and replaced two
approximate citations with new differentials that actually ask their arm its own
question. It could not execute (a)'s _independence_ requirement:

```console
$ git log --format="%an" | sort | uniq -c | sort -rn
     45 Naveen2070
      5 github-actions[bot]
```

One human author, repo-wide. The same person wrote the arms, wrote the audit,
wrote both coverage docs, and signed off the reasons. Condition (a) is not
partially met - it is **unmeetable as stated** on a single-maintainer project.

**Why it matters, and what it cost.** The independence clause was not
decoration. Round 4 put it in because self-review is what produced BUG-42's
wrong reasoning and the stale doc-comments in the first place. Round 5 found the
sixth instance of the class within an hour, inside the audited surface, in an
arm whose written reason says it is safe:

- `expr_kind_self_determined_coverage`'s `Unary` arm, reduction half: _"no
  mismatch is possible there and no separate test is needed for that half"_ -
  reasoning about the operator's **result** width, which this module's own doc
  comment forbids.
- `builtin_self_determined_coverage`'s `Nand`/`Nor`/`Xnor` arm: the same claim,
  cited to `matrix_nand_in_concat_matches_icarus`, whose operand is the bare
  identifier `nand(a)` - a shape that cannot discriminate on "regardless of
  operand width".

Both became [BUG-60](bugs.md) (CRITICAL, five reproductions).

The pattern is specific and repeatable: the audit applied the BUG-42 rule
correctly to most arms, and slipped back to the result-width question on exactly
the arms where the operator's result is _obviously_ the same width on both
sides. "Obvious" is where a solo reviewer's attention is cheapest, which is where
a second reader is worth most.

**Restated closure condition (a′), which a solo maintainer can actually meet.**
Replace the human-independence requirement with a mechanical one that does not
depend on out-thinking one's own intuition. Every arm returning `None` must
carry either:

1. a differential whose operand **renders narrower than its mimz width** - an
   `extend(x, N)` or equivalent, never a bare identifier, never a plain
   instance port; or
2. an explicit `NotApplicable` whose reason names a **checker rule, grammar
   restriction, or lowering pass by code/identifier** (e.g. "E0403 rejects this
   upstream", "parser-restricted to a `Wire` init") - never a property of the
   operator itself.

Both BUG-60 and BUG-61 fail this rule by inspection, without needing any Verilog
knowledge at the point of review. Roughly a dozen arms across the four matches
currently rest on some form of "no mismatch is possible", and each is a
candidate.

**Relationship to GAP-1.** (a′) is a mitigation, not a resolution. It makes the
per-arm surface auditable by one person; it does not remove the surface. GAP-1
remains the only option that makes the question stop existing, and BUG-60
strengthens the case: it is the first member of the family that neither the
classifier's width-mismatch check nor deeper fuzzing can reach, because both are
width-based and this divergence is value-based at matching widths.

**Fix.** Adopt (a′) as the project's stated closure condition in place of round
4's (a); re-audit the "no mismatch is possible" arms against it; record the
result so round 6 scores against a condition that is satisfiable.

**Adopted (2026-08-13, round-5 plan Task 2).** Rule (a′) now lives where an
arm's author actually reads it, not just in this file: `self_determined.rs`'s
and `kinds.rs`'s module docs state it in full, and all five coverage docs in
`tests/self_determined_regression.rs` (`expr_kind_self_determined_coverage`,
`builtin_self_determined_coverage`, `expr_kind_infer_kind_coverage`,
`binop_infer_kind_coverage`, `builtin_infer_call_coverage`) carry a pointer to
it in their own header comment, phrased so "no mismatch is possible" alone,
unattached to a narrow-operand differential or a named checker/grammar/
lowering fact, is no longer an acceptable reason.

**CLOSED (2026-08-13, round-5 plan Task 5).** Grepped both raw matches and
all five coverage docs for "regardless", "always", "in both models", "no
mismatch", "no test needed" and their paraphrases - every hit, not a sample.
Most were already sound, backed by a checked code-level fact (round-4 batch
8's variant-blind-by-construction arms; GATE arms whose text states what the
function body literally does). Two were not yet checked against the exact
position their claim needs:

- **The comparison operand-hoist.** `expr.rs`'s comparison arm hoists `lhs`/
  `rhs` independently at its own call site - real, but never proven by a
  narrow-rendering differential. Checked by hand (`mimz compile` +
  reading the emission) before adding a test - genuinely sound (`extend(a,8)
== b` hoists into `wire [7:0] __mimz_sub_1`) - pinned as
  `task5_comparison_operand_hoist_catches_a_mismatch_matches_icarus`.
- **`Abs`/`Min`/`Max` at a plain top-level assignment.** Their render arms
  embed the operand with no hoist call, the identical SHAPE BUG-60 needed a
  hoist for - worth checking rather than assuming sound. It is sound, and
  for a structural (LRM) reason rather than luck: a reduction's operand is
  UNCONDITIONALLY self-determined regardless of context (BUG-60's actual
  cause); a ternary's branches are self-determined only when the ternary
  itself sits in a self-determined position (BUG-52), so at plain top level
  they inherit the assignment's own context the same way BUG-24 established
  for ordinary operators. Verified against real Icarus with the
  widest-magnitude operand in each case (`abs(extend(a,8))`, `a=-8` →
  8; `min(extend(p,11),extend(p,11))`, `p=-9` → -9) before writing the
  differentials, not after - `task5_abs_operand_at_plain_top_level_
matches_icarus`, `task5_min_max_operand_at_plain_top_level_matches_icarus`.

No third instance found. Both checked claims were true; neither needed
filing as a new bug - the sweep found gaps in the audit trail (untested
positions), not gaps in the emitter. Workspace 1259/1259, fmt/clippy clean.
Rule (a′) now stands on real per-position verification of every arm the
"no mismatch is possible" grep surfaced, which is the bar round 4's
condition (a) asked for in spirit - met here by a mechanical, repeatable
check rather than a second human.

**Re-opened in substance by round 6 (2026-08-15), not re-opened as a status.**
The sweep's own claim ("every hit, not a sample") is falsified by one hit:
`builtin_self_determined_coverage`'s `Trunc` arm reads _"already exactly N bits
**regardless of position**, so `None` **needs no** recursion proof"_ - two of
this task's own keywords, on a `None` arm, cited to `trunc(a, 2)`, a **bare
identifier**. It is also false: `trunc(extend(x,8), 2)` inside a `fn` body
emits `(x)[(2)-1:0]`, an Icarus syntax error
([BUG-62](bugs.md) ⑥). GAP-15 stays CLOSED because its own scope - the arms -
was genuinely re-audited; the larger finding is that the scope was wrong, which
is [GAP-17](#gap-17-medium-process-closed-2026-08-17---rule-a-audits-the-arms-the-defects-are-at-the-call-sites) below.

---

## GAP-16 (HIGH, architectural, CLOSED 2026-08-16) - the self-determined hoist machinery is scoped to module bodies, and nothing states the scope

**Status:** CLOSED 2026-08-16 (round-6 plan Task 4 - see the second "Status
update" below; the heading/status line themselves were stale until 2026-08-18,
round-7 plan Task 9, which corrected them to match what the entry's own body
already said). Filed 2026-08-15. Source:
[`review-2026-08-15.md`](review-2026-08-15.md) (round 6).

**Status update (2026-08-15, same day, round-6 plan Tasks 1–3):** the core of
this gap - a silent fallback, no test, no assert, no written contract - is
closed. `hoist_unresolved` routes every `None` arm through one place; it
`debug_assert!`s unconditionally and pushes a real `Diag` for the positions
whose grammar needs a named wire. The invariant is now stated once, in
`kinds.rs`'s module doc, replacing the "already-correct fallback" wording this
gap quoted. `fn` bodies and testbench bodies now get a real `decls` (Task 2); a
parameter-valued `extend` width no longer silently drops the hoist at the
positions that need it (Task 3). **Narrower than "closed":** making the hoist
itself possible inside a `fn` body (the actual `reg`-based mechanism BUG-63's
fix shape describes) is still unstarted - that residual is now a clean
diagnostic rather than a silent gap, which is what this filing asked for, but
it is not the same as the machinery covering every context. Round 6's Tasks
1-4 also found and fixed a testbench-side flush bug in passing - hoisted
wires were never emitted into the emitted testbench text at all, only into
the module body - not part of the original filing.

**Status update (2026-08-16, round-6 plan Task 4):** the residual above is
also closed. `hoist_if_needed`/`hoist_slice_base_if_needed`, when
`self.in_fn_body`, now hoist into a function-local `reg` (a fresh
`fn_hoist_counter`/`fn_hoisted_regs`/`fn_hoisted_stmts` buffer on `Emitter`,
never shared with the module's own) instead of pushing the Task 1-era `Diag` -
so a `fn`-body hoist site genuinely covers the position instead of only
diagnosing it can't. GAP-16's own invariant ("a hoist site may not silently do
nothing when it cannot resolve a `Kind`") now holds with the machinery
present, not just the diagnostic, for every context this gap named. See
BUG-63's own entry for the verification detail.

**What.** `cur_decls` is built once per module (`emit_verilog/module/mod.rs:127`)
and is `kinds::infer_kind`'s only data source. Every hoist call site is gated on
`infer_kind` returning `Some` and renders the text **unchanged** otherwise.
Three emitter contexts therefore run the entire width-agreement machinery with
a `decls` map that cannot resolve the names in front of it: `fn` bodies
(`module/funcs.rs` - parameters never inserted), testbench bodies
(`testbench.rs:151` - `cur_decls: Default::default()`), and any expression whose
width argument is a module `parameter` (`infer_call`'s `const_fold(&args[1])?`).

Nothing in the code, the coverage docs, or the audit records that the machinery
has a scope. `infer_kind`'s own doc comment lists the unresolvable shapes and
`hoist_width_effect_operand`'s comment calls the fallback "already-correct" -
the closest the codebase comes to naming the boundary, and it names it as safe.

**Why it is a gap and not just BUG-62.** BUG-62 is the ten reproductions.
The gap is that **twelve call sites share one silent branch with no test, no
assert, and no written contract** - so the next context added (a `sim` block, a
`cover`, a future `always` lowering) inherits the same hole by default, exactly
as `fn` bodies and testbenches did. Every fix in the BUG-41/46/48/49 line
removed one _source_ of `None`; none of them made _reaching_ the fallback
observable.

**Fix.** Make the fallback loud (debug assert at every hoist site; a real
diagnostic where the position's grammar requires a named wire), give each
emitter context a real `decls`, and state the invariant once, in
`kinds.rs`'s module doc: _a hoist site may not silently do nothing when it
cannot resolve a `Kind`._

**Relationship to GAP-1.** A typed elaborated IR carries one width per node in
every context, so "which `decls` is in scope here" stops being a question that
can be answered wrong. This gap is the third distinct symptom of the same root
(after the checker/emitter and kernel/emitter duplications).

---

## GAP-17 (MEDIUM, process, CLOSED 2026-08-17) - rule (a′) audits the arms; the defects are at the call sites

**Status:** CLOSED 2026-08-17 (round-6 plan Tasks 7-8). Filed 2026-08-15.
Source: [`review-2026-08-15.md`](review-2026-08-15.md) (round 6). Verified
against current source: the call-site-keyed coverage doc this entry's own
"Fix" asked for exists (`HOIST_CALL_SITES`,
`tests/self_determined_regression.rs`, all 21 `hoist_if_needed`/
`hoist_slice_base_if_needed`/`hoist_width_effect_operand` call sites), and
rule (a′-2) carries the fourth category this entry's "secondary findings"
asked for - a `NotApplicable` naming "a checked fact about what the
emitter renders" - in `self_determined.rs`'s module doc.

**What.** [GAP-15](#gap-15-medium-process---the-per-arm-reasoning-audit-has-no-independent-party-and-cannot-have-one)'s
rule (a′) constrains the written reason of every `None`/`NotApplicable` **arm**
in `verilog_self_determined_kind`, `infer_kind`, and their sub-matches. Sorted
by where the defect actually was, this family's instances since round 3 are:

| in an arm's answer     | at a call site / in the plumbing                                               |
| ---------------------- | ------------------------------------------------------------------------------ |
| BUG-48, BUG-50, BUG-52 | BUG-46, BUG-47, BUG-49, BUG-53, BUG-55, BUG-59, BUG-60, BUG-61, BUG-62, BUG-63 |

**10 of 14 are call-site defects**, and the most recent arm defect is BUG-52,
two rounds before this filing. Round 5's own BUG-60 fix states the point
plainly: _"The classifier arms stay `None`, unchanged … only the render call
site needed the hoist."_

**Two secondary findings about the rule's text**, both from round 6's
independent classification of six arms (four disagreements with the codebase's
own verdicts):

1. **(a′-2)'s enumeration is missing the category that matters.** It admits a
   checker rule, a grammar restriction, or a lowering pass. It does not admit
   _a checked fact about what the emitter renders_ - which is precisely the
   question `self_determined.rs`'s module doc says to ask. Applied literally,
   (a′) rejects `Ident` ("a signal's declared width IS its self-determined
   width"), `Bool` (`expr.rs` always renders a SIZED `1'b1`) and `Encoding` -
   all sound, and the first of them the axiom (a′-1) itself rests on.
2. **A reason may not rest on a hoist without naming it.** `Trunc`'s arm is
   sound only because BUG-36's base-hoist fires; its text claims a property of
   the operator instead, and is false wherever that hoist does not fire
   (BUG-62 ⑥). Any arm whose safety depends on a hoist must name the call site
   and the condition under which it runs.

**Fix.** Re-scope: every `hoist_*` call site - and specifically every `None`
branch of one - needs the same written, checked reason (a′) demands of an arm,
recorded in a coverage doc keyed by call site rather than by `ExprKind`
variant. Amend (a′-2) with the fourth category and the no-implicit-hoist rule
above.

---

## GAP-18 (HIGH, architectural, CLOSED 2026-08-19) - the hoist buffer's flush point is a second scoping axis, and nothing watches it

**Status:** **CLOSED 2026-08-19**, round-8 plan Task 2, branch
`round8-class-closure` - this time genuinely, not just claimed. Widened
`assert_hoists_declared_before_use` (`emit_verilog/mod.rs`) from the
`__mimz_sub_N`/`__mimz_fn_sub_N` name family to every `wire`/`reg` declared
in the module body - the sentence the "To close this properly" section
below asked for, taken literally. Verified: fires on both of round 8's own
BUG-70 constructions (hand-built fixtures reproducing the pre-Task-1 broken
output - `task2_widened_invariant_fires_on_bug_70_construction_1`/`_2`,
`tests/hoist_declaration_order.rs`) and stays silent across the full
workspace test suite, 1300/1300 (`MIMZ_REQUIRE_WASM=1 REQUIRE_IVERILOG=1
cargo test --workspace`) - corpus included. Previously **NARROWED, not
closed** - reopened 2026-08-18 by round 8
([`review-2026-08-18.md`](review-2026-08-18.md), Part 2). Marked CLOSED
2026-08-18 by round-7 plan Tasks 1 and 3 (that closure note's overclaim is
what round 8 caught); filed 2026-08-17 by round 7
([`review-2026-08-17.md`](review-2026-08-17.md), Parts 3.1 and 12).

**What widening the scan actually required (2026-08-19).** More than the
"roughly fifteen lines" the "To close this properly" section below
estimated, and three guards beyond the ones it named, all found by re-running
the full test suite after each change rather than by inspection alone:

1. The three traps this entry's own "To close this properly" section named
   (declaration-line self-reference, `.portname(signal)`'s port half, a
   string literal's contents) - implemented as written.
2. **A fourth trap this entry didn't name**: an injected `function ...
endfunction` block (the `clog2` helper, or a user `fn`) is a SEPARATE
   Verilog scope, textually first in the module (injected at `fn_pos`,
   before every module-level declaration) for a reason unrelated to this
   axis. Its own locals can coincidentally share a name with an unrelated
   module-level `wire`/`reg` - confirmed empirically:
   `examples/english/counter.mimz`'s `reg value` collided with `CLOG2_FN`'s
   own `input integer value` the moment a design used both. Skipped
   entirely (both passes) rather than scanned.
3. **A naive "last identifier before the trailing `;`" is wrong twice over**,
   both found by re-running the corpus after switching to it: a keyword
   like `reg` is a character-SUFFIX of a real name that happens to end in
   those letters (`reg filter_out_reg;` "ends with" `reg;` as characters,
   though `reg` there is the tail of `filter_out_reg`, not a token); and a
   `mem`'s own declaration (`reg [W-1:0] name [0:(depth)-1];`) puts the
   depth expression's own identifier LAST in the line, not the declared
   name - `std/fifo.mimz`'s `DEPTH` and `varisai.mimz`'s `aazham` both hit
   this. Fixed by taking the FIRST identifier after the width bracket
   closes instead - correct for every declaration shape this emitter
   produces.
4. **The module HEADER (parameter list + port list) had to be excluded from
   the usage scan entirely** - not something this entry's own trap list
   anticipated needing beyond "a port name... declared before the body
   starts." A port's own name in `input wire a_tx,` is a declaration, never
   a use - but scanning it as one surfaced a genuinely SEPARATE,
   previously-unknown defect: bundle-field flattening's naming convention
   (`{wire}_{field}`) can coincidentally collide with an existing port name,
   producing a self-referential `assign` and a duplicate-declaration
   elaboration failure. Filed as [BUG-73](bugs/bug-71-80.md) - a different
   subsystem (bundle flattening, not hoisting), so it got its own fix rather
   than a workaround here: a shared `declared_signal_names` set that
   diagnoses the collision, landed in the same commit. Excluding the header
   from this scan is correct regardless (ports never reference a
   body-declared name in their own width).

**Correction (2026-08-18, round 8).** The closure note below claims the
invariant "covers the whole axis generically rather than the three sites
currently known." **It does not.** `assert_hoists_declared_before_use` checks
one _name family_ - `__mimz_sub_N` / `__mimz_fn_sub_N`. The axis this entry
describes is _any_ declaration landing after its use. Round 8 constructed two
cases, neither from BUG-66's three reproductions, where a hoist lands ahead of
an ordinary instance output wire: `mimz check` OK, `mimz compile` exit 0,
invariant silent, real Icarus refusing with `Unable to bind … declaration after
use`. Filed as [BUG-70](bugs/bug-61-70.md).

What _is_ true, and was verified: the invariant fires on all three of
[BUG-66](bugs/bug-61-70.md)'s sites when the fix is reverted, naming the
identifier, the use line and the declaration line; and it is silent across the
whole 226-file corpus. Its mechanism is sound. Its scope is one noun narrower
than this entry claimed.

**To close this properly**, one of two things, and the audit trail must say
which shipped:

- **Widen the invariant** to every `wire`/`reg` name declared in the module
  body, so the check matches the sentence above. Roughly fifteen lines more
  than what exists; the care is in not firing on the declaration line itself,
  on identifiers inside string literals, or on the port half of
  `.portname(signal)` in an instance connection. Round-8 plan Task 2.
- **Or keep the narrow version** and rewrite the closure note to describe what
  it actually covers, leaving this axis OPEN with
  [GAP-20](#gap-20-high-testing-open---the-three-pre-declaration-render-sites-are-outside-every-oracle-and-no-test-elaborates-the-corpus-it-ships)
  as its corpus-wide counterpart.

Note also that this invariant is a `debug_assert!` in a workspace that ships
`[profile.release] debug-assertions = true`, so a violation **aborts a release
binary** - the failure mode round-7 plan Task 2 removed from `hoist_unresolved`
the same day, ten lines away. Same `cfg!(test)` fix applies.

**Closed 2026-08-20, round-8 plan Task 5.** The `cfg!(test)` fix now applies
here too: the violation branch pushes a real `Diag` UNCONDITIONALLY, before
the gated `debug_assert!`, so a real `mimz` binary in either profile exits
non-zero with a message instead of aborting. See this file's own audit-log
entry ([`audit-log/2026-08.md`](audit-log/2026-08.md), 2026-08-20) for the
fix and its verification
(`task5_declaration_order_violation_is_a_diagnostic_not_a_panic_outside_tests`,
`tests/hoist_declaration_order.rs`).

**Status update (2026-08-18).** Both halves of this gap's own "Recommended
direction" landed. Task 1: the exact post-emission invariant this entry
specifies (`assert_hoists_declared_before_use`, `emit_verilog/mod.rs`) - no
`__mimz_sub_N`/`__mimz_fn_sub_N` may be referenced before its own declaration
line, checked once per assembled module body. Verified it fires on all three
of [BUG-66](bugs/bug-61-70.md)'s reproductions and stays silent across the
whole 226-file shipped corpus. Task 3: BUG-66 itself fixed (a second hoist
buffer + insertion point, spliced right after the module's own signal
declarations), so the axis is now both watched AND, for every currently known
site, correct.

**What.** [GAP-16](#gap-16-high-architectural-closed-2026-08-16---the-self-determined-hoist-machinery-is-scoped-to-module-bodies-and-nothing-states-the-scope)
is about **which `decls` map is in scope** when a hoist site asks
`infer_kind` a question. Round-6 Task 1 answered it with a real runtime
invariant (`hoist_unresolved`: a hoist site may not silently do nothing when it
cannot resolve a `Kind`), and that invariant works - it found
[BUG-67](bugs/bug-61-70.md) and half of [BUG-68](bugs/bug-61-70.md) in round 7,
unprompted.

There is a **second, orthogonal axis on the same machinery**: **when is the
hoist buffer flushed, relative to the render that filled it.** `module()` fills
one shared `hoisted_decls` string and splices it in at a single `hoist_pos`
captured at `module/mod.rs:385`. Three render sites run _before_ that line and
can each hoist - instance port connections, `reg` reset `initial` seeds, `mem`
init/depth - so their hoisted wires are declared after their own use
([BUG-66](bugs/bug-61-70.md)).

**Why it matters, and why GAP-16's invariant cannot cover it.** The two axes
have different shapes:

| axis       | question                    | failure                                                                     | detectable by                                                                                               |
| ---------- | --------------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| GAP-16     | which `decls` is in scope?  | a map is missing an entry → `infer_kind` returns `None`                     | a generic assert on the `None` branch - Task 1's `hoist_unresolved`                                         |
| **GAP-18** | when is the buffer flushed? | a buffer is emptied at the wrong point → a correct wire lands after its use | **nothing today** - this is the `Some(k)` branch; `infer_kind` resolved, the hoist fired, the wire is right |

**Evidence.** BUG-66's three reproductions, each `mimz check` OK, `mimz compile`
exit 0, each rejected by real `iverilog` with
`Unable to bind wire/reg/memory … declaration after use`. Also: a machine check
over the emitted `.v` of all 226 shipped `.mimz` files finds zero out-of-order
references, which is exactly why every instrument in the project is green - they
all watch the module-body expression emitter.

**Recommended direction.** Not another rule about where hoists may happen. A
**post-emission invariant** on the text the emitter produces:

> No `__mimz_sub_N` / `__mimz_fn_sub_N` may be referenced in the emitted output
> before the line that declares it.

Twenty lines, one `debug_assert!` in `emit()`, and it covers the whole axis
generically rather than the three sites currently known. It would have caught
the instance site six rounds ago and the `reg` site the day BUG-65 landed. Long
term, [GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times)
removes the axis entirely: with a typed elaborated IR a hoisted node is just a
node, and there is no buffer to flush at the wrong time.

---

## GAP-19 (MEDIUM, testing, CLOSED 2026-08-18) - `wasm_parity` skips silently, and CI never builds the artifact it needs

**Status:** CLOSED 2026-08-18 (round-7 plan Task 9). Filed 2026-08-17 by round 7
([`review-2026-08-17.md`](review-2026-08-17.md), Part 8).

**Status update (2026-08-18).** Both of the "recommended direction"'s two
changes landed. `.github/workflows/ci.yml`'s `check` job now installs
`wasm-pack` and builds `crates/mimz-wasm/pkg` before the `Tests` step, which
sets `MIMZ_REQUIRE_WASM: "1"`. `tests/wasm_parity.rs::run_parity` mirrors
`support::require_iverilog`'s own convention exactly: the early return is now
a hard failure when `MIMZ_REQUIRE_WASM` is set and the pkg is missing, a
silent skip otherwise. Verified all three states by hand: pkg present → both
tests pass for real; pkg missing + flag set → both fail loud; pkg missing, no
flag → silent skip, unchanged from before.

**What.** `tests/wasm_parity.rs::run_parity` opens with:

```rust
let pkg_dir = manifest_dir.join("crates").join("mimz-wasm").join("pkg");
if !pkg_dir.exists() {
    eprintln!("skipping WASM parity test: mimz-wasm pkg not built");
    return;
}
```

`crates/mimz-wasm/pkg` is not tracked, does not exist in a fresh checkout, and
**`.github/workflows/ci.yml` never builds it** - the file contains no occurrence
of the string `wasm` at all. So `all_examples_work_in_wasm` and
`all_showcase_work_in_wasm` are **vacuous passes**, in CI and locally, and count
2 toward the suite total.

**Why it matters.** Two effects, both bad in different directions:

1. **No coverage.** The one test that would catch a genuine native/WASM
   divergence has not run in CI, ever.
2. **A false diagnosis it invites.** Round-6 Task 6 saw these tests fail and
   recorded them as "a pre-existing native/WASM emitter parity gap … unrelated
   to this task". They are not a parity gap - `mimz-wasm` depends on
   `mimz-sim` → `mimz-core` and compiles the _same_ emitter, so a rebuilt `pkg`
   cannot diverge. The failure was a **stale build artifact**: a `pkg/` built
   before BUG-65 added its `// NOTE (BUG-65 …)` line to every clocked module's
   emitted Verilog. The behaviour is deterministic, not intermittent - it fails
   exactly when a stale `pkg/` is present and passes vacuously otherwise, which
   is why Task 6 saw it and Tasks 7–9 did not.

**Evidence.** `pkg/` confirmed absent and untracked at `7286b71`; `ci.yml`
grepped for `wasm` (0 matches); the skip path read directly.

**Recommended direction.** Two small changes: build the WASM package in CI
before the test job, and make the skip **loud** - mirror this repo's own
`REQUIRE_IVERILOG` convention with a `MIMZ_REQUIRE_WASM` env var that turns the
early return into a failure, so a release gate cannot score the test green
without having run it.

---

## GAP-20 (HIGH, testing, OPEN) - the three pre-declaration render sites are outside every oracle, and no test elaborates the corpus it ships

**Status:** OPEN. Filed 2026-08-18 by round 8
([`review-2026-08-18.md`](review-2026-08-18.md), Part 4 and Part 7).

**What.** Three render sites in `module()` - `mem` init/depth, `reg` reset seeds,
and instance port connections - sit inside the pre-declaration window that
[GAP-18](#gap-18-high-architectural-closed-2026-08-19---the-hoist-buffers-flush-point-is-a-second-scoping-axis-and-nothing-watches-it)
is about. **Nothing in the project generates an expression at any of them.**

| oracle                                           | reaches those three sites?                                                                                                                      |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| differential fuzz (`tests/differential_fuzz.rs`) | **no** - instance connections are `arg_of(&name, signed)`, a plain identifier; `mem` initialisers are literal `0`; `reg` resets are literal `0` |
| shipped corpus (226 files)                       | **no** - no example combines a composite `mem`/`reg` initialiser or instance-port expression with anything that hoists                          |
| goldens (33 fixtures)                            | **no** - same corpus, plus text-only comparison                                                                                                 |
| Icarus suite                                     | partially - it elaborates the 50 files that have `test` blocks, via their emitted testbenches                                                   |

**Why it matters.** [BUG-66](bugs/bug-61-70.md) (three sites) and
[BUG-70](bugs/bug-61-70.md) both live in exactly this blind spot, and so does the
`const if` half of [BUG-71](bugs/bug-71-80.md). Gate 5 was clean at 5000/5000
before BUG-66 was found, clean at 5000/5000 after BUG-70 was introduced, and
clean at 5000/5000 while BUG-71 has been live for the whole series. A green deep
fuzz is real evidence about the module-body expression emitter - which is what it
was built for - and no evidence at all about this axis.

**This has been predicted twice.** Round-7's own plan file lists, under
"Not done": _"instance port connection with a hoisting expression, non-constant
`reg` reset, `mem` init over a signal, `const if`, emitted-testbench verdict."_
Four of those five are where round 8's findings are. It was carried forward as
optional hardening both times.

**A second, cheaper hole in the same area.** No in-repo test elaborates the
**176 corpus files without `test` blocks** at all.
`every_emitted_testbench_reports_pass_under_vvp` covers the 50 that have one;
the goldens cover 33 by text comparison. A design that compiles to Verilog no
elaborator accepts, and that ships no `test` block, is caught by nothing today.
Round 8 ran that sweep by hand - `iverilog -g2005` over all 226 emitted `.v`,
about three minutes, 0 failures - and it is the check that would have caught
BUG-70's class corpus-wide the moment it landed.

**Recommended direction.** Two independent items, smallest first.

1. **A corpus elaboration test**, in `tests/icarus.rs`, sharing
   `support::corpus_files()` with its three existing siblings: compile every
   corpus file, run `iverilog -g2005` on the emitted `.v`, assert success, and
   assert a floor (`>= 226`) so the sweep cannot silently shrink - the exact
   failure round-7 plan Task 10 found in this test's own siblings. It needs no
   `--emit-testbench`; the testbench half is already covered. This closes the
   second hole outright and gives the first one a corpus-wide net.
2. **Fuzz vocabulary for the three sites** - an instance port connection, a
   `reg` reset and a `mem` initialiser that can each carry a generated
   expression rather than a literal. Three separate features, each independently
   landable; start with the instance connection, which is the one both BUG-66 and
   BUG-70 reached through. Acceptance criterion in the style rounds 5–7 used:
   name the seed inside 400 that first produces a hoisting instance-port
   connection, and confirm the fuzzer finds BUG-70 with its fix reverted.

Item 1 is a day's smaller than item 2 and catches more today. Item 2 is what
makes the next unknown site in this window loud instead of waiting for a
reviewer to construct it.

**Status update (2026-08-20), round-8 plan Task 4.** Item 1's own premise
was checked directly rather than assumed, and turned out to be **stale
before this task even started**: `every_emitted_verilog_passes_iverilog`
(`tests/icarus.rs`) already ran `iverilog -t null` over the whole 226-file
corpus with a `checked >= 226` floor - added by round 7's own `8a24f86`,
before this gap's own review was written. The "second, cheaper hole" section
above and this entry's own item 1 both claim no such in-repo test existed;
that claim was wrong even at filing time, not merely later closed. `-t null`
was empirically confirmed to perform the SAME full elaboration/binding pass
as a real target (hand-built BUG-70's own broken declaration order and fed
it to both - identical `Unable to bind wire/reg/memory` rejection), so the
corpus-wide net item 1 asks for was live the whole time; the only real gap
was an unpinned language generation, closed by adding `-g2005` to that same
test rather than duplicating it. **GAP-20 stays OPEN**: item 2 (fuzz
vocabulary for the three pre-declaration sites, round-8 plan Task 9) is
still undone, and remains this gap's actual open half - the "first" oracle
this axis lacks, not the corpus sweep.

**Status update (2026-08-20), round-8 plan Task 9.** Item 2's instance-port
connection piece is done - `gen_special_leaves`'s array-instance branch
(`tests/differential_fuzz.rs`) now connects a second instance's input
through `extend(<first instance's own output>, growth)` nested as a concat
member, about half the time, matching BUG-70's own reproduction verbatim
(`{ b, extend(u1.q, 8) }`). Confirmed live that this is the load-bearing
shape and not an easier one: a bare `extend(s.q, w2)` alone as the WHOLE
connection - no outer concat - compiles fine even with Task 1's fix
reverted (its target width is already explicit, so no hoist is needed at
all when it is the sole top-level expression); only nesting it as a concat
member reaches GAP-18's own widened invariant. Verified against a live
revert of Task 1 (disabling `declare_instance_outputs`'s pre-pass AND
restoring inline wire declaration in `instance()`'s `Dir::Out` arm - a bare
pre-pass disable alone reproduces a DIFFERENT failure, an undeclared
implicit net, not BUG-70's declare-after-use): the generated seed then
fails `mimz compile` itself with GAP-18's "declared name `s_q` referenced
before its own declaration", and compiles clean again with the fix
restored. Acceptance test:
`task9_instance_port_connection_reaches_a_hoisting_expression_within_400_seeds`
(seed `CLOCKED_SEED_BASE+15` at the time of writing).

**The `reg`-reset and `mem`-init thirds of item 2 remain undone**, for a
reason not visible until attempting them: both sites' declared value is
read directly by `mimz-sim`'s own `elaborate_project` (`reg`) and by the
kernel's power-on seed logic (`mem`), and that code path requires the value
to be a **compile-time constant** - a pre-existing kernel limitation
[BUG-66](bugs/bug-61-70.md)'s own entry already names (`icarus_only_clocked`,
`tests/self_determined_regression.rs`, bypasses `mimz-sim` entirely for its
own `reg`/`mem` reproductions for exactly this reason). Making either
generator site emit a genuinely non-constant expression would break
`differential_fuzz_clocked_matches_icarus`'s own `elaborate_project`/`run`
calls for every program that reaches it - not a small fix alongside the
instance-connection change, but a second, Icarus-only oracle path (mirroring
`icarus_only_clocked`) that the existing kernel-vs-Icarus differential
cannot host as-is. Left for a follow-up round rather than folded into this
one.

Long term, [GAP-1](#gap-1-high-architectural---no-ir-widthkind-semantics-implemented-three-times)
removes the window: with a typed elaborated IR there is no pre-declaration
render phase to be outside of.

## GAP-21 (LOW, language, OPEN) - `clog2(PARAM)` cannot size a port (Verilog-2005 port-list scoping)

**What:** a port width of the form `bits[clog2(PARAM)]` - the classic "address
bits for a FIFO of depth N" pattern - is rejected with `E0420`. Body widths
(`wire`/`reg`/`mem`) accept it: the emitter injects a Verilog-2005 `clog2`
constant function into the module body, so body widths track an
instantiation-time parameter override. Only the PORT list is closed to it.
Before 2026-08-22 this split was invisible: `mimz check` folded the width under
the module's default parameter binding and passed, then `mimz compile` died in
the emitter with an _uncoded_ error. The checker now fires `E0420` at check
time (`checker/widths/sigs.rs`, `reject_clog2_param_port_width`), so both
commands agree; fixture `tests/fixtures/errors/e0420_clog2_param_port.mimz`.

**Is it truly a Verilog-2005 limitation? Yes, with nuance.** IEEE
1364-2005 allows constant-function calls in constant expressions, but a module
port range is part of the module HEADER, which lexically precedes every
body-local function definition, and 1364-2005 provides no forward-reference or
package mechanism for it (SystemVerilog relaxes this via packages and `$clog2`;
Verilog-2001/2005 tool behavior for header-local constant functions is
uniformly reject-or-unreliable). So the emitter's original refusal was correct

- the defect was the checker/emitter disagreement, not the refusal. Workarounds
  that work today: size a body signal with `clog2(PARAM)`, or pass the computed
  width in as its own `int` parameter from the instantiation site.

**Why it matters / recommended direction:** LOW priority because the
workarounds are cheap and the error is now honest and early. If a future
edition targets SystemVerilog output (or emits package-scoped helpers), revisit
and allow `clog2(PARAM)` ports; that change would land alongside the
per-instance `const if` elaboration design in
[`docs/Ideas/language_plan.md`](../Ideas/language_plan.md) section 13, since
both need definition-site-vs-instance-site resolution machinery. Found by the
2026-08-22 doc-code audit (M11/L1).
