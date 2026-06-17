# Min-Mozhi: Simulator Ideas & Backlog

Forward-looking ideas for the Min-Mozhi simulator (Phase 1.5 and beyond). This is
a **backlog of ideas**, not committed scope. The committed Phase 1.5 plan lives in
[`../plan/phase-1.5-simulator.md`](../plan/phase-1.5-simulator.md); the execution
semantics being built are specified in [`../../spec/05-simulator.md`](../../spec/05-simulator.md).
Anything here graduates only via a logged Decision (and, if it touches the
language surface, a spec edit) — and breaking changes must respect the v0.1.0
freeze (R13).

---

## 1. Value model: 2-state core, with room to grow

### Why the core is 2-state (0/1) — decided

Min-Mozhi simulates **2-state** by design, not for lack of ambition:

- **Mandatory resets ⇒ no unknowns.** Every `reg` declares a reset value (a
  checker rule), so there is no uninitialized state for Verilog's `X` to model —
  the main reason `X` exists is removed by construction.
- **Honesty.** 2-state avoids the X-pessimism / X-optimism class of
  simulation-vs-synthesis mismatches.
- **`Z` not needed internally.** High-impedance is only for tristate, which
  Min-Mozhi confines to top-level pads (`inout`, reserved, Phase 2); internal
  tristate is not FPGA-synthesizable anyway.
- **Teachability.** A value is just bits.

This is recorded as a Decision in `docs/log/2026-06-16.md`.

### Idea: opt-in 4-state (X/Z) simulation/interop mode

- **Explanation:** keep the _language_ 2-state, but offer a 4-state **simulation
  mode** for debugging and Icarus interop — e.g. to mirror real X-propagation when
  a differential test disagrees, or to import/observe 4-state behavior at a
  boundary.
- **Why additive (not breaking):** it does not change what valid 2-state programs
  mean; it adds an observation/diagnostic mode. Making the _core_ language 4-state,
  by contrast, would be breaking (it changes results of existing programs) and —
  post-v0.1.0 — would need an Edition. So full 4-state in the core is treated as a
  non-goal we do not reserve for; a sim mode can land anytime.
- **Mechanism sketch:** a wider per-bit value (`0/1/X/Z`) inside the kernel, gated
  by a `mimz sim --4state` flag; the hand-written VCD writer extends to emit
  `x`/`z` (the VCD format already supports them, so the format is never the
  blocker); the synthesizable emitter is unaffected.

### Idea: `fixed[N, F]` and the "real" question

- **`real`/`time` are not core.** Floating point is not synthesizable; it belongs
  only to a future verification/sim layer, never to synthesizable RTL.
- The synthesizable answer to fractional math is **`fixed[N, F]`** (reserved,
  Phase 2 — see `language_plan.md`): integer arithmetic under the hood with a
  compiler-tracked radix. The simulator and VCD writer would render `fixed` values
  as their integer bit-vectors (optionally pretty-printed with the radix).

### VCD / emitter extensibility (for the record)

- The **VCD format** natively supports `x`/`z` scalars, `b01xz…` vectors, and
  `r<float>` reals — so growing the value model is never blocked by the format.
- Our **hand-written 2-state VCD writer** needs only a small extension to emit the
  extra value characters when a 4-state/real mode lands. Owning the writer (no
  external lib) makes that a localized change.
- The **Verilog emitter** could be extended too: 4-state would emit `x`/`z` (the
  smaller part; the larger part is type-system/checker work and dropping the
  no-unknowns guarantee); `real` would only ever target sim/testbench output.

---

## 2. Step-back ("time-travel") debugging

- **Explanation:** on an `expect`/assert failure, pause and let the user step the
  simulation **backward** cycle by cycle to see how the bad state arose.
- **Why feasible:** designs are small; recording the full per-cycle trace (or
  periodic snapshots + replay) is cheap. Rides the same kernel.
- **Status:** post-v1 stretch (the plan puts VCD + kernel first). No language
  surface — a tool feature.

## 3. Simulation-only assertions (`sim::fatal` / `sim::warn`)

- **Explanation:** user-invokable runtime diagnostics inside design/sim that render
  to Verilog `$fatal`/`$warning` for the differential, and print teaching messages
  under `mimz sim`/`mimz test`. Fenced like `test` blocks — never synthesized.
- **Status:** deferred from the first Phase 1.5 cut (`expect` covers test pass/fail
  for now). `sim` stays a namespace prefix, not a keyword.

## 4. Hardware REPL (`mimz repl`, idea 8.5)

- **Explanation:** an interactive loop to poke a combinational module's inputs and
  see outputs, growing into single-stepping sequential designs.
- **Why it rides this work:** the combinational evaluator (`src/sim/comb.rs`) is
  already callable on one module/expression; the Phase 1.5 kernel extends it to
  sequential. The WASM playground (Phase 4) reuses the same engine.
- **Status:** Phase 4; no new syntax.
- **Bigger sibling — `mimz tui` (idea 8.11):** a vim-like full-screen TUI
  workbench that, on start, asks the output mode (emit Verilog / run+log /
  waveform) and drives the WHOLE toolchain (clocked sim, VCD, `mimz test`, inline
  diagnostics) for real `.mimz` files with no IDE. 8.5's combinational evaluator
  is one engine it drives. Tool, not syntax; post-1.5. See `language_plan.md` 8.11.

## 5. Testbench ergonomics: the `await` evolution

- Phase 1.5 ships `await clk.cycles(n)` as a thin equivalent of `tick(clk, n)`
  (cycle-waiting only) alongside `tick`/`expect`.
- **Future idea (method-await, idea 3.3):** `let r = await uart.read_byte()` —
  suspend on a hardware response. Needs **callables/methods** (currently E1110), a
  large language feature; the syntax stays reserved until that lands. Open
  sub-decisions for even the cycle-waiting form (the `await` Tanglish/Tamil
  spelling; whether an `async` test marker is required) are tracked in the
  2026-06-16 log.

## 6. Performance & scale

- Baseline target: **≥1M cycle-events/sec** on the counter (phase plan). Ideas for
  later: event-queue batching, signal-change coalescing, and a compiled
  (closure-per-process) execution path if the AST-walking kernel becomes the
  bottleneck on larger designs.

---

_Cross-links: committed scope — [`../plan/phase-1.5-simulator.md`](../plan/phase-1.5-simulator.md);
semantics — [`../../spec/05-simulator.md`](../../spec/05-simulator.md);
decisions — `docs/log/2026-06-16.md`; related language ideas (fixed-point,
contracts, channels) — [`language_plan.md`](language_plan.md)._
