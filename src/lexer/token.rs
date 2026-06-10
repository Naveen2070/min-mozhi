//! Token kinds. Keywords are one `Kw` regardless of which language flavor
//! spelled them — the flavor is recorded separately for diagnostics/fmt.

use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kw {
    Module,
    In,
    Out,
    Wire,
    Reg,
    Clock,
    Reset,
    On,
    Rise,
    If,
    Else,
    Match,
    Enum,
    Let,
    Const,
    Repeat,
    Import,
    True,
    False,
    Test,
    For,
    Tick,
    Expect,
    And,
    Or,
    Not,
}

/// Which keyword skin a spelling came from (spec/03 Layer 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    English,
    Tanglish,
    Tamil,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokKind {
    Ident(String),
    Int { value: u128, raw: String },
    Str(String),
    Kw(Kw),

    // arithmetic
    Plus,     // +
    Minus,    // -
    Star,     // *
    PlusPct,  // +%  wrapping
    MinusPct, // -%
    StarPct,  // *%
    // shifts
    Shl, // <<
    Shr, // >>
    // bitwise
    Amp,   // &
    Pipe,  // |
    Caret, // ^
    Tilde, // ~
    // logical symbols
    AmpAmp,   // &&
    PipePipe, // ||
    Bang,     // !
    // comparison
    EqEq, // ==
    Ne,   // !=
    Lt,   // <
    Le,   // <=
    Gt,   // >
    Ge,   // >=
    // structure
    Assign,   // =   (wires only)
    LArrow,   // <-  (regs only)
    FatArrow, // =>
    Colon,
    Comma,
    Dot,
    DotDot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,

    Newline,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokKind,
    pub span: Span,
    /// Set only for keyword tokens. Consumed by `mimz fmt`/`translate` and
    /// error-language detection (Phase 1.8) — recorded from day one so the
    /// token stream never needs to change shape.
    #[allow(dead_code)]
    pub flavor: Option<Flavor>,
}

impl Token {
    pub fn is_kw(&self, kw: Kw) -> bool {
        matches!(self.kind, TokKind::Kw(k) if k == kw)
    }
}

/// Human name for error messages.
pub fn kind_name(kind: &TokKind) -> String {
    match kind {
        TokKind::Ident(s) => format!("identifier `{s}`"),
        TokKind::Int { raw, .. } => format!("number `{raw}`"),
        TokKind::Str(_) => "string".into(),
        TokKind::Kw(k) => format!("keyword `{k:?}`").to_lowercase(),
        TokKind::Newline => "end of line".into(),
        TokKind::Eof => "end of file".into(),
        other => format!("`{}`", punct_text(other)),
    }
}

fn punct_text(kind: &TokKind) -> &'static str {
    use TokKind::*;
    match kind {
        Plus => "+",
        Minus => "-",
        Star => "*",
        PlusPct => "+%",
        MinusPct => "-%",
        StarPct => "*%",
        Shl => "<<",
        Shr => ">>",
        Amp => "&",
        Pipe => "|",
        Caret => "^",
        Tilde => "~",
        AmpAmp => "&&",
        PipePipe => "||",
        Bang => "!",
        EqEq => "==",
        Ne => "!=",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        Assign => "=",
        LArrow => "<-",
        FatArrow => "=>",
        Colon => ":",
        Comma => ",",
        Dot => ".",
        DotDot => "..",
        LParen => "(",
        RParen => ")",
        LBracket => "[",
        RBracket => "]",
        LBrace => "{",
        RBrace => "}",
        _ => "?",
    }
}
