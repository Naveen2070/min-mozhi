# 07 — Decisions That Shaped the Code, and Where It Goes Next

The full dated record lives in [`docs/log/`](../log/) (Decision blocks)
and the contract in [`docs/architecture.md`](../architecture.md). This
page is the code-focused digest: what a contributor must understand
before "improving" something that is the way it is on purpose.

## Decisions baked into the current code

### Rust, stable toolchain, tiny dependency set

Rust for the compiler (memory-safe systems language, great error-message
ecosystem, single static binary — full rationale in the roadmap's Why-Rust
decision record). MSRV 1.85 (edition 2024). Dependencies are deliberately
few — each one earns its place; resist adding more: `clap`,
`serde`+`serde_json`+`toml` (the keyword table and the `--json` wire
format), `unicode-ident`, `unicode-normalization`, and — for the LSP
only, used by the bin-only `lsp.rs` module so the lib stays async-free —
`tower-lsp`+`tokio`.

### Lib + thin binary, modules per stage — then a 3-crate workspace

The trigger named in architecture section 5 FIRED on 2026-06-12: the LSP
(plus `--json` consumers) is the second consumer, so the module tree
moved into a `lib.rs` with `main.rs` as a thin CLI shell (arg parsing +
human/JSON rendering only). `project.rs` returns `LoadError` values
instead of printing and exiting — the lib never touches stdout/stderr.

A second trigger fired on 2026-07-09/10: the WASM playground needed a
dependency-optional-free build without feature-flag gymnastics, so the
lib split into a 3-crate workspace along the pure/impure axis —
`mimz-core` (lexer/parser/ast/checker/emit_verilog/etc, zero optional
deps), `mimz-sim` (event-driven simulator, depends only on mimz-core),
and the root shell crate (CLI, fs I/O, LSP, hw-emulation), which
re-exports both as a facade so every `mimz::…` path a caller already used
keeps compiling. Full rationale in
`docs/plan/workspace-split.local.md`. An IR, a query system, and
incremental compilation remain trigger-based and have not fired — named
triggers, not speculative scaffolding.

### The module-scoping pattern (refactor of 2026-06-10)

Big files were split with one repeating pattern, used by `lexer/`,
`parser/`, `ast/`, `emit_verilog/`, and (since 2026-06-12, when the
width pass hit 1859 lines) `checker/widths/`:

- `mod.rs` owns the struct, its state, and private plumbing;
- sibling files (`items.rs`, `expr.rs`, `module.rs`…) hold `impl` blocks
  for one concern each, reached via a few `pub(super)` entry points;
- Rust privacy does the rest: `mod.rs` items are visible to descendant
  modules without being `pub` anywhere else.

Follow the pattern when a file outgrows ~600 lines; don't pre-split.

### Keywords are data (`lang/keywords.toml`), loaded once, validated loudly

Native-speaker word review must be a data change, never a code change.
The table panics at startup on any inconsistency — CI catches table bugs
before any user can. See [`02-lexer.md`](02-lexer.md).

### Errors are values; multi-error everywhere

See [`06-diagnostics.md`](06-diagnostics.md). This shapes every
function signature in the pipeline (`Result<_, Vec<Diag>>`, parser
`Option` + recorded diags).

### The emitter is a string formatter, on purpose

Architecture invariant #6: "deliberately dumb and readable". Symbolic
widths, parens around everything, no const-eval, errors instead of
guesses for unsupported features. The Phase 2 IR demotes it to a
debugging backend; do not grow it into a compiler in the meantime.

### Safety rules enforced at the earliest possible stage

| Rule                                                             | Enforced today by                      | Eventually by |
| ---------------------------------------------------------------- | -------------------------------------- | ------------- |
| no `/` / `%` operators                                           | lexer                                  | lexer         |
| `=` wires-only vs `<-` regs-only                                 | parser                                 | parser        |
| reg requires reset value                                         | parser                                 | parser        |
| if-expression requires `else`                                    | parser                                 | parser        |
| no comparison chaining                                           | parser                                 | parser        |
| widths, single-driver, exhaustiveness, comb-DAG, clock ownership | **checker** (all landed 2026-06-11/12) | checker       |

The earlier a rule fires, the better the span and the simpler the
message. Push enforcement as early as it can correctly live.

