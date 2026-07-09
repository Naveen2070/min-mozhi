//! File-level items: the `syntax` directive, the whole-file entry point,
//! and the `import` / `const` / `enum` declarations.

use super::super::*;

impl Parser {
    // ---------- file level ----------

    /// Optional leading `syntax <profile>` directive (spec/04). `syntax thamizh`
    /// selects the SOV/postpositional word order; no directive means the default
    /// `code-order`. The directive is consumed here and never enters the AST, so
    /// a thamizh-order file and its code-order twin parse to the same tree.
    /// `thamizh` is the only profile word — code-order is the no-directive
    /// default (there is no `syntax code` form).
    fn syntax_directive(&mut self) {
        self.skip_newlines();
        if !self.at_kw(Kw::Syntax) {
            return;
        }
        self.bump(); // syntax
        if self.eat_kw(Kw::Thamizh) {
            self.profile = Profile::Thamizh;
            self.terminator();
        } else {
            let found = kind_name(self.peek_kind());
            let span = self.peek().span;
            self.error(
                span,
                "E1112",
                format!("unknown syntax profile: expected `thamizh`, found {found}"),
            );
            self.help(
                "`syntax thamizh` selects natural Tamil word order; omit the directive for the default code order (spec/04)",
            );
            self.sync_to_newline();
        }
    }

