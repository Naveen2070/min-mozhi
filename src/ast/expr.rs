//! Expression nodes, patterns, and operators. Re-exported through
//! `ast::*` so consumers never see the split.

use super::Ident;
use crate::span::Span;

/// An expression: the kind plus where it was written.
#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Every expression form in the language (spec/02 sections 1.3, 3).
/// All expressions are combinational — there are no side effects.
#[derive(Clone, Debug)]
pub enum ExprKind {
    /// Integer literal. `raw` keeps the spelling (`0b1010`, `0xFF`, `42`)
    /// so the emitter can preserve the writer's chosen base.
    Int {
        value: u128,
        raw: String,
    },
    /// `true` / `false` (a 1-bit value).
    Bool(bool),
    /// A signal, parameter, or constant name.
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
    /// `if c { a } else { b }` — an expression, so `else` is mandatory
    /// (safety rule: no inferred latches).
    IfExpr {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
    },
    /// `match x { pat => val, ... }` — must be exhaustive (checker-enforced).
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
    /// `{a, b, c}` — bit concatenation, widest part first (Verilog order).
    Concat(Vec<Expr>),
    /// `base[i]` — single-bit select.
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// `base[hi:lo]` — bit slice, both bounds inclusive.
    Slice {
        base: Box<Expr>,
        hi: Box<Expr>,
        lo: Box<Expr>,
    },
    /// A builtin call — the ONLY callable things; there are no user
    /// functions, only modules.
    Call {
        func: Builtin,
        args: Vec<Expr>,
    },
}

/// One `match` arm: `pat1, pat2 => value` (multiple patterns OR together).
#[derive(Clone, Debug)]
pub struct Arm {
    pub patterns: Vec<Pattern>,
    pub value: Expr,
}

/// What a `match` arm can match on. No bindings, no ranges (deferred —
/// spec/02 section 7).
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
    /// `_` — catch-all; makes the match exhaustive.
    Wildcard,
}

/// Unary (prefix) operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    /// `-x` — signed only, lossless (spec/02 section 1.7)
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

/// Binary operators. Precedence lives in the parser (`parser::expr::bin_op`),
/// not here — the AST is already a tree.
///
/// Width semantics (spec/02 section 3): `Add`/`Sub`/`Mul` are LOSSLESS —
/// the result grows (`N+1`, `N+1`, `N+M` bits). The `*Wrap` family (`+%`
/// `-%` `*%`) keeps the operand width and wraps, like real registers do.
/// There is no `/` or `%` (no division hardware by surprise).
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
    /// `&&` / `and` — operands must be 1-bit (no C-style truthiness).
    LogicAnd,
    /// `||` / `or`
    LogicOr,
}

/// The built-in functions — the complete list; users cannot define more.
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
