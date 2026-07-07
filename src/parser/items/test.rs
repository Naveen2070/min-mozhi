//! `test` blocks: the `test … for …` declaration, the test body
//! (`tick` / `expect` / drive / `if`), and the test-level `if`.

use super::super::*;

impl Parser {
    // ---------- tests ----------

    /// code-order: `testDecl = "test" string "for" ident [ "(" argList ")" ]
    /// testBlock`
    pub(super) fn test_decl(&mut self) -> Option<TestDecl> {
        let start = self.bump().span; // test
        let name = self.test_name()?;
        self.expect_kw(Kw::For, "`for` then the module under test")?;
        let module = self.qual_ident("the module under test")?;
        let args = self.test_args()?;
        let (body, end) = self.test_block()?;
        Some(TestDecl {
            name,
            module,
            args,
            body,
            span: start.join(end),
        })
    }

    /// thamizh-order: `testDecl = ident [ "(" argList ")" ] "kaaga" string
    /// "sodhanai" testBlock` — the module under test leads; `kaaga` (for) and
    /// `sodhanai` (test) are the trailing clause heads (spec/04). Produces the
    /// identical [`TestDecl`] AST as the code-order form, so a thamizh-order test
    /// and its code-order twin run the same way.
    pub(super) fn test_decl_thamizh(&mut self) -> Option<TestDecl> {
        let module = self.qual_ident("the module under test")?;
        let start = module.span;
        let args = self.test_args()?;
        self.expect_kw(Kw::For, "`kaaga` (for) after the module under test")?;
        let name = self.test_name()?;
        self.expect_kw(Kw::Test, "`sodhanai` (test) then the test body")?;
        let (body, end) = self.test_block()?;
        Some(TestDecl {
            name,
            module,
            args,
            body,
            span: start.join(end),
        })
    }

    /// The quoted test name — after `test` in code order, after `kaaga` in
    /// thamizh order.
    fn test_name(&mut self) -> Option<String> {
        if let TokKind::Str(s) = self.peek_kind().clone() {
            self.bump();
            Some(s)
        } else {
            let span = self.peek().span;
            self.error(
                span,
                "E1107",
                "expected a test name in quotes, e.g. `test \"counter counts\" for ...`",
            );
            None
        }
    }

    /// Optional `"(" name ":" expr { "," … } ")"` parameter list for the module
    /// under test, shared by both word-order profiles. An absent list is `[]`.
    fn test_args(&mut self) -> Option<Vec<NamedArg>> {
        let mut args = Vec::new();
        if self.eat(&TokKind::LParen) {
            loop {
                self.skip_newlines();
                if self.eat(&TokKind::RParen) {
                    break;
                }
                let pname = self.ident("a parameter name")?;
                self.expect(TokKind::Colon, "`:` then the parameter value")?;
                let value = self.expr()?;
                args.push(NamedArg { name: pname, value });
                self.skip_newlines();
                if !self.eat(&TokKind::Comma) {
                    self.expect(TokKind::RParen, "`,` or `)`")?;
                    break;
                }
            }
        }
        Some(args)
    }

