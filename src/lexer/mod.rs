//! Lexer: UTF-8 source (NFC-normalized by the caller) → token stream.
//!
//! - Recognizes the UNION of English/Tanglish/Tamil keyword spellings at all
//!   times — flavors mix freely in one file (spec/03 section 1).
//! - Unicode identifiers (XID rules, so Tamil script works everywhere).
//! - Newlines are statement terminators, Go-style: a line ending in an
//!   operator or open bracket continues onto the next line (spec/02 section 2).

pub mod keywords;
pub mod token;

use crate::diag::Diag;
use crate::span::Span;
use keywords::TABLE;
use token::{TokKind, Token};

/// Tokenize a whole file. On success the stream is newline-normalized
/// (see `postprocess_newlines`) and always ends with [`TokKind::Eof`].
/// On failure ALL lex errors are returned, not just the first.
pub fn lex(src: &str) -> Result<Vec<Token>, Vec<Diag>> {
    let mut lx = Lexer {
        src,
        chars: src.char_indices().collect(),
        pos: 0,
        toks: Vec::new(),
        diags: Vec::new(),
    };
    lx.run();
    if lx.diags.is_empty() {
        Ok(postprocess_newlines(lx.toks))
    } else {
        Err(lx.diags)
    }
}

/// Lexer state: a cursor over the pre-collected `(byte offset, char)`
/// pairs. Collecting up front trades memory for O(1) two-char lookahead,
/// which is all this grammar ever needs.
struct Lexer<'a> {
    src: &'a str,
    chars: Vec<(usize, char)>,
    pos: usize, // index into `chars`
    toks: Vec<Token>,
    diags: Vec<Diag>,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).map(|&(_, c)| c)
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).map(|&(_, c)| c)
    }

    /// Byte offset of the current position (= `src.len()` at end of input).
    fn offset(&self) -> usize {
        self.chars
            .get(self.pos)
            .map(|&(i, _)| i)
            .unwrap_or(self.src.len())
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn push(&mut self, kind: TokKind, start: usize) {
        let span = Span::new(start, self.offset());
        self.toks.push(Token {
            kind,
            span,
            flavor: None,
        });
    }

    /// Main loop: dispatch on the first char of each token. Whitespace and
    /// `//` comments vanish here; newlines become tokens (they terminate
    /// statements) and are filtered later by [`postprocess_newlines`].
    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.offset();
            match c {
                ' ' | '\t' | '\r' => {
                    self.bump();
                }
                '\n' => {
                    self.bump();
                    self.push(TokKind::Newline, start);
                }
                '/' if self.peek2() == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                '/' if self.peek2() == Some('*') => self.block_comment(start),
                '"' => self.string(start),
                c if c.is_ascii_digit() => self.number(start),
                c if unicode_ident::is_xid_start(c) || c == '_' => self.ident(start),
                _ => self.punct(start),
            }
        }
        let end = self.src.len();
        self.toks.push(Token {
            kind: TokKind::Eof,
            span: Span::new(end, end),
            flavor: None,
        });
    }

    fn block_comment(&mut self, start: usize) {
        self.bump(); // /
        self.bump(); // *
        let mut saw_newline = false;
        loop {
            match self.bump() {
                Some('*') if self.peek() == Some('/') => {
                    self.bump();
                    break;
                }
                Some('\n') => saw_newline = true,
                Some(_) => {}
                None => {
                    self.diags.push(
                        Diag::new(
                            Span::new(start, self.offset()),
                            "unterminated block comment",
                        )
                        .with_code("E1001")
                        .with_help("close it with `*/`"),
                    );
                    return;
                }
            }
        }
        // A multi-line comment still separates lines.
        if saw_newline {
            self.push(TokKind::Newline, start);
        }
    }

    fn string(&mut self, start: usize) {
        self.bump(); // opening quote
        let mut s = String::new();
        loop {
            match self.bump() {
                Some('"') => break,
                Some('\n') | None => {
                    self.diags.push(
                        Diag::new(Span::new(start, self.offset()), "unterminated string")
                            .with_code("E1002")
                            .with_help("close it with `\"` before the end of the line"),
                    );
                    return;
                }
                Some(c) => s.push(c),
            }
        }
        self.push(TokKind::Str(s), start);
    }

    /// Integer literal: decimal, `0b` binary, or `0x` hex, with `_`
    /// separators. The raw spelling is kept on the token so the Verilog
    /// emitter can preserve the writer's base.
    fn number(&mut self, start: usize) {
        let first = self.bump().unwrap();
        let (radix, mut raw) = if first == '0' && matches!(self.peek(), Some('b' | 'x')) {
            let marker = self.bump().unwrap();
            (if marker == 'b' { 2 } else { 16 }, format!("0{marker}"))
        } else {
            (10, first.to_string())
        };

        let mut digits = String::new();
        if radix == 10 {
            digits.push(first);
        }
        while let Some(c) = self.peek() {
            if c == '_' {
                raw.push(c);
                self.bump();
            } else if c.is_ascii_alphanumeric() {
                raw.push(c);
                digits.push(c);
                self.bump();
            } else {
                break;
            }
        }

        // Reject Tamil digits explicitly with a teaching message (decision B14).
        if let Some(c) = self.peek()
            && ('௦'..='௯').contains(&c)
        {
            self.diags.push(
                Diag::new(
                    Span::new(start, self.offset() + c.len_utf8()),
                    "Tamil digits are not used in literals",
                )
                .with_code("E1003")
                .with_help(
                    "write numbers with ASCII digits 0-9 — they are universal across all flavors",
                ),
            );
            self.bump();
            return;
        }

        match u128::from_str_radix(&digits, radix) {
            Ok(value) => self.push(TokKind::Int { value, raw }, start),
            Err(_) => {
                let what = match radix {
                    2 => "binary digits 0/1 after `0b`",
                    16 => "hex digits 0-9a-f after `0x`",
                    _ => "decimal digits",
                };
                self.diags.push(
                    Diag::new(
                        Span::new(start, self.offset()),
                        format!("`{raw}` is not a valid number"),
                    )
                    .with_code("E1004")
                    .with_help(format!("expected {what}; `_` separators are allowed")),
                );
            }
        }
    }

    /// Identifier or keyword. Unicode XID rules (so Tamil-script names
    /// work), then one lookup in the trilingual table decides: keyword
    /// (with its flavor recorded), reserved word (error), or identifier.
    fn ident(&mut self, start: usize) {
        let mut s = String::new();
        s.push(self.bump().unwrap());
        while let Some(c) = self.peek() {
            if unicode_ident::is_xid_continue(c) || c == '_' {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let span = Span::new(start, self.offset());
        if let Some((kw, flavor)) = TABLE.lookup(&s) {
            self.toks.push(Token {
                kind: TokKind::Kw(kw),
                span,
                flavor: Some(flavor),
            });
        } else if TABLE.is_reserved(&s) {
            self.diags.push(
                Diag::new(span, format!("`{s}` is a reserved word"))
                    .with_code("E1005")
                    .with_help(
                        "it is set aside for a future Min-Mozhi feature — pick another name",
                    ),
            );
        } else {
            self.toks.push(Token {
                kind: TokKind::Ident(s),
                span,
                flavor: None,
            });
        }
    }

    /// Operators and punctuation, longest-match-first (`+%` before `+`,
    /// `<-` before `<`). `/` and `%` are caught here with teaching errors —
    /// they do not exist in the language.
    fn punct(&mut self, start: usize) {
        use TokKind::*;
        let c = self.bump().unwrap();
        let two = |lx: &mut Self, kind: TokKind| {
            lx.bump();
            kind
        };
        let kind = match c {
            '+' if self.peek() == Some('%') => two(self, PlusPct),
            '+' => Plus,
            '-' if self.peek() == Some('%') => two(self, MinusPct),
            '-' => Minus,
            '*' if self.peek() == Some('%') => two(self, StarPct),
            '*' => Star,
            '<' if self.peek() == Some('<') => two(self, Shl),
            '<' if self.peek() == Some('=') => two(self, Le),
            '<' if self.peek() == Some('-') => two(self, LArrow),
            '<' => Lt,
            '>' if self.peek() == Some('>') => two(self, Shr),
            '>' if self.peek() == Some('=') => two(self, Ge),
            '>' => Gt,
            '=' if self.peek() == Some('=') => two(self, EqEq),
            '=' if self.peek() == Some('>') => two(self, FatArrow),
            '=' => Assign,
            '!' if self.peek() == Some('=') => two(self, Ne),
            '!' => Bang,
            '&' if self.peek() == Some('&') => two(self, AmpAmp),
            '&' => Amp,
            '|' if self.peek() == Some('|') => two(self, PipePipe),
            '|' => Pipe,
            '^' => Caret,
            '~' => Tilde,
            ':' => Colon,
            ',' => Comma,
            '.' if self.peek() == Some('.') => two(self, DotDot),
            '.' => Dot,
            '(' => LParen,
            ')' => RParen,
            '[' => LBracket,
            ']' => RBracket,
            '{' => LBrace,
            '}' => RBrace,
            '/' => {
                self.diags.push(
                    Diag::new(Span::new(start, self.offset()), "division `/` does not exist in Min-Mozhi")
                        .with_code("E1006")
                        .with_help("division synthesizes to large slow hardware — use shifts, or a stdlib divider module later (spec/02 section 3)"),
                );
                return;
            }
            '%' => {
                self.diags.push(
                    Diag::new(Span::new(start, self.offset()), "modulo `%` does not exist in Min-Mozhi")
                        .with_code("E1007")
                        .with_help("for wrapping arithmetic use `+%`, `-%`, `*%`; for low bits use slicing `x[k-1:0]`"),
                );
                return;
            }
            other => {
                self.diags.push(
                    Diag::new(
                        Span::new(start, self.offset()),
                        format!("unexpected character `{other}`"),
                    )
                    .with_code("E1008")
                    .with_help("Min-Mozhi has no token starting with this character"),
                );
                return;
            }
        };
        self.push(kind, start);
    }
}

/// Newline policy (spec/02 section 2): collapse runs, drop leading newlines, and
/// drop a newline when the previous token can't end a statement (operator,
/// comma, open bracket, `=`, `<-`, `=>`, `:`).
fn postprocess_newlines(toks: Vec<Token>) -> Vec<Token> {
    use TokKind::*;
    let mut out: Vec<Token> = Vec::with_capacity(toks.len());
    for t in toks {
        if matches!(t.kind, Newline) {
            match out.last() {
                None => continue,
                Some(prev) => {
                    let continues = matches!(
                        prev.kind,
                        Plus | Minus
                            | Star
                            | PlusPct
                            | MinusPct
                            | StarPct
                            | Shl
                            | Shr
                            | Amp
                            | Pipe
                            | Caret
                            | Tilde
                            | AmpAmp
                            | PipePipe
                            | Bang
                            | EqEq
                            | Ne
                            | Lt
                            | Le
                            | Gt
                            | Ge
                            | Assign
                            | LArrow
                            | FatArrow
                            | Colon
                            | Comma
                            | Dot
                            | DotDot
                            | LParen
                            | LBracket
                            | LBrace
                            | Newline
                    ) || matches!(
                        prev.kind,
                        Kw(token::Kw::And) | Kw(token::Kw::Or) | Kw(token::Kw::Not)
                    );
                    if continues {
                        continue;
                    }
                }
            }
        }
        out.push(t);
    }
    out
}

#[cfg(test)]
mod tests;
