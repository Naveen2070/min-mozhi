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

### Language features (window: alongside IR work — from `docs/Ideas/language_plan.md` section 7, Tier 3)

This is the single source of truth for the triaged feature backlog; the
phase-4 plan only points here. Every item needs a spec section + a Decision
block BEFORE code, and every new keyword (`secret`, `declassify`, `default`,
`pipeline`, `interface`, `chan`, `prove`, …) needs Tanglish + Tamil spellings
through keywords.toml + native-speaker review. Order below is the build order
from the triage; items late in the list may slip to the Phase 3 window.

- [ ] **Tagged unions with payloads** (2.7) — FIRST: enums + match exist;
      payload = tag bits + max-payload bits; gives `Result` (4.2) for free
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
- [ ] Item-level const-`if` (salvaged from 2.6) — conditional elaboration
- [ ] `count_ones`-style builtins (cheap version of 2.2)
- [ ] `pipeline(stages = N)` (salvaged from 6.1) — inserts N register stages +
      vendor retiming attribute; never promises Fmax
- [ ] **`prove` blocks** (6.3) — emit SystemVerilog assertions + drive
      SymbiYosys (never in-house SMT; the Icarus lean-on-tools pattern)
- [ ] Prove-backed shared-resource access (1.1 via 6.3) — checker accepts a
      guarded double-drive once the user-stated exclusion property is proved

### G5 security features (constitution goals since spec/01 v0.3)

- [ ] **`secret`/`declassify` explicit-flow taint pass** — SecVerilog lattice
      model in a checker pass; error when secret reaches a public out or
      unlabelled storage; timing side channels explicitly out of scope
      (`docs/Ideas/language_plan.md` 6.2, spec/01 G5)
- [ ] **`system_fault` sticky-fault network v1** — sticky fault reg +
      `FAULT_OUT` pin + safe-state mux on declared outputs + lockout until
      cold reset; plain synthesizable logic, no clock gating (clock-stop
      stays parked). Sim side lands earlier in Phase 1.5 (4.3, spec/01 G5)

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
