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
            match self.peek_kind() {
                TokKind::Eof => break,
                TokKind::Kw(Kw::Import) => {
                    if let Some(i) = self.import_decl() {
                        imports.push(i);
                    }
                }
                TokKind::Kw(Kw::Const) => {
                    if let Some(c) = self.const_decl() {
                        items.push(TopItem::Const(c));
                    }
                }
                TokKind::Kw(Kw::Module) => {
                    if let Some(m) = self.module() {
                        items.push(TopItem::Module(m));
                    }
                }
                TokKind::Kw(Kw::Enum) => {
                    if let Some(e) = self.enum_decl() {
                        items.push(TopItem::Enum(e));
                    }
                }
                TokKind::Kw(Kw::Test) => {
                    if let Some(t) = self.test_decl() {
                        items.push(TopItem::Test(t));
                    }
                }
                // thamizh order: a test header leads with the module under test,
                // so a bare identifier at file level starts `M(args) kaaga "…"
                // sodhanai { }`. The leading `ident()` always bumps, so the loop
                // makes progress even if the rest of the header is malformed.
                TokKind::Ident(_) if self.profile == Profile::Thamizh => {
                    if let Some(t) = self.test_decl_thamizh() {
                        items.push(TopItem::Test(t));
                    }
                }
                _ => {
                    let found = kind_name(self.peek_kind());
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1102",
                        format!("expected `module`, `import`, `const`, `enum`, or `test` at file level, found {found}"),
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
        Some(Import { path, span })
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

    /// `enumDecl = "enum" ident "{" ident { "," ident } [","] "}"`
    pub(super) fn enum_decl(&mut self) -> Option<EnumDecl> {
        self.bump(); // enum
        let name = self.ident("an enum name")?;
        self.expect(TokKind::LBrace, "`{` to start the variant list")?;
        let mut variants = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&TokKind::RBrace) {
                break;
            }
            variants.push(self.ident("a variant name")?);
            self.skip_newlines();
            if !self.eat(&TokKind::Comma) {
                self.skip_newlines();
                self.expect(TokKind::RBrace, "`,` or `}` in the variant list")?;
                break;
            }
        }
        if variants.is_empty() {
            self.error(name.span, "E1103", "an enum needs at least one variant");
            return None;
        }
        Some(EnumDecl { name, variants })
    }
}
