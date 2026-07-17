//! Token kinds. Keywords are one `Kw` regardless of which language flavor
//! spelled them — the flavor is recorded separately for diagnostics/fmt.

use crate::span::Span;

/// The keyword tokens — spelling-independent. `thoguthi` and `தொகுதி`
/// both lex to `Kw::Module`. The list mirrors `keywords.toml`; adding a
/// keyword means adding it in BOTH places (`keywords::kw_for_key` panics
/// at startup on a mismatch, so drift cannot ship).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kw {
    /// `module` keyword — declares a hardware module.
    Module,
    /// `in` keyword — declares an input port.
    In,
    /// `out` keyword — declares an output port.
    Out,
    /// `wire` keyword — declares a combinational signal.
    Wire,
    /// `reg` keyword — declares a clocked (stateful) signal.
    Reg,
    /// `mem` keyword — declares a memory array.
    Mem,
    /// `clock` keyword — declares/refers to a clock signal.
    Clock,
    /// `reset` keyword — declares/refers to a reset signal.
    Reset,
    /// `async` keyword — marks a reset as asynchronous.
    Async,
    /// `on` keyword — introduces a clocked/edge-triggered block.
    On,
    /// `rise` keyword — rising-edge trigger inside an `on` clause.
    Rise,
    /// `fall` keyword — falling-edge trigger inside an `on` clause.
    Fall,
    /// `if` keyword — conditional expression/statement.
    If,
    /// `else` keyword — alternate branch of an `if`.
    Else,
    /// `match` keyword — pattern-matching expression.
    Match,
    /// `enum` keyword — declares an enumerated type.
    Enum,
    /// `let` keyword — declares a local binding.
    Let,
    /// `const` keyword — declares a compile-time constant.
    Const,
    /// `repeat` keyword — item-level compile-time unroll.
    Repeat,
    /// `import` keyword — brings another file's items into scope.
    Import,
    /// `true` keyword — boolean literal.
    True,
    /// `false` keyword — boolean literal.
    False,
    /// `test` keyword — declares a testbench block.
    Test,
    /// `for` keyword — loop-variable binder in `repeat`/`loop` headers.
    For,
    /// `tick` keyword — advances the simulated clock by one cycle in a `test` block.
    Tick,
    /// `expect` keyword — asserts an expected value in a `test` block.
    Expect,
    /// `and` keyword — logical AND operator spelling.
    And,
    /// `or` keyword — logical OR operator spelling.
    Or,
    /// `not` keyword — logical NOT operator spelling.
    Not,
    /// `syntax` keyword — file-level directive selecting the keyword flavor.
    Syntax,
    /// `thamizh` / `தமிழ்` — flavor name used with `syntax` to select Tamil spellings.
    Thamizh,
    /// Combinational user-defined function keyword (`fn`/`function`/`saarbu`/`சார்பு`).
    Fn,
    /// `default` / `iyalbu` / `இயல்பு` — priority-lowest assignment in sequential blocks.
    Default,
    /// `bundle` / `kattai` / `கட்டை` — named group of signals (feature 2.4).
    Bundle,
    /// `return` keyword — returns a value from a `fn` body.
    Return,
    /// `loop` / `suzhal` / `சுழல்` — bounded compile-time unroll usable
    /// inside `on` blocks and `fn` bodies (distinct from `repeat`, which
    /// stays item-level only).
    Loop,
    /// `foreach` / `ovvondraga` / `ஒவ்வொன்றாக` — element/range loop sugar over
    /// `repeat`/bare `loop` (module-item, `on`-block, and `fn`-body
    /// positions); PROVISIONAL Tanglish/Tamil spellings, see `keywords.toml`.
    Foreach,
    /// `sync` / `othisai` / `ஒத்திசை` — modifies `loop` into a cycle-iterating
    /// FSM loop (`sync loop <name> on rise(clk) (var: lo..hi) -> result: ty = init { }`),
    /// distinct from the compile-time-unrolled `loop`.
    Sync,
    /// `sim` / `paavnai` / `பாவனை` — a hardware-emulation block inside a
    /// `test` block (throttling + peripheral binds). Simulation-only,
    /// never reaches the emitter.
    Sim,
    /// `bind` / `inai` / `இணை` — `bind <port> -> <peripheral>(args)` inside
    /// a `sim` block.
    Bind,
    /// `speed` / `vegam` / `வேகம்` — `speed <n>hz|khz|mhz(...)` inside a
    /// `sim` block; sets real-time throttling.
    Speed,
    /// `extern` / `anniya` / `அன்னிய` — declares the port shape of a real
    /// Verilog module living outside Min-Mozhi (Verilog FFI). PROVISIONAL
    /// Tanglish/Tamil spellings, see `keywords.toml`.
    Extern,
}

