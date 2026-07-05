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

mod bundle;
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

    /// `type = arrayType | scalarType`
    /// `arrayType = scalarType { "[" expr "]" }` — one or more trailing
    /// `[N]` suffixes wrap the preceding type in `Type::Array`, so
    /// `bits[8][4]` parses as `Array { elem: Bits(8), len: 4 }` and
    /// `bits[8][4][2]` as `Array { elem: Array { elem: Bits(8), len: 4 },
    /// len: 2 }` (nested arrays parse cleanly; the CHECKER rejects them —
    /// element-type restriction is not this grammar's job, matching this
    /// project's existing "lenient parser, narrowing checker" pattern).
    /// `scalarType = "bit" | "bits" "[" expr "]" | "signed" "[" expr "]" | ident`
    /// — type names are contextual (identifiers), not keywords; anything
    /// unrecognized is assumed to be an enum name and resolved later.
    pub(super) fn ty(&mut self) -> Option<Type> {
        let mut t = self.scalar_ty()?;
        while self.at(&TokKind::LBracket) {
            self.bump(); // [
            let len = self.expr()?;
            self.expect(TokKind::RBracket, "`]` after the array length")?;
            t = Type::Array {
                elem: Box::new(t),
                len: Box::new(len),
            };
        }
        Some(t)
    }

    /// The non-array type grammar — everything `ty()` used to do before
    /// this plan added the trailing-`[N]` array-suffix loop above. This is
    /// the EXACT prior body of `ty()`, renamed and unchanged in content,
    /// so every existing scalar/enum/bundle type still parses identically.
    fn scalar_ty(&mut self) -> Option<Type> {
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
            _ => {
                // Plain enum/bundle name, namespaced (`a.b.Foo`), OR parametric
                // bundle `Foo(X: N)` / `a.b.Foo(X: N)`.
                let mut path = Vec::new();
                let mut name = id;
                while self.at(&TokKind::Dot) {
                    self.bump();
                    let next = self.ident("a type name after `.`")?;
                    path.push(name);
                    name = next;
                }
                let qid = QualIdent {
                    span: name.span,
                    path,
                    name,
                    resolved_file: std::cell::Cell::new(None),
                };
                if self.eat(&TokKind::LParen) {
                    let mut args = Vec::new();
                    loop {
                        self.skip_newlines();
                        if self.eat(&TokKind::RParen) {
                            break;
                        }
                        let aname = self.ident("a parameter name")?;
                        self.expect(TokKind::Colon, "`:` then the parameter value")?;
                        let aval = self.expr()?;
                        args.push(NamedArg {
                            name: aname,
                            value: aval,
                        });
                        self.skip_newlines();
                        if !self.eat(&TokKind::Comma) {
                            self.expect(TokKind::RParen, "`,` or `)` after parameter")?;
                            break;
                        }
                    }
                    Some(Type::Bundle { name: qid, args })
                } else {
                    Some(Type::Named(qid))
                }
            }
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

    /// `syncLoop = "sync" "loop" ident "on" ("rise"|"fall") "(" ident ")"
    ///   "(" ident ":" expr ".." expr ")" "->" ident ":" type "=" expr seqBlock`
    /// — the `sync`/`loop` head is consumed by the caller in `module_item`
    /// (mirroring `async reset` there), which passes their combined span as
    /// `start`; this function picks up right after `loop`.
    pub(super) fn sync_loop_block(&mut self, start: Span) -> Option<ModuleItem> {
        let name = self.ident("a name for this sync loop, e.g. `sync loop find_first`")?;
        self.expect_kw(
            Kw::On,
            "`on` — a sync loop names its clock edge, e.g. `on rise(clk)`",
        )?;
        let edge = self.clock_edge_kw()?;
        let clock = self.clock_edge_args()?;
        self.expect(TokKind::LParen, "`(` then the loop variable and range")?;
        let var = self.ident("a loop variable name")?;
        self.expect(TokKind::Colon, "`:` then the range, e.g. `i: 0..8`")?;
        let lo = self.expr()?;
        self.expect(TokKind::DotDot, "`..` between the range bounds")?;
        let hi = self.expr()?;
        self.expect(TokKind::RParen, "`)` after the range")?;
        self.expect(
            TokKind::RArrow,
            "`->` then the named accumulator, e.g. `-> result: bits[8] = 0`",
        )?;
        let result_name = self.ident("a name for the accumulator")?;
        self.expect(TokKind::Colon, "`:` then the accumulator's type")?;
        let result_ty = self.ty()?;
        self.expect(
            TokKind::Assign,
            "`=` then the accumulator's reset value",
        )?;
        let result_init = self.expr()?;
        let (body, end) = self.seq_block()?;
        Some(ModuleItem::SyncLoop(SyncLoop {
            name,
            clock,
            edge,
            var,
            lo,
            hi,
            result_name,
            result_ty,
            result_init,
            body,
            span: start.join(end),
        }))
    }
}
