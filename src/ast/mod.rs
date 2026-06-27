//! The ONE shared AST (architecture invariant #1): no keyword-flavor or
//! word-order information survives past the parser. Spans everywhere.
//!
//! Module layout:
//! - `mod.rs`  — files, modules, declarations, sequential/test statements
//! - `expr.rs` — expressions, patterns, operators (re-exported here)

// Several fields are populated by the parser but only consumed by passes
// that land later in Phase 1/1.5 (checker, simulator, translate). Keep the
// contract complete now; drop this allow once those passes exist.
#![allow(dead_code)]

mod expr;
pub use expr::*;

use crate::span::Span;

/// One parsed `.mimz` source file: imports first, then top-level items.
#[derive(Clone, Debug)]
pub struct File {
    pub imports: Vec<Import>,
    pub items: Vec<TopItem>,
}

/// `import lib.adder` — path segments; resolution to a real file is
/// `project::load_project`'s job, not the AST's.
#[derive(Clone, Debug)]
pub struct Import {
    pub path: Vec<Ident>,
    pub span: Span,
}

/// Anything that may appear at file level (spec/02 section 1).
#[derive(Clone, Debug)]
pub enum TopItem {
    Const(ConstDecl),
    Module(Module),
    Enum(EnumDecl),
    Test(TestDecl),
    /// A user-defined combinational function (`fn name(params) -> ret { ... }`).
    /// Functions are pure and combinational — no registers, no clocks. The parser
    /// produces this starting in Task 3; no existing checker/emitter/sim path
    /// generates it yet.
    Func(FuncDecl),
    /// A top-level item that failed to parse. Produced ONLY by
    /// `parser::parse_recover` (the LSP path); the strict `parser::parse`
    /// pipeline never yields one, so codegen never sees it. The span covers
    /// the skipped source so tooling can locate the hole.
    Error(Span),
}

/// A user-defined combinational function declaration.
///
/// ```text
/// fn add(a: bits[4], b: bits[4]) -> bits[5] {
///     let sum = a + b
///     sum
/// }
/// ```
///
/// - `params` — the function's input parameters (each a hardware `Type`).
/// - `ret`    — the return type (a hardware `Type`).
/// - `locals` — `let` bindings that may appear before the final body expression.
/// - `body`   — the expression whose value the function returns.
#[derive(Clone, Debug)]
pub struct FuncDecl {
    /// The function name as written in source.
    pub name: Ident,
    /// Input parameters in declaration order.
    pub params: Vec<FnParam>,
    /// Return type.
    pub ret: Type,
    /// Local `let` bindings (`let x = expr`) in the function body, before
    /// the final return expression.
    pub locals: Vec<LocalLet>,
    /// The return expression (the last — and only non-`let` — expression in
    /// the body).
    pub body: Expr,
    /// Source span covering the whole declaration.
    pub span: Span,
}

/// One input parameter of a user-defined function.
///
/// Distinct from [`Param`] (which is a compile-time `int`/`bool` module
/// parameter). `FnParam.ty` is a hardware [`Type`] (`bit`, `bits[N]`, …).
#[derive(Clone, Debug)]
pub struct FnParam {
    /// Parameter name as written in source.
    pub name: Ident,
    /// Hardware type of this parameter.
    pub ty: Type,
    /// Source span of the parameter declaration.
    pub span: Span,
}

/// A `let` binding inside a function body: `let name = value`.
///
/// `value` is an expression evaluated combinationally when the function is
/// called. The bound name is in scope for subsequent `locals` and the `body`.
#[derive(Clone, Debug)]
pub struct LocalLet {
    /// The bound name.
    pub name: Ident,
    /// The defining expression.
    pub value: Expr,
    /// Source span of the `let` statement.
    pub span: Span,
}

/// A name with its source location. Used everywhere a user-written name
/// appears so errors can always point at it.
#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// `module Name(P: int = 8) { ... }` — the unit of hardware design.
#[derive(Clone, Debug)]
pub struct Module {
    pub name: Ident,
    /// Compile-time parameters (`WIDTH: int = 8`).
    pub params: Vec<Param>,
    pub items: Vec<ModuleItem>,
    pub span: Span,
}

