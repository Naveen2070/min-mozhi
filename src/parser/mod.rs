//! Recursive-descent parser for the `code-order` profile (spec/02 §5).
//! The `thamizh-order` profile (spec/04) arrives in Phase 1.8 — it will
//! share every expression/declaration routine here and flip only the
//! clause heads.
//!
//! Module layout:
//! - `mod.rs`   — entry point, `Parser` state, token plumbing, error recovery
//! - `items.rs` — file-level items, module bodies, sequential/test blocks
//! - `expr.rs`  — expressions: precedence climbing, match arms, builtins

mod expr;
mod items;
#[cfg(test)]
mod tests;

use crate::ast::*;
use crate::diag::Diag;
use crate::lexer::token::{Kw, TokKind, Token, kind_name};
use crate::span::Span;

pub fn parse(toks: Vec<Token>) -> Result<File, Vec<Diag>> {
    let mut p = Parser {
        toks,
        pos: 0,
        diags: Vec::new(),
    };
    let file = p.file();
    if p.diags.is_empty() {
        Ok(file)
    } else {
        Err(p.diags)
    }
}

pub(crate) struct Parser {
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diag>,
}

/// Token plumbing and error recovery, shared by `items.rs` and `expr.rs`.
impl Parser {
    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn peek_kind(&self) -> &TokKind {
        &self.peek().kind
    }

    fn at(&self, kind: &TokKind) -> bool {
        self.peek_kind() == kind
    }

    fn at_kw(&self, kw: Kw) -> bool {
        self.peek().is_kw(kw)
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos.min(self.toks.len() - 1)].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, kind: &TokKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, kw: Kw) -> bool {
        if self.at_kw(kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diag::new(span, msg));
    }

    /// Attach a help line to the most recent error.
    fn help(&mut self, help: impl Into<String>) {
        if let Some(d) = self.diags.last_mut() {
            d.help = Some(help.into());
        }
    }

    fn expect(&mut self, kind: TokKind, what: &str) -> Option<Token> {
        if self.at(&kind) {
            Some(self.bump())
        } else {
            let found = kind_name(self.peek_kind());
            let span = self.peek().span;
            self.error(span, format!("expected {what}, found {found}"));
            None
        }
    }

    fn expect_kw(&mut self, kw: Kw, what: &str) -> Option<()> {
        if self.eat_kw(kw) {
            Some(())
        } else {
            let found = kind_name(self.peek_kind());
            let span = self.peek().span;
            self.error(span, format!("expected {what}, found {found}"));
            None
        }
    }

    fn ident(&mut self, what: &str) -> Option<Ident> {
        if let TokKind::Ident(name) = self.peek_kind().clone() {
            let t = self.bump();
            Some(Ident { name, span: t.span })
        } else {
            let found = kind_name(self.peek_kind());
            let span = self.peek().span;
            self.error(span, format!("expected {what}, found {found}"));
            None
        }
    }

    fn skip_newlines(&mut self) {
        while self.eat(&TokKind::Newline) {}
    }

    /// Statement terminator: newline, or implicitly before `}` / EOF.
    fn terminator(&mut self) {
        if self.eat(&TokKind::Newline) {
            return;
        }
        if matches!(self.peek_kind(), TokKind::RBrace | TokKind::Eof) {
            return;
        }
        let found = kind_name(self.peek_kind());
        let span = self.peek().span;
        self.error(
            span,
            format!("expected end of line after statement, found {found}"),
        );
        self.sync_to_newline();
    }

    /// Error recovery: skip to the next newline or `}` so later statements
    /// still get checked (>1 error per run).
    fn sync_to_newline(&mut self) {
        loop {
            match self.peek_kind() {
                TokKind::Newline => {
                    self.bump();
                    return;
                }
                TokKind::RBrace | TokKind::Eof => return,
                _ => {
                    self.bump();
                }
            }
        }
    }
}
