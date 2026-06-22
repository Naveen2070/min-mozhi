//! Sequential (`on`) blocks: the clocked-block heads for both word-order
//! profiles, the seq-statement body, and the statement-level `if`.

use super::super::*;

impl Parser {
    /// code-order: `onBlock = "on" ("rise" | "fall") "(" ident ")" seqBlock`
    pub(super) fn on_block(&mut self) -> Option<ModuleItem> {
        let start = self.bump().span; // on
        let edge = self.clock_edge_kw()?;
        let clock = self.clock_edge_args()?;
        let (body, end) = self.seq_block()?;
        Some(ModuleItem::On(OnBlock {
            clock,
            edge,
            body,
            span: start.join(end),
        }))
    }

    /// thamizh-order: `onBlock = ("rise" | "fall") "(" ident ")" "on" seqBlock` —
    /// the clocked block with the edge head leading and `on` (pothu) trailing
    /// (spec/04 section 3: `yetram(clk) pothu { }`). Produces the identical
    /// [`OnBlock`] AST as the code-order form.
    pub(super) fn on_block_thamizh(&mut self) -> Option<ModuleItem> {
        // The dispatcher only calls this on a leading `rise`/`fall` head.
        let edge = if self.at_kw(Kw::Fall) {
            Edge::Fall
        } else {
            Edge::Rise
        };
        let start = self.bump().span; // rise / fall
        let clock = self.clock_edge_args()?;
        self.expect_kw(Kw::On, "`on` (pothu) after the clock edge in thamizh order")?;
        let (body, end) = self.seq_block()?;
        Some(ModuleItem::On(OnBlock {
            clock,
            edge,
            body,
            span: start.join(end),
        }))
    }

    /// `("rise" | "fall")` — the clock-edge head keyword, returning the [`Edge`].
    fn clock_edge_kw(&mut self) -> Option<Edge> {
        if self.at_kw(Kw::Rise) {
            self.bump();
            Some(Edge::Rise)
        } else if self.at_kw(Kw::Fall) {
            self.bump();
            Some(Edge::Fall)
        } else {
            let span = self.peek().span;
            self.error(
                span,
                "E1101",
                "expected `rise` or `fall` for the clock edge",
            );
            self.help("a sequential block is `on rise(clk) { … }` or `on fall(clk) { … }`");
            None
        }
    }

    /// `"(" ident ")"` — the clock name inside a `rise(clk)` edge head,
    /// shared by both word-order profiles.
    fn clock_edge_args(&mut self) -> Option<Ident> {
        self.expect(TokKind::LParen, "`(` then the clock name")?;
        let clock = self.ident("a clock name")?;
        self.expect(TokKind::RParen, "`)` after the clock name")?;
        Some(clock)
    }

