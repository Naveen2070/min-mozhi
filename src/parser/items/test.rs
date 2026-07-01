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
}
