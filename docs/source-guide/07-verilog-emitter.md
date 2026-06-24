# 7 — The Verilog Emitter (5 Files)

This takes the checked AST and turns it into synthesizable Verilog-2005 text.

## `emit_verilog/mod.rs` — The Big Picture

**`Project` struct** — project-wide tables of modules and enums.

**`Emitter` struct** — holds the output buffer, the project table, the constant environment, and any errors encountered during emission. The emitter might encounter problems (like non-const widths) and needs to report them rather than silently producing wrong Verilog.

**`emit(project, files)`** — the top-level entry. For each file, fold its constants; for each module, emit the Verilog.

**`REPEAT_BUDGET = 4096`** — maximum unroll iterations. Prevents a malicious file from producing gigabytes of Verilog.

## `emit_verilog/module.rs` — Module Shell

Generates:

```
module Name #(parameter W = 8) (input wire clk, output reg [W-1:0] y);
```

Plus:

- `localparam` for enum variants (`STATE_RED = 0, STATE_GREEN = 1`)
- Wire, reg, and memory declarations
- Memory power-on initialization: `initial for (i = 0; ...) mem[i] = init;`
- Instance auto-wiring: child outputs become `inst_port` wires
- Always-blocks with reset synthesis (`if (rst) ... else ...`)
- Combinational drive assignments

## `emit_verilog/expr.rs` — Expression Rendering

Expressions are rendered to Verilog:

- `if` → ternary `? :` operator
- `match` → nested ternary chain
- `+` (lossless) → Verilog `+` with result growing
- `+%` (wrapping) → Verilog `+` with width context
- Builtins → `$signed()`, `$unsigned()`, `~&`, etc.
- Enum variants → `localparam` constant names

## `emit_verilog/translit.rs` — Tamil Names → ASCII

This pre-pass runs after the checker (which sees original names) and before emission (which needs ASCII). It converts Tamil-script identifiers to an ISO-15919-flavored romanization: `விளக்கு` → `villakku`.

If two different Tamil names romanize the same way, the second gets `_2`. ASCII names and Verilog reserved words are never touched.

## `emit_verilog/testbench.rs` — Testbenches

Generates standalone Verilog testbench modules from inline `test` blocks. The testbench instantiates the DUT, drives inputs and clocks, and evaluates `expect` expressions using `$display("FAIL: ...")` and `$finish`.
