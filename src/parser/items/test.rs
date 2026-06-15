//! `test` blocks: the `test … for …` declaration, the test body
//! (`tick` / `expect` / drive / `if`), and the test-level `if`.

use super::super::*;

impl Parser {
    // ---------- tests ----------

    /// `testDecl = "test" string "for" ident [ "(" argList ")" ] testBlock`
    pub(super) fn test_decl(&mut self) -> Option<TestDecl> {
        let start = self.bump().span; // test
        let name = if let TokKind::Str(s) = self.peek_kind().clone() {
            self.bump();
            s
        } else {
            let span = self.peek().span;
            self.error(
                span,
                "E1107",
                "expected a test name in quotes: `test \"counter counts\" for ...`",
            );
            return None;
        };
        self.expect_kw(Kw::For, "`for` then the module under test")?;
        let module = self.ident("the module under test")?;
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
        let (body, end) = self.test_block()?;
        Some(TestDecl {
            name,
            module,
            args,
            body,
            span: start.join(end),
        })
    }

    /// `testBlock = "{" { tick | expect | drive | testIf } "}"`
    fn test_block(&mut self) -> Option<(Vec<TestStmt>, Span)> {
        self.expect(TokKind::LBrace, "`{` to start the test body")?;
        let mut stmts = Vec::new();
        let end = loop {
            self.skip_newlines();
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
