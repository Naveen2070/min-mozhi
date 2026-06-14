//! Recursive-descent parser. Two word-order profiles share every
//! expression/declaration routine and differ only at the clause heads
//! (spec/04, the Phase 1.8 grammar engine):
//!
//! - `code-order` (default, spec/02 section 5): English-derived order.
//! - `thamizh-order` (`syntax thamizh` directive): SOV/postpositional clause
//!   heads. It produces the EXACT same AST — the profile is consumed by the
//!   `syntax_directive` routine and never reaches the tree, so a thamizh-order
//!   file and its code-order twin emit byte-identical Verilog.
//!
//! Slice landed so far: the directive + the clocked-block flip
//! (`rise(clk) on { }`). The conditional / match / test flips and the
//! `translate --order` pretty-printer are the remaining 1.8 work items.
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

/// Parse a token stream into a [`File`]. Like the lexer, this collects ALL
/// errors it can (statement-level recovery via `sync_to_newline`) instead
/// of stopping at the first one.
pub fn parse(toks: Vec<Token>) -> Result<File, Vec<Diag>> {
    let mut p = Parser {
        toks,
        pos: 0,
        diags: Vec::new(),
        profile: Profile::CodeOrder,
        depth: 0,
        too_deep: false,
    };
    let file = p.file();
    if p.diags.is_empty() {
        Ok(file)
    } else {
        Err(p.diags)
    }
}

/// Word-order profile (spec/04). Selected once by the optional leading
/// `syntax thamizh` directive; it only steers which clause-head productions
/// the parser uses — it has no effect on the resulting AST.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Profile {
    /// English-derived order (spec/02 section 5). The default — no directive.
    CodeOrder,
    /// SOV/postpositional clause heads (`syntax thamizh`).
    Thamizh,
}

/// Parser state: a cursor over the token stream plus collected errors.
/// Parse routines return `Option<T>` — `None` means "errored, diagnostics
/// already recorded; caller decides where to recover".
pub(crate) struct Parser {
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diag>,
    /// The active word-order profile, set by the `syntax` directive.
    profile: Profile,
    /// Current recursive-descent nesting depth (see `enter`). Bounds
    /// the stack so a pathological `((((…))))` cannot abort the process.
    depth: usize,
    /// Latch: emit the "nested too deeply" diagnostic at most once per parse.
    too_deep: bool,
}

/// Hard cap on recursive-descent nesting. Each level of source nesting costs
/// ~12 Rust stack frames (`expr → binary(0..9) → unary → postfix → primary`),
/// and `mimz` parses on the default thread stack (1 MB on Windows), so the cap
/// must stay well under `stack / (12 * frame)`. 64 levels is far more than any
/// human-written HDL expression needs while leaving a wide safety margin;
/// deeper adversarial input fails with a clean E1113 instead of overflowing
/// the stack. See `enter`. (Machine-generated extremes can raise this
/// once parsing moves onto a dedicated large-stack thread.)
const MAX_DEPTH: usize = 64;

/// Token plumbing and error recovery, shared by `items.rs` and `expr.rs`.
///
/// Conventions: `at*` = look without consuming; `eat*` = consume if it
/// matches (returns whether it did); `expect*` = consume or record an
/// error (`None` on failure). `bump` never advances past Eof, so `peek`
/// is always safe.
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

    /// Record one parse error. The code is mandatory (same discipline as
    /// `Checker::err`) — the E11xx catalog lives in docs/code/06.
    fn error(&mut self, span: Span, code: &'static str, msg: impl Into<String>) {
        self.diags.push(Diag::new(span, msg).with_code(code));
    }

    /// Attach a help line to the most recent error.
    fn help(&mut self, help: impl Into<String>) {
        if let Some(d) = self.diags.last_mut() {
            d.help = Some(help.into());
        }
    }

    /// Enter one level of recursive descent. Returns `None` (after recording
    /// E1113 once) when nesting would risk a stack overflow, so a pathological
    /// input — `((((…))))`, `!!!!…x`, a 50k-deep `if`/`else if` chain — fails
    /// with a clean diagnostic instead of aborting the process. Every
    /// `self.enter()?` MUST be paired with a later `self.leave()`; the wrapper
    /// routines (`expr`, `unary`, `if_expr`, `seq_if`, `test_if`) do this.
    fn enter(&mut self) -> Option<()> {
        if self.depth >= MAX_DEPTH {
            if !self.too_deep {
                let span = self.peek().span;
                self.error(span, "E1113", "nested too deeply to parse safely");
                self.help(format!(
                    "Min-Mozhi limits nesting to {MAX_DEPTH} levels so the parser stays within its stack — flatten the expression or split it into named wires/consts"
                ));
                self.too_deep = true;
            }
            return None;
        }
        self.depth += 1;
        Some(())
    }

    /// Leave one level of recursive descent (pairs with `enter`).
    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn expect(&mut self, kind: TokKind, what: &str) -> Option<Token> {
        if self.at(&kind) {
            Some(self.bump())
        } else {
            let found = kind_name(self.peek_kind());
            let span = self.peek().span;
            self.error(span, "E1101", format!("expected {what}, found {found}"));
            None
        }
    }

    fn expect_kw(&mut self, kw: Kw, what: &str) -> Option<()> {
        if self.eat_kw(kw) {
            Some(())
        } else {
            let found = kind_name(self.peek_kind());
            let span = self.peek().span;
            self.error(span, "E1101", format!("expected {what}, found {found}"));
            None
        }
    }

    /// Expect an identifier; `what` describes it in the error message
    /// ("a module name", "a clock name", …) — context beats "expected
    /// identifier" for a learner.
    fn ident(&mut self, what: &str) -> Option<Ident> {
        if let TokKind::Ident(name) = self.peek_kind().clone() {
            let t = self.bump();
            Some(Ident { name, span: t.span })
        } else {
            let found = kind_name(self.peek_kind());
            let span = self.peek().span;
            self.error(span, "E1101", format!("expected {what}, found {found}"));
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
            "E1101",
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
