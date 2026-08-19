use super::*;

impl Emitter<'_> {
    /// Flatten `const if` nodes into the items they select, evaluating
    /// conditions against `self.env`. Items in the losing branch are dropped.
    /// Nested ConstIf is resolved recursively. Used by `module()` for loops
    /// that don't recurse.
    pub(in crate::emit_verilog) fn flatten_items(&self, items: &[ModuleItem]) -> Vec<ModuleItem> {
        let items = crate::ast::expand_sync_prims(items);
        let mut out = Vec::new();
        for item in &items {
            match item {
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &self.env)
                        .map(|v| v.to_i128_saturating())
                        .unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    out.extend(self.flatten_items(branch));
                }
                ModuleItem::SyncLoop(sl) => out.extend(crate::ast::lower_sync_loop(sl)),
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, &items) {
                        out.extend(lowered);
                    }
                    // `None` is unreachable here — emit only ever runs on
                    // already-checked programs, where E0417 would have
                    // failed the build first.
                }
                _ => out.push(item.clone()),
            }
        }
        out
    }

    /// Like `eval_consts` but recurses into `const if` winning branches so
    /// that consts declared inside a `const if` block are folded into the env.
    pub(super) fn eval_consts_items(&mut self, items: &[ModuleItem], mut base: Env) -> Env {
        for item in items {
            match item {
                ModuleItem::Const(c) => {
                    base = self.eval_consts(base, std::iter::once(c));
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &base)
                        .map(|v| v.to_i128_saturating())
                        .unwrap_or(0);
                    let branch: &[ModuleItem] = if val != 0 {
                        then
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    base = self.eval_consts_items(branch, base);
                }
                _ => {}
            }
        }
        base
    }

    /// Round-8 plan Task 1 (BUG-70): declare every instance's OUTPUT wires,
    /// for every instance under `items` (descending into `repeat`/`foreach`/
    /// `const if` the same way `emit_instances` does), and nothing else — no
    /// connections, no instantiation line. Called BEFORE `pre_decl_hoist_pos`
    /// is captured (`module()`, above the "Declarations" loop's own doc), so
    /// every instance output wire this module can ever reference already
    /// exists in `self.out` at a position earlier than that splice point.
    ///
    /// BUG-66's fix (round-7 plan Task 3) made this necessary: `emit_instances`
    /// (below) used to be the ONLY pass over instances, and it interleaves
    /// each instance's own output-wire declaration with the NEXT instance's
    /// connection rendering — so a hoist raised by instance N's input
    /// connection (reading an EARLIER instance's output, e.g. `u1.q`) got
    /// spliced at `pre_decl_hoist_pos`, which is captured once, before this
    /// whole region — strictly BEFORE `u1`'s own output wire, which
    /// `emit_instances` had only just written a few lines into the same
    /// region. `pre_decl_hoist_pos`'s safety argument ("every signal these
    /// sites can reference is already declared by then") was true for ports/
    /// parameters (BUG-66's own three reproductions) and false for exactly
    /// this one signal class, which this pre-pass now closes by construction:
    /// declare every instance output FIRST, entirely before the splice point,
    /// then let `emit_instances`'s connections (which may hoist) run after.
    ///
    /// A pure declaration walk — only needs the child's resolved interface,
    /// `width_subst`/`width_resolved`, and the resolved bundle fields, NONE
    /// of which call into a self-determined-position hoist site (those live
    /// in `expr.rs`'s VALUE-expression arms — `Unary`/`Concat`/`Replicate`/
    /// `Builtin::{SignedCast,UnsignedCast,Encoding,Nand,Nor,Xnor,Trunc}` —
    /// never in a width/type render), so this can never itself hoist and is
    /// safe above the insertion point by construction, matching the ordering
    /// the plan asked for.
    ///
    /// Resolution failures (unknown module) are not diagnosed here —
    /// `instance()`'s own connection-rendering pass, unchanged below, still
    /// resolves the identical target and pushes the `Diag`; duplicating the
    /// error here would double-report the same failure for no benefit. Emit
    /// only ever runs on already-checked programs, so in practice this path
    /// never actually fails to resolve — mirrors the same reasoning
    /// `emit_instances`'s own `ForEach` arm already gives for `None` being
    /// unreachable there.
    pub(super) fn declare_instance_outputs(&mut self, items: &[ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Inst(inst) => self.declare_one_instance_outputs(inst),
                ModuleItem::Repeat(r) => self.unroll(r, Self::declare_instance_outputs),
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                        self.declare_instance_outputs(&lowered);
                    }
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &self.env)
                        .map(|v| v.to_i128_saturating())
                        .unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.declare_instance_outputs(branch);
                }
                ModuleItem::SyncLoop(_) => {}
                _ => {}
            }
        }
    }

    /// One instance's own output-wire declarations — the same target
    /// resolution and `args` map `instance()` builds for itself below (kept
    /// as a second, independent resolution rather than threaded through,
    /// since the two passes run at genuinely different points in `self.out`
    /// and neither may borrow state across that gap); only the `Dir::Out`
    /// half of `instance()`'s own port loop, since that is the only half
    /// that declares anything.
    fn declare_one_instance_outputs(&mut self, inst: &Inst) {
        let Some((child_file, target)) = self.project.resolve_target_with_file(&inst.module) else {
            return; // `instance()`'s own pass reports this
        };
        let iname = self.inst_name(inst);
        let child_consts: Vec<(String, Expr)> = self
            .module_envs
            .get(&(child_file, target.name().name.clone()))
            .map(|env| {
                env.iter()
                    .filter(|(_, v)| !v.is_negative())
                    .map(|(n, v)| {
                        let kind = ExprKind::Int {
                            value: v.bits.clone(),
                            raw: v.to_string(),
                        };
                        (
                            n.clone(),
                            Expr {
                                kind,
                                span: inst.span,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut args: HashMap<&str, &Expr> =
            child_consts.iter().map(|(n, e)| (n.as_str(), e)).collect();
        for a in &inst.args {
            args.insert(a.name.name.as_str(), &a.value);
        }
        for item in target.items() {
            let ModuleItem::Port {
                dir: Dir::Out,
                name,
                ty,
            } = item
            else {
                continue;
            };
            let bundle_fields = match ty {
                Type::Bundle {
                    name: bname,
                    args: bargs,
                } => Some(self.resolve_bundle_fields_for_instance(bname, bargs, &args)),
                Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                    Some(self.resolve_bundle_fields_for_instance(id, &[], &args))
                }
                _ => None,
            };
            if let Some(fields) = &bundle_fields {
                for (fname, fty) in fields {
                    let wire_name = format!("{}_{}_{}", iname, name.name, fname);
                    if self.declare_signal_name(&wire_name, inst.span) {
                        let w = self.width_resolved(fty);
                        self.out.push_str(&format!("    wire {w}{wire_name};\n"));
                    }
                }
            } else {
                let wire_name = format!("{}_{}", iname, name.name);
                if self.declare_signal_name(&wire_name, inst.span) {
                    let w = self.width_subst(ty, &args);
                    self.out.push_str(&format!("    wire {w}{wire_name};\n"));
                }
            }
        }
    }

    /// Emit every instance in `items`, descending into `repeat` bodies and
    /// unrolling them (the loop variable is bound per iteration). Declared
    /// before drives so child-output wires exist when the drives use them.
    ///
    /// Round-8 plan Task 1 (BUG-70): output wires themselves are now declared
    /// by `declare_instance_outputs`'s own pre-pass, above, called before
    /// this one — the `Dir::Out` arm in `instance()` below only NAMES the
    /// wire for `port_conns`, it no longer writes the `wire` line itself.
    pub(super) fn emit_instances(&mut self, items: &[ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Inst(inst) => self.instance(inst),
                ModuleItem::Repeat(r) => self.unroll(r, Self::emit_instances),
                // Unlike `sync loop` below, `foreach` is pure sugar over
                // `repeat` (see `no_decls_in_repeat`, checker/names.rs) and
                // its body may legally contain an `inst` — lower and recurse
                // the same way `flatten_items`/`emit_drives` do, or an
                // instance array written with `foreach` would silently never
                // get instantiated.
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                        self.emit_instances(&lowered);
                    }
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &self.env)
                        .map(|v| v.to_i128_saturating())
                        .unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.emit_instances(branch);
                }
                // Lowered `sync loop` items never include an `Inst` — an
                // explicit no-op arm, not a stub, so a future item added to
                // the lowering that DOES need instance emission fails loudly
                // here instead of silently falling into the wildcard below.
                ModuleItem::SyncLoop(_) => {}
                _ => {}
            }
        }
    }

    /// Emit one child-module instantiation. Walks the CHILD's interface
    /// (not the connection list): inputs must be connected explicitly,
    /// clock/reset fall back to same-name signals, and each output gets an
    /// auto-declared wire named `{instance}_{port}` — which is exactly what
    /// `inst.port` field-accesses render to in `expr.rs`.
    fn instance(&mut self, inst: &Inst) {
        let Some((child_file, target)) = self.project.resolve_target_with_file(&inst.module) else {
            self.err(
                inst.module.span,
                format!("unknown module `{}`", inst.module.name.name),
                "is the file that defines it imported? (`import filename` at the top — spec/02 section 1.5)",
            );
            return;
        };

        // Flat Verilog name for this instance (`fa__3` for an array element
        // inside `repeat`, plain `fa` otherwise).
        let iname = self.inst_name(inst);

        // Substitute, inside child port-width expressions: the child's own
        // consts as folded literals (the parent's Verilog knows nothing
        // about a child's `const WIDTH`, and must never fold the PARENT's
        // same-named const instead), then child param names as this
        // instance's argument expressions — params win on a name clash.
        // Negative consts stay symbolic: they cannot be a `u128` literal,
        // and a negative width is already checker-rejected (E0410).
        let child_consts: Vec<(String, Expr)> = self
            .module_envs
            .get(&(child_file, target.name().name.clone()))
            .map(|env| {
                env.iter()
                    .filter(|(_, v)| !v.is_negative())
                    .map(|(n, v)| {
                        let kind = ExprKind::Int {
                            value: v.bits.clone(),
                            raw: v.to_string(),
                        };
                        (
                            n.clone(),
                            Expr {
                                kind,
                                span: inst.span,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut args: HashMap<&str, &Expr> =
            child_consts.iter().map(|(n, e)| (n.as_str(), e)).collect();
        for a in &inst.args {
            args.insert(a.name.name.as_str(), &a.value);
        }

        // Declare wires for child outputs, connect everything by name.
        let mut port_conns: Vec<String> = Vec::new();
        for item in target.items() {
            match item {
                ModuleItem::Clock(c) => {
                    // Implicit same-name connection (spec/02 section 1.5).
                    let sig = inst
                        .conns
                        .iter()
                        .find(|cn| cn.port.name == c.name)
                        .map(|cn| self.expr(&cn.signal))
                        .unwrap_or_else(|| c.name.clone());
                    port_conns.push(format!(".{}({})", c.name, sig));
                }
                ModuleItem::Reset { name: rstp, .. } => {
                    let sig = inst
                        .conns
                        .iter()
                        .find(|cn| cn.port.name == rstp.name)
                        .map(|cn| self.expr(&cn.signal))
                        .unwrap_or_else(|| rstp.name.clone());
                    port_conns.push(format!(".{}({})", rstp.name, sig));
                }
                ModuleItem::Port { dir, name, ty } => {
                    // Bundle ports flatten to one Verilog port per field,
                    // same convention as the module header (module.rs:60-78)
                    // and Drive-path (module.rs:762-807) — a bundle-typed
                    // port is never a single scalar Verilog port.
                    //
                    // NOTE: `args` here is this function's own instance-argument
                    // map (`HashMap<&str, &Expr>`, e.g. `{"W": &Expr(8)}` for
                    // this instantiation) — NOT the port's bundle-type args
                    // (bound as `bargs` below to avoid shadowing it). A bundle
                    // param forwarding the child's own parameter (`Handshake(W:
                    // W)`) must resolve against THIS instance's `args`, not stay
                    // symbolic — see `resolve_bundle_fields_for_instance`'s doc.
                    let bundle_fields = match ty {
                        Type::Bundle {
                            name: bname,
                            args: bargs,
                        } => Some(self.resolve_bundle_fields_for_instance(bname, bargs, &args)),
                        Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                            Some(self.resolve_bundle_fields_for_instance(id, &[], &args))
                        }
                        _ => None,
                    };
                    match dir {
                        Dir::In => {
                            let Some(conn) = inst.conns.iter().find(|c| c.port.name == name.name)
                            else {
                                self.err(
                                    inst.span,
                                    format!(
                                        "instance `{}` does not connect input `{}` of module `{}`",
                                        inst.name.name, name.name, target.name().name
                                    ),
                                    "every input must be connected: `let u = Mod() { port: signal }` (spec/02 section 1.5)",
                                );
                                continue;
                            };
                            if let Some(fields) = &bundle_fields {
                                if let ExprKind::Binary {
                                    op: BinOp::Coalesce,
                                    lhs: clhs,
                                    rhs: crhs,
                                } = &conn.signal.kind
                                {
                                    let (clhs, crhs) = (clhs.clone(), crhs.clone());
                                    for (fname, _) in fields.clone() {
                                        let r = self.coalesce_field_expr(&clhs, &crhs, &fname);
                                        port_conns.push(format!(".{}_{}({})", name.name, fname, r));
                                    }
                                } else {
                                    let sig = self.expr(&conn.signal);
                                    for (fname, _) in fields {
                                        port_conns.push(format!(
                                            ".{}_{}({}_{})",
                                            name.name, fname, sig, fname
                                        ));
                                    }
                                }
                            } else {
                                let sig = self.expr(&conn.signal);
                                port_conns.push(format!(".{}({})", name.name, sig));
                            }
                        }
                        // Round-8 plan Task 1 (BUG-70): the wire itself is
                        // already declared by `declare_instance_outputs`'s
                        // own pre-pass, strictly before `pre_decl_hoist_pos`
                        // — this arm only NAMES it for `port_conns`, so a
                        // hoist raised by a LATER instance's connection
                        // (which may read THIS instance's output) can never
                        // be spliced ahead of its own declaration.
                        Dir::Out => {
                            if let Some(fields) = &bundle_fields {
                                for (fname, _fty) in fields {
                                    let wire_name = format!("{}_{}_{}", iname, name.name, fname);
                                    port_conns
                                        .push(format!(".{}_{}({})", name.name, fname, wire_name));
                                }
                            } else {
                                let wire_name = format!("{}_{}", iname, name.name);
                                port_conns.push(format!(".{}({})", name.name, wire_name));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Unknown connection names → error.
        for c in &inst.conns {
            let known = target.items().iter().any(|i| match i {
                ModuleItem::Port { name, .. } => name.name == c.port.name,
                ModuleItem::Clock(n) | ModuleItem::Reset { name: n, .. } => n.name == c.port.name,
                _ => false,
            });
            if !known {
                self.err(
                    c.port.span,
                    format!(
                        "module `{}` has no port `{}`",
                        target.name().name,
                        c.port.name
                    ),
                    "",
                );
            }
        }

        let params = if inst.args.is_empty() {
            String::new()
        } else {
            let ps: Vec<String> = inst
                .args
                .iter()
                .map(|a| format!(".{}({})", a.name.name, self.expr(&a.value)))
                .collect();
            format!(" #({})", ps.join(", "))
        };
        // Must agree with the SAME `target`/`child_file` pair's declaration
        // header (`module()`, above) — same target, same emitted identifier.
        // Extern targets have no per-file disambiguation: there is exactly
        // one real external module regardless of which Min-Mozhi file
        // declared the `extern module` referring to it.
        let child_verilog_name = match target {
            ModuleTarget::Real(m) => self.project.verilog_module_name(child_file, m),
            ModuleTarget::Extern(em) => em
                .verilog_name
                .clone()
                .unwrap_or_else(|| em.name.name.clone()),
        };
        self.out.push_str(&format!(
            "    {}{} {} ({});\n",
            child_verilog_name,
            params,
            iname,
            port_conns.join(", ")
        ));
    }
}
