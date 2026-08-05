use super::*;

impl<'a> Checker<'a> {
    // `items` deliberately has NO `'a` bound (unlike `collect_decls`'s
    // `items: &'a [ModuleItem]`) — this fn only ever reads through `items`
    // for the duration of this call (via `ty`/`expr`/`lvalue`/`check_inst`,
    // none of which stash anything long-lived either); the only thing that
    // genuinely needs `'a` is `Scope<'a>` itself (`Bind::Inst`/`Bind::Enum`),
    // built once by `collect_decls` over the REAL AST and reused by later
    // passes. That independence is what lets a `ForEach` arm recurse here
    // with a freshly lowered, locally-owned `Vec<ModuleItem>` (see that arm
    // below) without needing to leak it to manufacture a fake `'a`.
    pub(super) fn walk_items(
        &mut self,
        file: usize,
        sc: &Scope<'a>,
        env: &mut Env,
        items: &[ModuleItem],
    ) {
        for item in items {
            match item {
                ModuleItem::Port { ty, name, .. } => {
                    self.ty(file, sc, env, ty);
                    self.reject_array_signal_type(file, ty, name.span, "a port");
                }
                ModuleItem::Wire { ty, init, name, .. } => {
                    self.ty(file, sc, env, ty);
                    self.reject_array_signal_type(file, ty, name.span, "a wire");
                    self.expr(file, sc, env, init);
                }
                ModuleItem::Reg { ty, reset, name, .. } => {
                    self.ty(file, sc, env, ty);
                    self.reject_array_signal_type(file, ty, name.span, "a register");
                    self.expr(file, sc, env, reset);
                }
                ModuleItem::Mem {
                    ty, depth, init, ..
                } => {
                    self.ty(file, sc, env, ty);
                    self.expr(file, sc, env, depth);
                    self.expr(file, sc, env, init);
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
                    // E0810: each reg may have at most one `default` per `on` block
                    let mut seen_defaults: std::collections::HashSet<&str> = Default::default();
                    for stmt in &on.body {
                        if let SeqStmt::Default { name, span, .. } = stmt
                            && !seen_defaults.insert(name.name.as_str()) {
                                self.err(
                                    file,
                                    *span,
                                    "E0810",
                                    format!(
                                        "duplicate `default` for `{}` in this `on` block",
                                        name.name
                                    ),
                                    "each reg may have at most one `default` per `on` block",
                                );
                            }
                    }
                    self.seq_stmts(file, sc, env, items, &on.body);
                }
                ModuleItem::Drive { lhs, rhs } => {
                    self.lvalue(file, sc, env, lhs);
                    self.expr(file, sc, env, rhs);
                }
                ModuleItem::Repeat(r) => {
                    self.no_decls_in_repeat(file, &r.items);
                    let lo = self.const_pos(file, env, &r.lo);
                    self.const_pos(file, env, &r.hi);
                    // The loop variable is a compile-time int inside the
                    // body. Its per-iteration values matter only to
                    // elaboration (later slice) — names resolve the same
                    // for every iteration, so one walk with `lo` suffices.
                    let shadowed = env.insert(
                        r.var.name.clone(),
                        consteval::ConstVal::from_i128(lo.unwrap_or(0)),
                    );
                    self.walk_items(file, sc, env, &r.items);
                    match shadowed {
                        Some(v) => env.insert(r.var.name.clone(), v),
                        None => env.remove(&r.var.name),
                    };
                }
                // `foreach` is pure sugar over `repeat`/`loop` — the ONLY
                // checker logic it genuinely owns is validating an
                // Elements-form source resolves to an array/mem type
                // (E0417); everything else (bound const-ness, name
                // resolution, `no_decls_in_repeat`, ...) is inherited for
                // free by lowering to `Repeat` and recursing into the SAME
                // `walk_items` this arm itself lives in — hitting the
                // `ModuleItem::Repeat` arm above on the next pass.
                ModuleItem::ForEach(fe) => {
                    if let ForEachSource::Elements(arr) = &fe.source
                        && crate::ast::array_like_len(&arr.name, items).is_none()
                    {
                        self.err(
                            file,
                            arr.span,
                            "E0417",
                            format!("`{}` is not an array or mem type", arr.name),
                            format!(
                                "`foreach {} in {}` needs `{}` to be a declared array/mem \
                                 signal — use `foreach {} in lo..hi` if you meant a range \
                                 instead",
                                fe.var.name, arr.name, arr.name, fe.var.name
                            ),
                        );
                        continue;
                    }
                    let Some(lowered) = crate::ast::lower_foreach_item(fe, items) else {
                        continue; // E0417 already pushed above
                    };
                    // `lowered` is a fresh owned `Vec` (a clone of `fe.items`
                    // with `fe.var` substituted), not part of the `'a` AST
                    // arena — but `walk_items` doesn't need `'a` for `items`
                    // (see the fn's own doc comment), so this borrows fine
                    // for just the duration of this recursive call.
                    self.walk_items(file, sc, env, &lowered);
                }
                ModuleItem::SyncLoop(sl) => {
                    match sc.names.get(&sl.clock.name) {
                        Some(Bind::Clock) => {}
                        Some(b) => {
                            let what = b.what();
                            self.err(
                                file,
                                sl.clock.span,
                                "E0109",
                                format!("`{}` is {what}, not a clock", sl.clock.name),
                                "a sync loop's `on rise(...)`/`on fall(...)` clause takes a \
                                 clock — declare one with `clock clk` (spec/02 section 1.2)",
                            );
                        }
                        None => self.unknown(file, &sl.clock.name, sl.clock.span),
                    }
                    self.ty(file, sc, env, &sl.result_ty);
                    self.expr(file, sc, env, &sl.result_init);
                    let lo_val = self.const_pos(file, env, &sl.lo);
                    self.const_pos(file, env, &sl.hi);
                    // `var` is a runtime counter, read-only inside the body —
                    // the generated FSM owns incrementing it (see
                    // `ast::sync_loop_lower`) — so, same as `Repeat`'s
                    // compile-time loop var above, one representative `env`
                    // entry is enough for `expr()`'s name lookup to resolve
                    // it; per-iteration values don't matter to name
                    // resolution.
                    //
                    // `result_name` differs: the body legitimately assigns to
                    // it (`result <- ...` accumulates every cycle — it lowers
                    // to a real reg, `<name>_acc`). `lvalue()` only allows
                    // Out/Wire/Reg targets found in `sc.names`, so an
                    // `env`-only entry would make every real sync-loop body
                    // fail with a spurious "cannot assign to constant"
                    // (E0108). Give it a real (body-local) `Bind::Reg` entry
                    // instead, via the same clone-and-extend scope idiom
                    // `ExprKind::Match`'s per-arm bindings already use above.
                    let shadowed_var = env.insert(
                        sl.var.name.clone(),
                        consteval::ConstVal::from_i128(lo_val.unwrap_or(0)),
                    );
                    let mut body_sc = Scope {
                        names: sc.names.clone(),
                    };
                    body_sc.names.insert(sl.result_name.name.clone(), Bind::Reg);
                    self.seq_stmts(file, &body_sc, env, items, &sl.body);
                    match shadowed_var {
                        Some(v) => env.insert(sl.var.name.clone(), v),
                        None => env.remove(&sl.var.name),
                    };
                }
                ModuleItem::Enum(e) => {
                    for v in &e.variants {
                        for field in &v.fields {
                            self.ty(file, sc, env, &field.ty);
                        }
                    }
                }
                ModuleItem::ConstIf { cond, then, els, span } => {
                    match consteval::eval(cond, env) {
                        Ok(val) => {
                            let branch = if !val.is_zero() {
                                then.as_slice()
                            } else {
                                els.as_deref().unwrap_or(&[])
                            };
                            self.walk_items(file, sc, env, branch);
                        }
                        Err(_) => {
                            self.err(
                                file,
                                *span,
                                "E0811",
                                "`const if` condition is not a compile-time constant",
                                "use only module parameters, consts, literals, and \
                                 arithmetic/comparison; runtime signals like ports are \
                                 not allowed",
                            );
                        }
                    }
                }
                ModuleItem::Clock(_)
                | ModuleItem::Reset { .. }
                | ModuleItem::Const(_) // evaluated in check_module
                | ModuleItem::Error(_) => {}
                ModuleItem::BundleDestructure { expr, .. } => {
                    self.expr(file, sc, env, expr);
                }
                ModuleItem::Assert(a) => {
                    self.expr(file, sc, env, &a.cond);
                }
            }
        }
    }

