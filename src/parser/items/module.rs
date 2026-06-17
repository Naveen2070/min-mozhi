//! Module bodies: the `module` declaration, the per-item dispatch in a
//! module body, and `port` declarations.

use super::super::*;

impl Parser {
    // ---------- modules ----------

    /// `module = "module" ident [ "(" paramList ")" ] "{" { moduleItem } "}"`
    pub(super) fn module(&mut self) -> Option<Module> {
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
    pub(super) fn module_item(&mut self) -> Option<ModuleItem> {
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
            TokKind::Kw(Kw::Rise | Kw::Fall) if self.profile == Profile::Thamizh => {
                self.on_block_thamizh()
            }
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
}
