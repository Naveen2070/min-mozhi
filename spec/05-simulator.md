# Min-Mozhi — Simulator

> **Spec v0.1 DRAFT — Phase 1.5 in progress.**
> (v0.1, 2026-06-16: initial skeleton — records the execution semantics decided at
> the Phase 1.5 kickoff so the spec leads the build. Stabilizes when Phase 1.5
> lands. The Tamil subtitle is deferred to native review, per R9.)
> Goal: run Min-Mozhi designs and their `test` blocks **with no external tool** —
> `mimz sim` produces waveforms, `mimz test` runs the test blocks and reports
> pass/fail with teaching-quality messages.

The simulator and its VCD writer are **built in-house** (no external simulation
or VCD library) — see the 2026-06-16 Decision in `docs/log/`.

---

## 1. Execution model

- **2-state** (`0`/`1`) only — no X/Z. Min-Mozhi's mandatory resets mean every
  register has a defined initial value, so there is no uninitialized state to
  model. (Revisiting this is a logged Decision.)
- **Event-driven, two-phase update.** Each clock edge: (1) compute every register's
  next value from current state (the `<-` right-hand sides) without committing;
  (2) commit all next values simultaneously. This makes non-blocking register
  semantics exact (no read-after-write ordering hazards).
- **Combinational settling** in topological order — the checker already guarantees
  the combinational graph is a DAG (no comb loops), so propagation terminates.
- Reuses `src/sim/comb.rs::eval_outputs` for combinational expression evaluation
  and `checker::consteval::eval` for constants/widths (single source of truth).

## 2. Stimulus & timing

- A simulation drives clocks and resets and advances time in cycles. `tick(clk)`
  advances **one** rising edge of `clk`; `tick(clk, n)` advances `n` rising edges.
- Reset is applied per the design's declared reset (synchronous, active per the
  reg's reset value) — the same semantics the Verilog emitter generates.

## 3. `test` blocks (`mimz test`)

`test` blocks are simulation-only and never emit hardware (spec/02 §1.10). Two
equivalent forms are supported:

```mimz
test "counter counts" for Counter(WIDTH: 4) {
  a = 3                 // drive an input
  tick(clk)             // advance one rising edge
  expect count == 1
  tick(clk, 3)          // advance 3 edges
  expect count == 4
}
```

```mimz
test "counter counts (await form)" for Counter(WIDTH: 4) {
  a = 3
  await clk.cycles(1)   // exactly equivalent to tick(clk, 1)
  expect count == 1
  await clk.cycles(3)   // == tick(clk, 3)
  expect count == 4
}
```

- `await <clock>.cycles(<expr>)` is **defined as** `tick(<clock>, <expr>)` — a
  readability alternative, not a new execution mechanism. It is a dedicated
  await-timing production, **not** general method-call syntax.
- `expect <bool-expr>` checks a condition at the current cycle. A failing `expect`
  **halts that test** and reports a teaching-quality message (the expression, the
  expected vs actual values, the cycle). `mimz test` exits non-zero if any test
  fails.

### Deferred (NOT in v0.1)

- **Method-await** (`let r = await uart.read_byte()`, idea 3.3) — needs
  callables/methods (currently E1110); the syntax stays reserved.
- The **`await` keyword** is reserved (English-only); its Tanglish/Tamil spellings
  come from native review before activation (R9/R11). Whether an `async` test-block
  marker is required is an open sub-decision (see the 2026-06-16 log).

## 4. Waveforms (`mimz sim`) — VCD output

- `mimz sim` produces a standard **IEEE-1364 2-state VCD**, viewable in GTKWave.
- Signal names are the design's identifiers (romanized for Tamil-script names, the
  same scheme the Verilog emitter uses); `$scope` nesting mirrors the module
  hierarchy.
- The VCD writer is hand-written (no dependency). Correctness is validated by the
  differential suite (same stimulus → compare against Icarus) and by GTKWave
  loading the file.

## 5. Out of scope (v1)

- 4-state (X/Z) simulation; `real`/`time` value types.
- `sim::fatal` / `sim::warn` simulation-only assertions — deferred to a later
  increment (`expect` covers test pass/fail for now).
- Step-back ("time-travel") debugging — post-v1 stretch.

---

_Status: DRAFT. Records the Phase 1.5 kickoff decisions (2026-06-16); fills in as
the simulator is built. The combinational evaluator (`src/sim/comb.rs`) already
exists and is reused; the event-driven kernel, stimulus, VCD writer, and test
runner are the Phase 1.5 deliverables._
