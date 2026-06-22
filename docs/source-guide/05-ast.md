# 5 — The AST: What the Parser Produces (2 Files)

The AST is the **single intermediate representation** that everything downstream uses — the checker, the Verilog emitter, the simulator, and the pretty-printer. It's deliberately flavor-blind and word-order-blind: all the surface variation (English vs Tamil keywords, SVO vs SOV clause order) is absorbed by the lexer and parser and never reaches the tree.

---

## `ast/mod.rs` — The Core Types

**`File`** — one parsed source: `imports` followed by `items` (modules, enums, constants, tests).

**`Import`** — `import lib.adder` with path segments and a span.

**`TopItem`** — anything at file level: a constant, module, enum, or test.

**`Ident`** — a name plus its source location. Used everywhere so errors can always point at the right place.

**`Module`** — the hardware module: name, compile-time parameters, body items, span.

**`Param`** — one parameter: name, type (`int` or `bool`), optional default.

**`ConstDecl`** — `const NAME: int = expr`. Compile-time constant.

**`EnumDecl`** — `enum Name { Var1, Var2, ... }`. Variants encode to the smallest binary width (`ceil(log2(count))` bits).

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

**`Repeat`** — `repeat var: lo..hi { body }`. Compile-time, not runtime. Bounds must be constant.

**`Inst`** — `let name = Module(params) { connections }`. Child outputs are read as `name.port`.

**`OnBlock`** — `on rise(clk) { body }` / `on fall(clk) { body }`.

**`SeqStmt`** — inside an `on` block: `lhs <- rhs` (register update) or `if cond { } [else { }]`.

**`LValue`** — where a value is written: `signal`, `signal[i]`, or `signal[hi:lo]`.

**`Type`** — `Bit` (single wire), `Bits(N)` (unsigned N-bit vector), `Signed(N)` (signed N-bit vector), or `Named(ident)` (enum type).

**`TestDecl`** — `test "name" for Module(args) { body }`.

**`TestStmt`** — inside a test: `Tick`, `Expect`, `Drive`, or `If`.

---

## `ast/expr.rs` — Expressions and Patterns

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
- **Call** — builtin function call

**`Pattern`** — what a match arm matches against:

- `Int` — exact integer
- `IntMask` — `0b1??` (binary don't-care)
- `Bool` — `true`/`false`
- `Variant` — `State.Red`
- `Wildcard` — `_` (catch-all)

**`BinOp`** has specific width rules:

- `Add`/`Sub`/`Mul` — **lossless**: result grows (N+1, N+1, N+M bits)
- `AddWrap`/`SubWrap`/`MulWrap` (`+%`/`-%`/`*%`) — **wrapping**: keeps operand width (like real hardware registers)
- No `/` or `%` — division doesn't exist (it synthesizes to big, slow hardware)

**`Builtin`** — the complete list of built-in functions: `Extend`, `Trunc`, `SignedCast`, `UnsignedCast`, `Min`, `Max`, `Abs`, `Nand`, `Nor`, `Xnor`. Users can't define their own — modules are the unit of reuse.
