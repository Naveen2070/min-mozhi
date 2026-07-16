# Phase 2 — IR + Synthesis

> **Own your middle layer.**
> Window: months 11–22 · Status: 🟡 in progress — language-features track
> underway (see checked items below); IR/synthesis track not started

## Goal

A Min-Mozhi intermediate representation (netlist-level) and a path to real
FPGA hardware via the open toolchain: `.mimz → IR → Yosys/nextpnr → bitstream`.
Verilog-2005 + Yosys/nextpnr is **this phase's standing backend strategy** —
the pragmatic path to real hardware now, since own logic synthesis is
research-grade (Yosys/nextpnr represent a decade-plus of community
engineering). This is a sequencing choice, not a verdict against ever
building a native backend: that ambition genuinely lives in Phase 3
(see [`ROADMAP.md`](../../ROADMAP.md)), pursued if/when the maintainer wants
to build it — see Risks below. Everything in this phase's IR/optimizer work exists to feed
the Yosys toolchain in the meantime.

## Work items

### Verilog FFI (high priority — sequence before further language-feature work)

The single highest-leverage gap for adoption beyond education: without a way
to instantiate existing Verilog/SystemVerilog IP (vendor primitives, AXI
interconnects, DDR controllers, existing SV modules) from Min-Mozhi, the
language hits a hard ceiling the moment a real design needs anything it
doesn't already have a construct for. Promoted out of the Synthesis-path
list below to its own item — this should land before further Tier-3
language-feature work, now that Enum Variant Construction has shipped
(below).

- [x] Design the **external Verilog wrapping** construct (Constitution: emit + wrap Verilog) — spec bump + Decision log required before any code,
      same gate as every other new construct in this plan —
      **✅ DONE 2026-07-15 (spec/02 §1.5c)** — `extern module Name(params) { doc: "...", ports }`,
      `ModuleTarget` shared real/extern resolution, `warn`/`strict` sim taint,
      `mimz.toml`/`--extern-src` file wiring — `tests/fixtures/extern/pll.mimz`
      (excluded from the `examples/` five-flavor sweep — needs a companion `.v` file)

### IR

- [ ] Design Min-Mozhi IR: typed netlist (cells, nets, widths, clock domains preserved)
- [ ] AST → IR lowering (enums encoded, match → mux trees, regs → FF cells)
- [ ] IR text format (dumpable, diffable, hand-writable for tests)
- [ ] IR validation pass (re-checks single-driver, widths — defense in depth)

### Optimizer (first passes)

- [ ] Constant folding / propagation
- [ ] Dead signal & dead cell elimination
- [ ] Mux-tree simplification

### Synthesis path (pragmatic first)

- [ ] IR → structural Verilog emitter (Yosys-friendly subset) **or** direct Yosys JSON netlist
- [ ] Yosys + nextpnr flow scripted: `mimz build blink.mimz --target ice40`
- [ ] Bitstream produced and verified **in CI/emulation** (no board owned yet — decision D8)
- [ ] Hello-hardware demo on a real iCE40 board (iCEBreaker) — **when a board is acquired**

### Study track (feeds Phase 3)

- [ ] Study Yosys internals: techmapping, ABC interaction
- [ ] Document findings in `docs/log/` as study notes

### Clock-domain crossing (deferred from spec v0.1)

- [ ] Design explicit CDC construct (`sync`) — spec update + Decision log entry
      (design inputs from `docs/Ideas/language_plan.md` section 1.2, triage
      2026-06-12: per-reg `@ clk` domain tags + a `sync.double_flop` stdlib
      synchronizer; cross-domain reads without one become a checker error)

### Language features (window: alongside IR work — from `docs/Ideas/language_plan.md` sections 7 (feasibility triage) and 10 (HDL parity gap analysis), Tier 3)

This is the single source of truth for the triaged feature backlog; the
phase-4 plan only points here. The ground rules:

- Every item needs a spec section + a Decision block BEFORE code.
- Every new keyword (`secret`, `declassify`, `default`, `pipeline`, `interface`,
  `chan`, `prove`, `fixed`, `requires`, `ensures`, …) needs Tanglish + Tamil
  spellings through lang/keywords.toml + native-speaker review (English-only while
  reserved, R11).
