//! Pass 3 — name resolution and module-structure rules.
//!
//! For every module: build the module scope (reporting E0003 in-module
//! duplicates and E0301 reg-without-reset), then walk every expression,
//! lvalue, instantiation, and type so that **every name points at a real
//! declaration** — signals, params, consts, enums/variants, modules,
//! instance ports. Test blocks only get their header checked (module +
//! params); body checking lands with the simulator (Phase 1.5).
//!
//! Width/driver/exhaustiveness rules are NOT here — they are the next
//! checker slices (docs/code/11-checker.md lists what is deferred).

use std::collections::HashMap;

use crate::ast::{
    Conn, Dir, EnumDecl, Expr, ExprKind, Inst, LValue, Module, ModuleItem, NamedArg, Pattern,
    SeqStmt, TestDecl, TopItem, Type,
};

use super::Checker;
use super::consteval::{self, Env};

/// What a name in module scope is bound to. Carries the node where it
/// helps produce a better error (enums, instances).
#[derive(Clone, Copy)]
enum Bind<'a> {
    In,
    Out,
    Wire,
    Reg,
    Clock,
    Reset,
    Param,
    Const,
    Enum(&'a EnumDecl),
    Inst(&'a Inst),
}

impl Bind<'_> {
    /// Human word for error messages ("`clk` is a clock — ...").
    fn what(&self) -> &'static str {
        match self {
            Bind::In => "an input port",
            Bind::Out => "an output port",
            Bind::Wire => "a wire",
            Bind::Reg => "a reg",
            Bind::Clock => "a clock",
            Bind::Reset => "a reset",
            Bind::Param => "a parameter",
            Bind::Const => "a constant",
            Bind::Enum(_) => "an enum",
            Bind::Inst(_) => "an instance",
        }
    }
}

struct Scope<'a> {
    names: HashMap<String, Bind<'a>>,
}

