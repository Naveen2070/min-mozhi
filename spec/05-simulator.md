# Min-Mozhi — Simulator

> **Spec v0.2 — Phase 1.5 complete (2026-06-16).**
> (v0.1, 2026-06-16: initial skeleton — recorded the execution semantics decided
> at the Phase 1.5 kickoff so the spec led the build. v0.2, 2026-06-16: Phase 1.5
> landed in full — B1–B8 plus the full-parity follow-on C1–C4 (combinational sim,
> instance flattening, `repeat` unrolling, enum signals); the differential now
> covers the entire single-file corpus (21 examples) bit-for-bit vs Icarus. The
> Tamil subtitle is deferred to native review, per R9.)
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

- A **clocked** simulation drives clocks and resets and advances time in cycles.
  `tick(clk)` advances **one** rising edge of `clk`; `tick(clk, n)` advances `n`
  rising edges.
- Reset is applied per the design's declared reset (synchronous, active per the
  reg's reset value) — the same semantics the Verilog emitter generates.
- A **combinational** module (no clock, no registers) settles instead of ticking:
  `mimz sim` (C1) captures one frame per input vector — `--in a=3,b=5` is a single
  settled frame, `--sweep a=0|1|2` is one frame per combination — and emits the
  same VCD/console trace. (`mimz eval` remains the one-shot CLI for a single set of
  inputs.)

## 3. `test` blocks (`mimz test`)

