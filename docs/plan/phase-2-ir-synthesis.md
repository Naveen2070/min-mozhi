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
