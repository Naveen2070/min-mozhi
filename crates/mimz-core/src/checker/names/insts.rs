use super::*;
use crate::ast::{Conn, ModuleTarget, NamedArg, TestDecl};

impl<'a> Checker<'a> {
    pub(super) fn check_inst(&mut self, file: usize, sc: &Scope<'a>, env: &Env, inst: &Inst) {
        if let Some(idx) = &inst.index {
            self.expr(file, sc, env, idx);
        }
        // Try a real module first, then an extern declaration — only report
        // "no module named" (E0102) if BOTH buckets are empty. If a bucket
        // is non-empty but ambiguous/unqualified-mismatch, `self.resolve`
        // already reports E0110/E0111 itself and returns `None` — reporting
        // E0102 on top of that would double-report, so those branches pass
        // a no-op `unknown` closure and let `resolve`'s internal reporting
        // stand alone.
        let module_candidates = self.modules.get(&inst.module.name.name).cloned();
        let extern_candidates = self.externs.get(&inst.module.name.name).cloned();
        let target: Option<ModuleTarget> = if let Some(candidates) = module_candidates {
            self.resolve(file, Some(candidates), &inst.module, |_| {})
                .map(ModuleTarget::Real)
        } else if let Some(candidates) = extern_candidates {
            self.resolve(file, Some(candidates), &inst.module, |_| {})
                .map(ModuleTarget::Extern)
        } else {
            self.err(
                file,
                inst.module.span,
                "E0102",
                format!(
                    "no module named `{}` in this project",
                    inst.module.name.name
                ),
                "check the spelling, or add the `import` that brings it in \
                 (spec/02 section 1.5)",
            );
            None
        };
        let Some(target) = target else {
            // Still resolve the argument/connection expressions.
            for NamedArg { value, .. } in &inst.args {
                self.expr(file, sc, env, value);
            }
            for Conn { signal, .. } in &inst.conns {
                self.expr(file, sc, env, signal);
            }
            return;
        };

        let params: Vec<&str> = target
            .params()
            .iter()
            .map(|p| p.name.name.as_str())
            .collect();
        for NamedArg { name, value } in &inst.args {
            if !params.contains(&name.name.as_str()) {
                let available = if params.is_empty() {
                    format!("`{}` takes no parameters", target.name().name)
                } else {
                    format!(
                        "`{}`'s parameters are: {}",
                        target.name().name,
                        params.join(", ")
                    )
                };
                self.err(
                    file,
                    name.span,
                    "E0106",
                    format!("`{}` has no parameter `{}`", target.name().name, name.name),
                    available,
                );
            }
            self.expr(file, sc, env, value);
        }

        // Data inputs must each be connected exactly once (E0302);
        // clock/reset ports may be omitted — they connect implicitly by
        // name (the emitter's rule, spec/02 section 1.5).
        let mut ins: Vec<&str> = Vec::new();
        let mut implicit: Vec<&str> = Vec::new();
        let mut outputs: Vec<&str> = Vec::new();
        for item in target.items() {
            match item {
                ModuleItem::Port {
                    dir: Dir::In, name, ..
                } => ins.push(&name.name),
                ModuleItem::Port {
                    dir: Dir::Out,
                    name,
                    ..
                } => outputs.push(&name.name),
                ModuleItem::Clock(n) | ModuleItem::Reset { name: n, .. } => implicit.push(&n.name),
                _ => {}
            }
        }
        let mut connected: Vec<&str> = Vec::new();
        for Conn { port, signal } in &inst.conns {
            if outputs.contains(&port.name.as_str()) {
                self.err(
                    file,
                    port.span,
                    "E0107",
                    format!("`{}` is an output of `{}`", port.name, target.name().name),
                    format!(
                        "outputs are not connected here — read them with \
                         `{}.{}` (spec/02 section 1.5)",
                        inst.name.name, port.name
                    ),
                );
            } else if !ins.contains(&port.name.as_str()) && !implicit.contains(&port.name.as_str())
            {
                let mut all = ins.clone();
                all.extend(&implicit);
                self.err(
                    file,
                    port.span,
                    "E0107",
                    format!(
                        "`{}` has no input named `{}`",
                        target.name().name,
                        port.name
                    ),
                    format!("`{}`'s inputs are: {}", target.name().name, all.join(", ")),
                );
            } else if connected.contains(&port.name.as_str()) {
                self.err(
                    file,
                    port.span,
                    "E0302",
                    format!("input `{}` is connected twice", port.name),
                    "every input is connected exactly once — delete the \
                     duplicate connection",
                );
            } else {
                connected.push(&port.name);
            }
            self.expr(file, sc, env, signal);
        }
        let missing: Vec<&str> = ins
            .iter()
            .copied()
            .filter(|i| !connected.contains(i))
            .collect();
        if !missing.is_empty() {
            self.err(
                file,
                inst.name.span,
                "E0302",
                format!(
                    "`{}` leaves input{} `{}` unconnected",
                    target.name().name,
                    if missing.len() == 1 { "" } else { "s" },
                    missing.join("`, `")
                ),
                "every input of an instance must be connected — hardware \
                 has no default arguments (clock/reset connect implicitly \
                 by name and may be omitted)",
            );
        }
    }

    pub(super) fn check_test(&mut self, file: usize, t: &'a TestDecl) {
        let candidates = self.modules.get(&t.module.name.name).cloned();
        let target = self.resolve(file, candidates, &t.module, |ck| {
            ck.err(
                file,
                t.module.span,
                "E0102",
                format!("no module named `{}` in this project", t.module.name.name),
                "check the spelling, or add the `import` that brings it in \
                 (spec/02 section 1.5)",
            );
        });
        let Some(target) = target else {
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

    /// `inst.field` — the field must be an OUTPUT port of the target
    /// module (inputs are connected at instantiation, not read back).
    pub(super) fn inst_output(&mut self, file: usize, inst: &'a Inst, field: &crate::ast::Ident) {
        // Item order within a module is free (`collect_decls`), so an
        // earlier item may reference this instance's output before
        // `check_inst` has run for it — `resolved_file` may still be
        // unset. Re-resolve independently rather than depending on that
        // ordering, but discard any diagnostics the probe pushes: if
        // `check_inst` already ran (and already reported E0102/E0110/E0111
        // for this same `inst.module`), we'd otherwise double-report.
        let before = self.diags.len();
        let module_candidates = self.modules.get(&inst.module.name.name).cloned();
        let target: Option<ModuleTarget> = if module_candidates.is_some() {
            self.resolve(file, module_candidates, &inst.module, |_| {})
                .map(ModuleTarget::Real)
        } else {
            let extern_candidates = self.externs.get(&inst.module.name.name).cloned();
            self.resolve(file, extern_candidates, &inst.module, |_| {})
                .map(ModuleTarget::Extern)
        };
        self.diags.truncate(before);
        let Some(target) = target else {
            return; // unknown/ambiguous/unmatched module already reported at the `let`
        };
        let mut outputs: Vec<&str> = Vec::new();
        let mut is_input = false;
        for item in target.items() {
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
                field.name,
                target.name().name
            )
        } else if outputs.is_empty() {
            format!("`{}` has no outputs", target.name().name)
        } else {
            format!(
                "`{}`'s outputs are: {}",
                target.name().name,
                outputs.join(", ")
            )
        };
        self.err(
            file,
            field.span,
            "E0104",
            format!(
                "`{}` has no output named `{}`",
                target.name().name,
                field.name
            ),
            help,
        );
    }
}
