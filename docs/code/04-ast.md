# 04 — The AST (`crates/mimz-core/src/ast/`)

The data structure every other stage agrees on.

## File layout

| File                 | Owns                                                                                                                                                                                                                      |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mod.rs`             | Files, modules, declarations, sequential/test statements                                                                                                                                                                  |
| `expr.rs`            | Expressions, patterns, operators — re-exported via `pub use`                                                                                                                                                              |
| `sync_loop_lower.rs` | Desugars `ModuleItem::SyncLoop` into `Port`/`Reg`/`On`/`Drive` primitives — the one shared function `emit_verilog` and `sim` both call                                                                                    |
| `foreach_lower.rs`   | Desugars `ForEach`/`SeqStmt::ForEach`/`FnStmt::ForEach` into `Repeat`/`SeqStmt::Loop`/`FnStmt::Loop` — checker validates the original `ForEach` node directly; emit/sim/pretty each lower on the spot                     |
| `builtin_bundles.rs` | Builds the two compiler-synthesized `__Valid`/`__ValidSigned` `BundleDecl`s backing `T?` sugar — never present in any `.mimz` source; shared by `checker::symbols::build_symbols` and `emit_verilog::Project::from_files` |

The split is purely for file size; `pub use expr::*` means consumers
write `ast::ExprKind` and never see it.

## Invariant #1: ONE shared AST

**No keyword-flavor or word-order information survives past the parser.**
A Tanglish counter and an English counter produce structurally identical
ASTs — that's why `tanglish_counter_compiles_to_identical_verilog`
(tests/examples.rs) can assert byte-identical Verilog output, and it's
what makes the Phase 1.8 grammar engine cheap: `thamizh-order` is a
parser profile, not a second AST. Every downstream pass (checker,
emitter, simulator) is automatically trilingual.

The only flavor trace anywhere is `Token::flavor` — and tokens stop at
the parser.

## Design rules

- **Spans everywhere.** Every node that an error could point at carries a
  `Span`. Adding a node type without a span is almost always a mistake —
  the checker will need to report on it eventually.
- **`Ident` = name + span**, used for every user-written name. Plain
  `String` appears only where there is genuinely no source location.
- **Literals keep their `raw` spelling** (`0b1010`, `0xFF`) alongside the
  parsed `value: u128`, so emitted Verilog preserves the writer's base
  and future error messages can quote the source exactly.
- **Widths are expressions, not numbers.** `bits[WIDTH]` stores the
  `WIDTH` expression as written. Const evaluation is a checker
  responsibility; the AST never pre-computes.
- **Structured, not stringly.** Builtins are an enum (`Builtin::Extend`…),
  operators are enums — there is no "look at the name again later".
  The single exception is `Type::Named(Ident)` for enum types, resolved
  against the symbol table at emit (later: check) time.

## Statement vs expression `if` — a deliberate split

| Form                           | Node               | `else`        | Why                                                           |
| ------------------------------ | ------------------ | ------------- | ------------------------------------------------------------- |
| `if` driving a value (wires)   | `ExprKind::IfExpr` | **mandatory** | a missing branch = an undriven wire in some cycles = a latch  |
| `if` inside `on` blocks (regs) | `SeqStmt::If`      | optional      | an unassigned register simply holds its value — no latch risk |

This distinction is load-bearing for the no-latches guarantee; keep it.

## `TopItem::Func` — combinational function declarations

`TopItem::Func(FuncDecl)` holds a file-level `fn` declaration: name, parameter list
(`Vec<FnParam>`), return type, a `stmts: Vec<FnStmt>` body, and a mandatory
`tail: Expr` — the fallthrough result if no statement returns. This replaced the
original `locals: Vec<LocalLet>` + `body: Expr` shape when `return` and
statement-level `if` were added to `fn` bodies (spec/02 section 1.13); every
pre-existing `fn` (locals + tail expr only) still parses to the same `stmts`/`tail`
shape unchanged. `FnParam` (name + type) is intentionally named differently from
the module-param `Param` (name + `ParamTy` + optional default) — they are
different constructs.

`FnStmt` mirrors `SeqStmt`'s shape (`on`-block statements) — same idea, a
different terminal node:

```rust
enum FnStmt {
    Let(LocalLet),                                    // immutable local binding
    If { cond: Expr, then: Vec<FnStmt>, els: Option<Vec<FnStmt>> }, // `else` OPTIONAL
    Return(Expr),                                      // yields `expr`, ends this path
    Error(Span),                                        // parse-recovery placeholder
}
```

`FnStmt::If`'s `els` is `Option`, unlike `ExprKind::IfExpr` (mandatory `else` —
see "Statement vs expression `if`" above): a statement-level branch that doesn't
return just falls through to the next statement, or ultimately `tail`, so there is
no latch risk to guard against.

`LocalLet` carries no `ty` field (the type is inferred). The emitter conservatively
declares locals as `integer` (32-bit) in the Verilog output; precise width inference
is a follow-up.

`ExprKind::FnCall { name: Ident, args: Vec<Expr> }` is the call site. It is
syntactically distinct from `ExprKind::Call { func: Builtin, … }` (built-ins):
the parser resolves the distinction by name at parse time, so downstream passes see
typed variants, never string names.

## `Type::Array` and `ExprKind::ArrayLit` — fixed-size `fn`-parameter arrays

```rust
Type::Array { elem: Box<Type>, len: Box<Expr> }   // `<elem>[N]`
ExprKind::ArrayLit(Vec<Expr>)                     // `[e1, ..., eN]`
```

`Type::Array` is produced by `ty()`'s trailing-`[N]`-suffix loop
(`03-parser.md`) wrapping any scalar type — `elem` is a bare `Box<Type>`
with no span of its own, so diagnostics anchor on `len`'s span instead
(the only span-bearing part of the node available). The parser is
deliberately lenient here: `bits[8][4][2]` parses cleanly to a nested
`Array { elem: Array { .. }, len: 2 }` even though nested arrays are not
supported — the checker narrows (`E0411`), matching this project's
existing "lenient parser, narrowing checker" pattern (see e.g. bundle
field validation).

`ExprKind::ArrayLit` is a primary expression, not a suffix — unlike `{`
(which disambiguates bundle-literal vs. concat/replicate by lookahead), a
`[` at the start of a primary is always an array literal; `arr[idx]`
indexing is handled separately by `postfix()`, which only recognizes `[`
**after** an existing base expression. Both nodes exist only where arrays
are supported today: `fn` parameters, `fn`-body `let` locals, and call
arguments — module-level ports/wires/registers reject `Type::Array`
outright (`E0416`), so the emitter never has to lower a real Verilog
array. See spec/02 section 1.14.

## Tagged-union enums — `EnumVariant` and `PayloadField`

`EnumDecl` (file-level `TopItem::Enum` or module-level `ModuleItem::Enum`) now
models tagged-union enums. Its structure:

```rust
EnumDecl {
    name: Ident,
    variants: Vec<EnumVariant>,
    span: Span,
    inferred_total_width: Cell<Option<u32>>,   // set by checker
}

EnumVariant {
    name: Ident,
    fields: Vec<PayloadField>,   // empty = tag-only variant
    span: Span,
}

PayloadField {
    name: Ident,   // documentation only — bindings in match are positional (D2)
    ty: Type,      // must be a concrete bit-vector (E0807 if not)
    span: Span,
}
```

A tag-only variant has `fields: vec![]`; a tagged variant lists one
`PayloadField` per declared field. The field `name` is documentation; match
arm bindings are positional (`Vec<Ident>` on `Pattern::Variant`).

`inferred_total_width` is set by the checker's width pass (like `LocalLet::inferred_width`).
It is `None` until the checker runs. Downstream passes (emitter, sim) use it to
compute tag bits (MSBs) and payload slices (LSBs). See spec/02 section 5a for
the full wire layout.

## Bundles — `BundleDecl`, `Type::Bundle`, `BundleLit`, `BundleDestructure`

`BundleDecl` (`TopItem::Bundle`) is a struct-like grouping of ports/signals:

```rust
BundleDecl {
    name: Ident,
    params: Vec<Param>,        // compile-time params, same grammar as module params
    fields: Vec<FieldDecl>,
    span: Span,
}

FieldDecl {
    name: Ident,
    ty: Type,   // must be a concrete bit-vector or enum (E0807/E0905)
    span: Span,
}
```

A bundle-typed value's _type_ is `Type::Bundle { name: QualIdent, args: Vec<NamedArg> }`
— `args` holds compile-time parameter overrides (empty for parameterless
bundles, e.g. plain `Handshake` vs. `MemBus(WIDTH: 32)`). Like
`Type::Named`, it's nominal-only: two bundles are the same type iff they
share a name, not a structurally-identical field list.

A bundle literal is `ExprKind::BundleLit(Vec<FieldInit>)` — `{ name: Ident,
value: Expr }` per field, order-independent (matched by name, not
position). `ModuleItem::BundleDestructure { bindings: Vec<Ident>, expr: Expr,
span }` is `let { f, g } = expr` — **partial** destructure is allowed
(`bindings` need not cover every field); a field-rename attempt (`let {
f: g } = expr`) is rejected at parse time (E0904, see spec/02 section 1.12).

The checker's own internal type representation for a resolved bundle value
(`Ty::Bundle` in `crates/mimz-core/src/checker/widths/mod.rs`) is a
**separate type**, not this `Type::Bundle` — see
[`11-checker.md`](11-checker.md) for how it resolves field types on demand
and backs bundle-typed `fn` argument/return shape-checking.

## `Error` placeholder nodes (parse recovery)

`TopItem`, `ModuleItem`, `SeqStmt`, `FnStmt`, and `TestStmt` each carry an
`Error(Span)` variant. It is a placeholder for a construct that failed to
parse, produced **only** by `parser::parse_recover` (the LSP path); the
strict `parser::parse` returns `Err` on the same input, so an `Error` node
**never reaches codegen**. The span covers the skipped source so tooling can
locate the hole. Every downstream `match` handles the variant — the checker
skips it (no cascade diagnostics); the emitter/simulator/pretty-printer treat
it as a documented unreachable no-op. See
[`03-parser.md`](03-parser.md#two-entry-points-parse-strict-vs-parse_recover-best-effort).
There is intentionally no `ExprKind::Error` yet (it would need an
unknown-width path through type inference).

## About `#![allow(dead_code)]`

The parser populates the **complete** language contract — including
fields nothing consumes yet (`TestDecl` bodies, `Repeat` items,
`Inst::index`, `Token::flavor`). The alternative — trimming the AST to
what the emitter uses and re-growing it later — would churn every parser
function twice. The allow is documented in `mod.rs` and **must be removed
once the checker and simulator consume these fields** (they will), so
real dead code can't hide behind it forever.

## Things the AST intentionally does NOT model

- Comments/whitespace (trivia). Today's pipeline drops them. The
  `mimz fmt`/`translate` tools need them, which is a logged evolution
  trigger (architecture section 5): trivia-preserving lexing mode, not an
  AST change.
- Resolved names, computed widths, type info. Those will live in
  checker-output side tables (or a typed wrapper), keeping the parse AST
  a faithful record of what was written.
