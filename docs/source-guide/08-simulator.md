# 8 — The Simulator (9 Files)

This is a full event-driven simulator that runs Min-Mozhi designs **without** any Verilog tools. It produces VCD waveforms (viewable in GTKWave) and console traces.

## `crates/mimz-sim/src/sim/value.rs` — The Value Model

**`Val`** — a bit-vector value: `bits: u128`, `width: u32`, `signed: bool`. Two-state model only (no `X` or `Z` — Min-Mozhi doesn't have them in v0.1).

**`eval(expr, resolver, consts)`** — this is THE expression evaluator, used by both the comb evaluator and the event-driven kernel. It handles every operator, builtin, and expression form with correct width semantics:

- `+ - *` → lossless (grow width)
- `+% -% *%` → wrapping (keep width)
- Comparisons → 1-bit result
- `extend`/`trunc` → resize
- `match` with don't-care patterns

This file also interprets `fn` bodies directly (no pre-lowering pass exists
for them, unlike module items/`on`-blocks): a `FnStmt::Loop` unrolls
in-place with Rust's own early-return giving `return` its short-circuit
behavior for free, and `FnStmt::ForEach` lowers on the spot via
`ast::lower_foreach_fn` (see [`05-ast.md`](05-ast.md)) into the equivalent
`Loop` before interpreting it the same way.

## `crates/mimz-sim/src/sim/comb.rs` — Combinational Evaluation

**`eval_outputs(file, module, inputs, params)`** — evaluates a clockless module: set inputs, resolve combinational drivers in dependency order, return all output values.

Rejects designs with registers, `on` blocks, instances, or repeat (with clear messages).

## `crates/mimz-sim/src/sim/elaborate.rs` — Flattening the Design

**`elaborate_project(files, module, params)`** — turns an AST module with concrete parameter values into a flat `Design`:

- **Instance flattening**: child modules are inlined with name-prefixed signals: `inst.port` becomes wire `inst_port`
- **`repeat` unrolling**: loop variable is a compile-time constant
- **`sync loop`/`foreach` lowering**: every `ModuleItem::SyncLoop` and module-level `ModuleItem::ForEach` is lowered (to `Port`/`Reg`/`On`/`Drive` primitives, or to `Repeat`, respectively) BEFORE the elaboration worklist runs — by the time the worklist sees the item list, neither node shape exists anymore
- **Enum encoding**: variants mapped to `0, 1, 2...` with width `ceil(log2(n))`
- **Width folding**: `bits[W]` where `W=8` becomes concrete `width=8`

`MAX_INSTANCE_DEPTH = 16` prevents stack overflow from recursive instantiation.

**`Design` struct** — the flat representation: inputs, outputs, registers, wires, clocks, resets, combinational drivers (signal → expression), and clocked processes.

## `crates/mimz-sim/src/sim/kernel.rs` — The Simulation Engine

**`simulate(design, opts)`** — the event-driven, two-phase commit engine:

- **Phase 1 — Evaluate**: compute every register's NEXT value from current state and combinational logic
- **Phase 2 — Commit**: update all registers at once (non-blocking, matching real hardware)

**Reset**: when reset is high, every register takes its declared reset value instead of the computed next value.

**Edges**: handles `rise` and `fall` in a single period. A negedge register sees the NEW posedge values from the same cycle.

**Memory**: sparse cell storage (only written cells are tracked), init value for unwritten cells, reads during a cycle see the OLD value (non-blocking write behavior).

**`loop`/`suzhal`**: an on-block `SeqStmt::Loop` (what's left after `sync
loop`/`foreach` are pre-lowered in `elaborate.rs`) unrolls right here, at
process-execution time each cycle — not folded away during elaboration like
`repeat`.

## `crates/mimz-sim/src/sim/run.rs` — Default Stimulus

**`run(design, opts)`** — drives a clocked design: toggle the clock, assert reset for initial cycles, hold specified inputs.

**`comb_run(design, vectors)`** — one vector → one settled frame, for combinational designs.

**`MAX_SIM_CYCLES = 1_000_000`** — prevents OOM on adversarial input.

## `crates/mimz-sim/src/sim/harness.rs` — Test Block Runner

**`run_test(files, src, decl)`** — runs one `test "name" for M(args) { body }`:

1. Elaborates the module-under-test with the test's parameters
2. Interprets the body step by step:
   - `drive name = value` → set an input
   - `tick(clk, N)` → advance N cycles
   - `expect expr` → assert true; on failure, prints a teaching message showing the expression, cycle, and each operand's actual value
   - `if cond { }` → branch on state

## `crates/mimz-sim/src/sim/trace.rs` — Console Traces

**`render(timeline, style, scope)`** — two styles:

- `"table"` — every cycle, column-aligned
- `"changes"` — only when signals change (like `$monitor`)

## `crates/mimz-sim/src/sim/vcd.rs` — VCD Waveform Output

**`to_vcd(timeline)`** — hand-written 2-state VCD writer (no external crate). Assigns short ASCII codes to signals, dumps initial state, then outputs change-only updates at each timestamp. Opens in GTKWave.
