//! File-level items, module bodies, sequential (`on`) blocks, and `test`
//! blocks. Expressions live in `expr.rs`.
//!
//! Each routine carries its production from the EBNF grammar
//! (spec/02 section 5) as a doc comment — keep them in sync with the spec.

use super::*;

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
    pub(super) fn file(&mut self) -> File {
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
    fn const_decl(&mut self) -> Option<ConstDecl> {
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
    fn param_ty(&mut self) -> Option<ParamTy> {
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
    fn enum_decl(&mut self) -> Option<EnumDecl> {
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

    // ---------- modules ----------

    /// `module = "module" ident [ "(" paramList ")" ] "{" { moduleItem } "}"`
    fn module(&mut self) -> Option<Module> {
        let start = self.bump().span; // module
        let name = self.ident("a module name")?;
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
        self.expect(TokKind::LBrace, "`{` to start the module body")?;
        let mut items = Vec::new();
        let end = loop {
            self.skip_newlines();
            if let TokKind::RBrace = self.peek_kind() {
                break self.bump().span;
            }
            if let TokKind::Eof = self.peek_kind() {
                let span = self.peek().span;
                self.error(
                    span,
                    "E1101",
                    format!("module `{}` is missing its closing `}}`", name.name),
                );
                break span;
            }
            match self.module_item() {
                Some(item) => items.push(item),
                None => self.sync_to_newline(),
            }
        };
        Some(Module {
            name,
            params,
            items,
            span: start.join(end),
        })
    }

    /// One item in a module body, dispatched on the leading keyword.
    /// A leading identifier means a combinational drive (`lhs = rhs`) —
    /// `lhs <- rhs` outside an `on` block is caught here with a teaching
    /// error rather than a generic parse failure.
    fn module_item(&mut self) -> Option<ModuleItem> {
        match self.peek_kind().clone() {
            TokKind::Kw(Kw::In) | TokKind::Kw(Kw::Out) => self.port(),
            TokKind::Kw(Kw::Clock) => {
                self.bump();
                let name = self.ident("a clock name")?;
                self.terminator();
                Some(ModuleItem::Clock(name))
            }
            TokKind::Kw(Kw::Reset) => {
                self.bump();
                let name = self.ident("a reset name")?;
                self.terminator();
                Some(ModuleItem::Reset(name))
            }
            TokKind::Kw(Kw::Wire) => {
                self.bump();
                let name = self.ident("a wire name")?;
                self.expect(TokKind::Colon, "`:` then the wire's type")?;
                let ty = self.ty()?;
                self.expect(
                    TokKind::Assign,
                    "`=` — every wire is driven where it is declared",
                )?;
                let init = self.expr()?;
                self.terminator();
                Some(ModuleItem::Wire { name, ty, init })
            }
            TokKind::Kw(Kw::Reg) => {
                self.bump();
                let name = self.ident("a register name")?;
                self.expect(TokKind::Colon, "`:` then the register's type")?;
                let ty = self.ty()?;
                if !self.at(&TokKind::Assign) {
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1104",
                        format!("register `{}` has no reset value", name.name),
                    );
                    self.help(
                        "every reg declares its reset value: `reg name: type = 0` — no uninitialized state (spec/02 section 1.2)",
                    );
                    return None;
                }
                self.bump(); // =
                let reset = self.expr()?;
                self.terminator();
                Some(ModuleItem::Reg { name, ty, reset })
            }
            TokKind::Kw(Kw::Const) => Some(ModuleItem::Const(self.const_decl()?)),
            TokKind::Kw(Kw::Enum) => Some(ModuleItem::Enum(self.enum_decl()?)),
            TokKind::Kw(Kw::Let) => self.inst(),
            TokKind::Kw(Kw::On) => self.on_block(),
            TokKind::Kw(Kw::Rise) if self.profile == Profile::Thamizh => self.on_block_thamizh(),
            TokKind::Kw(Kw::Repeat) => self.repeat_block(),
            TokKind::Ident(_) => {
                let lhs = self.lvalue()?;
                if self.at(&TokKind::LArrow) {
                    let span = self.peek().span;
                    self.error(
                        span,
                        "E1105",
                        "`<-` is only for registers inside an `on` block",
                    );
                    self.help(
                        "combinational drives use `=`; sequential updates use `<-` inside `on rise(clk) { ... }` (spec/02 section 1.2)",
                    );
                    return None;
                }
                self.expect(TokKind::Assign, "`=` to drive this signal")?;
                let rhs = self.expr()?;
                self.terminator();
                Some(ModuleItem::Drive { lhs, rhs })
            }
            other => {
                let found = kind_name(&other);
                let span = self.peek().span;
                self.error(
                    span,
                    "E1101",
                    format!(
                        "expected a declaration or assignment in the module body, found {found}"
                    ),
                );
                None
            }
        }
    }

    /// `port = ("in" | "out") ident ":" type`
    fn port(&mut self) -> Option<ModuleItem> {
        let dir = if self.bump().is_kw(Kw::In) {
            Dir::In
        } else {
            Dir::Out
        };
        let name = self.ident("a port name")?;
        self.expect(TokKind::Colon, "`:` then the port's type")?;
        let ty = self.ty()?;
        self.terminator();
        Some(ModuleItem::Port { dir, name, ty })
    }

    /// `inst = "let" ident [ "[" expr "]" ] "=" ident "(" [argList] ")"
    ///         [ "{" connList "}" ]`
    fn inst(&mut self) -> Option<ModuleItem> {
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
        let module = self.ident("a module name")?;
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

    /// code-order: `onBlock = "on" "rise" "(" ident ")" seqBlock`
    fn on_block(&mut self) -> Option<ModuleItem> {
        let start = self.bump().span; // on
        self.expect_kw(Kw::Rise, "`rise` — Min-Mozhi v0.2 is rising-edge only")?;
        let clock = self.clock_edge_args()?;
        let (body, end) = self.seq_block()?;
        Some(ModuleItem::On(OnBlock {
            clock,
            body,
            span: start.join(end),
        }))
    }

    /// thamizh-order: `onBlock = "rise" "(" ident ")" "on" seqBlock` — the
    /// clocked block with the edge head leading and `on` (pothu) trailing
    /// (spec/04 section 3: `yetram(clk) pothu { }`). Produces the identical
    /// [`OnBlock`] AST as the code-order form.
    fn on_block_thamizh(&mut self) -> Option<ModuleItem> {
        let start = self.bump().span; // rise
        let clock = self.clock_edge_args()?;
        self.expect_kw(Kw::On, "`on` (pothu) after the clock edge in thamizh order")?;
        let (body, end) = self.seq_block()?;
        Some(ModuleItem::On(OnBlock {
            clock,
            body,
            span: start.join(end),
        }))
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
                _ => match self.seq_stmt() {
                    Some(s) => stmts.push(s),
                    None => self.sync_to_newline(),
                },
            }
        };
        Some((stmts, end))
    }

    /// `seqStmt = seqIf | lvalue "<-" expr` — using `=` on a register is
    /// caught here with a teaching error (the `=`/`<-` safety rule).
    fn seq_stmt(&mut self) -> Option<SeqStmt> {
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

    /// `lvalue = ident [ "[" expr [ ":" expr ] "]" ]`
    pub(super) fn lvalue(&mut self) -> Option<LValue> {
        let base = self.ident("a signal name")?;
        let mut span = base.span;
        let index = if self.eat(&TokKind::LBracket) {
            let first = self.expr()?;
            let second = if self.eat(&TokKind::Colon) {
                Some(self.expr()?)
            } else {
                None
            };
            let t = self.expect(TokKind::RBracket, "`]`")?;
            span = span.join(t.span);
            Some((first, second))
        } else {
            None
        };
        Some(LValue { base, index, span })
    }

    /// `type = "bit" | "bits" "[" expr "]" | "signed" "[" expr "]" | ident`
    /// — type names are contextual (identifiers), not keywords; anything
    /// unrecognized is assumed to be an enum name and resolved later.
    fn ty(&mut self) -> Option<Type> {
        let id = self.ident("a type — `bit`, `bits[N]`, `signed[N]`, or an enum name")?;
        match id.name.as_str() {
            "bit" => Some(Type::Bit),
            "bits" => {
                self.expect(TokKind::LBracket, "`[` — bit vectors are written `bits[N]`")?;
                let n = self.expr()?;
                self.expect(TokKind::RBracket, "`]` after the width")?;
                Some(Type::Bits(Box::new(n)))
            }
            "signed" => {
                self.expect(
                    TokKind::LBracket,
                    "`[` — signed vectors are written `signed[N]`",
                )?;
                let n = self.expr()?;
                self.expect(TokKind::RBracket, "`]` after the width")?;
                Some(Type::Signed(Box::new(n)))
            }
            _ => Some(Type::Named(id)),
        }
    }

    /// `repeatBlock = "repeat" ident ":" expr ".." expr "{" { moduleItem } "}"`
    fn repeat_block(&mut self) -> Option<ModuleItem> {
        let start = self.bump().span; // repeat
        let var = self.ident("a loop variable name")?;
        self.expect(TokKind::Colon, "`:` then the range, e.g. `repeat i: 0..8`")?;
        let lo = self.expr()?;
        self.expect(TokKind::DotDot, "`..` between the range bounds")?;
        let hi = self.expr()?;
        self.expect(TokKind::LBrace, "`{` to start the repeat body")?;
        let mut items = Vec::new();
        let end = loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokKind::RBrace => break self.bump().span,
                TokKind::Eof => {
                    let span = self.peek().span;
                    self.error(span, "E1101", "`repeat` block is missing its closing `}`");
                    break span;
                }
                _ => match self.module_item() {
                    Some(i) => items.push(i),
                    None => self.sync_to_newline(),
                },
            }
        };
        Some(ModuleItem::Repeat(Repeat {
            var,
            lo,
            hi,
            items,
            span: start.join(end),
        }))
    }

    // ---------- tests ----------

    /// `testDecl = "test" string "for" ident [ "(" argList ")" ] testBlock`
    fn test_decl(&mut self) -> Option<TestDecl> {
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
