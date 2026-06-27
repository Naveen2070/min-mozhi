//! File-level items, module bodies, sequential (`on`) blocks, and `test`
//! blocks. Expressions live in `expr.rs`.
//!
//! Each routine carries its production from the EBNF grammar
//! (spec/02 section 5) as a doc comment — keep them in sync with the spec.
//!
//! File layout (split, house module pattern as in checker/widths/):
//! `mod.rs` keeps the shared helpers (`ty`, `lvalue`, `expr_to_lvalue`,
//! `ident_from_expr`, `repeat_block`); `file.rs` parses file-level items;
//! `module.rs` parses module bodies; `inst.rs` parses instantiations;
//! `seq.rs` parses sequential (`on`) blocks; `test.rs` parses `test` blocks.

mod file;
mod func;
mod inst;
mod module;
mod seq;
mod test;

use super::*;

impl Parser {
    /// Reinterpret an already-parsed expression as an assignment target —
    /// `ident`, `ident[i]`, or `ident[hi:lo]`. The thamizh-order seq statement
    /// parses its head as an expression before it knows whether it is a
    /// condition or an assignment lvalue, so this recovers the `LValue` the
    /// code-order path gets straight from `lvalue()`. Emits E1101 if the
    /// expression is not a valid target.
    pub(super) fn expr_to_lvalue(&mut self, e: Expr) -> Option<LValue> {
        let span = e.span;
        match e.kind {
            ExprKind::Ident(name) => Some(LValue {
                base: Ident { name, span },
                index: None,
                span,
            }),
            ExprKind::Index { base, index } => {
                let base = self.ident_from_expr(*base)?;
                Some(LValue {
                    span: base.span.join(span),
                    base,
                    index: Some((*index, None)),
                })
            }
            ExprKind::Slice { base, hi, lo } => {
                let base = self.ident_from_expr(*base)?;
                Some(LValue {
                    span: base.span.join(span),
                    base,
                    index: Some((*hi, Some(*lo))),
                })
            }
            _ => {
                self.error(
                    span,
                    "E1101",
                    "the left of `<-` must be a register name, optionally indexed",
                );
                self.help("for example: `value <- value +% 1` or `bus[0] <- bit`");
                None
            }
        }
    }

    /// The base of an indexed lvalue must be a bare register name.
    fn ident_from_expr(&mut self, e: Expr) -> Option<Ident> {
        match e.kind {
            ExprKind::Ident(name) => Some(Ident { name, span: e.span }),
            _ => {
                self.error(e.span, "E1101", "the indexed thing must be a register name");
                None
            }
        }
    }

    /// `lvalue = ident [ "[" expr [ ":" expr ] "]" ]`
    pub(super) fn lvalue(&mut self) -> Option<LValue> {
        let base = self.ident("a signal name")?;
        let mut span = base.span;
        let index = if self.eat(&TokKind::LBracket) {
            let first = self.expr()?;
            let second = if self.eat(&TokKind::Colon) {
                Some(self.expr()?)
            } else {
                None
            };
            let t = self.expect(TokKind::RBracket, "`]`")?;
            span = span.join(t.span);
            Some((first, second))
        } else {
            None
        };
        Some(LValue { base, index, span })
    }

    /// `type = "bit" | "bits" "[" expr "]" | "signed" "[" expr "]" | ident`
    /// — type names are contextual (identifiers), not keywords; anything
    /// unrecognized is assumed to be an enum name and resolved later.
    pub(super) fn ty(&mut self) -> Option<Type> {
        let id = self.ident("a type — `bit`, `bits[N]`, `signed[N]`, or an enum name")?;
        match id.name.as_str() {
            "bit" => Some(Type::Bit),
            "bits" => {
                self.expect(TokKind::LBracket, "`[` — bit vectors are written `bits[N]`")?;
                let n = self.expr()?;
                self.expect(TokKind::RBracket, "`]` after the width")?;
                Some(Type::Bits(Box::new(n)))
            }
            "signed" => {
                self.expect(
                    TokKind::LBracket,
                    "`[` — signed vectors are written `signed[N]`",
                )?;
                let n = self.expr()?;
                self.expect(TokKind::RBracket, "`]` after the width")?;
                Some(Type::Signed(Box::new(n)))
            }
            _ => Some(Type::Named(id)),
        }
    }

    /// `repeatBlock = "repeat" ident ":" expr ".." expr "{" { moduleItem } "}"`
    pub(super) fn repeat_block(&mut self) -> Option<ModuleItem> {
        let start = self.bump().span; // repeat
        let var = self.ident("a loop variable name")?;
        self.expect(TokKind::Colon, "`:` then the range, e.g. `repeat i: 0..8`")?;
        let lo = self.expr()?;
        self.expect(TokKind::DotDot, "`..` between the range bounds")?;
        let hi = self.expr()?;
        self.expect(TokKind::LBrace, "`{` to start the repeat body")?;
        let mut items = Vec::new();
        let end = loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokKind::RBrace => break self.bump().span,
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(span, "E1101", "`repeat` block is missing its closing `}`");
                    break span;
                }
                _ => {
                    let start = self.peek().span;
                    match self.module_item() {
                        Some(i) => items.push(i),
                        None => {
                            self.sync_to_newline();
                            items.push(ModuleItem::Error(self.span_since(start)));
                        }
                    }
                }
            }
        };
        Some(ModuleItem::Repeat(Repeat {
            var,
            lo,
            hi,
            items,
            span: start.join(end),
        }))
    }
}