/// Type of a compile-time parameter or constant. Only `int` and `bool` —
/// hardware types (`bits[N]` etc.) are not compile-time values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamTy {
    Int,
    Bool,
}

/// One module parameter: `WIDTH: int = 8`. A `None` default means the
/// instantiation must provide a value.
#[derive(Clone, Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: ParamTy,
    pub default: Option<Expr>,
}

/// `const NAME: int = expr` — file-level or module-level compile-time value.
#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub name: Ident,
    pub ty: ParamTy,
    pub value: Expr,
}

/// `enum State { Red, Green }` — variants encode to the smallest binary
/// width (`clog2(variant count)`); the emitter renders them as localparams.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: Ident,
    pub variants: Vec<Ident>,
}

/// Port direction (`in` / `out`). No `inout` — it is a reserved word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    In,
    Out,
}

/// Anything that may appear in a module body. Declaration order is free —
/// the emitter regroups (ports, declarations, instances, assigns, always).
#[derive(Clone, Debug)]
pub enum ModuleItem {
    /// `in name: type` / `out name: type`.
    Port {
        dir: Dir,
        name: Ident,
        ty: Type,
    },
    /// `clock clk` — clocks are a dedicated type, never plain bits (safety
    /// rule: clock-domain typing).
    Clock(Ident),
    /// `reset rst` (synchronous, active-high) or `async reset rst`
    /// (asynchronous). `is_async` widens every always-block that uses this
    /// reset to `@(… or posedge rst)`; polarity stays active-high (v0.2).
    Reset {
        name: Ident,
        is_async: bool,
    },
    /// `wire name: type = expr` — declared and driven in one statement;
    /// an undriven wire cannot be written.
    Wire {
        name: Ident,
        ty: Type,
        init: Expr,
    },
    /// `reg name: type = reset_value` — the reset value is mandatory
    /// (safety rule: no uninitialized state).
    Reg {
        name: Ident,
        ty: Type,
        reset: Expr,
    },
    /// `mem name: element_type[DEPTH] = init` — an addressable memory of
    /// `DEPTH` elements. Read combinationally (`m[addr]`), written on a clock
    /// (`m[addr] <- v` inside `on`). The init value is mandatory and seeds
    /// every cell at power-on (safety rule: no uninitialized state); `depth`
    /// must const-evaluate to a positive width.
    Mem {
        name: Ident,
        ty: Type,
        depth: Expr,
        init: Expr,
    },
    Const(ConstDecl),
    Enum(EnumDecl),
    /// Child-module instantiation (`let u = Adder(...) { ... }`).
    Inst(Inst),
    /// Sequential block (`on rise(clk) { ... }`).
    On(OnBlock),
    /// Combinational drive of an output port or a slice: `count = value`.
    Drive {
        lhs: LValue,
        rhs: Expr,
    },
    /// Compile-time generation (`repeat i: 0..8 { ... }`).
    Repeat(Repeat),
    /// A module-body item that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

/// `repeat i: lo..hi { ... }` — compile-time unrolling, NOT a runtime loop.
/// Bounds must const-evaluate; unrolling happens in the checker pass
/// (Phase 1 work item 4), so the emitter currently rejects it cleanly.
#[derive(Clone, Debug)]
pub struct Repeat {
    pub var: Ident,
    /// Range is half-open: `lo..hi` runs `lo, lo+1, …, hi-1`.
    pub lo: Expr,
    pub hi: Expr,
    pub items: Vec<ModuleItem>,
    pub span: Span,
}

/// `let name = Module(param: value) { port: signal, ... }`.
/// Child outputs are read as `name.port`; the emitter auto-wires them.
#[derive(Clone, Debug)]
pub struct Inst {
    pub name: Ident,
    /// `let name[i] = ...` inside `repeat`.
    pub index: Option<Expr>,
    /// The module being instantiated (resolved by name, project-wide).
    pub module: Ident,
    /// Compile-time parameter overrides.
    pub args: Vec<NamedArg>,
    /// Input/clock/reset connections. Same-named clock/reset connect
    /// implicitly when omitted; inputs never do.
    pub conns: Vec<Conn>,
    pub span: Span,
}

/// `name: value` — one parameter binding in an instantiation or test header.
#[derive(Clone, Debug)]
pub struct NamedArg {
    pub name: Ident,
    pub value: Expr,
}

/// `port: signal` — one port connection in an instantiation.
#[derive(Clone, Debug)]
pub struct Conn {
    pub port: Ident,
    pub signal: Expr,
}

/// Which clock edge a sequential block (and its registers) triggers on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// `on rise(clk)` — Verilog `posedge`.
    Rise,
    /// `on fall(clk)` — Verilog `negedge`.
    Fall,
}

