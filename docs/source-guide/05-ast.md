# 5 — The AST: What the Parser Produces (4 Files)

The AST is the **single intermediate representation** that everything downstream uses — the checker, the Verilog emitter, the simulator, and the pretty-printer. It's deliberately flavor-blind and word-order-blind: all the surface variation (English vs Tamil keywords, SVO vs SOV clause order) is absorbed by the lexer and parser and never reaches the tree.

---

## `crates/mimz-core/src/ast/mod.rs` — The Core Types

**`File`** — one parsed source: `imports` followed by `items` (modules, enums, constants, tests).

**`Import`** — `import lib.adder` with path segments and a span.

**`TopItem`** — anything at file level: a constant, module, enum, or test.

**`Ident`** — a name plus its source location. Used everywhere so errors can always point at the right place.

**`Module`** — the hardware module: name, compile-time parameters, body items, span.

**`Param`** — one parameter: name, type (`int` or `bool`), optional default.

**`ConstDecl`** — `const NAME: int = expr`. Compile-time constant.

**`EnumDecl`** — `enum Name { Var1, Var2, ... }`. Variants encode to the smallest binary width (`ceil(log2(count))` bits). Since v0.2.15, variants can carry **payload fields**: `Data(val: bits[8])` — stored as `EnumVariant` with a `Vec<PayloadField>`.

**`ModuleItem`** — everything that can go in a module body:

- **Port** — `in`/`out` with name and type
- **Clock** — just the clock name
- **Reset** — name plus whether it's asynchronous
- **Wire** — name, type, and mandatory drive expression
- **Reg** — name, type, and mandatory reset value
- **Mem** — name, element type, depth, and mandatory init value
- **Const** / **Enum** — inline declarations
- **Inst** — child module instantiation
- **On** — sequential clocked block
- **Drive** — combinational assignment (`lhs = rhs`)
- **Repeat** — compile-time unrolling
- **Error** — a placeholder for a construct that failed to parse (see below)

**`Repeat`** — `repeat var: lo..hi { body }`. Compile-time, not runtime. Bounds must be constant.

**`Inst`** — `let name = Module(params) { connections }`. Child outputs are read as `name.port`.

**`OnBlock`** — `on rise(clk) { body }` / `on fall(clk) { body }`.

**`SeqStmt`** — inside an `on` block: `lhs <- rhs` (register update) or `if cond { } [else { }]`.

**`LValue`** — where a value is written: `signal`, `signal[i]`, or `signal[hi:lo]`.

**`Type`** — `Bit` (single wire), `Bits(N)` (unsigned N-bit vector), `Signed(N)` (signed N-bit vector), `Named(ident)` (enum type), or `Bundle(name, args)` (a bundle type by name, e.g. `MemBus(WIDTH: 32)` — `args` is empty for parameterless bundles; nominal-only today, matched by name not structural field-list).

**`BundleDecl`** — `bundle Name(params) { fields }`: a struct-like grouping of ports/signals. `TopItem::Bundle` holds one. Bundle-typed values flatten to individual Verilog signals at emit time (name-mangled, e.g. `bus_in_valid`) — see [`05-emit-verilog.md`](../code/05-emit-verilog.md).

**`TestDecl`** — `test "name" for Module(args) { body }`.

**`TestStmt`** — inside a test: `Tick`, `Expect`, `Drive`, or `If`.

**Error placeholders** — `TopItem`, `ModuleItem`, `SeqStmt`, and `TestStmt` each also have an `Error(span)` variant. It marks a spot where parsing failed but recovery kept going. These only appear when the tree comes from `parse_recover` (the editor/LSP path); the normal compile path uses the strict `parse`, which refuses a broken tree, so the checker and emitter never have to deal with a real one. There's no `Error` for expressions yet.

---

## `crates/mimz-core/src/ast/expr.rs` — Expressions and Patterns

**`Expr`** — an expression: a `kind` (what it is) plus a `span` (where it was written).

**`ExprKind`** — every expression form:

- **Int** — literal with value and raw spelling
- **Bool** — `true` / `false`
- **Ident** — a signal, parameter, or constant name
- **Field** — `base.field` (enum variant or instance port)
- **Unary** / **Binary** — operator expressions
- **IfExpr** — expression `if` (ALWAYS has `else`, unlike statement if)
- **Match** — pattern match with scrutinee and arms
- **Concat** — `{a, b, c}` bit concatenation
- **Replicate** — `{N{...}}` repetition
- **Index** — `base[i]`
- **Slice** — `base[hi:lo]`
- **Call** — builtin function call (10 built-ins: `extend`, `trunc`, `min`, `max`, `abs`, `nand`, `nor`, `xnor`, `signed`, `unsigned`)
- **FnCall** — user-defined `fn` call (v0.2.14): inlined at emit time

**`Pattern`** — what a match arm matches against:

- `Int` — exact integer
- `IntMask` — `0b1??` (binary don't-care)
- `Bool` — `true`/`false`
- `Variant` — `State.Red` (with optional `bindings: Vec<Pattern>` for tagged-union payload extraction)
- `Variant.Multi` — multi-field variant match like `Packet.Ctrl(k, _)`
- `Wildcard` — `_` (catch-all)

**`BinOp`** has specific width rules:

- `Add`/`Sub`/`Mul` — **lossless**: result grows (N+1, N+1, N+M bits)
- `AddWrap`/`SubWrap`/`MulWrap` (`+%`/`-%`/`*%`) — **wrapping**: keeps operand width (like real hardware registers)
- No `/` or `%` — division doesn't exist (it synthesizes to big, slow hardware)

**`Builtin`** — the complete list of built-in functions: `Extend`, `Trunc`, `SignedCast`, `UnsignedCast`, `Min`, `Max`, `Abs`, `Nand`, `Nor`, `Xnor`. Since v0.2.14, users can also define their own combinational functions via `fn` (covered by `ExprKind::FnCall`).

---

## `crates/mimz-core/src/ast/sync_loop_lower.rs` — Lowering `sync loop`

`sync loop` is sugar over primitives the rest of the pipeline already
understands: this file rewrites a `ModuleItem::SyncLoop` into an equivalent
`Port`/`Reg`/`On`/`Drive` combination (a small FSM: an index register, a
`start`/`done` handshake, and the loop body's per-iteration logic driven off
that index) BEFORE the checker or emitter ever see a `SyncLoop` node. The
checker, emitter, pretty-printer, and simulator all consume the lowered
form — only the parser and pretty-printer round-trip the original syntax.

## `crates/mimz-core/src/ast/foreach_lower.rs` — Lowering `foreach`

Same idea, for `foreach`: rewrites both forms (`foreach i in lo..hi` and
`foreach v in <array-or-mem>`) into the equivalent `Repeat`/`Loop` node —
elements-form iteration becomes an index-bound `Repeat` that substitutes the
bound variable with `source[idx]` throughout the body. Exposes one lowering
function per syntax position, since a `foreach` can appear as a module item,
inside an `on` block, or inside a `fn` body, and each position's surrounding
node shape differs: `lower_foreach_item`, `lower_foreach_in_seq` (recurses
into nested `if`s too), and `lower_foreach_fn`. Like `sync loop`, everything
downstream of the parser sees only `Repeat`/`Loop` — never a raw `ForEach`
node — except the checker, which validates `ForEach` directly before
lowering (see [`06-checker.md`](06-checker.md)) so its errors (`E0417`) can
point at the original `foreach` syntax.