### `loop`/`suzhal` and `sync loop` — two different unroll shapes, one keyword pair

`loop`/`suzhal`/சுழல் (spec v0.2.21, 2026-07-05) is a **compile-time**
unroll usable inside `on` blocks and `fn` bodies — same unrolling-at-build
model as `repeat`, just with `fn`-body early-return support threaded through
via continuation-passing in the emitter and a real early return in the
interpreter. `sync loop` (spec v0.2.22, 2026-07-06) is a different thing
entirely: a **multi-cycle** FSM+counter that actually costs `hi - lo + 1`
clock cycles in hardware, lowered to ordinary `Port`/`Reg`/`On`/`Drive`
primitives before the checker or emitter ever see it. `sync` is a
**dual-purpose token** — also reserved for a not-yet-implemented CDC
synchronizer builtin-namespace (`sync.double_flop(...)`) — disambiguated by
the token immediately following it (`loop`/`suzhal`/சுழல் vs `.`), so the
two future grammars never conflict. See spec/03's keyword table entry for
`KW_SYNC` for the full disambiguation note.

### `Ty::Bundle` replaces the `Wcx::bundle_sigs` side-table (2026-07-11)

Bundle-typed values used to type-check via a special-cased side-table
(`Wcx::bundle_sigs`) rather than a real type. `Ty::Bundle` (commit
`8e9d575`) gave them a real `Ty` — nominal identity plus on-demand field
resolution via `resolve_bundle_fields` — so a bundle behaves like any other
type in the width pass instead of needing bundle-specific plumbing at every
call site. This is what let bundle-typed `fn` call arguments (`a935b90`)
and return values (`58181bf`) get shape-checked the same way any other
`fn` argument/return does, immediately afterward.

### `foreach` — sugar, not a new execution model (2026-07-12/13)

`foreach <var> in <source> { }` (range form `0..N`, elements form over an
array or `mem`) desugars to `repeat`/`loop` at the AST level
(`ast::foreach_lower`) before the checker, emitter, pretty-printer, or
simulator ever see a `ForEach` node — only the checker validates `ForEach`
directly, so its one error (E0417: elements-form source must be array/mem
typed) can point at the original syntax. The keyword was reserved
2026-07-12 and the sugar landed 2026-07-13. Its Tanglish/Tamil spellings
(`ovvondraga`/ஒவ்வொன்றாக, "one by one, each") are PROVISIONAL placeholders,
maintainer-authorized so four-flavor tooling works now, pending
native-speaker review (R9/R11) — same review-gate pattern as `sync`/`loop`
above.

### `Enum.Variant(args)` construction — positional args, zero new codes, match the existing per-layer duplication (2026-07-14)

The tagged-union feature (2.7) shipped with matching (`Pattern::Variant`)
but no way to actually build one — `Packet.Ctrl(k)` was a parse error.
Three decisions, made during brainstorming before implementation:

- **Positional arguments only**, no named-field syntax — mirrors
  `Pattern::Variant`'s own binding convention (D2: enum field _names_ are
  documentation only) so the read side and write side agree.
- **Zero new error codes**: E0806 (payload arity mismatch) and E0401
  (generic width mismatch) are reused with generalized wording, rather than
  minting construction-specific codes, since the underlying failure modes
  are identical to their pattern-side counterparts.
