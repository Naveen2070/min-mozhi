# 7 — The Verilog Emitter (5 Files)

This takes the checked AST and turns it into synthesizable Verilog-2005 text.

## `crates/mimz-core/src/emit_verilog/mod.rs` — The Big Picture

**`Project` struct** — project-wide tables of modules and enums.

**`Emitter` struct** — holds the output buffer, the project table, the constant environment, and any errors encountered during emission. The emitter might encounter problems (like non-const widths) and needs to report them rather than silently producing wrong Verilog.

**`emit(project, files)`** — the top-level entry. For each file, fold its constants; for each module, emit the Verilog.

**`REPEAT_BUDGET = 4096`** — maximum unroll iterations. Prevents a malicious file from producing gigabytes of Verilog.

## `crates/mimz-core/src/emit_verilog/module.rs` — Module Shell

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

**`loop`/`suzhal`** (`SeqStmt::Loop` in an `on` block, `FnStmt::Loop` in a
`fn` body) is unrolled by this module directly — no separate lowering pass,
the emitter walks the loop body `hi - lo` times itself (fn-body loops thread
each iteration's continuation so an inner `return` short-circuits correctly).

**`foreach`** never gets its own emission path: every site that can hold a
`ForEach` node (module item, `on`-block statement, `fn`-body statement)
calls `crate::ast::lower_foreach_item`/`lower_foreach_in_seq`/
`lower_foreach_fn` (see [`05-ast.md`](05-ast.md)) on the spot, then emits the
resulting `Repeat`/`Loop` exactly as above — `None` (an unresolvable
elements-form source) is unreachable here since the checker's `E0417` would
already have failed the build.

**`sync loop`** (`ModuleItem::SyncLoop`) is different: `crate::ast::lower_sync_loop`
rewrites it into real `Port`/`Reg`/`On`/`Drive` items (an index register plus
a `start`/`done` handshake FSM) BEFORE this module's normal item-emission
loop runs, so by the time `module.rs` sees it, it's indistinguishable from
hand-written primitives — there is no dedicated `SyncLoop`-shaped Verilog
output at all.

## `crates/mimz-core/src/emit_verilog/expr.rs` — Expression Rendering

Expressions are rendered to Verilog:

- `if` → ternary `? :` operator
- `match` → nested ternary chain
- `+` (lossless) → Verilog `+` with result growing
- `+%` (wrapping) → Verilog `+` with width context
- `fn` calls → **inlined**: the function body is substituted at the call site with arguments replaced
- Builtins → `$signed()`, `$unsigned()`, `~&`, etc.
- Enum variants → `localparam` constant names
- Tagged unions → **tag wire** + **payload extraction**: `{tag_bits, payload_bits}` width, variant tag as `localparam` values, payload extracted by field position in `assign` statements

## `crates/mimz-core/src/emit_verilog/translit.rs` — Tamil Names → ASCII

This pre-pass runs after the checker (which sees original names) and before emission (which needs ASCII). It converts Tamil-script identifiers to an ISO-15919-flavored romanization: `விளக்கு` → `villakku`.

If two different Tamil names romanize the same way, the second gets `_2`. ASCII names and Verilog reserved words are never touched.

## `crates/mimz-core/src/emit_verilog/testbench.rs` — Testbenches

Generates standalone Verilog testbench modules from inline `test` blocks. The testbench instantiates the DUT, drives inputs and clocks, and evaluates `expect` expressions using `$display("FAIL: ...")` and `$finish`.