- The `..` spread operator is reserved at the lexer/grammar level (not the keyword
  table) when interfaces/bundles are specced.

Order below is the build order from the triage; items late in the list may slip
to the Phase 3 window.

RTL-parity pull-forward (added from the HDL gap analysis,
`docs/Ideas/language_plan.md` section 10, 2026-06-15) — synthesizable gaps vs
VHDL/Verilog/SV, ordered cheapest-first; these precede the original Tier-3 list:

- [x] **Replication `{N{x}}`** (gap section 10, add-now) — compile-time N; parser +
      checker width + emitter `{N{x}}`. Smallest single win, no new keyword.
      **✅ DONE 2026-06-17 (spec/02 v0.2.8)** — `examples/.../replicate.mimz`
- [x] **Don't-care `match` patterns** `0b1??` (gap section 10, add-now) — the
      casex/casez analogue; pattern parsing + exhaustiveness rule.
      **✅ DONE 2026-06-17 (spec/02 v0.2.9)** — `examples/.../priority.mimz`
- [x] **Falling-edge `on fall(clk)`** (gap section 10, add-now) — `fall` promoted
      from reserved to active; negedge sequential block (parser + emitter + checker).
      **✅ DONE 2026-06-17 (spec/02 v0.2.10)** — `examples/.../dual_edge.mimz`
- [x] **Memories / arrays / RAM (`mem`)** (gap section 10) — array type + indexed
      clocked read/write + emitter array; highest "every HDL has it" value.
      **✅ DONE 2026-06-17 (spec/02 v0.2.11, new section 1.11)** —
      `examples/.../regfile.mimz`
- [x] **Combinational `function`** (gap section 10 — NEW, not previously tracked) —
      pure/stateless user functions inlined at emit; unblocks pipe `|>` (8.6)
      **✅ DONE 2026-06-28 (spec/02 v0.2.14)** — `examples/.../fn_*.mimz`
- [x] **Async reset / reset polarity** (gap section 10) — small spec+emitter widening
      over today's sync active-high only. **✅ DONE 2026-06-17 (spec/02 v0.2.12,
      active-high `async reset`)** — `examples/.../async_reset.mimz`; active-low
      polarity still open
- [x] **Packages / namespacing** (gap section 10 — NEW) — modest module-namespacing
      step beyond bare `import`; consider
      **✅ DONE 2026-07-02 (spec/02 v0.2.19, §1.5b)** — per-file module/enum/bundle
      uniqueness, qualified `a.b.Name` disambiguation (E0110/E0111) —
      `tests/fixtures/packages/qual_top.mimz`
- [x] **Controlled loop `suzhal`/`சுழல்`** (gap section 10) — bounded/FSM-lowered
      iteration distinct from `repeat`; static/provable trip-count bound is the
      load-bearing rule. Both spellings already reserved — **✅ DONE 2026-07-05 (spec v0.2.22)**
- [x] **`foreach`** (gap section 10 — NEW) — sugar over `repeat`/`suzhal` once
      array/`mem` types exist
      **✅ DONE 2026-07-12** — `examples/*/foreach_sum.mimz`, `foreach_fill.mimz`
- [x] **Tagged unions with payloads** (2.7) — FIRST of the original Tier-3 line:
      enums + match exist; payload = tag bits + max-payload bits; gives `Result`
      (4.2) for free
      **✅ DONE 2026-06-28 (spec/02 v0.2.15)** — `examples/.../tagged_packet.mimz`
- [x] **Enum Variant Construction (Payload)** — follow-up to Tagged Unions (2.7):
      `Enum.Variant(expr1, expr2)` construction syntax, positional args, zero new
      error codes (E0806/E0401 reused) — **✅ DONE 2026-07-14 (spec/02 §5a)** —
      `examples/.../enum_construct.mimz`
- [x] **Interfaces/bundles + destructuring** (2.4) — flatten to nets in the
      emitter; unlocks the next three items — **✅ DONE 2026-07-01 (spec v0.2.18)**
- [x] **Structural interface matching (2.9)** — `A`/`B` bundles with
      identical field shapes now interchange regardless of declared name;
      subset-of-fields + exact-type-on-shared-fields, fully automatic —
      **✅ DONE 2026-07-16** (branch `phase-2-structural-interface-matching`,
      spec/02 §1.12, new E0910) — also fixed a pre-existing bug where
      bundle-typed ports broke Verilog emission at instantiation
