//! Instantiation resolution: bind a child's parameters for one
//! instantiation site (explicit args in the parent's env, defaults in
//! the child's), type `inst.output` reads, and width-check every
//! connection against the child's port types under THAT binding.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{Conn, Dir, Inst, ModuleItem, ModuleTarget};

use super::super::Checker;
use super::super::consteval::{self, Env};
use super::super::names::Scope;
use super::{Config, Ty, Wcx, same, show};

/// One instantiation, resolved: which module (real or extern), in which
/// file, with the child-side environment (file consts + parameter binding)
/// ready for evaluating the child's port types.
struct ChildBinding<'a> {
    file: usize,
    target: ModuleTarget<'a>,
    env: Env,
    binding: Vec<(String, i128)>,
}

/// The child's name scope, for resolving `Type::Named`/`Type::Bundle` port
/// types under `resolve_ty_silent`. Real modules always have one (built by
/// `resolve_names`/`check_module`); extern modules never get a scope entry
/// (they have no body to elaborate), but that's harmless — extern port
/// types are restricted to scalar `bit`/`bits[N]`/`signed[N]` (E1302), and
/// none of those arms ever consult `cx.sc`, so an empty scope is exact,
/// not an approximation.
fn child_scope<'a>(checker: &Checker<'a>, child: &ChildBinding<'a>) -> Option<Rc<Scope<'a>>> {
    match child.target {
        ModuleTarget::Real(_) => checker
            .scopes
            .get(&(child.file, child.target.name().name.clone()))
            .cloned(),
        ModuleTarget::Extern(_) => Some(Rc::new(Scope {
            names: HashMap::new(),
        })),
    }
}

impl<'a> Checker<'a> {
    /// The width of `inst.output` in the parent: the child's port type,
    /// evaluated under this instantiation's parameter binding. Resolution
    /// is silent — the child's own config check owns its errors.
    pub(super) fn inst_output_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        inst: &'a Inst,
        field: &'a crate::ast::Ident,
    ) -> Ty<'a> {
        let Some(child) = self.child_binding(cx, inst, false) else {
            return Ty::Unknown;
        };
        let Some(csc) = child_scope(self, &child) else {
            return Ty::Unknown;
        };
        for item in child.target.items() {
            if let ModuleItem::Port {
                dir: Dir::Out,
                name,
                ty,
            } = item
                && name.name == field.name
            {
                let mut ccx = Wcx {
                    file: child.file,
                    sc: csc,
                    env: child.env,
                    sigs: HashMap::new(),
                };
                return self.resolve_ty_silent(&mut ccx, ty);
            }
        }
        Ty::Unknown // E0104 already reported
    }

    /// Bind the child's parameters for one instantiation: explicit args
    /// evaluate in the PARENT's env; omitted ones take their defaults
    /// (child env, left to right). Returns the child's file, module, env
    /// (file consts + binding), and the binding itself.
    fn child_binding(
        &mut self,
        cx: &Wcx<'a>,
        inst: &'a Inst,
        report: bool,
    ) -> Option<ChildBinding<'a>> {
        // names.rs (pass 3) already resolved this instantiation —
        // `resolved_file` is unset only for the already-reported
        // ambiguous/unknown cases, where the sole candidate (`.first()`) is
        // the safe fallback (bare-and-unambiguous, or "nothing to find").
        let target_file = inst.module.resolved_file.get();
        let (cfile, target): (usize, ModuleTarget<'a>) =
            if let Some(candidates) = self.modules.get(&inst.module.name.name) {
                let &(f, m) = candidates
                    .iter()
                    .find(|&&(f, _)| Some(f) == target_file)
                    .or_else(|| candidates.first())?;
                (f, ModuleTarget::Real(m))
            } else {
                let candidates = self.externs.get(&inst.module.name.name)?;
                let &(f, em) = candidates
                    .iter()
                    .find(|&&(f, _)| Some(f) == target_file)
                    .or_else(|| candidates.first())?;
                (f, ModuleTarget::Extern(em))
            };
        let mut cenv = self.file_consts[cfile].clone();
        let mut binding = Vec::new();
        for p in target.params() {
            let arg = inst.args.iter().find(|a| a.name.name == p.name.name);
            let mut v = None;
            if let Some(arg) = arg {
                match consteval::eval(&arg.value, &cx.env) {
                    Ok(x) => v = Some(x),
                    Err(d) => {
                        if report {
                            self.diags.push(d.with_file(cx.file));
                        }
                    }
                }
            }
            if v.is_none()
                && let Some(d) = &p.default
            {
                v = consteval::eval(d, &cenv).ok();
            }
            let v = v?;
            cenv.insert(p.name.name.clone(), v);
            binding.push((p.name.name.clone(), v));
        }
        Some(ChildBinding {
            file: cfile,
            target,
            env: cenv,
            binding,
        })
    }

    /// Width-check one instantiation: every connection against the
    /// child's port type under THIS binding, then enqueue the child
    /// config so its internals are checked under it too.
    pub(super) fn check_inst_widths(
        &mut self,
        cx: &mut Wcx<'a>,
        inst: &'a Inst,
        found: &mut Vec<Config>,
    ) {
        let Some(child) = self.child_binding(cx, inst, true) else {
            return;
        };
        let csc = child_scope(self, &child);
        for Conn { port, signal } in &inst.conns {
            let mut expected = Ty::Unknown; // unknown/output ports: E0107 owns it
            for item in child.target.items() {
                match item {
                    ModuleItem::Port {
                        dir: Dir::In,
                        name,
                        ty,
                    } if name.name == port.name => {
                        if let Some(csc) = &csc {
                            let mut ccx = Wcx {
                                file: child.file,
                                sc: csc.clone(),
                                env: child.env.clone(),
                                sigs: HashMap::new(),
                            };
                            expected = self.resolve_ty_silent(&mut ccx, ty);
                        }
                    }
                    ModuleItem::Clock(n) if n.name == port.name => expected = Ty::Clock,
                    ModuleItem::Reset { name: n, .. } if n.name == port.name => {
                        expected = Ty::Reset
                    }
                    _ => {}
                }
            }
            match expected {
                Ty::Unknown => {
                    let _ = self.infer_ty(cx, signal);
                }
                t => {
                    let got = self.infer_ty(cx, signal);
                    if let (Ty::Unknown, _) | (_, Ty::Unknown) = (got, t) {
                        continue;
                    }
                    if let Ty::CtInt(v) = got {
                        self.fit(cx, signal.span, v, t);
                        continue;
                    }
                    if !same(&got, &t) {
                        self.err(
                            cx.file,
                            signal.span,
                            "E0401",
                            format!(
                                "`{}`'s port `{}` is {}, but this connection is {}",
                                child.target.name().name,
                                port.name,
                                show(&t),
                                show(&got)
                            ),
                            "widths must match exactly at module boundaries — \
                             `extend`/`trunc`/slice the signal, or change the \
                             parameter this width comes from",
                        );
                    }
                }
            }
        }
        found.push((child.file, child.target.name().name.clone(), child.binding));
    }
}