impl<'a> Checker<'a> {
    pub(super) fn resolve_names(&mut self) {
        let files = self.files;
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                match item {
                    TopItem::Module(m) => self.check_module(file, m),
                    TopItem::Test(t) => self.check_test(file, t),
                    TopItem::Const(_) | TopItem::Enum(_) => {} // earlier passes
                }
            }
        }
    }

    fn check_module(&mut self, file: usize, m: &'a Module) {
        let mut sc = Scope {
            names: HashMap::new(),
        };
        for p in &m.params {
            self.declare(file, &mut sc, &p.name, Bind::Param);
        }
        self.collect_decls(file, &mut sc, &m.items);

        // E0301 — registers load their reset value on reset, so a module
        // with regs and no `reset` line has unreachable initialization.
        let has_reg = sc.names.values().any(|b| matches!(b, Bind::Reg));
        let has_reset = sc.names.values().any(|b| matches!(b, Bind::Reset));
        if has_reg && !has_reset {
            self.err(
                file,
                m.name.span,
                "E0301",
                format!("module `{}` has registers but no `reset`", m.name.name),
                "every reg declares a reset value, and that value is loaded when \
                 reset is asserted — add a `reset rst` line (spec/02 section 1.2)",
            );
        }

        // Environment for const positions: file consts + module consts.
        let mut env = self.file_consts[file].clone();
        for item in &m.items {
            if let ModuleItem::Const(c) = item {
                match consteval::eval(&c.value, &env) {
                    Ok(v) => {
                        env.insert(c.name.name.clone(), v);
                    }
                    Err(d) => self.diags.push(d.with_file(file)),
                }
            }
        }

        self.walk_items(file, &sc, &mut env, &m.items);
    }

    /// Declarations, recursively through `repeat` bodies (declaration
    /// order in a module is free; `repeat` instantiates arrays but the
    /// names are declared once).
    fn collect_decls(&mut self, file: usize, sc: &mut Scope<'a>, items: &'a [ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Port { dir, name, .. } => {
                    let bind = if *dir == Dir::In { Bind::In } else { Bind::Out };
                    self.declare(file, sc, name, bind);
                }
                ModuleItem::Clock(n) => self.declare(file, sc, n, Bind::Clock),
                ModuleItem::Reset(n) => self.declare(file, sc, n, Bind::Reset),
                ModuleItem::Wire { name, .. } => self.declare(file, sc, name, Bind::Wire),
                ModuleItem::Reg { name, .. } => self.declare(file, sc, name, Bind::Reg),
                ModuleItem::Const(c) => self.declare(file, sc, &c.name, Bind::Const),
                ModuleItem::Enum(e) => self.declare(file, sc, &e.name, Bind::Enum(e)),
                ModuleItem::Inst(i) => self.declare(file, sc, &i.name, Bind::Inst(i)),
                ModuleItem::Repeat(r) => self.collect_decls(file, sc, &r.items),
                ModuleItem::On(_) | ModuleItem::Drive { .. } => {}
            }
        }
    }

    fn declare(
        &mut self,
        file: usize,
        sc: &mut Scope<'a>,
        name: &crate::ast::Ident,
        bind: Bind<'a>,
    ) {
        if let Some(prev) = sc.names.get(&name.name) {
            let what = prev.what();
            self.err(
                file,
                name.span,
                "E0003",
                format!("`{}` is declared twice in this module", name.name),
                format!(
                    "there is already {what} named `{}` — pick a different name",
                    name.name
                ),
            );
        } else {
            sc.names.insert(name.name.clone(), bind);
        }
    }

    fn walk_items(&mut self, file: usize, sc: &Scope<'a>, env: &mut Env, items: &'a [ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Port { ty, .. } => self.ty(file, sc, env, ty),
                ModuleItem::Wire { ty, init, .. } => {
                    self.ty(file, sc, env, ty);
                    self.expr(file, sc, env, init);
                }
                ModuleItem::Reg { ty, reset, .. } => {
                    self.ty(file, sc, env, ty);
                    self.expr(file, sc, env, reset);
                }
                ModuleItem::Inst(i) => self.check_inst(file, sc, env, i),
                ModuleItem::On(on) => {
                    match sc.names.get(&on.clock.name) {
                        Some(Bind::Clock) => {}
                        Some(b) => {
                            let what = b.what();
                            self.err(
                                file,
                                on.clock.span,
                                "E0109",
                                format!("`{}` is {what}, not a clock", on.clock.name),
                                "`on rise(...)` takes a clock — declare one with \
                                 `clock clk` (spec/02 section 1.2)",
                            );
                        }
                        None => self.unknown(file, &on.clock.name, on.clock.span),
                    }
                    self.seq_stmts(file, sc, env, &on.body);
                }
                ModuleItem::Drive { lhs, rhs } => {
                    self.lvalue(file, sc, env, lhs);
                    self.expr(file, sc, env, rhs);
                }
                ModuleItem::Repeat(r) => {
                    let lo = self.const_pos(file, env, &r.lo);
                    self.const_pos(file, env, &r.hi);
                    // The loop variable is a compile-time int inside the
                    // body. Its per-iteration values matter only to
                    // elaboration (later slice) — names resolve the same
                    // for every iteration, so one walk with `lo` suffices.
                    let shadowed = env.insert(r.var.name.clone(), lo.unwrap_or(0));
                    self.walk_items(file, sc, env, &r.items);
                    match shadowed {
                        Some(v) => env.insert(r.var.name.clone(), v),
                        None => env.remove(&r.var.name),
                    };
                }
                ModuleItem::Clock(_)
                | ModuleItem::Reset(_)
                | ModuleItem::Const(_) // evaluated in check_module
                | ModuleItem::Enum(_) => {}
            }
        }
    }

    fn seq_stmts(&mut self, file: usize, sc: &Scope<'a>, env: &Env, stmts: &'a [SeqStmt]) {
        for s in stmts {
            match s {
                SeqStmt::Assign { lhs, rhs } => {
                    self.lvalue(file, sc, env, lhs);
                    self.expr(file, sc, env, rhs);
                }
                SeqStmt::If { cond, then, els } => {
                    self.expr(file, sc, env, cond);
                    self.seq_stmts(file, sc, env, then);
                    if let Some(els) = els {
                        self.seq_stmts(file, sc, env, els);
                    }
                }
            }
        }
    }

    /// A position that must const-evaluate today (`repeat` bounds).
    /// Returns the value if it did.
    fn const_pos(&mut self, file: usize, env: &Env, e: &Expr) -> Option<i128> {
        match consteval::eval(e, env) {
            Ok(v) => Some(v),
            Err(d) => {
                self.diags.push(d.with_file(file));
                None
            }
        }
    }

    fn ty(&mut self, file: usize, sc: &Scope<'a>, env: &Env, ty: &'a Type) {
        match ty {
            Type::Bit => {}
            Type::Bits(w) | Type::Signed(w) => self.expr(file, sc, env, w),
            Type::Named(n) => {
                if self.lookup_enum(sc, &n.name).is_none() {
                    self.err(
                        file,
                        n.span,
                        "E0103",
                        format!("unknown type `{}`", n.name),
                        format!(
                            "the only named types are enums — declare \
                             `enum {} {{ ... }}` or import the file that does",
                            n.name
                        ),
                    );
                }
            }
        }
    }

    /// Enum lookup: module scope first, then file-level enums project-wide.
    fn lookup_enum(&self, sc: &Scope<'a>, name: &str) -> Option<&'a EnumDecl> {
        if let Some(Bind::Enum(e)) = sc.names.get(name).copied() {
            return Some(e);
        }
        self.enums.get(name).map(|&(_, e)| e)
    }

    fn check_inst(&mut self, file: usize, sc: &Scope<'a>, env: &Env, inst: &'a Inst) {
        if let Some(idx) = &inst.index {
            self.expr(file, sc, env, idx);
        }
        let target = self.modules.get(&inst.module.name).map(|&(_, m)| m);
        let Some(target) = target else {
            self.err(
                file,
                inst.module.span,
                "E0102",
                format!("no module named `{}` in this project", inst.module.name),
                "check the spelling, or add the `import` that brings it in \
                 (spec/02 section 1.5)",
            );
            // Still resolve the argument/connection expressions.
            for NamedArg { value, .. } in &inst.args {
                self.expr(file, sc, env, value);
            }
            for Conn { signal, .. } in &inst.conns {
                self.expr(file, sc, env, signal);
            }
            return;
        };

        let params: Vec<&str> = target.params.iter().map(|p| p.name.name.as_str()).collect();
        for NamedArg { name, value } in &inst.args {
            if !params.contains(&name.name.as_str()) {
                let available = if params.is_empty() {
                    format!("`{}` takes no parameters", target.name.name)
                } else {
                    format!(
                        "`{}`'s parameters are: {}",
                        target.name.name,
                        params.join(", ")
                    )
                };
                self.err(
                    file,
                    name.span,
                    "E0106",
                    format!("`{}` has no parameter `{}`", target.name.name, name.name),
                    available,
                );
            }
            self.expr(file, sc, env, value);
        }

        let mut inputs: Vec<&str> = Vec::new();
        let mut outputs: Vec<&str> = Vec::new();
        for item in &target.items {
            match item {
                ModuleItem::Port {
                    dir: Dir::In, name, ..
                } => inputs.push(&name.name),
                ModuleItem::Port {
                    dir: Dir::Out,
                    name,
                    ..
                } => outputs.push(&name.name),
                ModuleItem::Clock(n) | ModuleItem::Reset(n) => inputs.push(&n.name),
                _ => {}
            }
        }
        for Conn { port, signal } in &inst.conns {
            if outputs.contains(&port.name.as_str()) {
                self.err(
                    file,
                    port.span,
                    "E0107",
                    format!("`{}` is an output of `{}`", port.name, target.name.name),
                    format!(
                        "outputs are not connected here — read them with \
                         `{}.{}` (spec/02 section 1.5)",
                        inst.name.name, port.name
                    ),
                );
            } else if !inputs.contains(&port.name.as_str()) {
                self.err(
                    file,
                    port.span,
                    "E0107",
                    format!("`{}` has no input named `{}`", target.name.name, port.name),
                    format!("`{}`'s inputs are: {}", target.name.name, inputs.join(", ")),
                );
            }
            self.expr(file, sc, env, signal);
        }
    }

    fn check_test(&mut self, file: usize, t: &'a TestDecl) {
        let target = self.modules.get(&t.module.name).map(|&(_, m)| m);
        let Some(target) = target else {
            self.err(
                file,
                t.module.span,
                "E0102",
                format!("no module named `{}` in this project", t.module.name),
                "check the spelling, or add the `import` that brings it in \
                 (spec/02 section 1.5)",
            );
            return;
        };
        let params: Vec<&str> = target.params.iter().map(|p| p.name.name.as_str()).collect();
        for NamedArg { name, .. } in &t.args {
            if !params.contains(&name.name.as_str()) {
                self.err(
                    file,
                    name.span,
                    "E0106",
                    format!("`{}` has no parameter `{}`", target.name.name, name.name),
                    "test headers set the module's compile-time parameters only",
                );
            }
        }
        // Test BODIES are checked when the simulator lands (Phase 1.5) —
        // they reference the module's ports, which needs port typing.
    }

    fn lvalue(&mut self, file: usize, sc: &Scope<'a>, env: &Env, lv: &'a LValue) {
        match sc.names.get(&lv.base.name) {
            Some(Bind::Out | Bind::Wire | Bind::Reg) => {}
            Some(Bind::In) => self.err(
                file,
                lv.base.span,
                "E0108",
                format!("cannot assign to input port `{}`", lv.base.name),
                "inputs are driven by the parent module — to compute a local \
                 value, declare a `wire`",
            ),
            Some(b) => {
                let what = b.what();
                self.err(
                    file,
                    lv.base.span,
                    "E0108",
                    format!("cannot assign to `{}` — it is {what}", lv.base.name),
                    "only output ports, wires (at their declaration), and regs \
                     (inside `on`) can be assigned",
                );
            }
            None if env.contains_key(&lv.base.name) => self.err(
                file,
                lv.base.span,
                "E0108",
                format!("cannot assign to `{}` — it is a constant", lv.base.name),
                "consts are compile-time values; use a `wire` or `reg` for \
                 something that varies",
            ),
            None => self.unknown(file, &lv.base.name, lv.base.span),
        }
        if let Some((i, hi)) = &lv.index {
            self.expr(file, sc, env, i);
            if let Some(hi) = hi {
                self.expr(file, sc, env, hi);
            }
        }
    }

    fn expr(&mut self, file: usize, sc: &Scope<'a>, env: &Env, e: &'a Expr) {
        match &e.kind {
            ExprKind::Int { .. } | ExprKind::Bool(_) => {}
            ExprKind::Ident(name) => {
                if !sc.names.contains_key(name) && !env.contains_key(name) {
                    self.unknown(file, name, e.span);
                }
            }
            ExprKind::Field { base, field } => {
                // `blink[i].out` — unwrap the instance-array index.
                let core = match &base.kind {
                    ExprKind::Index { base: b, index } if matches!(b.kind, ExprKind::Ident(_)) => {
                        self.expr(file, sc, env, index);
                        b
                    }
                    _ => base,
                };
                let ExprKind::Ident(name) = &core.kind else {
                    self.err(
                        file,
                        field.span,
                        "E0105",
                        "only enums and instances have fields",
                        "`.` reads an enum variant (`State.Red`) or an instance \
                         output (`add.sum`)",
                    );
                    self.expr(file, sc, env, base);
                    return;
                };
                match sc.names.get(name).copied() {
                    Some(Bind::Inst(inst)) => self.inst_output(file, inst, field),
                    Some(b)
                        if self.lookup_enum(sc, name).is_none() && !matches!(b, Bind::Enum(_)) =>
                    {
                        let what = b.what();
                        self.err(
                            file,
                            field.span,
                            "E0105",
                            format!("`{name}` is {what} — it has no fields"),
                            "`.` reads an enum variant (`State.Red`) or an \
                             instance output (`add.sum`)",
                        );
                    }
                    _ => match self.lookup_enum(sc, name) {
                        Some(en) => {
                            if !en.variants.iter().any(|v| v.name == field.name) {
                                let list: Vec<&str> =
                                    en.variants.iter().map(|v| v.name.as_str()).collect();
                                self.err(
                                    file,
                                    field.span,
                                    "E0103",
                                    format!("enum `{name}` has no variant `{}`", field.name),
                                    format!("`{name}`'s variants are: {}", list.join(", ")),
                                );
                            }
                        }
                        None => self.unknown(file, name, core.span),
                    },
                }
            }
            ExprKind::Unary { expr, .. } => self.expr(file, sc, env, expr),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(file, sc, env, lhs);
                self.expr(file, sc, env, rhs);
            }
            ExprKind::IfExpr { cond, then, els } => {
                self.expr(file, sc, env, cond);
                self.expr(file, sc, env, then);
                self.expr(file, sc, env, els);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(file, sc, env, scrutinee);
                for arm in arms {
                    for p in &arm.patterns {
                        if let Pattern::Variant { enum_name, variant } = p {
                            match self.lookup_enum(sc, &enum_name.name) {
                                Some(en) => {
                                    if !en.variants.iter().any(|v| v.name == variant.name) {
                                        let list: Vec<&str> =
                                            en.variants.iter().map(|v| v.name.as_str()).collect();
                                        self.err(
                                            file,
                                            variant.span,
                                            "E0103",
                                            format!(
                                                "enum `{}` has no variant `{}`",
                                                enum_name.name, variant.name
                                            ),
                                            format!(
                                                "`{}`'s variants are: {}",
                                                enum_name.name,
                                                list.join(", ")
                                            ),
                                        );
                                    }
                                }
                                None => self.unknown(file, &enum_name.name, enum_name.span),
                            }
                        }
                    }
                    self.expr(file, sc, env, &arm.value);
                }
            }
            ExprKind::Concat(parts) => {
                for p in parts {
                    self.expr(file, sc, env, p);
                }
            }
            ExprKind::Index { base, index } => {
                self.expr(file, sc, env, base);
                self.expr(file, sc, env, index);
            }
            ExprKind::Slice { base, hi, lo } => {
                self.expr(file, sc, env, base);
                self.expr(file, sc, env, hi);
                self.expr(file, sc, env, lo);
            }
            ExprKind::Call { args, .. } => {
                for a in args {
                    self.expr(file, sc, env, a);
                }
            }
        }
    }

    /// `inst.field` — the field must be an OUTPUT port of the target
    /// module (inputs are connected at instantiation, not read back).
    fn inst_output(&mut self, file: usize, inst: &'a Inst, field: &crate::ast::Ident) {
        let Some(target) = self.modules.get(&inst.module.name).map(|&(_, m)| m) else {
            return; // unknown module already reported at the `let`
        };
        let mut outputs: Vec<&str> = Vec::new();
        let mut is_input = false;
        for item in &target.items {
            match item {
                ModuleItem::Port {
                    dir: Dir::Out,
                    name,
                    ..
                } => outputs.push(&name.name),
                ModuleItem::Port {
                    dir: Dir::In, name, ..
                } => {
                    is_input |= name.name == field.name;
                }
                _ => {}
            }
        }
        if outputs.contains(&field.name.as_str()) {
            return;
        }
        let help = if is_input {
            format!(
                "`{}` is an input of `{}` — inputs are connected at the `let`, \
                 only outputs are read with `.`",
                field.name, target.name.name
            )
        } else if outputs.is_empty() {
            format!("`{}` has no outputs", target.name.name)
        } else {
            format!(
                "`{}`'s outputs are: {}",
                target.name.name,
                outputs.join(", ")
            )
        };
        self.err(
            file,
            field.span,
            "E0104",
            format!(
                "`{}` has no output named `{}`",
                target.name.name, field.name
            ),
            help,
        );
    }

    fn unknown(&mut self, file: usize, name: &str, span: crate::span::Span) {
        self.err(
            file,
            span,
            "E0101",
            format!("unknown name `{name}`"),
            "nothing with this name is declared in this module — check the \
             spelling, or declare it as a port, wire, reg, or const",
        );
    }
}
