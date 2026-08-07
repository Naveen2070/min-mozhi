# Architectural, language & process gaps

Findings that are **not defects** — nothing here is wrong behavior against the
current spec. These are structural limits, missing capabilities, and missing test
oracles that constrain what the project can safely become. Filed here so they are
trackable and rankable rather than living only inside a review narrative.

Split from the other audit files deliberately:

| File                           | What belongs there                                                               |
| ------------------------------ | -------------------------------------------------------------------------------- |
| [`bugs.md`](bugs.md)           | Wrong behavior against the spec — a program does the wrong thing                 |
| [`security.md`](security.md)   | Input-triggered crashes, overflow, memory safety                                 |
| [`hardening.md`](hardening.md) | Preventive measures added, and what was checked and found safe                   |
| **`gaps.md`** (this file)      | Correct-but-limited: architecture debt, absent language features, absent oracles |

Each entry states: **what**, **why it matters**, **evidence**, and the
**recommended direction**. New gaps append here; nothing is deleted. When a gap
is closed, its status is edited in place (same convention as `bugs.md`).

Source: [`review-2026-08-02.md`](review-2026-08-02.md).

## Index

| ID                                                                                                            | Gap                                                                     | Severity   | Status |
| ------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- | ---------- | ------ |
| [GAP-1](#gap-1-high-architectural--no-ir-widthkind-semantics-implemented-three-times)                         | No IR; width/kind semantics implemented three times                     | HIGH       | OPEN   |
| [GAP-2](#gap-2-medium--simulator-is-2-state-with-a-whole-value-unknown-flag-no-xz-no-tri-state)               | 2-state simulator; no X/Z, no tri-state/`inout`                         | MEDIUM     | OPEN   |
| [GAP-3](#gap-3-medium--parser-violates-the-projects-own-mandatory-help-contract)                              | Parser violates the mandatory-help contract (14/60 sites)               | MEDIUM     | CLOSED |
| [GAP-4](#gap-4-lowmedium--string-keyed-name-resolution-throughout-no-interning)                               | String-keyed name resolution; no interning                              | LOW→MEDIUM | OPEN   |
| [GAP-5](#gap-5-high-testing--no-declared-type-vs-produced-value-oracle-self-determined-positions-ungenerated) | No declared-type-vs-value oracle; self-determined positions ungenerated | HIGH       | CLOSED |
| [GAP-6](#gap-6-medium-language--no-assertions-assertassumecover)                                              | No assertions (`assert`/`assume`/`cover`)                               | MEDIUM     | CLOSED |
| [GAP-7](#gap-7-medium-language--no-enumbits-cast)                                                             | No enum↔bits cast                                                       | MEDIUM     | CLOSED |
| [GAP-8](#gap-8-medium-language--surface-gaps-division-attributes-pipelines-type-generics)                     | Surface gaps: division, attributes, pipelines, type generics            | MEDIUM     | OPEN   |
| [GAP-9](#gap-9-medium-dx--lsp-feature-set-and-missing-fix-it-spans)                                           | LSP feature set + missing fix-it spans                                  | MEDIUM     | OPEN   |
| [GAP-10](#gap-10-low-process--no-coverage-measurement-checker-and-emitter-unfuzzed)                           | No coverage measurement; checker and emitter unfuzzed                   | LOW        | OPEN   |

---

## GAP-1 (HIGH, architectural) — No IR; width/kind semantics implemented three times

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
width-conformance oracle — the ninth instance of this exact family since
BUG-11). Every future operator or builtin re-opens the same three-way drift
surface.

**Evidence.** `emit_verilog/kinds.rs:6` states the duplication is deliberate —
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

`Ext` becomes an explicit node the emitter **must** materialize — which makes
BUG-28 structurally impossible rather than a case-table entry.

Also unlocks: constant folding, dead-signal elimination, `--emit-netlist` for
debugging, per-module parallel emit, and any future non-Verilog backend (which
the Constitution's _"Verilog interop forever, even after native backends exist"_
clause already presumes).

**Related.** [GAP-4](#gap-4-lowmedium--string-keyed-name-resolution-throughout-no-interning)
should land as part of this, not separately.

---

## GAP-2 (MEDIUM) — Simulator is 2-state with a whole-value unknown flag; no X/Z, no tri-state

**Status:** OPEN. Filed 2026-08-02.

**What.** `Val` (`crates/mimz-sim/src/sim/value/mod.rs:36`) carries
`unknown: bool` — a single flag for the entire vector, with `bits` documented as
_"MEANINGLESS when `unknown` is `true`."_ There is no per-bit X, no Z, and no
`inout`. A `grep` over `spec/` returns zero hits for `inout` / `tristate` /
`'bz`.

**Why it matters.**

- `{1'b0, x_value}` cannot be modeled — the known half is lost.
- Uninitialized-register detection (the number-one real-world use of X in RTL
  simulation) is impossible.
- Bidirectional buses, open-drain I²C, external memory DQ pins — all
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

Ship `inout` / tri-state **after**, as a separate design — it needs
resolution-function semantics that directly contradict the current single-driver
rule (E0501).

**Sequencing.** Build on top of
[GAP-1](#gap-1-high-architectural--no-ir-widthkind-semantics-implemented-three-times),
not before it — otherwise the X rules get written twice.

---

## GAP-3 (MEDIUM) — Parser violates the project's own mandatory-help contract

**Status:** CLOSED 2026-08-04. Filed 2026-08-02.

**What.** `Checker::err` (`crates/mimz-core/src/checker/mod.rs:109`) takes `help`
as a **required** parameter — _"the teaching contract (spec/01 G1) is not
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
`return` — a beginner cannot recover from that message.

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
(`crates/mimz-core/src/parser/mod.rs`) — byte-for-byte the drafted signature
above, mirroring `Checker::err`. All 60 call sites across `mod.rs`, `expr.rs`,
and the 8 `items/*.rs` files filled with a construct-specific teaching help
line; the 14 sites with a separate opt-in `self.help(...)` had it folded into
the `error()` call, and `help()` itself deleted (no longer reachable).
Test: `every_parse_error_carries_a_help_line`
(`crates/mimz-core/src/parser/tests/safety_and_precedence.rs`) sweeps a
broken-source case per `E11xx` code (in-crate, mirroring the existing
`every_parse_error_carries_a_code` structural test, rather than a new
fixtures-directory + CLI subprocess suite — the same contract, smaller
surface). Watched it fail (`garbage here` / E1102 had no help) before the fix.

---

## GAP-4 (LOW→MEDIUM) — String-keyed name resolution throughout; no interning

**Status:** OPEN. Filed 2026-08-02.

**What.** `ExprKind::Ident(String)`, `HashMap<String, …>` symbol tables,
`HashMap<(usize, String), Rc<Scope>>` scopes, and ~203 `.clone()` calls in
non-test `mimz-core`. Every one of the nine checker passes re-resolves each name
by string hash; the emitter and the simulator then do it again. Only
`QualIdent.resolved_file` caches anything.

**Why it matters — and why it is not urgent.** Measured on a release build:

| Design                    | `mimz check` | `mimz compile` |
| ------------------------- | ------------ | -------------- |
| 4,008 lines / 2,000 regs  | 0.77 s       | 1.67 s         |
| 16,008 lines / 8,000 regs | 1.65 s       | —              |

Scaling is roughly linear (4× input → 2.1× wall, startup-dominated). No quadratic
blow-up in name resolution, driver analysis, or the combinational-cycle DAG
check — better than a `HashMap<String, _>` design would suggest.

But there is no interning, no `SymbolId`, and no arena. The ceiling arrives with
real designs (a soft CPU plus peripherals is 50–200k lines of generated RTL) and
it arrives as a wall, not a slope.

**Direction.** Intern identifiers to `Symbol(u32)` during parse, key everything on
the id, and give `Ident` a `Cell<Option<DefId>>` resolved once. Expect 3–10× on
the checker and a large drop in allocator pressure.

**Sequencing.** Do this as part of
[GAP-1](#gap-1-high-architectural--no-ir-widthkind-semantics-implemented-three-times),
**not** as a standalone refactor — otherwise the churn is paid twice.

---

## GAP-5 (HIGH, testing) — No declared-type-vs-produced-value oracle; self-determined positions ungenerated

**Status:** CLOSED 2026-08-04. Both directions' testing infrastructure
landed 2026-08-03 (static matrix, width-conformance property, randomized
position-aware generation), and everything that infrastructure found
(BUG-34, BUG-35, BUG-36) is now fixed — see `bugs.md` for each. The
oracle gap itself (the actual subject of this entry) is closed: the
fuzzer now asserts width-conformance on every run, and its generator now
reaches every self-determined position with random `Builtin`-wrapped
fragments, not just hand-picked ones.

**Update 2026-08-03 (branch `bug-33-gap-5-perf-and-width-oracle`).**
Direction 1's width-conformance assertion landed in
`tests/differential_fuzz.rs` (`assert_bits_fit_width`): after every kernel
evaluation (both the combinational and clocked differential tests), every
signal's produced `Bits` is checked against the width the SIMULATOR itself
resolved during elaboration (`comb::Output::width`, `Timeline::signals`) —
an independent authority from the fuzzer's own generator bookkeeping. No
generator change was needed, as GAP-5's own direction predicted. Running the
now-instrumented fuzzer at deeper `N` (validating the new assertion) found
zero width-conformance violations but surfaced an unrelated, real
kernel-vs-Icarus divergence at `N=100` — filed and fixed as
[BUG-34](bugs.md) (chained shifts on a signed operand).

**Update 2026-08-03 (branch `bug-34-chained-signed-shifts`, same day).**
Direction 2's fuzzer generator extension landed too — GAP-5 is now FULLY
addressed at the infrastructure level (both static matrix and randomized
generation exist; what remains is fixing what they find). `wrap_builtin`
(`tests/differential_fuzz.rs`) wraps a randomly generated fragment in a
randomly chosen `Builtin` call (`Extend`/`Trunc`/`SignedCast`/
`UnsignedCast`/`Abs`/`Min`/`Max`/`Nand`/`Nor`/`Xnor` — the same set
`tests/self_determined_regression.rs`'s static matrix classifies),
following the exact width/kind rule each builtin's `call_ty` uses. Wired
into `gen_expr`'s dispatch as a new combinator, it needed no separate
per-position wiring: `combine_concat`, `combine_same_width`'s comparison
operators, and `cast_to` already accept any composite fragment as an
operand, so a builtin-wrapped fragment reaches the concat-member,
comparison-operand, and `signed`/`unsigned`-argument self-determined
positions purely through the generator's existing composition. Running it
at `N=300` immediately found two NEW, real, previously-unknown bugs
(exactly what this direction was for) — filed as [BUG-35](bugs.md) (a
shift whose left operand is a builtin call isn't hoisted in a
self-determined position) and [BUG-36](bugs.md) (`trunc()` of a
non-identifier expression emits an invalid Verilog part-select — BUG-20's
own class, reopened through a different call site). Both left OPEN,
deliberately out of scope for the branch that found them.

**What.** The test architecture is strong — Icarus differential in two layers,
`REQUIRE_IVERILOG=1` so it can never silently skip, a 1003-line random-program
differential fuzzer, 4 libFuzzer targets, docs/grammar sync tests, WASM parity.
But **every oracle compares simulator vs. Verilog.** There is no oracle asserting
**declared type vs. produced value**.

**Why it matters.** This is exactly the shape of the two most serious findings in
[`review-2026-08-02.md`](review-2026-08-02.md):

- **BUG-28 / BUG-29** pass the simulator and fail the hardware. They survive the
  fuzzer because its generator is documented as checker-clean _"by construction —
  every combine step unifies operand widths via `extend()`"_
  (`tests/differential_fuzz.rs:8`), which keeps every `extend` in a
  **context-determined** position. The broken case only appears in a
  **self-determined** position, which the generator never produces.
- **BUG-30** fails _neither_ oracle: the simulator and Verilog agree with each
  other, and both disagree with the declared type.

**Direction — two additions, in priority order.**

1. **Width-conformance property.** After every simulator evaluation, assert the
   produced `Val` fits the checker's declared width for that expression. Wire it
   into the existing fuzzer — it needs no new generator, only a new assertion,
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
   gained `matrix_shape` — an exhaustive match over `Builtin` (no wildcard; a
   14th variant is a compile error until classified there) plus one
   differential test per testable variant. Tracing the actual call graph while
   fixing BUG-29 found the "5 positions" framing overstates the real
   dimensionality: `verilog_self_determined_kind`, `kind_is_inferrable`, and
   `hoist_if_needed` are pure functions of the expression alone and are the
   _exact same three functions_ at every call site (`Concat`/`Replicate` share
   one code path byte-for-byte; a comparison operand and a `$signed`/
   `$unsigned` argument are two more, already exercised) — so one test per
   builtin at one position (a `Concat` member) exercises the whole shared
   mechanism, and a replication COUNT can never carry a runtime builtin call
   at all (`replicate_ty` requires it compile-time-constant, and it folds to
   a literal before emit ever reaches this code). The fuzzer's own generator
   extension (placing RANDOM programs, not hand-picked ones, into these
   positions) is not done — still open, and is where BUG-28/29-_shaped_ bugs
   in operators (not builtins) would be caught. Direction 1
   (width-conformance property, BUG-30's own oracle gap) is also still open.

---

## GAP-6 (MEDIUM, language) — No assertions (`assert`/`assume`/`cover`)

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
`S05xx` family, direction 3's ask — landed as its own dedicated code
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

## GAP-7 (MEDIUM, language) — No enum↔bits cast

**Status:** CLOSED 2026-08-07. Filed 2026-08-02.

**What.** There is no way to obtain an enum value's bit encoding.
`unsigned(enumval)` is rejected; an enum in a concat produces E0403.

**Why it matters.** The encoding already exists and is stable — the emitter
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
gated behind an exhaustiveness-checked construct — an unchecked int→enum cast
would reintroduce the invalid-state class the enum type exists to prevent.

**See also.** [BUG-31](bugs.md) — the diagnostic a user currently hits when they
try this is actively misleading.

**Fix.** Landed the explicit `encoding(e)` builtin, exactly as this entry's
own Direction recommended over overloading `unsigned(x)`. `Builtin::Encoding`
joins the existing plain-identifier builtin table (`from_name`) — not a
keyword, so no `lang/keywords.toml`/trilingual work at all, unlike GAP-6.
The checker requires `Ty::Enum` and returns `bits(en.inferred_total_width)`
— the FULL tag+max-payload width for a tagged union, not just the tag;
everything else is a brand-new **`E0418`** (next free `E04xx` slot), never a
reuse of `E0407`, per this project's one-code-per-rule catalog policy. The
emitter and simulator needed no enum-specific logic at all: an enum-typed
signal is already tracked as a generic `Kind{width, signed}` at that layer,
so `encoding` reuses `unsigned(x)`'s exact mechanism (self-determined-
position classification, hoisting, `$unsigned(...)` codegen, runtime eval)
byte-for-byte in every exhaustive `Builtin` table it joins. GAP-5's own
position-matrix (`tests/self_determined_regression.rs`) gained `Encoding`'s
classification plus two Icarus differential tests (tag-only and
payload-carrying enum) — found and worked around two separate, narrow,
pre-existing `mimz-sim` bugs along the way (`comb::eval_outputs` doesn't
model any enum signal yet, filed as [BUG-38](bugs.md); a `reg`'s reset
value can't be a payload-carrying `EnumConstruct` expression, filed as
[BUG-39](bugs.md)), neither fixed here, both filed for a future branch.
Five new examples (`enum_encoding.mimz` × english/tanglish/tamil/
mixed/tamil-pure) exercise the motivating "expose FSM state on a debug port"
use case this entry's own Why-it-matters described. The reverse direction
(`bits` → `enum`) stays permanently unsupported, as this entry's own Related
note specified — not deferred, rejected. Full workspace green throughout,
`fmt`/`clippy -D warnings` clean.

---

## GAP-8 (MEDIUM, language) — Surface gaps: division, attributes, pipelines, type generics

**Status:** OPEN. Filed 2026-08-02.

Grouped because each is individually small to state and each blocks a specific
downstream domain.

### 8a — No `/` or `%`

`BinOp` (`crates/mimz-core/src/ast/expr.rs:225`) has no `Div` or `Mod`.
Defensible — dividers are expensive and should not be implicit — but there is no
`divmod`-by-constant either, so power-of-two division needs manual shifts with no
width-checking help and no diagnostic explaining why. **Direction:** allow
division by a compile-time constant power of two (folds to a shift, fully
checked), and give a teaching error for the general case that names the cost.

### 8b — No synthesis attributes

No `ram_style`, `keep`, `dont_touch`, `async_reg`, `max_fanout`. Once real FPGA
designs are generated these are needed and there is **no escape hatch at all** —
`extern module` covers instantiating foreign Verilog but not annotating
mimz-generated signals. **Direction:** an attribute syntax with an explicit
vendor-mapping table, so the mapping is data rather than emitter special cases.
Interacts with [BUG-32](bugs.md) (memory style).

### 8c — No pipeline construct

Manual stage registers only. Given the teaching mission, and that pipelining is
_the_ concept students struggle with most, a `pipeline` / `stage` form would be
both high-value and differentiating — and it is a natural fit for a language that
already owns clock-domain information statically. **Direction:** design after
[GAP-1](#gap-1-high-architectural--no-ir-widthkind-semantics-implemented-three-times);
a pipeline construct wants an IR to lower into.

### 8d — No parameterized-type generics

Modules take `int`/`bool` parameters only. No type parameters, so no generic
FIFO-of-T. `bundle` softens this but does not replace it. Compounds with BUG-12
(`fn` cannot be parameterized by module scope), which is the bigger day-to-day
tax of the two. **Direction:** BUG-12 first; type generics are a v0.5-scale
language change and warrant an RFC.

---

## GAP-9 (MEDIUM, DX) — LSP feature set and missing fix-it spans

**Status:** OPEN. Filed 2026-08-02.

**What.** `src/lsp.rs` advertises `hover_provider`, `definition_provider`, and
`completion_provider`, plus push diagnostics. Missing: find-references, rename,
document symbols, formatting, semantic tokens, and code actions.

Separately, `JsonDiag` (`crates/mimz-core/src/diag.rs:282`) carries
`severity / code / message / help / path / line / col / span` — but **no
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
3. Then find-references and rename — `analysis.rs`'s symbol index is already
   structured to support both, so these are mostly plumbing.
4. Document symbols, semantic tokens, formatting (`mimz fmt` already exists;
   exposing it over LSP is small).

---

## GAP-10 (LOW, process) — No coverage measurement; checker and emitter unfuzzed

**Status:** OPEN. Filed 2026-08-02.

**What.** Two process gaps in an otherwise strong assurance story.

**10a — No coverage measurement in CI.** No `cargo llvm-cov` (or equivalent)
step, so untested paths are invisible. With 1114 tests the _absolute_ number is
reassuring, but nothing shows which of the nine checker passes, which emitter
branches, or which `Builtin` arms are actually exercised. BUG-28 lived in an
unexercised arm.

**10b — The checker and the emitter are unfuzzed.** The four libFuzzer targets
are `lex_parse_compile`, `lex_parse_eval`, `pretty_roundtrip`, and
`translate_roundtrip`. They cover lex/parse/eval and two round-trip properties.
The passes where BUG-28/29 live — the checker's width rules and the emitter's
self-determined-position logic — have no fuzz target, only hand-written tests and
the differential fuzzer (whose generator gap is
[GAP-5](#gap-5-high-testing--no-declared-type-vs-produced-value-oracle-self-determined-positions-ungenerated)).

**Direction.**

1. Add `cargo llvm-cov` to CI as a **reported, non-gating** number first; set a
   floor only once the baseline is known.
2. Add a fifth fuzz target driving `checker::check` → `emit_verilog::emit` on
   generated-but-plausible ASTs, asserting the two invariants that must always
   hold: _checker-accepted input never panics the emitter_, and _emitted Verilog
   always elaborates under `iverilog -t null`_.

**See also.** [HARD-9](hardening.md) tracks 10b as a recommended hardening item.
