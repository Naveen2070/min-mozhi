use super::*;

impl Pretty {
    pub(super) fn file(&mut self, f: &File) {
        if self.order == Order::Thamizh {
            // `syntax thamizh` (in the target flavor) so the output re-parses
            // under the thamizh-order profile.
            let s = format!("{} {}", self.kw(Kw::Syntax), self.kw(Kw::Thamizh));
            self.line(&s);
            self.blank();
        }
        for imp in &f.imports {
            let path = imp
                .path
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join(".");
            let s = format!("{} {path}", self.kw(Kw::Import));
            self.line(&s);
        }
        if !f.imports.is_empty() && !f.items.is_empty() {
            self.blank();
        }
        for (i, item) in f.items.iter().enumerate() {
            if i > 0 {
                self.blank();
            }
            self.top_item(item);
        }
    }

    fn top_item(&mut self, item: &TopItem) {
        match item {
            TopItem::Const(c) => self.const_decl(c),
            TopItem::Module(m) => self.module(m),
            TopItem::Enum(e) => self.enum_decl(e),
            TopItem::Test(t) => self.test_decl(t),
            TopItem::Func(f) => self.func_decl(f),
            // Unreachable: pretty-printing runs on a strict-parsed tree, which
            // never carries an `Error` placeholder.
            TopItem::Error(_) => {}
            TopItem::Bundle(b) => self.bundle_decl(b),
            TopItem::ExternModule(em) => self.extern_module(em),
        }
    }

    fn const_decl(&mut self, c: &ConstDecl) {
        let s = format!(
            "{} {}: {} = {}",
            self.kw(Kw::Const),
            c.name.name,
            param_ty(c.ty),
            self.expr(&c.value, self.indent)
        );
        self.line(&s);
    }

    fn func_decl(&mut self, f: &FuncDecl) {
        let params = f
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name.name, self.ty(&p.ty, self.indent)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = self.ty(&f.ret, self.indent);
        let head = format!("{} {}({params}) -> {ret} {{", self.kw(Kw::Fn), f.name.name);
        self.line(&head);
        self.indent += 1;
        for st in &f.stmts {
            self.fn_stmt(st);
        }
        let tail = self.expr(&f.tail, self.indent);
        self.line(&tail);
        self.indent -= 1;
        self.line("}");
    }

    /// Render one `fn`-body statement. Mirrors [`Self::seq_stmt`] — same
    /// shape, `return` instead of `<-`/`default`.
    fn fn_stmt(&mut self, st: &FnStmt) {
        let ind = self.indent;
        match st {
            FnStmt::Let(local) => {
                let v = self.expr(&local.value, ind);
                let s = format!("{} {} = {v}", self.kw(Kw::Let), local.name.name);
                self.line(&s);
            }
            FnStmt::If { cond, then, els } => {
                let if_kw = self.kw(Kw::If);
                let cond = self.operand(cond, ind);
                let head = match self.order {
                    Order::Code => format!("{if_kw} {cond} {{"),
                    Order::Thamizh => format!("{cond} {if_kw} {{"),
                };
                self.line(&head);
                self.indent += 1;
                for s in then {
                    self.fn_stmt(s);
                }
                self.indent -= 1;
                match els {
                    None => self.line("}"),
                    Some(else_body) => {
                        let s = format!("}} {} {{", self.kw(Kw::Else));
                        self.line(&s);
                        self.indent += 1;
                        for s in else_body {
                            self.fn_stmt(s);
                        }
                        self.indent -= 1;
                        self.line("}");
                    }
                }
            }
            FnStmt::Return(expr) => {
                let v = self.expr(expr, ind);
                let s = format!("{} {v}", self.kw(Kw::Return));
                self.line(&s);
            }
            FnStmt::Loop {
                var, lo, hi, body, ..
            } => {
                let kw = self.kw(Kw::Loop);
                let lo_s = self.expr(lo, ind);
                let hi_s = self.expr(hi, ind);
                let head = format!("{kw} {}: {lo_s}..{hi_s} {{", var.name);
                self.line(&head);
                self.indent += 1;
                for s in body {
                    self.fn_stmt(s);
                }
                self.indent -= 1;
                self.line("}");
            }
            FnStmt::ForEach {
                var, source, body, ..
            } => {
                let head = format!(
                    "{} {} {} {} {{",
                    self.kw(Kw::Foreach),
                    var.name,
                    self.kw(Kw::In),
                    self.foreach_source(source, ind)
                );
                self.line(&head);
                self.indent += 1;
                for s in body {
                    self.fn_stmt(s);
                }
                self.indent -= 1;
                self.line("}");
            }
            FnStmt::Error(_) => {} // parse-recovery placeholder; never reached on the codegen path
        }
    }