    /// `file = { importDecl } { constDecl | module | enumDecl | testDecl }`
    ///
    /// The whole-file entry point. Never fails — a bad item records its
    /// error and recovery skips to the next line.
    pub(in crate::parser) fn file(&mut self) -> File {
        self.syntax_directive();
        let mut imports = Vec::new();
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            // Start of this item — sizes an `Error` placeholder if it fails to
            // parse (for `parse_recover`; `parse` discards the tree anyway).
            let start = self.peek().span;
            match self.peek_kind() {
                TokKind::Eof => break,
                TokKind::Kw(Kw::Import) => {
                    if let Some(i) = self.import_decl() {
                        imports.push(i);
                    }
                }
                TokKind::Kw(Kw::Const) => match self.const_decl() {
                    Some(c) => items.push(TopItem::Const(c)),
                    None => items.push(TopItem::Error(self.span_since(start))),
                },
                TokKind::Kw(Kw::Module) => match self.module() {
                    Some(m) => items.push(TopItem::Module(m)),
                    None => items.push(TopItem::Error(self.span_since(start))),
                },
                TokKind::Kw(Kw::Enum) => match self.enum_decl() {
                    Some(e) => items.push(TopItem::Enum(e)),
                    None => items.push(TopItem::Error(self.span_since(start))),
                },
                TokKind::Kw(Kw::Test) => match self.test_decl() {
                    Some(t) => items.push(TopItem::Test(t)),
                    None => items.push(TopItem::Error(self.span_since(start))),
                },
                TokKind::Kw(Kw::Fn) => match self.func_decl() {
                    Some(f) => items.push(f),
                    None => items.push(TopItem::Error(self.span_since(start))),
                },
                TokKind::Kw(Kw::Bundle) => match self.bundle_decl() {
                    Some(b) => items.push(b),
                    None => items.push(TopItem::Error(self.span_since(start))),
                },
                // thamizh order: a test header leads with the module under test,
                // so a bare identifier at file level starts `M(args) kaaga "…"
                // sodhanai { }`. The leading `ident()` always bumps, so the loop
                // makes progress even if the rest of the header is malformed.
                TokKind::Ident(_) if self.profile == Profile::Thamizh => {
                    match self.test_decl_thamizh() {
                        Some(t) => items.push(TopItem::Test(t)),
                        None => items.push(TopItem::Error(self.span_since(start))),
                    }
                }
                _ => {
                    let found = kind_name(self.peek_kind());
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1102",
                        format!("expected `module`, `import`, `const`, `enum`, `fn`, `bundle`, or `test` at file level, found {found}"),
                    );
                    // Always make progress. `sync_to_newline` STOPS at `}`
                    // without consuming it (it is a block terminator inside
                    // items) — but a `}` here is stray (e.g. unbalanced braces
                    // left by error recovery inside a module). Bump it directly,
                    // or the loop spins forever re-reporting the same token.
                    if matches!(self.peek_kind(), TokKind::RBrace) {
                        self.bump();
                    } else {
                        self.sync_to_newline();
                    }
                    items.push(TopItem::Error(self.span_since(start)));
                }
            }
        }
        File { imports, items }
    }

    /// `importDecl = ( "import" | "include" ) ident { "." ident }`
    ///
    /// `include` is an English alias of `import` (keywords.toml) — both lex
    /// to the same `Kw::Import` token, so this routine never sees the
    /// difference.
    fn import_decl(&mut self) -> Option<Import> {
        let start = self.bump().span; // import / include
        let mut path = vec![self.ident("a file name to import")?];
        while self.eat(&TokKind::Dot) {
            path.push(self.ident("a path segment after `.`")?);
        }
        let span = start.join(path.last().unwrap().span);
        self.terminator();
        Some(Import {
            path,
            span,
            resolved_file: std::cell::Cell::new(None),
        })
    }

    /// `constDecl = "const" ident ":" paramTy "=" expr`
    pub(super) fn const_decl(&mut self) -> Option<ConstDecl> {
        self.bump(); // const
        let name = self.ident("a constant name")?;
        self.expect(TokKind::Colon, "`:` then `int` or `bool`")?;
        let ty = self.param_ty()?;
        self.expect(TokKind::Assign, "`=` and a compile-time value")?;
        let value = self.expr()?;
        self.terminator();
        Some(ConstDecl { name, ty, value })
    }

    /// `paramTy = "int" | "bool"` — these are contextual names, not
    /// keywords, so they arrive as identifiers.
    pub(super) fn param_ty(&mut self) -> Option<ParamTy> {
        let id = self.ident("`int` or `bool`")?;
        match id.name.as_str() {
            "int" => Some(ParamTy::Int),
            "bool" => Some(ParamTy::Bool),
            other => {
                self.error(
                    id.span,
                    "E1111",
                    format!(
                        "parameters and constants are compile-time `int` or `bool`, not `{other}`"
                    ),
                );
                None
            }
        }
    }

    /// `enumDecl = "enum" ident "{" enumVariant { "," enumVariant } [","] "}"`
    /// `enumVariant = ident` (tag-only; payload parsing is added in T2)
    pub(super) fn enum_decl(&mut self) -> Option<EnumDecl> {
        let start = self.bump().span; // enum
        let name = self.ident("an enum name")?;
        self.expect(TokKind::LBrace, "`{` to start the variant list")?;
        let mut variants = Vec::new();
        let end = loop {
            self.skip_newlines();
            if let TokKind::RBrace = self.peek_kind() {
                break self.bump().span;
            }
            let vname = self.ident("a variant name")?;
            let vstart = vname.span;
            let mut fields = vec![];
            let vspan = if self.eat(&TokKind::LParen) {
                // Reject `A()` — tag-only variants have no parens (D1).
                if matches!(self.peek_kind(), TokKind::RParen) {
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1113",
                        format!(
                            "empty payload list — use `{}` not `{}()` for a tag-only variant",
                            vname.name, vname.name
                        ),
                    );
                    return None;
                }
                loop {
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokKind::RParen) {
                        break;
                    }
                    let fname = self.ident("a field name")?;
                    let fstart = fname.span;
                    self.expect(TokKind::Colon, "`:` after payload field name")?;
                    let fty = self.ty()?;
                    let fspan = self.span_since(fstart);
                    fields.push(PayloadField {
                        name: fname,
                        ty: fty,
                        span: fspan,
                    });
                    self.skip_newlines();
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                }
                let rparen = self
                    .expect(TokKind::RParen, "`)` to close payload fields")?
                    .span;
                vstart.join(rparen)
            } else {
                vstart
            };
            variants.push(EnumVariant {
                name: vname,
                fields,
                span: vspan,
            });
            self.skip_newlines();
            if !self.eat(&TokKind::Comma) {
                self.skip_newlines();
                let end = self.peek().span;
                self.expect(TokKind::RBrace, "`,` or `}` in the variant list")?;
                break end;
            }
        };
        if variants.is_empty() {
            self.error(name.span, "E1103", "an enum needs at least one variant");
            return None;
        }
        Some(EnumDecl {
            name,
            variants,
            span: start.join(end),
            inferred_total_width: std::cell::Cell::new(None),
        })
    }
}