- [ ] **Bundle-typed `fn` return via inlining** (new, discovered 2026-07-16
      alongside 2.9 as BUG-10 in `docs/audit/bugs.md`) — a Verilog
      `function` can only return one value, so a bundle-typed `fn` return
      can't flatten to per-field outputs the way ports/wires/params do
      (fixed 2026-07-16, same day). Needs a different codegen strategy:
      inline the function body at each call site (uniquify locals per call
      site) instead of emitting a real Verilog `function` call. The checker
      already accepts bundle-typed `fn` returns structurally (feature 2.9,
      `check_return_ty`) — this closes the checker/emitter gap that leaves.
      No examples/goldens depend on this today; not urgent, but a real,
      honest gap (spec/02 §1.12 documents it as open, not silently broken).
- [ ] `?` valid-bundle sugar (2.1 re-targeted): `bits[N]?` =
      `{valid, data}`, `??` = mux on valid — never tri-state
- [ ] **Channels tier (a)** (3.1) — Decoupled-style valid/ready/data bundles,
      explicit handshake + must-consume lint on unused channel reads
      (the honest salvage of affine tokens, 1.3)
- [ ] Wire type inference (2.3 other half) — `wire sum = a + b`; widths.rs
      already computes the type, only the parser requires the annotation
- [x] **`default` assignments** (salvaged from 3.2) — value unless assigned this cycle — **DONE 2026-06-30** (branch `phase-2-default-and-const-if`, spec v0.2.17)
- [x] **Item-level const-`if`** (salvaged from 2.6; section 9 confirms 8.4) —
      conditional elaboration as a **keyword**, not a `$` sigil; the general
      `$comptime` interpreter is rejected (`repeat` + const-`if` cover ~90%) — **DONE 2026-06-30** (branch `phase-2-default-and-const-if`, spec v0.2.17)