`test` blocks are simulation-only and never emit hardware (spec/02 section 1.10). The
`tick`/`expect` form below is **implemented** (B6); the `await` form is reserved
pending its native-review spelling (see Deferred). Two equivalent forms are
specified:

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
  **halts that test** and reports a teaching-quality message (the expression's
  source, the cycle, and — for a comparison — each side's actual value). `mimz
test` exits non-zero if any test fails.
- **Implemented (B6, `src/sim/harness.rs`):** `mimz test <file>` runs every
  `test` block (drive / `tick` / `expect` / `if`), prints `ok` / `FAIL` per test
  with the teaching message, and sets the exit code. `--filter <substr>` selects
  tests by name; `--trace` / `--trace=changes` (with `--verbose` / `--signals`)
  add a per-cycle console trace, riding the same snapshot as `mimz sim`.

### Deferred (NOT in v0.1)

- **Method-await** (`let r = await uart.read_byte()`, idea 3.3) — needs
  callables/methods (currently E1110); the syntax stays reserved.
- The **`await` keyword** is reserved (English-only); its Tanglish/Tamil spellings
  come from native review before activation (R9/R11). **`async` is now also
  reserved** (spec/03 v0.2.7) so the open sub-decision — whether the `await`
  test-timing form needs an `async` test-block marker — can be settled later
  without a freeze-breaking keyword addition (see the 2026-06-16 log).

## 4. Waveforms (`mimz sim`) — VCD output

- `mimz sim` produces a standard **IEEE-1364 2-state VCD**, viewable in GTKWave.
- Signal names are the design's identifiers (romanized for Tamil-script names, the
  same scheme the Verilog emitter uses); `$scope` nesting mirrors the module
  hierarchy.
- The VCD writer is hand-written (no dependency). Correctness is validated by the
  differential suite (same stimulus → compare against Icarus) and by GTKWave
  loading the file.

### Validation & performance (B8)

- **Differential vs Icarus (`tests/icarus.rs`, Layer 3):** the kernel runs a
  design in-process while the emitted Verilog runs the SAME stimulus under
  `iverilog`/`vvp`; the per-step output values must match **bit-for-bit**.

  - Covers 12 ASCII-named english examples (C1): clocked (counter, shift register,
    edge detector, blinker) and combinational over generated input vectors (adder,
    comparator, mux4, datapath, window, full_adder, and SIGNED `bitops` /
    `signed_math`). Compared via Verilog `%b` so signedness needs no
    special-casing.
  - This is independent of Layer 2 (Icarus vs hand-written asserts) — Layer 3 pits
    our simulator directly against Icarus.
  - Covers the **entire single-file corpus** — english + pure-Tamil, cross-file
    instances, `repeat`/instance arrays, and enum FSMs — every example the emitter
    compiles also simulates here, bit-for-bit.

- **Perf baseline:** the event-driven kernel sustains **≥ 1M cycle-events/sec**
  on the counter in release (`tests/sim.rs`), measured on the bare `tick` hot
  path.

## 5. Console trace (`--trace`)

Both `mimz sim` and `mimz test` accept an opt-in console trace, **off by default**
(normal output — the VCD + run status, or the test pass/fail + messages — is
unchanged). The tracer rides the **same per-cycle signal snapshot that feeds the
VCD**, so the console view always matches the waveform.

- `--trace` — an every-cycle table (one row per clock cycle, columns = signals).
- `--trace=changes` — print a line only when a watched signal changes
  (`$monitor`-style; compact on long or idle runs).
- **Scope:** default is interface + state (inputs, outputs, registers).
  `--verbose` widens to all signals (incl. internal wires); `--signals <a,b,…>`
  selects an explicit subset (unknown names are a clean error).

This is a CLI/observation feature only — no language surface, no synthesizable
output. (`sim::fatal` / `sim::warn`, deferred below, are a separate user-log
feature, not this uniform engine-driven trace.)

## 5.5. Hardware emulation (`sim` blocks, opt-in)

A `sim { speed ... bind ... }` block inside a `test` block binds ports to
virtual peripherals (`led`, `uart_tx`, `uart_rx`) and throttles execution
to a declared real-world clock rate, opt-in via `mimz test --emulate`
(default off; auto-degrades to a no-op with a logged note when stdout
isn't a real terminal). `uart_tx`/`uart_rx` decode/encode 8-N-1 serial at
an independent `baud` rate (derived against the sim block's `speed`) to a
dashboard log and/or a local TCP socket. Simulation-only — `mimz compile`
never sees any of it. Full design:
[`docs/superpowers/specs/2026-07-07-hw-emulation-led-design.local.md`](../docs/superpowers/specs/2026-07-07-hw-emulation-led-design.local.md)
(Spec 1, `led`) and
[`docs/superpowers/specs/2026-07-08-hw-emulation-uart-design.local.md`](../docs/superpowers/specs/2026-07-08-hw-emulation-uart-design.local.md)
(Spec 2, `uart_tx`/`uart_rx`) (both files are gitignored/local — if
they're not present, see `docs/Ideas/hardware_emulation.md` for the
original proposal).

## 6. Out of scope (v1)

- 4-state (X/Z) simulation; `real`/`time` value types.
- **Sub-cycle / clock-independent timing.** The kernel samples once per clock
  period, so an `async reset` is modeled as ≡ a sync reset at the sample points —
  it does not show a reset landing **between** edges.

  - The async behavior is realized faithfully in the emitted Verilog
    (`always @(… or posedge rst)`) and confirmed by the Icarus differential under
    clock-aligned stimulus; sub-cycle reset timing (recovery/removal,
    metastability) is a timing-closure concern, not RTL-functional.
  - This shares the Tier-2 "higher-fidelity engine" with X/Z above. The concrete
    path to higher fidelity is the **three-tier roadmap in
    [`docs/plan/phase-1.5-simulator.md`](../docs/plan/phase-1.5-simulator.md)**
    (current status: Tier 3 — delegate timing-faithful runs to the Verilog
    oracle).

- `sim::fatal` / `sim::warn` simulation-only assertions — deferred to a later
  increment (`expect` covers test pass/fail for now).
- Step-back ("time-travel") debugging — post-v1 stretch.

---

_Status: Phase 1.5 complete (2026-06-16) — the engine is fully built (B1–B8) and
at full parity with the emitter (C1–C4): elaboration
(`src/sim/elaborate.rs`), the event-driven two-phase kernel (`src/sim/kernel.rs`,
reusing the shared evaluator `src/sim/value.rs`), the default stimulus
(`src/sim/run.rs`), the hand-written VCD writer (`src/sim/vcd.rs`), the console
trace (`src/sim/trace.rs`), the `test`-block harness (`src/sim/harness.rs`), and
the `mimz sim` / `mimz test` commands — validated by the Icarus differential and
the ≥1M cycle-events/sec perf baseline (B8). The combinational evaluator
(`src/sim/comb.rs`) is reused. The C1–C4 follow-on brought full structural
parity: the elaborator flattens cross-file instances (C2), unrolls `repeat`
(C3), and encodes enum signals (C4), so the differential now covers the **entire
single-file corpus (21 examples)** bit-for-bit vs Icarus. Deferred within v1: the
`await clk.cycles(n)` test sugar (awaits its native-review spelling), `sim::fatal`
/ `sim::warn`, and 4-state simulation._
