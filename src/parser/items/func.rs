//! Parses file-level `fn` declarations into [`TopItem::Func`] nodes.
//!
//! A combinational function is pure (no registers, no clocks). The body is
//! zero or more statements (`let`, statement-level `if`, `return`) followed
//! by exactly one tail expression — the guaranteed fallthrough value.

use super::super::*;

impl Parser {
    /// `fnDecl = "fn" ident "(" [param {"," param}] ")" "->" type
    ///           "{" {fnStmt} expr "}"` (spec/02 section 5)
    /// `param = ident ":" type`
    pub(super) fn func_decl(&mut self) -> Option<TopItem> {
        let start = self.bump().span; // fn

        let name = self.ident("a function name")?;
        self.expect(TokKind::LParen, "`(` to start the parameter list")?;

        let mut params = Vec::new();
        if !matches!(self.peek_kind(), TokKind::RParen) {
            loop {
                let pstart = self.peek().span;
                let pname = self.ident("a parameter name")?;
                self.expect(TokKind::Colon, "`:` then the parameter type")?;
                let ty = self.ty()?;
                params.push(FnParam {
                    span: self.span_since(pstart),
                    name: pname,
                    ty,
                });
                if !self.eat(&TokKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokKind::RParen, "`)` to close the parameter list")?;
        self.expect(TokKind::RArrow, "`->` then the return type")?;
        let ret = self.ty()?;

        self.expect(TokKind::LBrace, "`{` to start the function body")?;

        let (stmts, tail, end) = self.fn_body()?;

        Some(TopItem::Func(FuncDecl {
            name,
            params,
            ret,
            stmts,
            tail,
            span: start.join(end),
        }))
    }

    /// `fnBody = {fnStmt} expr "}"` — statements, then the mandatory tail
    /// expression, then the closing brace. Shared by the top-level `fn`
    /// body (called just above) — kept as its own method so a future
    /// `suzhal`/`loop` unroll (Spec 2) can reuse it for a nested block.
    ///
    /// `let`/`return` are prefix-keyword-only in BOTH word orders (checked
    /// first, unconditionally). Statement-level `if` and the tail
    /// expression are order-dependent: in `thamizh` order the condition
    /// precedes `enil` (`Kw::If`), so the head must be parsed as an
    /// expression FIRST and the trailing keyword decides whether it's a
    /// statement-`if` (`enil` → `fn_if_thamizh`), a match-expression tail
    /// (`thernthedu`/`Kw::Match` → `match_expr_thamizh`), or simply the tail
    /// itself — mirrors `seq_stmt_thamizh`/`expr_thamizh` exactly. Code
    /// order is unchanged from before this fix.
    fn fn_body(&mut self) -> Option<(Vec<FnStmt>, Expr, Span)> {
        let mut stmts = Vec::new();
        let tail = loop {
            self.skip_newlines();
            if self.at_kw(Kw::Let) {
                stmts.push(self.fn_let_stmt()?);
                continue;
            }
            if self.at_kw(Kw::Return) {
                stmts.push(self.fn_return_stmt()?);
                continue;
            }
            if self.profile == Profile::Thamizh {
                let head = self.binary(0)?;
                if self.at_kw(Kw::If) {
                    stmts.push(self.fn_if_thamizh(head)?);
                    continue;
                }
                if self.at_kw(Kw::Match) {
                    break self.match_expr_thamizh(head)?;
                }
                break head;
            }
            if self.at_kw(Kw::If) {
                stmts.push(self.fn_if()?);
                continue;
            }
            break self.expr()?;
        };
        self.skip_newlines();
        let end = self
            .expect(TokKind::RBrace, "`}` to close the function body")?
            .span;
        Some((stmts, tail, end))
    }

    /// `let ident "=" expr`
    fn fn_let_stmt(&mut self) -> Option<FnStmt> {
        let lstart = self.bump().span; // let
        let lname = self.ident("a local name")?;
        self.expect(TokKind::Assign, "`=` after the local name")?;
        let value = self.expr()?;
        let lend = value.span;
        Some(FnStmt::Let(LocalLet {
            name: lname,
            value,
            span: lstart.join(lend),
            inferred_width: std::cell::Cell::new(None),
        }))
    }

    /// `"return" expr`
    fn fn_return_stmt(&mut self) -> Option<FnStmt> {
        self.bump(); // return
        let value = self.expr()?;
        Some(FnStmt::Return(value))
    }

    /// `fnIf = "if" expr "{" {fnStmt} "}" [ "else" (fnIf | "{" {fnStmt} "}") ]`
    /// — statement-level `if`; `else` is OPTIONAL (a branch that doesn't
    /// return just falls through to the next statement, mirrors `seq_if`).
    fn fn_if(&mut self) -> Option<FnStmt> {
        self.enter()?;
        let r = self.fn_if_inner();
        self.leave();
        r
    }

    fn fn_if_inner(&mut self) -> Option<FnStmt> {
        self.bump(); // if
        let cond = self.expr()?;
        let then = self.fn_stmt_block()?;
        let save = self.pos;
        self.skip_newlines();
        let els = if self.at_kw(Kw::Else) {
            self.bump();
            if self.at_kw(Kw::If) {
                Some(vec![self.fn_if()?])
            } else {
                Some(self.fn_stmt_block()?)
            }
        } else {
            self.pos = save;
            None
        };
        Some(FnStmt::If { cond, then, els })
    }

    /// `thamizh`-order fn statement-`if`: `<cond> "enil" "{" {fnStmt} "}"
    /// ["illaiyenil" (fnIfThamizh | "{" {fnStmt} "}")]`. The condition is
    /// already parsed (the `head` from `fn_body`/`fn_stmt_block`); from
    /// `enil` onward mirrors `fn_if` and builds the SAME `FnStmt::If`.
    /// `else` (`illaiyenil`) stays OPTIONAL, same as `fn_if`. Depth-guarded
    /// like `fn_if`/`seq_if_thamizh`: the guard must wrap the whole call
    /// (including the recursive `illaiyenil <cond> enil …` chain) for a deep
    /// chain to fail with E1113 instead of overflowing the stack.
    fn fn_if_thamizh(&mut self, cond: Expr) -> Option<FnStmt> {
        self.enter()?;
        let r = self.fn_if_thamizh_inner(cond);
        self.leave();
        r
    }

    fn fn_if_thamizh_inner(&mut self, cond: Expr) -> Option<FnStmt> {
        self.bump(); // enil (Kw::If)
        let then = self.fn_stmt_block()?;
        let save = self.pos;
        self.skip_newlines();
        let els = if self.at_kw(Kw::Else) {
            self.bump(); // illaiyenil
            // A chained `illaiyenil <cond> enil …` starts with a condition;
            // a plain alternative starts with `{` (mirrors
            // `seq_if_thamizh_inner`'s identical disambiguation).
            if !self.at(&TokKind::LBrace) {
                let head = self.binary(0)?;
                Some(vec![self.fn_if_thamizh(head)?])
            } else {
                Some(self.fn_stmt_block()?)
            }
        } else {
            self.pos = save;
            None
        };
        Some(FnStmt::If { cond, then, els })
    }

    /// `"{" {fnStmt} "}"` — a statement block with NO tail expression
    /// (unlike the top-level `fn_body`, an `if`/`else` block inside a `fn`
    /// body is statements only; the enclosing function's `tail` is still
    /// the one mandatory fallthrough expression).
    fn fn_stmt_block(&mut self) -> Option<Vec<FnStmt>> {
        self.expect(TokKind::LBrace, "`{` to start the block")?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokKind::RBrace => {
                    self.bump();
                    break;
                }
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(span, "E1101", "block is missing its closing `}`");
                    break;
                }
                _ => {
                    let start = self.peek().span;
                    let stmt = if self.at_kw(Kw::Let) {
                        self.fn_let_stmt()
                    } else if self.at_kw(Kw::Return) {
                        self.fn_return_stmt()
                    } else if self.profile == Profile::Thamizh {
                        match self.binary(0) {
                            Some(head) if self.at_kw(Kw::If) => self.fn_if_thamizh(head),
                            Some(_) => {
                                let found = kind_name(self.peek_kind());
                                self.error(
                                    self.peek().span,
                                    "E1101",
                                    format!(
                                        "expected `let`, `if`, or `return` inside the `fn` block, found {found}"
                                    ),
                                );
                                None
                            }
                            None => None, // binary(0) already reported its own error
                        }
                    } else if self.at_kw(Kw::If) {
                        self.fn_if()
                    } else {
                        let found = kind_name(self.peek_kind());
                        self.error(
                            self.peek().span,
                            "E1101",
                            format!(
                                "expected `let`, `if`, or `return` inside the `fn` block, found {found}"
                            ),
                        );
                        None
                    };
                    match stmt {
                        Some(s) => stmts.push(s),
                        None => {
                            self.sync_to_newline();
                            stmts.push(FnStmt::Error(self.span_since(start)));
                        }
                    }
                }
            }
        }
        Some(stmts)
    }
}