- **Architecture: match the existing per-layer duplication precedent
  rather than centralize** (Option B from brainstorming). This codebase
  already computes `tag_w`/`max_payload_w`/field-packing independently in
  four places for the pattern-matching side (checker's names.rs arity
  check, widths pass, the Verilog emitter's `arm_binding_exprs`, and the
  simulator's `variant_bindings`) rather than sharing one helper.
  `EnumConstruct`'s new `ExprKind` variant follows the same shape: one new
  arm per existing pass, each re-deriving the same layout math its
  pattern-side sibling already has, instead of introducing a shared
  abstraction this codebase has consistently avoided elsewhere.

One correctness issue surfaced only once real values were run through the
simulator and Icarus: `ExprKind::Concat` evaluates each part to its own
natural width (a bare int literal is its minimal bit-width, not the
tag/field/padding width the layout requires), so both backends now pin
every concat part's width explicitly — the simulator via `extend(_, N)`,
the emitter via an explicitly-sized `N'd<value>` literal instead of
Verilog's unsized-literal-in-concat default (32 bits per the LRM).

### `extern module` — Verilog FFI: `ModuleTarget`, coarse taint, and the warn/strict split (2026-07-15)

The Constitution promises Min-Mozhi will eventually wrap real Verilog
(spec/01 §4.2); `extern module Name(params) { doc: "...", ports }`
(design doc `docs/superpowers/specs/2026-07-15-verilog-ffi-design.local.md`)
is the first cut, landed in one day across lexer → parser → checker →
emitter → simulator → config/CLI → fixtures (commits `7aa1000`
through this one). Four decisions worth recording:

- **`ModuleTarget<'a>` mirrors `Module`'s shape exactly, rather than a
  separate ports-only type.** `ExternModule` (`crates/mimz-core/src/ast/mod.rs`)
  is deliberately shaped like `Module` minus a body, and every
  connection-check/emission call site that used to resolve a name to
  `&Module` now resolves to `ModuleTarget::{Real, Extern}` — a thin enum
  exposing `.name()`/`.params()`/`.items()` generically. Because those
  three accessors are all any existing connection-checking or
  instantiation-emission code ever reads, none of that code needed a
  single behavioral change; only `is_extern()` is consulted at the few
  spots that must genuinely diverge (skip elaboration, prefer
  `verilog_name` when emitting). Same reuse-over-new-abstraction
  instinct as `Enum.Variant` construction above, applied the opposite
  direction — one shared type instead of one new arm per pass.
- **Coarse whole-value `unknown` taint on `Val`, not true four-state
  simulation or emulation-host wiring.** `mimz-sim`'s `Val`
  (`crates/mimz-sim/src/sim/value.rs`) is a 2-state bit-vector; real
  per-bit X-propagation would be a simulator-wide rework (every
  operator, the VCD writer, the Icarus differential suite) — its own
  project, not scoped to one feature. Rewiring the hardware-emulation
  peripheral system (`src/emulate/`, `EmulationHost`) to let a native
  Rust twin drive an extern instance was rejected too: `EmulationHost`
  binds top-level design ports today, not arbitrary child instances,
  and most IP that actually needs `extern` (DDR controllers, PCIe
  cores) can't be accurately simulated without the vendor's own
  protected models anyway — a hand-written behavioral twin has low
  expected payoff for real cost. Instead `Val` gained one `unknown: bool`
  field (whole-value, not per-bit); every operator's dispatch point
  propagates it (`unary`/`binary` and `eval`'s `Concat`/`Replicate`/
  `Index`/`Slice` arms) — one propagation rule, not per-operator
  special-casing.
- **`warn` (default) vs `strict` sim mode, config- and CLI-selected.**
  `warn` lowers an extern instance's outputs to `unknown`-tainted at
  elaboration and keeps simulating (one warning printed per instance,
  first touch); a `test`/`expect` against a tainted value still fails
  loudly, so `warn` mode can never silently pass on faked data. `strict`
  hard-errors at elaboration the moment any extern instance is seen,
  before running anything. The choice belongs to the project (or a
  one-off invocation), not the compiler, since whether a black box is
  acceptable depends entirely on what the surrounding test is checking.
- **File wiring is a union, never an override.** `mimz.toml`'s
  `[compile] verilog_files` (companion `.v` files) and the repeatable
  `--extern-src` CLI flag are additive — a project's checked-in default
  list and a one-off ad hoc file never fight each other, matching the
  same union pattern the codebase already uses elsewhere for
  config-vs-CLI defaults.

See `tests/fixtures/extern/pll.mimz`/`pll_alias.mimz`/`pll.v` for the
worked with/without-alias examples and `tests/extern.rs` for the
end-to-end coverage; extern-module fixtures are deliberately excluded
from the five-flavor/Icarus-differential sweep (they need a companion
`.v` file that pipeline doesn't model).

### Structural bundle matching — one shared helper, four call sites (2026-07-16)

Bundles were nominally typed since their introduction (`Ty::Bundle`
equality was `a.name == b.name`) — feature 2.9 in
`docs/plan/phase-2-ir-synthesis.md` always intended to relax this
(`spec/02`'s bundle rules carried a "deferred to feature 2.9" breadcrumb
from day one). Implementation found a **prerequisite bug** during design:
bundle-typed ports connected across a module-instantiation boundary emitted
broken Verilog (the emitter treated a bundle-typed port as a single scalar
port/wire) — fixed first, since the feature's most natural use case (module
ports) sat directly on top of it.

The structural rule itself: the required bundle's fields must be a subset
of the provided bundle's fields, every shared field's type must match
EXACTLY (no width coercion — the constitution's no-silent-truncation
guarantee holds here too), extra fields on the provided side are ignored.
One pure helper (`BundleShapeMatch` + `Checker::bundle_shape_match`) is
consumed by four call sites (`Drive`-path, `let`/`fn`-arg `expect_ty`,
`fn`-return `check_return_ty`, module-port `check_inst_widths`) — each
keeps its own pre-existing error code for "field type differs" (`E0907`
for `Drive`-path and `let`/`fn`-arg `expect_ty`, `E0804` for `fn`-return
`check_return_ty`, `E0401` for module-port `check_inst_widths`) and shares
one new code, `E0910`,
for "field missing entirely" (a genuinely new failure category no call
site could raise under the old nominal-only rule). Two call sites
(`ops.rs` operator typing, `patterns.rs` if/match-arm unification)
deliberately keep the old nominal rule — bundles reaching an operator are
already nonsensical, and if/match-arm structural unification would need
threading resolution context through `unify_arms`, out of this feature's
approved scope; a documented gap, not an oversight.

The emitter needed a fix (the prerequisite bug) but zero NEW changes for
structural matching itself — bundle flattening at every site was already
purely field-NAME-driven (an assignment iterates the LHS bundle's own
field list and emits `assign lhs_field = rhs_field`, never comparing the
two bundle TYPE names), so once the checker widened what it accepts, the
emission layer needed nothing further — the same "generic code, checker
widens what it accepts" precedent `extern module`/`ModuleTarget`
established one feature earlier.

### `T?`/`??` valid-bundle sugar — reuse over a parallel type system (2026-07-17)

`T?` (`bit?`/`bits[N]?`/`signed[N]?`) desugars at parse time to a reference
to one of two compiler-synthesized bundle declarations
(`ast::builtin_valid_bundles`: `__Valid(N: int = 1) { valid: bit, data:
bits[N] }` and `__ValidSigned(N: int = 1) { valid: bit, data: signed[N] }`)
rather than becoming a new `Type::Optional` variant with its own checker/
emitter/simulator arms. The deciding factor: `§1.12`'s structural bundle
matching (feature 2.9, shipped one day earlier) already gives a bundle
type free field-extraction, free flattening in both backends, and free
literal-construction diagnostics (E0901/E0902) — a parallel `Optional`
type would have needed to re-derive all three from scratch for a shape
that a bundle already models exactly. The parser resolves `T?` to a
`Type::Bundle` node naming `__Valid`/`__ValidSigned`; everywhere past the
parser, it is an ordinary bundle and nothing downstream needs to know it
came from sugar (diagnostics render it back as `T?` — Task 6 — purely
cosmetic, not a separate code path).

**Accepted consequence: structural interchange.** Because `T?` is just a
bundle with a well-known shape, feature 2.9's own rule applies to it for
free — a user-declared `bundle Maybe { valid: bit, data: bits[8] }` is
structurally identical to `bits[8]?` and satisfies it (and vice versa) at
every call site. This was not special-cased away; it's the natural, and
intentional, result of choosing reuse over a new type. Task 11's
regression test proves it's not just type-checked but byte-identical at
emission: a `bits[8]?`-typed wire and a nominally-different but
identically-shaped user bundle produce the same Verilog.

**Scope correction: OR-mux touches every call site, not one rule.** The
plan for `??`'s OR-mux form (`T? ?? T? -> T?`) originally assumed a single
dedicated emission rule would cover it, on the theory that `??` is "just
another expression" the existing generic expression-emission path would
handle. That's wrong for a bundle-typed result: a bundle doesn't have one
Verilog value to substitute, it has one value **per field**, and a
bundle-typed expression gets its fields extracted at whichever of several
places consumes it — wire/reg initializer, `Drive` RHS, module-port
connection, `fn`-call argument (four sites in `mimz-core`'s emitter;
wire-init and `Drive` in `mimz-sim`'s simulator, which lacks the other two
— see the two new bugs.md entries below). Each site independently asks
"what's the value of field X of this bundle-typed expression", so OR-mux
lowering had to live in a shared per-field helper
(`coalesce_field_expr`/`bundle_field_expr`) called FROM every one of
those sites, not a single top-level rule the way unwrap-form (which
collapses to one ordinary scalar ternary, `a_valid ? a_data : b`) could
be. Discovered while writing the plan itself, before any code was wrong —
a genuine scope correction, not a review finding.

**Lesson from code review: chained `??` needs recursive lowering.** The
OR-mux helper's first version, in both crates, handled only a single
`lhs ?? rhs` pair: it called `self.expr(operand)` (emitter) or built a
bare `Field` node (simulator) on each operand to get its per-field value.
That's correct when both operands are plain bundle-typed signals, but `??`
is left-associative and chains (`x ?? y ?? z` parses as
`Coalesce(Coalesce(x, y), z)`), so an operand can itself be a `Coalesce`
node — a bundle-typed compound expression, not a signal. Treating it as a
signal and bolting `_fname` onto its rendered text (emitter) or handing it
to `Field`'s generic fallback (simulator, which recurses through the
UNWRAP arm — the wrong semantics for a still-bundle-typed value) produced
syntactically invalid Verilog for the emitter case and silently wrong
values for the simulator case. Caught in Task 8's code review before it
shipped; the fix recurses into a nested `Coalesce` operand
(`coalesce_operand_field` / `bundle_field_expr`'s own `Coalesce` arm)
instead of rendering it as an opaque signal — Task 10's simulator OR-mux
was written with the fix already in place, citing the emitter's review
finding directly rather than repeating the mistake. The lesson: a
left-associative chaining operator whose lowering touches sub-fields, not
just sub-values, cannot assume its operands are leaves — the naive
per-call-site "render the operand, then suffix a field name" approach is
only correct for the base case, and silently wrong (not a compile error)
for the recursive one.

## How the code has actually turned out (honest notes)

- The front end (lexer → parser → emitter) went from empty repo to
  25 passing tests in one day (2026-06-10) — the string-emitter decision
  is what made that possible.
- The trilingual thesis held with **zero** extra code outside the lexer:
  the EN/Tanglish byte-identical-output test passed on first run once the
  table loaded. The "one shared AST" invariant is doing real work.
- Pitfalls hit during implementation, recorded so they don't recur: the TOML
  root-key ordering trap (`02-lexer.md`), `use TokKind::*` glob shadowing
  the `Kw` type in pattern positions (write `Kw(token::Kw::And)`),
  clippy's `manual_strip` pushing us to the cleaner `verilog_literal`
  helper.

## Where the code goes next (in order)

1. ~~**Checker**~~ — ✅ landed 2026-06-11/12, seven passes in
   `crates/mimz-core/src/checker/` (symbols, consteval, names, widths, drivers, funcs, clocks);
   every spec/02 section 6 rule is now compiler-enforced.
2. ~~**Stable error codes**~~ — ✅ complete 2026-06-12: every diagnostic
   in the compiler carries one (checker E0xxx, lexer E10xx, parser
   E11xx, loader E12xx — full map in docs/code/06).
3. ~~**Icarus Verilog differential tests**~~ — ✅ landed 2026-06-11
   (`tests/icarus.rs`, self-checking TBs per example).
4. ~~**`repeat` emission + emitter hardening**~~ — ✅ done: unrolling via
   const-eval, transliteration, golden files (Phase 1 complete).
5. ~~**Phase 1.8 grammar engine**~~ — ✅ done: the second parser profile
   (`thamizh-order`) ships all five clause flips (clocked block,
   conditional, if-expression, match, test header), same productions, same
   AST. The parser was built one-token-lookahead to keep this cheap.
6. ~~**Phase 1.5 simulator**~~ — ✅ done: the event-driven engine, VCD/console
   trace, `mimz sim`/`mimz test`, and full parity (C1 combinational, C2
   instance flattening, C3 `repeat` unroll, C4 enum signals) all ship,
   validated by the Icarus differential. The first consumer of
   `TestDecl`/`TestStmt`.

Each of these has a full plan file in [`docs/plan/`](../plan/). The next
pipeline work is the Phase 2 IR (the named trigger that demotes the string
emitter to a debugging backend).
