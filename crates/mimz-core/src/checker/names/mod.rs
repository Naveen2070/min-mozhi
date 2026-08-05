//! Pass 3 — name resolution and module-structure rules.
//!
//! For every module: build the module scope (reporting E0003 in-module
//! duplicates and E0301 reg-without-reset), then walk every expression,
//! lvalue, instantiation, and type so that **every name points at a real
//! declaration** — signals, params, consts, enums/variants, modules,
//! instance ports. Test blocks only get their header checked (module +
//! params); body checking lands with the simulator (Phase 1.5).
//!
//! Width/driver/exhaustiveness rules are NOT here — they live in the
//! later passes (widths/drivers/clocks). This pass DOES own the
//! structure rules that need only names: reg-requires-reset (E0301) and
//! instantiation completeness (E0302 — every input connected exactly
//! once; clock/reset connect implicitly by name).

use std::collections::HashMap;

use crate::ast::{
    BundleDecl, Dir, EnumDecl, Expr, ExprKind, ForEachSource, Inst, LValue, Module, ModuleItem,
    Pattern, SeqStmt, TopItem, Type,
};

use super::Checker;
use super::consteval::{self, Env};

mod exprs;
mod funcs;
mod insts;
mod items;
mod resolve;

/// What a name in module scope is bound to. Carries the node where it
/// helps produce a better error (enums, instances). Shared with the
/// width pass (`widths.rs`), which reuses the scopes this pass builds.
#[derive(Clone, Copy)]
pub(super) enum Bind<'a> {
    In,
    Out,
    Wire,
    Reg,
    Mem,
    Clock,
    Reset,
    Param,
    Const,
    Enum(&'a EnumDecl),
    Inst(&'a Inst),
    #[expect(dead_code)]
    Bundle(&'a BundleDecl),
}

impl Bind<'_> {
    /// Human word for error messages ("`clk` is a clock — ...").
    pub(super) fn what(&self) -> &'static str {
        match self {
            Bind::In => "an input port",
            Bind::Out => "an output port",
            Bind::Wire => "a wire",
            Bind::Reg => "a reg",
            Bind::Mem => "a memory",
            Bind::Clock => "a clock",
            Bind::Reset => "a reset",
            Bind::Param => "a parameter",
            Bind::Const => "a constant",
            Bind::Enum(_) => "an enum",
            Bind::Inst(_) => "an instance",
            Bind::Bundle(_) => "a bundle",
        }
    }
}

/// One module's name table. Built here (pass 3), then stored on the
/// `Checker` so the width pass (pass 4) resolves against the same table
/// instead of rebuilding it.
pub(super) struct Scope<'a> {
    pub(super) names: HashMap<String, Bind<'a>>,
}

