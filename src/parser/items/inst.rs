//! Instance declarations: `let name = Module(params) { connections }`.

use super::super::*;

impl Parser {
    /// `inst = "let" ident [ "[" expr "]" ] "=" ident "(" [argList] ")"
    ///         [ "{" connList "}" ]`
    pub(super) fn inst(&mut self) -> Option<ModuleItem> {
        let start = self.bump().span; // let
        let name = self.ident("an instance name")?;
        let index = if self.eat(&TokKind::LBracket) {
            let e = self.expr()?;
            self.expect(TokKind::RBracket, "`]` after the instance index")?;
            Some(e)
        } else {
            None
        };
        self.expect(TokKind::Assign, "`=` then the module to instantiate")?;
        let module = self.qual_ident("a module name")?;
        self.expect(TokKind::LParen, "`(` for the parameter list (may be empty)")?;
        let mut args = Vec::new();
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
                self.expect(TokKind::RParen, "`,` or `)` in the parameter list")?;
                break;
            }
        }
        let mut conns = Vec::new();
        let mut end = self.toks[self.pos - 1].span;
        if self.eat(&TokKind::LBrace) {
            loop {
                self.skip_newlines();
                if let TokKind::RBrace = self.peek_kind() {
                    end = self.bump().span;
                    break;
                }
                let port = self.ident("a port name to connect")?;
                self.expect(TokKind::Colon, "`:` then the signal to connect")?;
                let signal = self.expr()?;
                conns.push(Conn { port, signal });
                self.skip_newlines();
                if !self.eat(&TokKind::Comma) {
                    self.skip_newlines();
                    if let Some(t) =
                        self.expect(TokKind::RBrace, "`,` or `}` in the connection list")
                    {
                        end = t.span;
                    }
                    break;
                }
            }
        }
        self.terminator();
        Some(ModuleItem::Inst(Inst {
            name,
            index,
            module,
            args,
            conns,
            span: start.join(end),
        }))
    }
}