    /// `seqBlock = "{" { seqStmt } "}"` — returns the statements plus the
    /// closing brace's span (for the parent's span join).
    fn seq_block(&mut self) -> Option<(Vec<SeqStmt>, Span)> {
        self.expect(TokKind::LBrace, "`{` to start the block")?;
        let mut stmts = Vec::new();
        let end = loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokKind::RBrace => break self.bump().span,
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(span, "E1101", "block is missing its closing `}`");
                    break span;
                }
                _ => {
                    let start = self.peek().span;
                    match self.seq_stmt() {
                        Some(s) => stmts.push(s),
                        None => {
                            self.sync_to_newline();
                            stmts.push(SeqStmt::Error(self.span_since(start)));
                        }
                    }
                }
            }
        };
        Some((stmts, end))
    }

    /// `seqStmt = seqIf | lvalue "<-" expr` — using `=` on a register is
    /// caught here with a teaching error (the `=`/`<-` safety rule). In
    /// `thamizh` order the clause head trails the operand, so that case is
    /// handled expression-first by `seq_stmt_thamizh`.
    fn seq_stmt(&mut self) -> Option<SeqStmt> {
        if self.profile == Profile::Thamizh {
            return self.seq_stmt_thamizh();
        }
        if self.at_kw(Kw::If) {
            return self.seq_if();
        }
        if let TokKind::Ident(_) = self.peek_kind() {
            let lhs = self.lvalue()?;
            if self.at(&TokKind::Assign) {
                let span = self.peek().span;
                self.error(span, "E1106", "`=` is only for wires, outside `on` blocks");
                self.help(
                    "registers update with `<-` inside `on` blocks: `value <- value +% 1` (spec/02 section 1.2)",
                );
                return None;
            }
            self.expect(TokKind::LArrow, "`<-` to update this register")?;
            let rhs = self.expr()?;
            self.terminator();
            return Some(SeqStmt::Assign { lhs, rhs });
        }
        let found = kind_name(self.peek_kind());
        let span = self.peek().span;
        self.error(
            span,
            "E1101",
            format!("expected a register update or `if` inside the `on` block, found {found}"),
        );
        None
    }

    /// `seqIf = "if" expr seqBlock [ "else" (seqIf | seqBlock) ]` —
    /// statement-level `if`: `else` is OPTIONAL here (an unassigned
    /// register holds its value; no latch risk, unlike wires).
    fn seq_if(&mut self) -> Option<SeqStmt> {
        self.enter()?;
        let r = self.seq_if_inner();
        self.leave();
        r
    }

    fn seq_if_inner(&mut self) -> Option<SeqStmt> {
        self.bump(); // if
        let cond = self.expr()?;
        let (then, _) = self.seq_block()?;
        // `else` may sit after a newline.
        let save = self.pos;
        self.skip_newlines();
        let els = if self.at_kw(Kw::Else) {
            self.bump();
            if self.at_kw(Kw::If) {
                Some(vec![self.seq_if()?])
            } else {
                let (b, _) = self.seq_block()?;
                Some(b)
            }
        } else {
            self.pos = save;
            None
        };
        Some(SeqStmt::If { cond, then, els })
    }

    /// `thamizh`-order seq statement: either `<cond> enil seqBlock …` or the
    /// unchanged register update `lvalue <- expr`. Both can begin with an
    /// identifier, so we parse the head as an expression and let the trailing
    /// token decide (no backtracking, spec/04): `enil` → conditional, `<-` →
    /// assignment (the head is reinterpreted as the lvalue), `=` → the teaching
    /// error.
    fn seq_stmt_thamizh(&mut self) -> Option<SeqStmt> {
        let head = self.binary(0)?;
        if self.at_kw(Kw::If) {
            return self.seq_if_thamizh(head);
        }
        if self.at(&TokKind::Assign) {
            let span = self.peek().span;
            self.error(span, "E1106", "`=` is only for wires, outside `on` blocks");
            self.help(
                "registers update with `<-` inside `on` blocks: `value <- value +% 1` (spec/02 section 1.2)",
            );
            return None;
        }
        self.expect(TokKind::LArrow, "`<-` to update this register")?;
        let lhs = self.expr_to_lvalue(head)?;
        let rhs = self.expr()?;
        self.terminator();
        Some(SeqStmt::Assign { lhs, rhs })
    }

    /// `thamizh`-order seq `if`: `<cond> "enil" seqBlock [ "illaiyenil"
    /// (<cond> "enil" … | seqBlock) ]`. The condition is already parsed; from
    /// `enil` onward it mirrors `seq_if` and builds the SAME `SeqStmt::If`.
    /// `else` (`illaiyenil`) is optional (a register holds its value — no latch).
    /// Depth-guarded like `seq_if`: the `illaiyenil <cond> enil …` chain recurses
    /// here, so the guard must wrap the whole call (incl. the recursion) for a
    /// deep chain to fail with E1113 instead of overflowing the stack.
    fn seq_if_thamizh(&mut self, cond: Expr) -> Option<SeqStmt> {
        self.enter()?;
        let r = self.seq_if_thamizh_inner(cond);
        self.leave();
        r
    }

    fn seq_if_thamizh_inner(&mut self, cond: Expr) -> Option<SeqStmt> {
        self.bump(); // enil (Kw::If)
        let (then, _) = self.seq_block()?;
        let save = self.pos;
        self.skip_newlines();
        let els = if self.at_kw(Kw::Else) {
            self.bump(); // illaiyenil
            // A chained `illaiyenil <cond> enil …` starts with a condition;
            // a plain alternative starts with `{`.
            if !self.at(&TokKind::LBrace) {
                let head = self.binary(0)?;
                Some(vec![self.seq_if_thamizh(head)?])
            } else {
                let (b, _) = self.seq_block()?;
                Some(b)
            }
        } else {
            self.pos = save;
            None
        };
        Some(SeqStmt::If { cond, then, els })
    }
}
