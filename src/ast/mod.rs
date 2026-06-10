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
    /// `reset rst` — synchronous, active-high (v0.2).
    Reset(Ident),
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

/// `on rise(clk) { ... }` — everything inside updates registers with `<-`
/// on the rising edge of `clock`. Rising-edge only in v0.2 (`fall` is
/// reserved).
#[derive(Clone, Debug)]
pub struct OnBlock {
    pub clock: Ident,
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
}
