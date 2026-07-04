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
    /// `{N{a, b}}` — replication: the inner concatenation `{a, b}` repeated
    /// `count` times (Verilog `{N{...}}`). `count` is a compile-time constant.
    Replicate {
        count: Box<Expr>,
        parts: Vec<Expr>,
    },
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
    /// A builtin call — the ONLY callable things before user functions land.
    Call {
        func: Builtin,
        args: Vec<Expr>,
    },
    /// A call to a user-defined combinational function.
    ///
    /// `name` is the function name; `args` are the positional argument
    /// expressions. The parser produces this starting in Task 3; no existing
    /// checker/emitter/sim path handles it yet — those are added in later tasks.
    FnCall {
        /// The function being called.
        name: super::Ident,
        /// Positional arguments, one per [`FnParam`](super::FnParam).
        args: Vec<Expr>,
    },
    /// `{ field: expr, ... }` — a bundle literal.
    /// Disambiguated from `Concat` by the parser: if the first element after `{`
    /// is `IDENT ":"`, it is a bundle literal; otherwise it is a concat.
    BundleLit(Vec<FieldInit>),
    /// `[e1, e2, ..., eN]` — an array literal. All elements must share one
    /// element type and width (checker-enforced, E0414).
    ArrayLit(Vec<Expr>),
}

/// One field initializer in a bundle literal: `valid: expr`.
#[derive(Clone, Debug)]
pub struct FieldInit {
    /// Field name as written (checked against bundle declaration by the checker).
    pub name: super::Ident,
    /// The expression driving this field.
    pub value: Expr,
    pub span: Span,
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
    /// `0b1??` — a binary literal with don't-care bits. `mask` is 1 where the
    /// bit must equal `value` (don't-care bits are 0 in both); `width` is the
    /// digit count. Matches `s` iff `s & mask == value`.
    IntMask {
        value: u128,
        mask: u128,
        width: u32,
        raw: String,
    },
    Bool(bool),
    /// `Enum.Variant` or `Enum.Variant(b1, b2, ...)` — tag-only patterns
    /// have `bindings: vec![]`; tagged patterns list one binding per field.
    Variant {
        enum_name: Ident,
        variant: Ident,
        /// Positional binding names (empty for tag-only patterns).
        bindings: Vec<Ident>,
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
    /// `min(a, b)` — the smaller of two same-width values
    Min,
    /// `max(a, b)` — the larger of two same-width values
    Max,
    /// `abs(x)` — absolute value of a `signed[N]` (result `signed[N+1]`)
    Abs,
    /// `nand(x)` — negated and-reduction (`~&x` in Verilog)
    Nand,
    /// `nor(x)` — negated or-reduction (`~|x`)
    Nor,
    /// `xnor(x)` — negated xor-reduction (`~^x`)
    Xnor,
    /// `clog2(n)` — the one COMPILE-TIME builtin: folds to the bits needed to
    /// address `n` items. Valid only where a constant is (widths, consts,
    /// parameter defaults); the checker rejects it in a runtime value position.
    Clog2,
}

impl Builtin {
    /// Map a spelling to its variant and arity; `None` if not a builtin.
    /// This is the single source of truth for all builtin names — the parser
    /// and checker both call this instead of maintaining separate lists.
    pub fn from_name(name: &str) -> Option<(Builtin, usize)> {
        match name {
            "extend" => Some((Builtin::Extend, 2)),
            "trunc" => Some((Builtin::Trunc, 2)),
            "signed" => Some((Builtin::SignedCast, 1)),
            "unsigned" => Some((Builtin::UnsignedCast, 1)),
            "min" => Some((Builtin::Min, 2)),
            "max" => Some((Builtin::Max, 2)),
            "abs" => Some((Builtin::Abs, 1)),
            "nand" => Some((Builtin::Nand, 1)),
            "nor" => Some((Builtin::Nor, 1)),
            "xnor" => Some((Builtin::Xnor, 1)),
            "clog2" => Some((Builtin::Clog2, 1)),
            _ => None,
        }
    }
}
