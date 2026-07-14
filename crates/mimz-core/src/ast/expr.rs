//! Expression nodes, patterns, and operators. Re-exported through
//! `ast::*` so consumers never see the split.

use super::Ident;
use crate::span::Span;

/// An expression: the kind plus where it was written.
#[derive(Clone, Debug)]
pub struct Expr {
    /// The expression's shape and operands.
    pub kind: ExprKind,
    /// Source span of the expression.
    pub span: Span,
}

/// Every expression form in the language (spec/02 sections 1.3, 3).
/// All expressions are combinational — there are no side effects.
#[derive(Clone, Debug)]
pub enum ExprKind {
    /// Integer literal. `raw` keeps the spelling (`0b1010`, `0xFF`, `42`)
    /// so the emitter can preserve the writer's chosen base.
    Int {
        /// The literal's numeric value.
        value: u128,
        /// The literal exactly as written (`0b1010`, `0xFF`, `42`, …).
        raw: String,
    },
    /// `true` / `false` (a 1-bit value).
    Bool(bool),
    /// A signal, parameter, or constant name.
    Ident(String),
    /// `base.field` — enum variant (`State.Red`) or instance port (`add.sum`).
    Field {
        /// The expression being projected from.
        base: Box<Expr>,
        /// The field, variant, or port name after the dot.
        field: Ident,
    },
    /// `-x`, `~x`, `!x`, or a reduction (`&x`/`|x`/`^x`) — see [`UnOp`].
    Unary {
        /// Which unary operator.
        op: UnOp,
        /// The operand.
        expr: Box<Expr>,
    },
    /// A two-operand arithmetic, comparison, bitwise, or logical expression
    /// — see [`BinOp`].
    Binary {
        /// Which binary operator.
        op: BinOp,
        /// Left operand.
        lhs: Box<Expr>,
        /// Right operand.
        rhs: Box<Expr>,
    },
    /// `if c { a } else { b }` — an expression, so `else` is mandatory
    /// (safety rule: no inferred latches).
    IfExpr {
        /// The condition; must be 1-bit.
        cond: Box<Expr>,
        /// Value when `cond` is true.
        then: Box<Expr>,
        /// Value when `cond` is false.
        els: Box<Expr>,
    },
    /// `match x { pat => val, ... }` — must be exhaustive (checker-enforced).
    Match {
        /// The value being matched.
        scrutinee: Box<Expr>,
        /// Arms tried in order; the first matching pattern wins.
        arms: Vec<Arm>,
    },
    /// `{a, b, c}` — bit concatenation, widest part first (Verilog order).
    Concat(Vec<Expr>),
    /// `{N{a, b}}` — replication: the inner concatenation `{a, b}` repeated
    /// `count` times (Verilog `{N{...}}`). `count` is a compile-time constant.
    Replicate {
        /// How many times to repeat `parts`; must const-evaluate.
        count: Box<Expr>,
        /// The concatenation being repeated.
        parts: Vec<Expr>,
    },
    /// `base[i]` — single-bit select.
    Index {
        /// The vector being indexed.
        base: Box<Expr>,
        /// The bit position.
        index: Box<Expr>,
    },
    /// `base[hi:lo]` — bit slice, both bounds inclusive.
    Slice {
        /// The vector being sliced.
        base: Box<Expr>,
        /// Upper bound (inclusive).
        hi: Box<Expr>,
        /// Lower bound (inclusive).
        lo: Box<Expr>,
    },
    /// A builtin call — the ONLY callable things before user functions land.
    Call {
        /// Which builtin.
        func: Builtin,
        /// Argument expressions, positional per [`Builtin::from_name`]'s arity.
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
    /// `Enum.Variant(arg1, arg2, ...)` — constructs a payload-carrying (or
    /// tag-only, with zero args) enum value. `args` are positional, in the
    /// variant's declared field order. Only fires when the base before `.`
    /// is a bare identifier (`postfix()` in the parser) — this is a
    /// dedicated node, not built by reusing `Field`, so `enum_name` here is
    /// never ambiguous with an instance/bundle field access.
    EnumConstruct {
        /// The enum type name.
        enum_name: super::Ident,
        /// The variant name.
        variant: super::Ident,
        /// Positional arguments, one per [`PayloadField`](super::PayloadField).
        args: Vec<Expr>,
    },
}

/// One field initializer in a bundle literal: `valid: expr`.
#[derive(Clone, Debug)]
pub struct FieldInit {
    /// Field name as written (checked against bundle declaration by the checker).
    pub name: super::Ident,
    /// The expression driving this field.
    pub value: Expr,
    /// Source span of the `name: expr` pair.
    pub span: Span,
}

/// One `match` arm: `pat1, pat2 => value` (multiple patterns OR together).
#[derive(Clone, Debug)]
pub struct Arm {
    /// Patterns for this arm, OR'd together.
    pub patterns: Vec<Pattern>,
    /// The value produced when any pattern matches.
    pub value: Expr,
}

/// What a `match` arm can match on. No bindings, no ranges (deferred —
/// spec/02 section 7).
#[derive(Clone, Debug)]
pub enum Pattern {
    /// An exact integer literal — matches `s` iff `s == value`.
    Int {
        /// The literal's numeric value.
        value: u128,
        /// The literal exactly as written.
        raw: String,
    },
    /// `0b1??` — a binary literal with don't-care bits. `mask` is 1 where the
    /// bit must equal `value` (don't-care bits are 0 in both); `width` is the
    /// digit count. Matches `s` iff `s & mask == value`.
    IntMask {
        /// The bits that must match (don't-care bits are 0 here).
        value: u128,
        /// 1 where the bit is checked, 0 where it's a don't-care (`?`).
        mask: u128,
        /// Digit count as written.
        width: u32,
        /// The literal exactly as written, including `?` don't-cares.
        raw: String,
    },
    /// `true` / `false` — matches a 1-bit value.
    Bool(bool),
    /// `Enum.Variant` or `Enum.Variant(b1, b2, ...)` — tag-only patterns
    /// have `bindings: vec![]`; tagged patterns list one binding per field.
    Variant {
        /// The enum type name.
        enum_name: Ident,
        /// The variant name.
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
    /// `|x` — OR-reduction to a single bit.
    RedOr,
    /// `^x` — XOR-reduction to a single bit (parity).
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
    /// `+` — lossless add; result is `N+1` bits wide.
    Add,
    /// `-` — lossless subtract; result is `N+1` bits wide.
    Sub,
    /// `*` — lossless multiply; result is `N+M` bits wide.
    Mul,
    /// `+%` — wrapping add; keeps the operand width.
    AddWrap,
    /// `-%` — wrapping subtract; keeps the operand width.
    SubWrap,
    /// `*%` — wrapping multiply; keeps the operand width.
    MulWrap,
    /// `<<` — logical left shift.
    Shl,
    /// `>>` — logical right shift.
    Shr,
    /// `&` — bitwise AND.
    BitAnd,
    /// `|` — bitwise OR.
    BitOr,
    /// `^` — bitwise XOR.
    BitXor,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
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
