# Phase 1.5 — Own Simulator

> **Your own behavioral engine — no external tools.**
> Window: months 10–12, **after Phase 1.8** (solo-dev order, decision D3) ·
> Target: 31 May 2027 · Status: ⚪ not started

## Goal

`mimz sim counter.mimz` runs a simulation and writes a VCD waveform, with no
Icarus or any external tool involved.

## Work items

- [ ] Elaboration: AST → flat signal/process graph (instances expanded, params folded)
- [ ] Event-driven simulation kernel: two-phase update (compute `<-` values, then commit) so register semantics are exact
- [ ] Combinational propagation in topological order (DAG already guaranteed by Phase 1 checks)
- [ ] Clock/reset stimulus generation
- [ ] VCD writer (viewable in GTKWave)
- [ ] `test` blocks from `spec/02` section 1.10: input drives, `tick(clk)`, `expect`, run via `mimz test`
- [ ] Differential testing: same example, same stimulus → compare against Icarus results
- [ ] Performance baseline: ≥1M cycle-events/sec on the counter (Rust pays off here)

### From the ideas triage (`docs/Ideas/language_plan.md` section 7, Tier 2)

- [ ] `sim::` namespace: simulation-only asserts (`sim::fatal`, `sim::warn`) —
      never synthesized, fenced like `test` blocks (idea 4.1); also carries the
      sim side of `system_fault` (translates to a fatal halt)
- [ ] Test-syntax ruling: keep `tick(clk)`/`expect` or adopt
      `await clk.cycles(n)` style (idea 3.3) — decide when `test` blocks land,
      one Decision block; whichever wins must stay teachable to the spec/01 persona
- [ ] Step-back ("time-travel") debugging (idea 6.4): on `expect`/assert
      failure pause and allow cycle-by-cycle `step back` — feasible because this
      simulator records the full trace; designs are small, history is cheap.
      Post-v1 stretch: VCD + kernel come first

## Milestone

`mimz sim` + `mimz test` run all examples; waveforms open in GTKWave;
results match Icarus bit-for-bit on the differential suite.

## Exit criteria

1. No external tool needed for simulate/test workflows.
2. `test` blocks pass/fail with teaching-quality messages.
3. Differential suite green against Icarus.

## Risks / notes

- Two-phase commit is the correctness heart — write kernel unit tests before
  wiring it to the frontend.
- Don't build a full 4-state (X/Z) simulator in v1; Min-Mozhi semantics are
  2-state by design (resets are mandatory). Log this as a Decision if revisited.
- The triage-sourced work items above are stretch goals: kernel correctness
  and the Icarus differential suite always outrank them.