impl<'a> Checker<'a> {
    pub(super) fn resolve_names(&mut self) {
        let files = self.files;
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                match item {
                    TopItem::Module(m) => self.check_module(file, m),
                    TopItem::Test(t) => self.check_test(file, t),
                    TopItem::Const(_) => {} // earlier passes
                    TopItem::Bundle(b) => {
                        for field in &b.fields {
                            self.validate_bundle_field_type(file, &field.ty, field.span);
                        }
                    }
                    TopItem::Enum(e) => {
                        let env = self.file_consts[file].clone();
                        let sc = Scope {
                            names: HashMap::new(),
                        };
                        for v in &e.variants {
                            for field in &v.fields {
                                self.ty(file, &sc, &env, &field.ty);
                            }
                        }
                    }
                    TopItem::Error(_) => {} // parse-recovery placeholder
                    TopItem::Func(f) => self.check_func_names(file, f),
                    TopItem::ExternModule(_) => {} // full checking lands in Task 3
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
        // Build env BEFORE collect_decls so ConstIf conditions can be evaluated
        // during declaration scanning (spec D-CONSTIF-4: losing branch is fully discarded).
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

        self.collect_decls(file, &mut sc, &env, &m.items);

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

        self.walk_items(file, &sc, &mut env, &m.items);

        // Hand the scope to the width and driver passes, keyed by (file,
        // name) — same-named modules from different files are legal (spec/02
        // section 1.5b), so no "canonical owner" disambiguation is needed:
        // each module gets its own scope under its own file.
        self.scopes
            .insert((file, m.name.name.clone()), std::rc::Rc::new(sc));
    }

    /// Declarations, recursively through `repeat` and `const if` bodies (declaration
    /// order in a module is free; `repeat` instantiates arrays but the
    /// names are declared once; `const if` losing branch is fully discarded).
    fn collect_decls(
        &mut self,
        file: usize,
        sc: &mut Scope<'a>,
        env: &Env,
        items: &'a [ModuleItem],
    ) {
        for item in items {
            match item {
                ModuleItem::Port { dir, name, .. } => {
                    let bind = if *dir == Dir::In { Bind::In } else { Bind::Out };
                    self.declare(file, sc, name, bind);
                }
                ModuleItem::Clock(n) => self.declare(file, sc, n, Bind::Clock),
                ModuleItem::Reset { name: n, .. } => self.declare(file, sc, n, Bind::Reset),
                ModuleItem::Wire { name, init, .. } => {
                    self.declare(file, sc, name, Bind::Wire);
                    // `sync.pulse` namespaces 4 generated signals off its
                    // OWN wire's name — declare them here (same precedent
                    // as `SyncLoop` below) so a collision with a
                    // user-declared signal is caught as an ordinary E0003,
                    // before `ast::expand_sync_prims` ever runs.
                    if let ExprKind::Call {
                        func: crate::ast::Builtin::SyncPulse,
                        ..
                    } = &init.kind
                    {
                        let mk = |suffix: &str| crate::ast::Ident {
                            name: format!("__sync_{}_{suffix}", name.name),
                            span: name.span,
                        };
                        self.declare(file, sc, &mk("toggle"), Bind::Reg);
                        self.declare(file, sc, &mk("stage0"), Bind::Reg);
                        self.declare(file, sc, &mk("stage1"), Bind::Reg);
                        self.declare(file, sc, &mk("stage2"), Bind::Reg);
                    }
                }
                ModuleItem::Reg { name, .. } => self.declare(file, sc, name, Bind::Reg),
                ModuleItem::Mem { name, .. } => self.declare(file, sc, name, Bind::Mem),
                ModuleItem::Const(c) => self.declare(file, sc, &c.name, Bind::Const),
                ModuleItem::Enum(e) => self.declare(file, sc, &e.name, Bind::Enum(e)),
                ModuleItem::Inst(i) => self.declare(file, sc, &i.name, Bind::Inst(i)),
                ModuleItem::Repeat(r) => self.collect_decls(file, sc, env, &r.items),
                // Same "declared once, raw body" treatment as `Repeat` above —
                // this recurses into the RAW (unlowered) `fe.items`, not the
                // lowered `Repeat`, mirroring `Repeat`'s own comment: whatever
                // this foreach body's items directly declare gets picked up
                // once, without per-iteration substitution (substitution only
                // matters to elaboration/width checks, not to name collection).
                ModuleItem::ForEach(fe) => self.collect_decls(file, sc, env, &fe.items),
                ModuleItem::SyncLoop(sl) => {
                    // A sync loop namespaces 4 generated signals off its own
                    // name — declare them here so the existing E0003 check
                    // (in `declare`, below) catches a collision with a
                    // user-declared signal or another sync loop's generated
                    // names, same as any other declaration.
                    let mk = |suffix: &str| crate::ast::Ident {
                        name: format!("{}_{suffix}", sl.name.name),
                        span: sl.name.span,
                    };
                    self.declare(file, sc, &mk("start"), Bind::In);
                    self.declare(file, sc, &mk("done"), Bind::Out);
                    self.declare(file, sc, &mk("result"), Bind::Out);
                    self.declare(file, sc, &mk("running"), Bind::Out);
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val =
                        consteval::eval(cond, env).unwrap_or_else(|_| consteval::ConstVal::zero());
                    let branch = if !val.is_zero() {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.collect_decls(file, sc, env, branch);
                }
                ModuleItem::On(on) => {
                    // `sync.double_flop` namespaces 1 generated stage reg
                    // off its own `<-` target's name — same precedent as
                    // `sync.pulse` above / `SyncLoop` below.
                    for stmt in &on.body {
                        if let SeqStmt::Assign {
                            lhs,
                            rhs:
                                Expr {
                                    kind:
                                        ExprKind::Call {
                                            func: crate::ast::Builtin::SyncDoubleFlop,
                                            ..
                                        },
                                    ..
                                },
                        } = stmt
                        {
                            let hidden = crate::ast::Ident {
                                name: format!("__sync_{}_stage0", lhs.base.name),
                                span: lhs.base.span,
                            };
                            self.declare(file, sc, &hidden, Bind::Reg);
                        }
                    }
                }
                ModuleItem::Drive { .. } | ModuleItem::Error(_) => {}
                ModuleItem::BundleDestructure { .. } => {} // checker stub (T5)
                ModuleItem::Assert(_) => {}                // declares nothing
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

#[cfg(test)]
mod tests;
