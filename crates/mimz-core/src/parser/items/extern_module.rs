//! `extern module Name(params) { doc: "...", ports }` — Verilog FFI
//! declaration. Mirrors `module()`'s param-list parsing and `port()`'s
//! port-line parsing exactly (both reused verbatim); the body loop here
//! additionally accepts an optional `doc: STRING` line and otherwise only
//! `Port`/`Clock`/`Reset` items — anything else (a `wire`, `on` block,
//! etc.) has no body to belong to and is a parse error.

use super::super::*;

impl Parser {
    // ---------- extern modules ----------

    /// `externModule = "extern" "module" ident [ "=" string ]
    ///   [ "(" paramList ")" ] "{" [ "doc" ":" string ] { port | clock | reset } "}"`
    pub(super) fn extern_module(&mut self) -> Option<ExternModule> {
        let start = self.bump().span; // extern
        self.expect(TokKind::Kw(Kw::Module), "`module` after `extern`")?;
        let name = self.ident("an extern module name")?;
        let verilog_name = if self.eat(&TokKind::Assign) {
            if let TokKind::Str(s) = self.peek_kind().clone() {
                self.bump();
                Some(s)
            } else {
                let found = kind_name(self.peek_kind());
                let span = self.peek().span;
                self.error(
                    span,
                    "E1101",
                    format!(
                        "expected a string literal naming the real Verilog module after `=`, found {found}"
                    ),
                );
                return None;
            }
        } else {
            None
        };
        let mut params = Vec::new();
        if self.eat(&TokKind::LParen) {
            loop {
                self.skip_newlines();
                if self.eat(&TokKind::RParen) {
                    break;
                }
                let pname = self.ident("a parameter name")?;
                self.expect(TokKind::Colon, "`:` then `int` or `bool`")?;
                let ty = self.param_ty()?;
                let default = if self.eat(&TokKind::Assign) {
                    Some(self.expr()?)
                } else {
                    None
                };
                params.push(Param {
                    name: pname,
                    ty,
                    default,
                });
                self.skip_newlines();
                if !self.eat(&TokKind::Comma) {
                    self.expect(TokKind::RParen, "`,` or `)` in the parameter list")?;
                    break;
                }
            }
        }
        self.expect(TokKind::LBrace, "`{` to start the extern module body")?;
        self.skip_newlines();
        let doc = if matches!(self.peek_kind(), TokKind::Ident(s) if s == "doc") {
            self.bump();
            self.expect(TokKind::Colon, "`:` after `doc`")?;
            let d = if let TokKind::Str(s) = self.peek_kind().clone() {
                self.bump();
                s
            } else {
                let found = kind_name(self.peek_kind());
                let span = self.peek().span;
                self.error(
                    span,
                    "E1101",
                    format!("expected a string literal after `doc:`, found {found}"),
                );
                return None;
            };
            self.terminator();
            Some(d)
        } else {
            None
        };
        let mut items = Vec::new();
        let end = loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokKind::RBrace => break self.bump().span,
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1101",
                        format!("extern module `{}` is missing its closing `}}`", name.name),
                    );
                    break span;
                }
                TokKind::Kw(Kw::In) | TokKind::Kw(Kw::Out) => {
                    items.push(self.port()?);
                }
                TokKind::Kw(Kw::Clock) => {
                    self.bump();
                    let cname = self.ident("a clock name")?;
                    self.terminator();
                    items.push(ModuleItem::Clock(cname));
                }
                TokKind::Kw(Kw::Reset) => {
                    self.bump();
                    let rname = self.ident("a reset name")?;
                    self.terminator();
                    items.push(ModuleItem::Reset {
                        name: rname,
                        is_async: false,
                    });
                }
                other => {
                    let found = kind_name(other);
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1101",
                        format!(
                            "`extern module` only accepts `in`/`out`/`clock`/`reset` \
                             declarations — there is no body for `wire`/`reg`/`on`/etc. \
                             to belong to, found {found}"
                        ),
                    );
                    return None;
                }
            }
        };
        Some(ExternModule {
            name,
            verilog_name,
            params,
            doc,
            items,
            span: start.join(end),
        })
    }
}