/// `on rise(clk) { ... }` / `on fall(clk) { ... }` — everything inside updates
/// registers with `<-` on the chosen `edge` of `clock`.
#[derive(Clone, Debug)]
pub struct OnBlock {
    pub clock: Ident,
    pub edge: Edge,
    pub body: Vec<SeqStmt>,
    pub span: Span,
}

/// A statement inside an `on` block. Registers may be left unassigned on
/// some paths (they hold their value) — unlike wires, no `else` is forced.
#[derive(Clone, Debug)]
pub enum SeqStmt {
    /// `lhs <- rhs` — the only register assignment.
    Assign { lhs: LValue, rhs: Expr },
    /// Statement-level `if` (distinct from the expression-level `if`,
    /// which lives in [`ExprKind::IfExpr`] and requires `else`).
    If {
        cond: Expr,
        then: Vec<SeqStmt>,
        els: Option<Vec<SeqStmt>>,
    },
    /// A sequential statement that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

/// Assignment target: a signal, one bit of it, or a slice.
#[derive(Clone, Debug)]
pub struct LValue {
    pub base: Ident,
    /// `[i]` or `[hi:lo]`.
    pub index: Option<(Expr, Option<Expr>)>,
    pub span: Span,
}

/// A hardware type. Widths are expressions (often parameter names like
/// `WIDTH`), not numbers — const evaluation is the checker's job.
#[derive(Clone, Debug)]
pub enum Type {
    /// `bit` — a single wire.
    Bit,
    /// `bits[N]` — unsigned N-bit vector.
    Bits(Box<Expr>),
    /// `signed[N]` — two's-complement N-bit vector; never mixes with
    /// `bits` without an explicit cast (spec/02 section 1.7).
    Signed(Box<Expr>),
    /// Enum type by name.
    Named(Ident),
}

/// `test "name" for Module(args) { ... }` — runs on the Phase 1.5
/// simulator; parsed and validated today so test files are not a dead end.
#[derive(Clone, Debug)]
pub struct TestDecl {
    /// The quoted human-readable test name.
    pub name: String,
    /// The module under test.
    pub module: Ident,
    /// Parameter values for this test run.
    pub args: Vec<NamedArg>,
    pub body: Vec<TestStmt>,
    pub span: Span,
}

/// A statement inside a `test` block (spec/02 section 1.10).
#[derive(Clone, Debug)]
pub enum TestStmt {
    /// `tick(clk)` / `tick(clk, n)` — advance n clock cycles (default 1).
    Tick { clock: Ident, count: Option<Expr> },
    /// `expect expr` — assert the expression is true now.
    Expect(Expr),
    /// `name = value` — drive an input of the module under test.
    Drive { name: Ident, value: Expr },
    If {
        cond: Expr,
        then: Vec<TestStmt>,
        els: Option<Vec<TestStmt>>,
    },
    /// A test statement that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    #[test]
    fn func_decl_node_constructs() {
        let sp = Span::new(0, 0);
        let _ = TopItem::Func(FuncDecl {
            name: Ident {
                name: "f".into(),
                span: sp,
            },
            params: vec![],
            ret: Type::Bit,
            locals: vec![],
            body: Expr {
                kind: ExprKind::Bool(true),
                span: sp,
            },
            span: sp,
        });
    }
}
