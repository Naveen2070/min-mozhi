# 05 — The Verilog Emitter (`src/emit_verilog/`)

ASTs + project symbol table → one Verilog-2005 source string.

## File layout

| File        | Owns                                                                                                        |
| ----------- | ----------------------------------------------------------------------------------------------------------- |
| `mod.rs`    | `Project` symbol table, `emit()` entry, `Emitter` state, helpers (`clog2`, `enum_const`, `verilog_literal`) |
| `module.rs` | Module shells, ports, declarations, instances, always-blocks                                                |
| `expr.rs`   | Expression rendering (incl. `match` → ternary chains)                                                       |

Same module-scoping pattern as the parser: state and shared helpers in
`mod.rs`, the other files are `impl Emitter` blocks entered via
`pub(super) fn module()` / `expr()` / `expr_subst()`.

## Architecture invariant #6: deliberately dumb and readable

The emitter is **string-based** — it formats text directly from the AST,
no IR in between. Key consequence: **widths are emitted symbolically**.
`bits[WIDTH]` becomes `[(WIDTH)-1:0]` and module parameters pass straight
through to Verilog parameters. No const evaluation happens here at all.

This is staged on purpose (see
[`07-decisions-and-evolution.md`](07-decisions-and-evolution.md)): a
string emitter was the fastest path to "a counter compiles", and the
Phase 2 IR will demote it to a debugging backend rather than grow it into
a compiler.

Corollary: **parenthesize everything**. Every compound expression renders
wrapped in `(...)`. Ugly, unambiguous, correct — prettiness is a future
emitter's job, correctness is this one's.

## How a module is emitted (`module.rs`)

Source order inside a `.mimz` module body is free; output is regrouped
into conventional Verilog order:

1. **Header**: `module Name #(parameter ...) ( ports );` — ports (incl.
   clock/reset) appear in the order they were declared in the source.
2. **Enum localparams**: each variant becomes
   `localparam [w-1:0] STATE_RED = 0;` with `w = clog2(variant count)`.
3. **Declarations**: `wire`/`reg` with their width strings.
4. **Instances** (see below).
5. **Combinational drives**: `assign` for every `wire ... = ...` and
   every `lhs = rhs` drive.
6. **Always-blocks**: one `always @(posedge clk)` per `on` block.

### Instances — the auto-wiring contract

`instance()` walks the **child module's interface** (not the connection
list), which is what makes the errors precise:

- Every child **input** must be connected explicitly → error if missing.
- **clock/reset** fall back to connecting a same-named signal in the
  parent when omitted (spec/02 section 1.5's implicit connection).
- Every child **output** gets an auto-declared wire named
  `{instance}_{port}` — and that is exactly what `inst.port` field
  accesses render to in `expr.rs`. The two files meet at this naming
  convention; change it in both places or not at all.
- Child port widths may mention child parameters (`bits[WIDTH]`); when
  declaring the auto-wires the emitter substitutes the instance's
  argument expressions for those parameter names (`width_subst` /
  `expr_subst`). That substitution map is the only reason `expr_subst`
  exists.
- Connections naming a port the child doesn't have are errors.

### Always-blocks — the generated reset

The writer never writes reset logic; the language guarantees it. For each
`on` block:

```text
always @(posedge clk) begin
    if (rst) begin
        <reg> <= <its declared reset value>;   // for every reg this block assigns
    end else begin
        <the translated on-block body>
    end
end
```

`collect_assigned` gathers every register the block writes (recursing
through both `if` branches); each gets its declared reset value in the
reset branch. Registers the block never writes are untouched — a module
with two `on` blocks resets each register in the block that owns it.
If the module declares no `reset`, the body is emitted without the
reset wrapper.

This works because the parser already guaranteed every `reg` has a reset
value — safety rules compose.

## Expressions (`expr.rs`)

Mostly 1:1 symbol mapping. The interesting cases:

- **Wrapping ops** `+%`/`-%`/`*%` emit plain `+`/`-`/`*`: same-width
  Verilog arithmetic already wraps. (Lossless `+`/`-`/`*` emit the same
  thing today — width-growth enforcement is the checker's job. Verilog
  semantics make the result correct when widths are right; the checker
  will make wrong widths impossible.)
- **`match` → nested ternaries**: each arm becomes
  `(scrutinee == pat) ? value : (...)`; multi-pattern arms OR their
  comparisons; the final (or wildcard) arm is the default. Exhaustiveness
  is not checked yet — checker work.
- **`Enum.Variant`** → the localparam name (`STATE_RED`);
  **`instance.port`** → the auto-wire (`add_sum`). Disambiguated by
  looking the base name up in `project.enums`.
- **`extend(x, N)`** emits just `(x)` — Verilog zero/sign-extends in
  assignment context automatically; the call exists for the checker to
  verify widths. **`trunc(x, N)`** emits `x[(N)-1:0]`.
- **Literals** preserve the writer's base via the token's `raw` spelling:
  `0xFF` → `'hFF`, `0b1010` → `'b1010`, decimal stays decimal.

## Known gaps (clean errors, not wrong output)

The emitter's rule for unimplemented features: **error, never guess.**

| Gap                           | Error points at    | Lands with                                     |
| ----------------------------- | ------------------ | ---------------------------------------------- |
| `repeat` unrolling            | the `repeat` block | checker's const-eval (work item 4)             |
| Non-ASCII identifiers         | the identifier     | a transliteration pass (Verilog is ASCII-only) |
| Field access on complex exprs | the expression     | checker/IR                                     |

## Testing

`tests/examples.rs` compiles every example and asserts on the output —
including `tanglish_counter_compiles_to_identical_verilog`, which proves
the trilingual thesis at the byte level. When you change emission, run a
generated `.v` through a real tool (Icarus Verilog planned for CI) — the
integration tests check our expectations, not Verilog's.
