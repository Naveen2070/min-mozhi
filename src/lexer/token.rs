//! Token kinds. Keywords are one `Kw` regardless of which language flavor
//! spelled them — the flavor is recorded separately for diagnostics/fmt.

use crate::span::Span;

/// The keyword tokens — spelling-independent. `thoguthi` and `தொகுதி`
/// both lex to `Kw::Module`. The list mirrors `keywords.toml`; adding a
/// keyword means adding it in BOTH places (`keywords::kw_for_key` panics
/// at startup on a mismatch, so drift cannot ship).
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

/// Every kind of token the lexer can produce. Punctuation variants carry
/// no payload; their spelling lives in [`punct_text`].
#[derive(Clone, Debug, PartialEq)]
pub enum TokKind {
    Ident(String),
    /// Integer literal; `raw` preserves the written form (`0b1010`, `0xFF`).
    Int {
        value: u128,
        raw: String,
    },
    /// String literal — currently only used for test names.
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

    /// Statement terminator (Go-style; see `lexer::postprocess_newlines`).
    Newline,
    /// Always the last token — the parser relies on it to never run off
    /// the end of the stream.
    Eof,
}

/// One token: what it is, where it is, and (for keywords) which language
/// flavor spelled it.
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
    /// Is this token the given keyword (any spelling)?
    pub fn is_kw(&self, kw: Kw) -> bool {
        matches!(self.kind, TokKind::Kw(k) if k == kw)
    }
}

/// Human name for error messages: "identifier \`foo\`", "keyword
/// \`module\`", "\`+%\`", "end of line", …
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

/// Source spelling of a punctuation token (for error messages).
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
