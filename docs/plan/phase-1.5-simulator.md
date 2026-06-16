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

#### Simulator-on-par-with-compiler — full-parity follow-on (workflow: `full-parity-simulator-workflow.md`)

To make the simulator cover every example the emitter compiles. Tracked as
increments C1–C4.

**C1 — combinational simulation + signed-aware differential — ✅ DONE (2026-06-16).**

_C1 was PLANNED to:_ add a clockless `mimz sim` path (`comb_run`, `--in`/`--sweep`);
make the Icarus differential signedness-agnostic; and broaden the bit-for-bit
differential to **all single-module examples** — every combinational + signed +
remaining clocked design.

_What landed (done):_

- [x] `comb_run` (`src/sim/run.rs`) — `mimz sim` runs **combinational** modules:
      `--in` settles one frame, `--sweep a=0|1|2` one frame per combination; same
      VCD/trace path. (+5 lib unit, +3 sim integration, −1 obsolete reject test.)
- [x] **Signed-aware differential via Verilog `%b`** (binary) — replaced `%0d`; the
      Layer-3 differential auto-routes clocked-vs-combinational, with per-example
      param overrides. Now covers **12 ASCII-named english examples** incl. SIGNED
      (`bitops`, `signed_math`).
- [x] **Bug found + fixed by the new differential:** the shared evaluator's lossless
      signed `+`/`*` (`src/sim/value.rs`) added raw bits without sign-extending a
      negative operand → wrong result (also affected `mimz eval`). Fixed to use
      `as_i128` (matches Verilog). Regression `signed_lossless_add_sign_extends`.

_What was PLANNED for C1 but NOT done (carried forward):_

- [ ] **Romanized tamil-pure / `vilakku` examples are NOT in the bit-for-bit
      differential** — the C1 plan said "all single-module examples", but these were
      scoped out: their emitted Verilog identifiers (module + ports) are romanized,
      so they differ from the source names our kernel uses; an auto-generated TB must
      apply the emitter's `romanize` to names on both sides. They keep Layer-1
      (elaborate) + Layer-2 (hand-written romanized TBs) for now. Small follow-up.

_Out of C1 scope by design (the rest of full parity — C2–C4):_

- [x] **C2 — instance / multi-module elaboration** (2026-06-16): `elaborate_project`
      in `src/sim/elaborate.rs` flattens `let` instances (incl. across `import`s) —
      each child inlined with signals prefixed `{inst}_{name}`, `inst.port` → wire
      `inst_port` (matches the emitter), so the flat `Design` is bit-for-bit
      equivalent. `mimz sim`/`mimz test` now `load_project`. `alu` (`Top`) and
      `chained` added to the Layer-3 differential (16 → 18 examples).
- [ ] **C3 — `repeat` unrolling** in `src/sim/elaborate.rs`. Unblocks `ripple_adder`.
- [ ] **C4 — enum-typed signals** (`value.rs::type_width` errors on `Type::Named`).
      Unblocks the `traffic_light` FSM.
- [ ] Optional: per-design golden VCD beyond the counter (byte-lock is counter-only).

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

_Module instances are flattened (C2); `repeat` and enum signals are still rejected
by the simulator's elaborator (C3/C4 — logged, additive follow-ups); the emitter
already lowers all of them._

## Risks / notes

- Two-phase commit is the correctness heart — write kernel unit tests before
  wiring it to the frontend.
- Don't build a full 4-state (X/Z) simulator in v1; Min-Mozhi semantics are
  2-state by design (resets are mandatory). Log this as a Decision if revisited.
- The triage-sourced work items above are stretch goals: kernel correctness
  and the Icarus differential suite always outrank them.