    fn enum_decl(&mut self, e: &EnumDecl) {
        let ind = self.indent;
        let variants = e
            .variants
            .iter()
            .map(|v| {
                if v.fields.is_empty() {
                    v.name.name.clone()
                } else {
                    let fields = v
                        .fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name.name, self.ty(&f.ty, ind)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}({fields})", v.name.name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let s = format!("{} {} {{ {variants} }}", self.kw(Kw::Enum), e.name.name);
        self.line(&s);
    }

    fn bundle_decl(&mut self, b: &BundleDecl) {
        let params = if b.params.is_empty() {
            String::new()
        } else {
            let ps = b
                .params
                .iter()
                .map(|p| self.param(p))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({ps})")
        };
        let head = format!("{} {}{params} {{", self.kw(Kw::Bundle), b.name.name);
        self.line(&head);
        self.indent += 1;
        for f in &b.fields {
            let s = format!("{}: {}", f.name.name, self.ty(&f.ty, self.indent));
            self.line(&s);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn module(&mut self, m: &Module) {
        let params = if m.params.is_empty() {
            String::new()
        } else {
            let ps = m
                .params
                .iter()
                .map(|p| self.param(p))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({ps})")
        };
        let head = format!("{} {}{params} {{", self.kw(Kw::Module), m.name.name);
        self.line(&head);
        self.indent += 1;
        for it in &m.items {
            self.module_item(it);
        }
        self.indent -= 1;
        self.line("}");
    }

    /// `extern module Name [= "RealName"] [(params)] { [doc: "..."] ports }`
    /// — reuses `module_item` verbatim for the body (it only ever contains
    /// `Port`/`Clock`/`Reset` items, all of which `module_item` already
    /// renders correctly).
    fn extern_module(&mut self, em: &ExternModule) {
        let alias = match &em.verilog_name {
            Some(v) => format!(" = {}", self.quote(v)),
            None => String::new(),
        };
        let params = if em.params.is_empty() {
            String::new()
        } else {
            let ps = em
                .params
                .iter()
                .map(|p| self.param(p))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({ps})")
        };
        let head = format!(
            "{} {} {}{alias}{params} {{",
            self.kw(Kw::Extern),
            self.kw(Kw::Module),
            em.name.name
        );
        self.line(&head);
        self.indent += 1;
        if let Some(doc) = &em.doc {
            let s = format!("doc: {}", self.quote(doc));
            self.line(&s);
        }
        for it in &em.items {
            self.module_item(it);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn param(&self, p: &Param) -> String {
        let base = format!("{}: {}", p.name.name, param_ty(p.ty));
        match &p.default {
            Some(d) => format!("{base} = {}", self.expr(d, self.indent)),
            None => base,
        }
    }

    fn module_item(&mut self, it: &ModuleItem) {
        let ind = self.indent;
        match it {
            ModuleItem::Port { dir, name, ty } => {
                let kw = match dir {
                    Dir::In => self.kw(Kw::In),
                    Dir::Out => self.kw(Kw::Out),
                };
                let s = format!("{kw} {}: {}", name.name, self.ty(ty, ind));
                self.line(&s);
            }
            ModuleItem::Clock(c) => {
                let s = format!("{} {}", self.kw(Kw::Clock), c.name);
                self.line(&s);
            }
            ModuleItem::Reset { name: r, is_async } => {
                let s = if *is_async {
                    format!("{} {} {}", self.kw(Kw::Async), self.kw(Kw::Reset), r.name)
                } else {
                    format!("{} {}", self.kw(Kw::Reset), r.name)
                };
                self.line(&s);
            }
            ModuleItem::Wire { name, ty, init } => {
                let s = format!(
                    "{} {}: {} = {}",
                    self.kw(Kw::Wire),
                    name.name,
                    self.ty(ty, ind),
                    self.expr(init, ind)
                );
                self.line(&s);
            }
            ModuleItem::Reg { name, ty, reset } => {
                let s = format!(
                    "{} {}: {} = {}",
                    self.kw(Kw::Reg),
                    name.name,
                    self.ty(ty, ind),
                    self.expr(reset, ind)
                );
                self.line(&s);
            }
            ModuleItem::Mem {
                name,
                ty,
                depth,
                init,
            } => {
                let s = format!(
                    "{} {}: {}[{}] = {}",
                    self.kw(Kw::Mem),
                    name.name,
                    self.ty(ty, ind),
                    self.expr(depth, ind),
                    self.expr(init, ind)
                );
                self.line(&s);
            }
            ModuleItem::Const(c) => self.const_decl(c),
            ModuleItem::Enum(e) => self.enum_decl(e),
            ModuleItem::Inst(inst) => self.inst(inst),
            ModuleItem::On(on) => self.on_block(on),
            ModuleItem::Drive { lhs, rhs } => {
                let s = format!("{} = {}", self.lvalue(lhs, ind), self.expr(rhs, ind));
                self.line(&s);
            }
            ModuleItem::Assert(a) => self.assert_stmt(a),
            ModuleItem::Repeat(r) => self.repeat(r),
            ModuleItem::ForEach(fe) => self.foreach(fe),
            ModuleItem::SyncLoop(sl) => self.sync_loop(sl),
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                let ind = self.indent;
                let head = format!(
                    "{} {} ({}) {{",
                    self.kw(Kw::Const),
                    self.kw(Kw::If),
                    self.expr(cond, ind)
                );
                self.line(&head);
                self.indent += 1;
                for it in then {
                    self.module_item(it);
                }
                self.indent -= 1;
                if let Some(el) = els {
                    self.line(&format!("}} {} {{", self.kw(Kw::Else)));
                    self.indent += 1;
                    for it in el {
                        self.module_item(it);
                    }
                    self.indent -= 1;
                }
                self.line("}");
            }
            ModuleItem::Error(_) => {} // unreachable on a strict-parsed tree
            ModuleItem::BundleDestructure { bindings, expr, .. } => {
                let bs = bindings
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let rhs = self.expr(expr, ind);
                let s = format!("{} {{ {bs} }} = {rhs}", self.kw(Kw::Let));
                self.line(&s);
            }
        }
    }

    fn inst(&mut self, inst: &Inst) {
        let ind = self.indent;
        let idx = match &inst.index {
            Some(e) => format!("[{}]", self.expr(e, ind)),
            None => String::new(),
        };
        // The parameter list is always parenthesized — `Module()` when empty —
        // because the parser requires `(` after the module name.
        let a = inst
            .args
            .iter()
            .map(|na| format!("{}: {}", na.name.name, self.expr(&na.value, ind)))
            .collect::<Vec<_>>()
            .join(", ");
        let args = format!("({a})");
        let conns = inst
            .conns
            .iter()
            .map(|c| format!("{}: {}", c.port.name, self.expr(&c.signal, ind)))
            .collect::<Vec<_>>()
            .join(", ");
        let conns = if conns.is_empty() {
            " {}".to_string()
        } else {
            format!(" {{ {conns} }}")
        };
        let s = format!(
            "{} {}{idx} = {}{args}{conns}",
            self.kw(Kw::Let),
            inst.name.name,
            inst.module.to_dotted()
        );
        self.line(&s);
    }

    /// `assert(cond)` / `assert(cond, "msg")` — shared by the module-item
    /// and seq-statement printers (`pretty/seq.rs` calls this too).
    pub(super) fn assert_stmt(&mut self, a: &AssertStmt) {
        let kw = self.kw(Kw::Assert);
        let cond = self.expr(&a.cond, self.indent);
        let s = match &a.msg {
            Some(m) => format!("{kw}({cond}, {})", self.quote(m)),
            None => format!("{kw}({cond})"),
        };
        self.line(&s);
    }

    fn repeat(&mut self, r: &Repeat) {
        let ind = self.indent;
        let head = format!(
            "{} {}: {}..{} {{",
            self.kw(Kw::Repeat),
            r.var.name,
            self.expr(&r.lo, ind),
            self.expr(&r.hi, ind)
        );
        self.line(&head);
        self.indent += 1;
        for it in &r.items {
            self.module_item(it);
        }
        self.indent -= 1;
        self.line("}");
    }

    /// Renders a `foreach` source clause: `lo..hi` for `Range`, or the bare
    /// element identifier for `Elements`. Shared by the module-item,
    /// seq-stmt, and fn-stmt `foreach` printers so the match isn't
    /// duplicated three times.
    pub(super) fn foreach_source(&self, source: &ForEachSource, ind: usize) -> String {
        match source {
            ForEachSource::Range { lo, hi } => {
                format!("{}..{}", self.expr(lo, ind), self.expr(hi, ind))
            }
            ForEachSource::Elements(id) => id.name.clone(),
        }
    }

    fn foreach(&mut self, fe: &ForEach) {
        let ind = self.indent;
        let head = format!(
            "{} {} {} {} {{",
            self.kw(Kw::Foreach),
            fe.var.name,
            self.kw(Kw::In),
            self.foreach_source(&fe.source, ind)
        );
        self.line(&head);
        self.indent += 1;
        for it in &fe.items {
            self.module_item(it);
        }
        self.indent -= 1;
        self.line("}");
    }

    /// `sync loop <name> on rise(clk) (var: lo..hi) -> result: ty = init { body }`.
    /// Grammar fixes this word order regardless of `self.order` (unlike the
    /// standalone `on` block, which reverses for Thamizh) — see
    /// `parser::items::sync_loop_block`'s doc comment.
    fn sync_loop(&mut self, sl: &SyncLoop) {
        let ind = self.indent;
        let edge = self.kw(match sl.edge {
            Edge::Rise => Kw::Rise,
            Edge::Fall => Kw::Fall,
        });
        let head = format!(
            "{} {} {} {} {edge}({}) ({}: {}..{}) -> {}: {} = {} {{",
            self.kw(Kw::Sync),
            self.kw(Kw::Loop),
            sl.name.name,
            self.kw(Kw::On),
            sl.clock.name,
            sl.var.name,
            self.expr(&sl.lo, ind),
            self.expr(&sl.hi, ind),
            sl.result_name.name,
            self.ty(&sl.result_ty, ind),
            self.expr(&sl.result_init, ind),
        );
        self.line(&head);
        self.indent += 1;
        for st in &sl.body {
            self.seq_stmt(st);
        }
        self.indent -= 1;
        self.line("}");
    }
}

fn param_ty(t: ParamTy) -> &'static str {
    match t {
        ParamTy::Int => "int",
        ParamTy::Bool => "bool",
    }
}