    /// `testBlock = "{" { tick | expect | drive | testIf } "}"`
    fn test_block(&mut self) -> Option<(Vec<TestStmt>, Span)> {
        self.expect(TokKind::LBrace, "`{` to start the test body")?;
        let mut stmts = Vec::new();
        let end = loop {
            self.skip_newlines();
            let start = self.peek().span;
            match self.peek_kind().clone() {
                TokKind::RBrace => break self.bump().span,
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(span, "E1101", "test block is missing its closing `}`");
                    break span;
                }
                TokKind::Kw(Kw::Tick) => {
                    self.bump();
                    self.expect(TokKind::LParen, "`(` then the clock name")?;
                    let clock = self.ident("a clock name")?;
                    let count = if self.eat(&TokKind::Comma) {
                        Some(self.expr()?)
                    } else {
                        None
                    };
                    self.expect(TokKind::RParen, "`)`")?;
                    self.terminator();
                    stmts.push(TestStmt::Tick { clock, count });
                }
                TokKind::Kw(Kw::Expect) => {
                    self.bump();
                    let e = self.expr()?;
                    self.terminator();
                    stmts.push(TestStmt::Expect(e));
                }
                TokKind::Kw(Kw::If) => {
                    if let Some(s) = self.test_if() {
                        stmts.push(s);
                    } else {
                        self.sync_to_newline();
                        stmts.push(TestStmt::Error(self.span_since(start)));
                    }
                }
                TokKind::Kw(Kw::Sim) => {
                    if let Some(s) = self.sim_block() {
                        stmts.push(TestStmt::Sim(s));
                    } else {
                        self.sync_to_newline();
                        stmts.push(TestStmt::Error(self.span_since(start)));
                    }
                }
                TokKind::Ident(_) => {
                    let name = self.ident("an input name")?;
                    self.expect(TokKind::Assign, "`=` to drive this input")?;
                    let value = self.expr()?;
                    self.terminator();
                    stmts.push(TestStmt::Drive { name, value });
                }
                other => {
                    let found = kind_name(&other);
                    let span = self.peek().span;
                    self.error(span, "E1107", format!("expected `tick`, `expect`, an input drive, or `if` in the test body, found {found}"));
                    self.sync_to_newline();
                    stmts.push(TestStmt::Error(self.span_since(start)));
                }
            }
        };
        Some((stmts, end))
    }

    /// `testIf = "if" expr testBlock [ "else" (testIf | testBlock) ]`
    fn test_if(&mut self) -> Option<TestStmt> {
        self.enter()?;
        let r = self.test_if_inner();
        self.leave();
        r
    }

    fn test_if_inner(&mut self) -> Option<TestStmt> {
        self.bump(); // if
        let cond = self.expr()?;
        let (then, _) = self.test_block()?;
        let save = self.pos;
        self.skip_newlines();
        let els = if self.at_kw(Kw::Else) {
            self.bump();
            if self.at_kw(Kw::If) {
                Some(vec![self.test_if()?])
            } else {
                let (b, _) = self.test_block()?;
                Some(b)
            }
        } else {
            self.pos = save;
            None
        };
        Some(TestStmt::If { cond, then, els })
    }

    /// `simBlock = "sim" "{" [ "speed" speedExpr ] bindStmt* "}"`
    fn sim_block(&mut self) -> Option<SimBlock> {
        let start = self.bump().span; // sim
        self.expect(TokKind::LBrace, "`{` to start the sim block")?;
        let mut speed = None;
        let mut binds = Vec::new();
        let end = loop {
            self.skip_newlines();
            match self.peek_kind().clone() {
                TokKind::RBrace => break self.bump().span,
                TokKind::Kw(Kw::Speed) => {
                    self.bump();
                    match self.speed_expr() {
                        Some(e) => {
                            speed = Some(e);
                            self.terminator();
                        }
                        None => {
                            self.sync_to_sim_close();
                            return None;
                        }
                    }
                }
                TokKind::Kw(Kw::Bind) => match self.bind_stmt() {
                    Some(b) => binds.push(b),
                    None => {
                        self.sync_to_sim_close();
                        return None;
                    }
                },
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(span, "E1114", "sim block is missing its closing `}`");
                    break span;
                }
                other => {
                    let found = kind_name(&other);
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1114",
                        format!("expected `speed` or `bind` in the sim block, found {found}"),
                    );
                    self.sync_to_sim_close();
                    return None;
                }
            }
        };
        Some(SimBlock {
            speed,
            binds,
            span: start.join(end),
        })
    }

    /// On any parse failure inside a `sim` block, skip forward and consume
    /// the block's own closing `}` (there is no nested `{` inside a `sim`
    /// block — `bind`'s config uses `(...)`, not braces — so the next `}`
    /// is always this block's own). Without this, the caller's
    /// `sync_to_newline` stops one line short and the outer `test` block
    /// mistakes this `}` for its own, truncating the rest of the test body.
    fn sync_to_sim_close(&mut self) {
        while !matches!(self.peek_kind(), TokKind::RBrace | TokKind::Eof) {
            self.bump();
        }
        self.bump(); // consume `}` (no-op at Eof)
    }

    /// `speedExpr = ("hz" | "khz" | "mhz") "(" expr ")"` — desugars to
    /// `expr * <multiplier>` (a plain `Mul` binary expr). Not a general
    /// builtin: see the design doc's Grammar section for why this stays
    /// local to this one production instead of growing `Builtin`.
    fn speed_expr(&mut self) -> Option<Expr> {
        let unit_span = self.peek().span;
        let unit = self.ident("`hz`, `khz`, or `mhz`")?;
        let mult: u128 = match unit.name.as_str() {
            "hz" => 1,
            "khz" => 1_000,
            "mhz" => 1_000_000,
            other => {
                self.error(
                    unit_span,
                    "E1114",
                    format!("expected `hz`, `khz`, or `mhz`, found `{other}`"),
                );
                return None;
            }
        };
        self.expect(TokKind::LParen, "`(` then the frequency")?;
        let n = self.expr()?;
        let end = self.expect(TokKind::RParen, "`)`")?.span;
        Some(Expr {
            span: unit_span.join(end),
            kind: ExprKind::Binary {
                op: BinOp::Mul,
                lhs: Box::new(n),
                rhs: Box::new(Expr {
                    span: unit_span.join(end),
                    kind: ExprKind::Int {
                        value: mult,
                        raw: mult.to_string(),
                    },
                }),
            },
        })
    }

    /// `bindStmt = "bind" ident "->" ident "(" bindArg* ")"`
    fn bind_stmt(&mut self) -> Option<Bind> {
        let start = self.bump().span; // bind
        let port = self.ident("the port name to bind")?;
        self.expect(TokKind::RArrow, "`->` then the peripheral name")?;
        let peripheral = self.ident("the peripheral name (e.g. `led`)")?;
        self.expect(TokKind::LParen, "`(` then the peripheral's config")?;
        let mut args = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokKind::RParen) {
                break;
            }
            let name = self.ident("a config key")?;
            self.expect(TokKind::Colon, "`:` then the value")?;
            let arg_start = self.peek().span;
            let value = match self.peek_kind().clone() {
                TokKind::Str(s) => {
                    self.bump();
                    BindArgValue::Str(s)
                }
                TokKind::Ident(_) => BindArgValue::Ident(self.ident("a config value")?.name),
                other => {
                    let found = kind_name(&other);
                    self.error(
                        arg_start,
                        "E1114",
                        format!("expected a config value, found {found}"),
                    );
                    return None;
                }
            };
            args.push(BindArg {
                name,
                value,
                span: self.span_since(arg_start),
            });
            self.skip_newlines();
            if !self.eat(&TokKind::Comma) {
                self.expect(TokKind::RParen, "`,` or `)`")?;
                break;
            }
        }
        self.terminator();
        Some(Bind {
            span: self.span_since(start),
            port,
            peripheral,
            args,
        })
    }
}
