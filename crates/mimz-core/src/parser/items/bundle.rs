//! Bundle declaration parser: `bundle Name(params) { fields }`.

use super::super::*;

impl Parser {
    /// `bundleDecl = "bundle" IDENT ["(" paramList ")"] "{" { fieldDecl } "}"`
    /// `fieldDecl  = IDENT ":" type ( "," | newline | implicit-before-"}" )`
    pub(super) fn bundle_decl(&mut self) -> Option<TopItem> {
        let start = self.bump().span; // bundle
        let name = self.ident("a bundle name")?;

        // Optional parameter list — same grammar as module params.
        let mut params = Vec::new();
        if self.eat(&TokKind::LParen) {
            loop {
                self.skip_newlines();
                if self.eat(&TokKind::RParen) {
                    break;
                }
                let pname = self.ident("a parameter name")?;
                self.expect(TokKind::Colon, "`:` then `int` or `bool`")?;
                let pty = self.param_ty()?;
                let default = if self.eat(&TokKind::Assign) {
                    Some(self.expr()?)
                } else {
                    None
                };
                params.push(Param {
                    name: pname,
                    ty: pty,
                    default,
                });
                self.skip_newlines();
                if !self.eat(&TokKind::Comma) {
                    self.expect(TokKind::RParen, "`,` or `)` in the parameter list")?;
                    break;
                }
            }
        }

        self.expect(TokKind::LBrace, "`{` to start the bundle body")?;
        let mut fields = Vec::new();
        let end = loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokKind::RBrace => break self.bump().span,
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1101",
                        format!("bundle `{}` is missing its closing `}}`", name.name),
                    );
                    break span;
                }
                _ => {
                    let fstart = self.peek().span;
                    let fname = self.ident("a field name")?;
                    self.expect(TokKind::Colon, "`:` then the field type")?;
                    let fty = self.ty()?;
                    let fend = self.peek().span;
                    // Accept comma OR newline as field separator (both are valid).
                    if !self.eat(&TokKind::Comma) {
                        self.terminator();
                    }
                    fields.push(FieldDecl {
                        name: fname,
                        ty: fty,
                        span: fstart.join(fend),
                    });
                }
            }
        };

        Some(TopItem::Bundle(BundleDecl {
            name,
            params,
            fields,
            span: start.join(end),
        }))
    }
}
