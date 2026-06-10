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

#[derive(Clone, Debug)]
pub struct File {
    pub imports: Vec<Import>,
    pub items: Vec<TopItem>,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub path: Vec<Ident>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TopItem {
    Const(ConstDecl),
    Module(Module),
    Enum(EnumDecl),
    Test(TestDecl),
}

#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub name: Ident,
    pub params: Vec<Param>,
    pub items: Vec<ModuleItem>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamTy {
    Int,
    Bool,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: ParamTy,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub name: Ident,
    pub ty: ParamTy,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: Ident,
    pub variants: Vec<Ident>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    In,
    Out,
}

#[derive(Clone, Debug)]
pub enum ModuleItem {
    Port { dir: Dir, name: Ident, ty: Type },
    Clock(Ident),
    Reset(Ident),
    Wire { name: Ident, ty: Type, init: Expr },
    Reg { name: Ident, ty: Type, reset: Expr },
    Const(ConstDecl),
    Enum(EnumDecl),
    Inst(Inst),
    On(OnBlock),
    Drive { lhs: LValue, rhs: Expr },
    Repeat(Repeat),
}

#[derive(Clone, Debug)]
pub struct Repeat {
    pub var: Ident,
    pub lo: Expr,
    pub hi: Expr,
    pub items: Vec<ModuleItem>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Inst {
    pub name: Ident,
    /// `let name[i] = ...` inside `repeat`.
    pub index: Option<Expr>,
    pub module: Ident,
    pub args: Vec<NamedArg>,
    pub conns: Vec<Conn>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct NamedArg {
    pub name: Ident,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct Conn {
    pub port: Ident,
    pub signal: Expr,
}

#[derive(Clone, Debug)]
pub struct OnBlock {
    pub clock: Ident,
    pub body: Vec<SeqStmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum SeqStmt {
    /// `lhs <- rhs` — the only register assignment.
    Assign { lhs: LValue, rhs: Expr },
    If {
        cond: Expr,
        then: Vec<SeqStmt>,
        els: Option<Vec<SeqStmt>>,
    },
}

#[derive(Clone, Debug)]
pub struct LValue {
    pub base: Ident,
    /// `[i]` or `[hi:lo]`.
    pub index: Option<(Expr, Option<Expr>)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Type {
    Bit,
    Bits(Box<Expr>),
    Signed(Box<Expr>),
    /// Enum type by name.
    Named(Ident),
}

#[derive(Clone, Debug)]
pub struct TestDecl {
    pub name: String,
    pub module: Ident,
    pub args: Vec<NamedArg>,
    pub body: Vec<TestStmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TestStmt {
    Tick {
        clock: Ident,
        count: Option<Expr>,
    },
    Expect(Expr),
    Drive {
        name: Ident,
        value: Expr,
    },
    If {
        cond: Expr,
        then: Vec<TestStmt>,
        els: Option<Vec<TestStmt>>,
    },
}