- [ ] `count_ones`-style builtins (cheap version of 2.2)
- [x] **`clog2` const-builtin** (noted in `phase-4-ecosystem.md` stdlib +
      `phase-1.5-simulator.md`) — **DONE 2026-06-27** (branch
      `feat/clog2-const-builtin`, spec/02 §1.8 v0.2.13): exposes the ceil-log2
      the compiler already computed for enum widths as a user-facing
      **compile-time** builtin (alongside `min`/`max`/`abs`), one shared
      `clog2_bits` so enum + `clog2` widths can't drift. Reused E0202/E0407 (no
      new codes). `clog2(literal/const)` folds; `clog2(PARAM)` in a **body**
      width lowers to an injected Verilog-2005 `clog2` constant function so the
      width tracks a parameter override (Icarus-validated: `iverilog -g2005` +
      `DEPTH=5` override sim). Only a `clog2(PARAM)` in a **port** width errors
      (the body function can't reach the header). Stdlib `Fifo` refactored
      `AW` → `DEPTH` (`bits[clog2(DEPTH)]` pointers) across all four flavors +
      the pure-Tamil twin, with a **generalized wrap** (`+% 1`, else `DEPTH-1`
      to 0) so any `DEPTH >= 1` works — not just powers of two; goldens
      regenerated, Icarus differential green, `DEPTH=5` sim verified
- [ ] `pipeline(stages = N)` (salvaged from 6.1) — inserts N register stages +
      vendor retiming attribute; never promises Fmax
- [ ] **`prove` blocks** (6.3) — emit SystemVerilog assertions + drive
      SymbiYosys (never in-house SMT; the Icarus lean-on-tools pattern)
- [ ] Prove-backed shared-resource access (1.1 via 6.3) — checker accepts a
      guarded double-drive once the user-stated exclusion property is proved

#### section 8 additive ideas (deep triage 2026-06-13, `language_plan.md` section 9)

All edition-safe (no freeze pressure); 8.9/8.10 already shipped in Phase 1, the
rest land here or in Phase 4. Reserve `fixed`/`requires`/`ensures` is done; spec
section + Decision block still required before code, same as above.

- [ ] **Elm-style didactic errors** (8.1, Tier 2 — **incremental/ongoing**, IS
      the G1 promise) — Phase 1 shipped the base (E-codes + `help:`); extend
      `Diag` toward full "teaching errors" plus a long-form `mimz explain` for
      each code. ASCII hardware diagrams must depict real hardware (honesty). No
      freeze pressure; improve as codes are added
- [ ] **Native fixed-point `fixed[N, F]`** (8.3) — highest standalone DSP/edu
      value; integer adders/multipliers under the hood, compiler aligns radix
      points. Needs float literals + a rounding/overflow spec section (the
      honest part). Keyword `fixed` reserved
- [ ] **Contracts `requires` / `ensures`** (8.2, after `prove`) — caller-side
      `requires` (e.g. compile-time div-by-zero) is the high-value half; rides
      the `prove`/SymbiYosys backend. Keywords `requires`/`ensures` reserved
- [ ] Struct/bundle update `State { active: 1, ..old }` (8.8, after bundles) —
      named base stays honest; low risk
- [ ] Spread `..bus` module wiring (8.7, after bundles) — allow only spreading a
      **declared interface type** so connectivity stays greppable (rank-1
      honesty tension otherwise)
- [ ] Pipe `|>` (8.6, **parked**) — blocked on callables (only builtins exist,
      E1110) and a 2nd way to write calls (G1 one-way); revisit once extension
      functions land
- [ ] **Port-declaration grouping** `in { a, b, c: bits[8] }` (external review
      2026-06-26, `language_plan.md` section 11 item 6) — **DECISION-PENDING, not
      committed.** Mechanically trivial parser sugar (desugars to N separate port
      decls; additive, edition-safe, zero freeze cost), but it is a second surface
      for a declaration that already has one → **needs a G1 ruling** (same basis
      ternary `?:` / range-slice were rejected) before any code (R13). Same gate
      applies to the expression-level `let … in …` sliver of review item 2

### G5 security features (constitution goals since spec/01 v0.3)

- [ ] **`secret`/`declassify` explicit-flow taint pass** — SecVerilog lattice
      model in a checker pass; error when secret reaches a public out or
      unlabelled storage; timing side channels explicitly out of scope
      (`docs/Ideas/language_plan.md` 6.2, spec/01 G5)
- [ ] **`system_fault` sticky-fault network v1** — sticky fault reg +
      `FAULT_OUT` pin + safe-state mux on declared outputs + lockout until
      cold reset; plain synthesizable logic, no clock gating (clock-stop
      stays parked). Sim side lands earlier in Phase 1.5 (4.3, spec/01 G5)

### Verification layer (deferred, revisitable — NOT a rejection)

From the HDL gap analysis (`docs/Ideas/language_plan.md` section 10, 2026-06-15).

SV-style DV — `class`/OOP, `rand`/constraints, functional coverage
(covergroup/coverpoint/cross), concurrent (SVA) assertions, `fork/join`,
dynamic/associative arrays, queues — is **not** in the synthesizable language but
is **kept open as a future co-goal** (user intent 2026-06-15: include verification
logic later if needed).

It belongs to a fenced **verification layer** that rides the **simulator track**
(Phase 1.5+) and the **`prove`** track, never synthesized — the same fence as
today's `test` blocks. Pursuing the heavy DV pieces is a deliberate **spec/01
co-goal amendment** once the simulator is mature.

Build the already-mapped substitutes first:

- `test`/`tick`/`expect` (have),
- `sim::*` asserts (Phase 1.5),
- `prove` → SymbiYosys (above),
- `requires`/`ensures` (above).

## Milestone

`mimz build blink.mimz --target ice40` produces a verified bitstream through
the open toolchain (LED demo on real hardware as soon as a board exists).

## Exit criteria

1. IR documented in `docs/architecture.md` + a spec addendum.
2. All examples lower to IR, pass IR validation, and survive optimizer passes
   with simulation-equivalent behavior (differential suite extended to IR level).
3. Real-hardware demo reproducible from README instructions.

## Risks / notes

- **Own logic synthesis is research-grade** — that is why this phase rides on
  Yosys for techmapping. Building our own mapper moves to Phase 3+ and only if
  still desired then.
- IR design decisions are the highest-leverage decisions of the project after
  the grammar — every one gets a Decision block in the log.