/// Which keyword skin a spelling came from (spec/03 Layer 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// Plain English keyword spellings (e.g. `module`).
    English,
    /// Romanized Tamil keyword spellings (e.g. `thoguthi`).
    Tanglish,
    /// Tamil-script keyword spellings (e.g. `தொகுதி`).
    Tamil,
}

/// Every kind of token the lexer can produce. Punctuation variants carry
/// no payload; their spelling lives in `punct_text`.
#[derive(Clone, Debug, PartialEq)]
pub enum TokKind {
    /// Identifier — the name text.
    Ident(String),
    /// Integer literal; `raw` preserves the written form (`0b1010`, `0xFF`).
    Int {
        /// The parsed numeric value.
        value: u128,
        /// The original written form, as it appeared in source.
        raw: String,
    },
    /// Binary don't-care literal — `0b1??`, valid only in a `match` pattern.
    /// `mask` has a 1 where the bit must match `value` (don't-care bits are 0
    /// in both), `width` is the digit count; `raw` keeps the spelling.
    MaskedInt {
        /// The known bits' value (don't-care bits are 0).
        value: u128,
        /// Bitmask marking which bits are known (1) vs. don't-care (0).
        mask: u128,
        /// Digit count of the literal.
        width: u32,
        /// The original written form, as it appeared in source.
        raw: String,
    },
    /// String literal — currently only used for test names.
    Str(String),
    /// A keyword, spelling-independent (see `Kw`).
    Kw(Kw),

    // arithmetic
    /// `+` — addition.
    Plus, // +
    /// `-` — subtraction.
    Minus, // -
    /// `*` — multiplication.
    Star, // *
    /// `+%` — wrapping addition.
    PlusPct, // +%  wrapping
    /// `-%` — wrapping subtraction.
    MinusPct, // -%
    /// `*%` — wrapping multiplication.
    StarPct, // *%
    // shifts
    /// `<<` — left shift.
    Shl, // <<
    /// `>>` — right shift.
    Shr, // >>
    // bitwise
    /// `&` — bitwise AND.
    Amp, // &
    /// `|` — bitwise OR.
    Pipe, // |
    /// `^` — bitwise XOR.
    Caret, // ^
    /// `~` — bitwise NOT.
    Tilde, // ~
    // logical symbols
    /// `&&` — logical AND.
    AmpAmp, // &&
    /// `||` — logical OR.
    PipePipe, // ||
    /// `!` — logical NOT.
    Bang, // !
    /// `?` — valid-bundle type suffix.
    Question, // ?
    /// `??` — fallback/mux operator.
    QQ, // ??
    // comparison
    /// `==` — equality.
    EqEq, // ==
    /// `!=` — inequality.
    Ne, // !=
    /// `<` — less than.
    Lt, // <
    /// `<=` — less than or equal.
    Le, // <=
    /// `>` — greater than.
    Gt, // >
    /// `>=` — greater than or equal.
    Ge, // >=
    // structure
    /// `=` — assignment (wires only).
    Assign, // =   (wires only)
    /// `<-` — clocked assignment (regs only).
    LArrow, // <-  (regs only)
    /// `=>` — match-arm separator.
    FatArrow, // =>
    /// `->` — function return-type arrow.
    RArrow,
    /// `:` — type/label separator.
    Colon,
    /// `,` — item separator.
    Comma,
    /// `.` — field/member access.
    Dot,
    /// `..` — range separator.
    DotDot,
    /// `(` — opening parenthesis.
    LParen,
    /// `)` — closing parenthesis.
    RParen,
    /// `[` — opening bracket.
    LBracket,
    /// `]` — closing bracket.
    RBracket,
    /// `{` — opening brace.
    LBrace,
    /// `}` — closing brace.
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
    /// What kind of token this is.
    pub kind: TokKind,
    /// Where in the source this token appears.
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
        TokKind::MaskedInt { raw, .. } => format!("don't-care pattern `{raw}`"),
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
        Question => "?",
        QQ => "??",
        EqEq => "==",
        Ne => "!=",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        Assign => "=",
        LArrow => "<-",
        FatArrow => "=>",
        RArrow => "->",
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