    // Same "no `'a` needed" reasoning as `walk_items` — see its doc comment.
    fn seq_stmts(
        &mut self,
        file: usize,
        sc: &Scope<'a>,
        env: &mut Env,
        module_items: &[ModuleItem],
        stmts: &[SeqStmt],
    ) {
        for s in stmts {
            match s {
                SeqStmt::Assign { lhs, rhs } => {
                    self.lvalue(file, sc, env, lhs);
                    self.expr(file, sc, env, rhs);
                }
                SeqStmt::If { cond, then, els } => {
                    self.expr(file, sc, env, cond);
                    self.seq_stmts(file, sc, env, module_items, then);
                    if let Some(els) = els {
                        self.seq_stmts(file, sc, env, module_items, els);
                    }
                }
                SeqStmt::Default { name, val, span } => {
                    match sc.names.get(&name.name) {
                        Some(Bind::Reg) => {}
                        Some(_) => self.err(
                            file,
                            *span,
                            "E0809",
                            format!("`default` target `{}` is not a reg", name.name),
                            "only `reg` signals can have default assignments; \
                             wires are always driven combinationally",
                        ),
                        None => self.unknown(file, &name.name, name.span),
                    }
                    self.expr(file, sc, env, val);
                }
                SeqStmt::Loop {
                    var, lo, hi, body, ..
                } => {
                    // `loop` unrolls at compile time, same as `ModuleItem::Repeat`
                    // — its bounds must const-evaluate, so reuse `const_pos`
                    // (which reports E0201, `repeat`'s own bound-checking path)
                    // instead of silently defaulting a non-const bound to 0.
                    let lo_val = self.const_pos(file, env, lo);
                    self.const_pos(file, env, hi);
                    // The loop variable is a compile-time int inside the body,
                    // same one-representative-walk model as `ModuleItem::Repeat`
                    // (per-iteration values matter only to elaboration, not name
                    // resolution).
                    let shadowed = env.insert(
                        var.name.clone(),
                        consteval::ConstVal::from_i128(lo_val.unwrap_or(0)),
                    );
                    self.seq_stmts(file, sc, env, module_items, body);
                    match shadowed {
                        Some(v) => env.insert(var.name.clone(), v),
                        None => env.remove(&var.name),
                    };
                }
                // Same "lower to `Loop`, recurse into the same fn" delegation
                // as `ModuleItem::ForEach` above — see that arm's comment.
                // `SeqStmt` has no local-binding statement, so the Elements
                // form substitutes `var` throughout `body` instead of
                // introducing a new binding (see `lower_foreach_seq`'s doc
                // comment) — no synthesized declaration, so (unlike the
                // `ModuleItem` form) there's no `no_decls_in_repeat`-style
                // concern here.
                SeqStmt::ForEach {
                    var,
                    source,
                    body,
                    span,
                } => {
                    if let ForEachSource::Elements(arr) = source
                        && crate::ast::array_like_len(&arr.name, module_items).is_none()
                    {
                        self.err(
                            file,
                            arr.span,
                            "E0417",
                            format!("`{}` is not an array or mem type", arr.name),
                            format!(
                                "`foreach {} in {}` needs `{}` to be a declared array/mem \
                                 signal — use `foreach {} in lo..hi` if you meant a range \
                                 instead",
                                var.name, arr.name, arr.name, var.name
                            ),
                        );
                        continue;
                    }
                    let Some(lowered) =
                        crate::ast::lower_foreach_seq(var, source, body, *span, module_items)
                    else {
                        continue; // E0417 already pushed above
                    };
                    self.seq_stmts(file, sc, env, module_items, &lowered);
                }
                SeqStmt::Assert(a) => {
                    self.expr(file, sc, env, &a.cond);
                }
                SeqStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// E0303 — a `repeat` body may only generate hardware (drives,
    /// instances, nested `repeat`s), never declare it. A declaration
    /// inside `repeat` would mean N copies of the same name — there is no
    /// such thing; declare the signal once outside and drive bit `i`
    /// inside. Reports each offending item at its own span (the immediate
    /// level only; nested `repeat`s are checked when the walk reaches
    /// them).
    fn no_decls_in_repeat(&mut self, file: usize, items: &[ModuleItem]) {
        for item in items {
            let (span, what) = match item {
                ModuleItem::Drive { .. }
                | ModuleItem::Inst(_)
                | ModuleItem::Repeat(_)
                | ModuleItem::ForEach(_)
                | ModuleItem::SyncLoop(_)
                | ModuleItem::ConstIf { .. }
                | ModuleItem::Assert(_)
                | ModuleItem::Error(_) => continue,
                ModuleItem::Port { name, .. } => (name.span, "an input/output port"),
                ModuleItem::Wire { name, .. } => (name.span, "a wire"),
                ModuleItem::Reg { name, .. } => (name.span, "a register"),
                ModuleItem::Mem { name, .. } => (name.span, "a memory"),
                ModuleItem::Clock(n) => (n.span, "a clock"),
                ModuleItem::Reset { name: n, .. } => (n.span, "a reset"),
                ModuleItem::Const(c) => (c.name.span, "a const"),
                ModuleItem::Enum(e) => (e.name.span, "an enum"),
                ModuleItem::On(on) => (on.span, "an `on` block"),
                ModuleItem::BundleDestructure { span, .. } => (*span, "a bundle destructure"),
            };
            self.err(
                file,
                span,
                "E0303",
                format!("`repeat` cannot contain {what}"),
                "`repeat` unrolls at compile time — it may only generate \
                 hardware (drives, instances, nested `repeat`s). Declare \
                 the signal once outside the loop and drive bit `i` inside \
                 (spec/02 section 1.6).",
            );
        }
    }

    /// A position that must const-evaluate today (`repeat` bounds).
    /// Returns the value if it did.
    pub(super) fn const_pos(&mut self, file: usize, env: &Env, e: &Expr) -> Option<i128> {
        match consteval::eval(e, env) {
            Ok(v) => Some(v.to_i128_saturating()),
            Err(d) => {
                self.diags.push(d.with_file(file));
                None
            }
        }
    }

    pub(super) fn ty(&mut self, file: usize, sc: &Scope<'a>, env: &Env, ty: &Type) {
        match ty {
            Type::Bit => {}
            Type::Bits(w) | Type::Signed(w) => self.expr(file, sc, env, w),
            Type::Bundle { name, .. } => {
                let candidates = self.bundles.get(&name.name.name).cloned();
                self.resolve(file, candidates, name, |ck| {
                    ck.err(
                        file,
                        name.span,
                        "E0906",
                        format!("unknown bundle type `{}`", name.name.name),
                        "declare the bundle at file level before using it as a type",
                    );
                });
            }
            Type::Named(n) => {
                // Module-scope enum shadows any project-wide import — unchanged
                // behavior. Only once that's ruled out do we resolve against
                // the project tables (enum first, then bundle — same
                // "enum OR bundle" disjunction as before, now going through
                // `resolve` so an ambiguous/qualified reference gets its own
                // E0110/E0111 instead of silently picking the first file).
                let sc_enum = matches!(sc.names.get(&n.name.name), Some(Bind::Enum(_)));
                if !sc_enum {
                    if self.enums.contains_key(&n.name.name) {
                        let candidates = self.enums.get(&n.name.name).cloned();
                        self.resolve(file, candidates, n, |_| {});
                    } else if self.bundles.contains_key(&n.name.name) {
                        let candidates = self.bundles.get(&n.name.name).cloned();
                        self.resolve(file, candidates, n, |_| {});
                    } else {
                        self.err(
                            file,
                            n.span,
                            "E0103",
                            format!("unknown type `{}`", n.name.name),
                            format!(
                                "named types are `enum` or `bundle` declarations — declare \
                                 `enum {} {{ ... }}` or `bundle {} {{ ... }}` at file level, \
                                 or import the file that does",
                                n.name.name, n.name.name
                            ),
                        );
                    }
                }
            }
            Type::Array { elem, len } => {
                self.ty(file, sc, env, elem);
                self.expr(file, sc, env, len);
            }
        }
    }
}
