use super::*;

impl Emitter<'_> {
    /// Emit every combinational drive in `items` (`wire` inits and `=`
    /// drives), unrolling `repeat` bodies. Indices and the loop variable
    /// fold to literals, so `sum[i] = …` becomes `assign sum[2] = …`.
    pub(super) fn emit_drives(&mut self, items: &[ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Wire { name, ty, init } => {
                    // `emit_drives` (unlike the `flat`-driven loops above) walks
                    // the RAW module item list, not `flatten_items`'s output —
                    // same precedent as `SyncLoop`/`ForEach` below: a bare
                    // `sync.pulse(...)` initializer must be expanded on the
                    // spot, then its lowered items (hidden regs/on-blocks are
                    // skipped here — they render via `flat` elsewhere; only
                    // the final rewritten `Wire` matters to this pass) driven
                    // through this same function recursively.
                    if let ExprKind::Call {
                        func: Builtin::SyncPulse,
                        ..
                    } = &init.kind
                    {
                        let lowered = crate::ast::expand_sync_prims(std::slice::from_ref(item));
                        self.emit_drives(&lowered);
                        continue;
                    }
                    // Bundle wires: emit one assign per field.
                    let binfo = match ty {
                        Type::Bundle { name: bn, args } => Some((bn.clone(), args.clone())),
                        Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                            Some((id.clone(), vec![]))
                        }
                        _ => None,
                    };
                    if let Some((bname, args)) = binfo {
                        let fields = self.resolve_bundle_fields(&bname, &args);
                        if let ExprKind::BundleLit(inits) = &init.kind {
                            let inits = inits.clone();
                            for (fname, fty) in &fields {
                                if let Some(fi) = inits.iter().find(|fi| fi.name.name == *fname) {
                                    let r = self.sized_field_expr(&fi.value, fty);
                                    self.out.push_str(&format!(
                                        "    assign {}_{} = {};\n",
                                        name.name, fname, r
                                    ));
                                }
                            }
                        } else if let ExprKind::Binary {
                            op: BinOp::Coalesce,
                            lhs: clhs,
                            rhs: crhs,
                        } = &init.kind
                        {
                            for (fname, _) in &fields {
                                let r = self.coalesce_field_expr(clhs, crhs, fname);
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {r};\n",
                                    name.name
                                ));
                            }
                        } else {
                            // RHS is a plain signal: emit signame_field = rhs_field.
                            let r = self.expr(init);
                            for (fname, _) in &fields {
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {r}_{fname};\n",
                                    name.name
                                ));
                            }
                        }
                    } else {
                        let rhs = self.expr(init);
                        self.out
                            .push_str(&format!("    assign {} = {};\n", name.name, rhs));
                    }
                }
                ModuleItem::Drive { lhs, rhs } => {
                    // If LHS is a bundle signal, flatten to one assign per field.
                    let binfo = self.bundle_sigs.get(&lhs.base.name).cloned();
                    if let Some((bname, args)) = binfo {
                        let fields = self.resolve_bundle_fields(&bname, &args);
                        if let ExprKind::BundleLit(inits) = &rhs.kind {
                            let inits = inits.clone();
                            for (fname, fty) in &fields {
                                if let Some(fi) = inits.iter().find(|fi| fi.name.name == *fname) {
                                    let r = self.sized_field_expr(&fi.value, fty);
                                    self.out.push_str(&format!(
                                        "    assign {}_{} = {};\n",
                                        lhs.base.name, fname, r
                                    ));
                                }
                            }
                        } else if let ExprKind::Binary {
                            op: BinOp::Coalesce,
                            lhs: clhs,
                            rhs: crhs,
                        } = &rhs.kind
                        {
                            for (fname, _) in &fields {
                                let r = self.coalesce_field_expr(clhs, crhs, fname);
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {r};\n",
                                    lhs.base.name
                                ));
                            }
                        } else {
                            // RHS is a bundle signal (e.g. `rsp = req`).
                            let rhs_name = match &rhs.kind {
                                ExprKind::Ident(n) => n.clone(),
                                _ => self.expr(rhs),
                            };
                            for (fname, _) in &fields {
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {rhs_name}_{fname};\n",
                                    lhs.base.name
                                ));
                            }
                        }
                    } else {
                        let l = self.lvalue(lhs);
                        let r = self.expr(rhs);
                        self.out.push_str(&format!("    assign {l} = {r};\n"));
                    }
                }
                ModuleItem::Repeat(r) => self.unroll(r, Self::emit_drives),
                // `emit_drives` (unlike the `flat`-driven loops above) walks
                // the RAW module item list, not `flatten_items`'s output, so
                // a `sync loop` here must be lowered on the spot — lowering
                // happens once (no per-iteration substitution like `unroll`).
                ModuleItem::SyncLoop(sl) => {
                    let lowered = crate::ast::lower_sync_loop(sl);
                    self.emit_drives(&lowered);
                }
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                        self.emit_drives(&lowered);
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
                    self.emit_drives(branch);
                }
                _ => {}
            }
        }
    }
}
