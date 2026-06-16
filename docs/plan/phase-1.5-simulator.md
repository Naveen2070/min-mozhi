# Phase 1.5 — Own Simulator

> **Your own behavioral engine — no external tools.**
> Window: months 10–12, **after Phase 1.8** (solo-dev order, decision D3) ·
> Target: 31 May 2027 · Status: 🟢 **feature-complete (2026-06-16, branch
> `phase-1.5-simulator`)** — all eight core work items land (B1–B8); see the
> 2026-06-16 dev log. Stabilizes when committed + the release step opens (D7).

## Goal

`mimz sim counter.mimz` runs a simulation and writes a VCD waveform, with no
Icarus or any external tool involved.

## Work items

- [x] **B1** Elaboration: AST → flat signal/process graph, params/widths/reset folded (`src/sim/elaborate.rs`). Single-module for now; instances/`repeat` rejected with a clear message (a later increment — see notes).
- [x] **B2** Event-driven simulation kernel: two-phase update (compute `<-` values, then commit) so register semantics are exact (`src/sim/kernel.rs`; shared evaluator `src/sim/value.rs`).
- [x] **B3** Combinational propagation in topological order — the kernel's memoized resolver settles the DAG on demand and reports comb cycles.
- [x] **B4** Clock/reset stimulus generation (`src/sim/run.rs` → `Timeline`).
- [x] **B5** VCD writer (viewable in GTKWave) + console trace (`--trace`/`--trace=changes`) + the `mimz sim` command (`src/sim/{vcd,trace}.rs`, `src/commands/sim.rs`).
- [x] **B6** `test` blocks from `spec/02` section 1.10: input drives, `tick(clk[,n])`, `expect`, `if`/`else`, run via `mimz test` with teaching-quality failure messages + exit codes (`src/sim/harness.rs`, `src/commands/test.rs`). (The `await clk.cycles(n)` form is decided but parked on its native-review spelling — see the test-syntax stretch item.)
- [x] **B7** Test-header thamizh-order flip (`M(args) kaaga "…" sodhanai { }`) — the 5th word-order flip; execution is the oracle.
- [x] **B8** Differential testing: same example, same stimulus → compare against Icarus **bit-for-bit**, three ways (our kernel == our VCD waveform == Icarus), on counter / shift register / edge detector (`tests/icarus.rs`, Layer 3).
- [x] **B8** Performance baseline: ≥1M cycle-events/sec on the counter — measured ~2.3M in release (best of 5, to reject load-induced dips) (`tests/sim.rs`).

### From the ideas triage (`docs/Ideas/language_plan.md` section 7, Tier 2)

- [ ] `sim::` namespace: simulation-only asserts (`sim::fatal`, `sim::warn`) —
      never synthesized, fenced like `test` blocks (idea 4.1); also carries the
      sim side of `system_fault` (translates to a fatal halt). **Deferred** (logged
      2026-06-16): `expect` covers test pass/fail for the first cut; this is an
      additive later increment, not a v1 blocker.
- [x] Test-syntax ruling: keep `tick(clk)`/`expect` or adopt
      `await clk.cycles(n)` style (idea 3.3). **Decided** (2026-06-16): support
      BOTH — `tick`/`expect` ships now (B6); the `await clk.cycles(n)` form is
      defined as exactly `tick(clk, n)` and stays parked until native review
      supplies the `await` Tamil/Tanglish spelling (R9/R11) and the `async`-marker
      sub-decision is settled (`async` reserved 2026-06-16, spec/03 v0.2.7).
- [ ] Step-back ("time-travel") debugging (idea 6.4): on `expect`/assert
      failure pause and allow cycle-by-cycle `step back` — feasible because this
      simulator records the full trace; designs are small, history is cheap.
      **Post-v1 stretch** (VCD + kernel came first, as planned).
- [x] Note: the combinational evaluator is what the Phase 4 hardware REPL (idea
      8.5) rides on — it stays callable on a single module/expression via
      `src/sim/comb.rs` + `mimz eval` (the down-payment shipped before B1).

### Still open after Phase 1.5 (additive — none block the release)

- The `await clk.cycles(n)` test form (needs the native-review `await` spelling).
- `sim::fatal` / `sim::warn` simulation-only asserts.
- Step-back debugging.

#### Differential-coverage follow-ups (B8 is real but narrow) — TODO

The B8 bit-for-bit differential (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`)
currently covers **3 clocked single-module designs** (counter / shift register /
edge detector). Layer 1 still elaborates all 72 examples under Icarus, but the
**value-level** "our kernel == our VCD == Icarus" check is narrow. To widen it:

- [ ] **Multi-module / instance elaboration in the simulator.** `src/sim/elaborate.rs`
      rejects sub-module instances and `repeat` today, so `mimz sim` / `mimz test`
      can't run those designs at all (e.g. `ripple_adder`). The Verilog emitter
      already lowers them — this is the sim-side follow-up. Unblocks adding
      instance/`repeat` designs to the differential.
- [ ] **Signed-aware differential comparison.** The differential reads Icarus
      values via Verilog `%0d`, which prints a _signed_ wire as a negative number
      while our compare uses unsigned magnitudes — so signed-output designs
      (`signed_math`, signed `alu` paths) are excluded to avoid a false mismatch.
      Mask/interpret each port by its declared signedness on both sides (~15 lines)
      to fold them in.
- [ ] **Broaden the clocked-design differential** once the two above land — aim to
      cover every clocked single-module example (combinational-only examples stay
      on `mimz eval`, not `mimz sim`).
- [ ] Optional: a per-design golden VCD beyond the counter (the byte-for-byte lock
      is currently counter-only).

## Milestone

`mimz sim` + `mimz test` run the examples; waveforms open in GTKWave; results
match Icarus bit-for-bit on the differential suite. ✅ Met: the differential
checks our kernel == our VCD waveform == Icarus per cycle (counter / shift
register / edge detector), and a golden VCD locks the writer's exact bytes.

## Exit criteria

1. ✅ No external tool needed for simulate/test workflows (`mimz sim` / `mimz test`
   run the in-house kernel; Icarus is only a test-time oracle).
2. ✅ `test` blocks pass/fail with teaching-quality messages (the failing
   expression's source, the cycle, each comparison side's value) + exit codes.
3. ✅ Differential suite green against Icarus — three-way (kernel / VCD / Icarus),
   plus the ≥1M cycle-events/sec perf baseline.

_Single-module only for now: instances/`repeat` are rejected by the simulator's
elaborator (a logged, additive follow-up); the emitter already lowers them._

## Risks / notes

- Two-phase commit is the correctness heart — write kernel unit tests before
  wiring it to the frontend.
- Don't build a full 4-state (X/Z) simulator in v1; Min-Mozhi semantics are
  2-state by design (resets are mandatory). Log this as a Decision if revisited.
- The triage-sourced work items above are stretch goals: kernel correctness
  and the Icarus differential suite always outrank them.
