# Phase 2 — IR + Synthesis

> **Own your middle layer.**
> Window: months 11–22 · Status: ⚪ not started

## Goal

A Min-Mozhi intermediate representation (netlist-level) and a path to real
FPGA hardware via the open toolchain: `.mimz → IR → Yosys/nextpnr → bitstream`.

## Work items

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
- [ ] Design the **external Verilog wrapping** construct (Constitution: emit + wrap Verilog) — spec bump + Decision log

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
- [ ] **Combinational `function`** (gap section 10 — NEW, not previously tracked) —
      pure/stateless user functions inlined at emit; unblocks pipe `|>` (8.6)
- [x] **Async reset / reset polarity** (gap section 10) — small spec+emitter widening
      over today's sync active-high only. **✅ DONE 2026-06-17 (spec/02 v0.2.12,
      active-high `async reset`)** — `examples/.../async_reset.mimz`; active-low
      polarity still open
- [ ] **Packages / namespacing** (gap section 10 — NEW) — modest module-namespacing
      step beyond bare `import`; consider
- [ ] **Controlled loop `suzhal`/`சுழல்`** (gap section 10) — bounded/FSM-lowered
      iteration distinct from `repeat`; static/provable trip-count bound is the
      load-bearing rule. Both spellings already reserved
- [ ] **`foreach`** (gap section 10 — NEW) — sugar over `repeat`/`suzhal` once
      array/`mem` types exist
- [ ] **Tagged unions with payloads** (2.7) — FIRST of the original Tier-3 line:
      enums + match exist; payload = tag bits + max-payload bits; gives `Result`
      (4.2) for free
- [ ] **Interfaces/bundles + destructuring** (2.4) — flatten to nets in the
      emitter; unlocks the next three items
- [ ] Structural interface matching (2.9) — small checker rule once bundles exist
- [ ] `?` valid-bundle sugar (2.1 re-targeted): `bits[N]?` =
      `{valid, data}`, `??` = mux on valid — never tri-state
- [ ] **Channels tier (a)** (3.1) — Decoupled-style valid/ready/data bundles,
      explicit handshake + must-consume lint on unused channel reads
      (the honest salvage of affine tokens, 1.3)
- [ ] Wire type inference (2.3 other half) — `wire sum = a + b`; widths.rs
      already computes the type, only the parser requires the annotation
- [ ] `default` assignments (salvaged from 3.2) — value unless assigned this cycle
- [ ] Item-level const-`if` (salvaged from 2.6; section 9 confirms 8.4) —
      conditional elaboration as a **keyword**, not a `$` sigil; the general
      `$comptime` interpreter is rejected (`repeat` + const-`if` cover ~90%)
- [ ] `count_ones`-style builtins (cheap version of 2.2)
- [ ] **`clog2` const-builtin** (noted in `phase-4-ecosystem.md` stdlib +
      `phase-1.5-simulator.md`, never tracked here) — expose the ceil-log2 the
      compiler already computes internally for enum widths as a user-facing
      const-builtin (alongside `min`/`max`/`abs`). Compile-time only; unblocks
      single-parameter parametric memories — `Fifo(WIDTH, DEPTH)` derives its own
      pointer width instead of passing a redundant `AW`. Additive, no new keyword
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
