//! Parses file-level `fn` declarations into [`TopItem::Func`] nodes.
//!
//! A combinational function is pure (no registers, no clocks). The body is
//! zero or more `let` bindings followed by exactly one return expression.

use super::super::*;

impl Parser {
    /// `fnDecl = "fn" ident "(" [param {"," param}] ")" "->" type
    ///           "{" {localLet} expr "}"` (spec/02 section 5)
    /// `param = ident ":" type` ; `localLet = "let" ident "=" expr`
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

        let mut locals = Vec::new();
        loop {
            self.skip_newlines();
            if !self.at_kw(Kw::Let) {
                break;
            }
            let lstart = self.bump().span; // let
            let lname = self.ident("a local name")?;
            self.expect(TokKind::Assign, "`=` after the local name")?;
            let value = self.expr()?;
            let lend = value.span;
            locals.push(LocalLet {
                name: lname,
                value,
                span: lstart.join(lend),
            });
            self.skip_newlines();
        }

        let body = self.expr()?;
        self.skip_newlines();
        let end = self
            .expect(TokKind::RBrace, "`}` to close the function body")?
            .span;

        Some(TopItem::Func(FuncDecl {
            name,
            params,
            ret,
            locals,
            body,
            span: start.join(end),
        }))
    }
}
