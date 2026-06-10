//! Expression nodes, patterns, and operators. Re-exported through
//! `ast::*` so consumers never see the split.

use super::Ident;
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Int {
        value: u128,
        raw: String,
    },
    Bool(bool),
    Ident(String),
    /// `base.field` — enum variant (`State.Red`) or instance port (`add.sum`).
    Field {
        base: Box<Expr>,
        field: Ident,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    IfExpr {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
    Concat(Vec<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        base: Box<Expr>,
        hi: Box<Expr>,
        lo: Box<Expr>,
    },
    Call {
        func: Builtin,
        args: Vec<Expr>,
    },
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub patterns: Vec<Pattern>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Int {
        value: u128,
        raw: String,
    },
    Bool(bool),
    /// `Enum.Variant`
    Variant {
        enum_name: Ident,
        variant: Ident,
    },
    Wildcard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    /// `-x` — signed only, lossless (spec/02 §1.7)
    Neg,
    /// `~x`
    BitNot,
    /// `!x` / `not x`
    LogicNot,
    /// prefix `&x` `|x` `^x` reductions
    RedAnd,
    RedOr,
    RedXor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    AddWrap,
    SubWrap,
    MulWrap,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    LogicAnd,
    LogicOr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    /// `extend(x, N)` — zero-extends bits, sign-extends signed
    Extend,
    /// `trunc(x, N)`
    Trunc,
    /// `signed(x)` — free reinterpret cast
    SignedCast,
    /// `unsigned(x)`
    UnsignedCast,
}
