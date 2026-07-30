use super::*;

impl<'a> Checker<'a> {
    /// Inject payload binding types into `cx.sigs` for one match arm.
    /// Returns `(name, prev)` pairs so the caller can restore the prior state
    /// after checking the arm body. Silent — E0807 was already emitted at the
    /// enum's declaration site.
    pub(super) fn inject_arm_bindings(
        &mut self,
        cx: &mut Wcx<'a>,
        en: &'a EnumDecl,
        patterns: &[Pattern],
    ) -> Vec<(String, Option<Ty<'a>>)> {
        // A payload field type (e.g. `bits[SOME_CONST]`) must resolve
        // against the enum's OWN declaring file's consts, not the match
        // site's — the same cross-file distinction `EnumConstruct`'s width
        // check already makes (widths/expr.rs). Falls back to `cx.file`
        // only if the lookup somehow misses (defensive; `en` came from a
        // successfully-inferred `Ty::Enum`, so this file is always found).
        let enum_file = self
            .enums
            .get(&en.name.name)
            .and_then(|v| v.first())
            .map(|&(f, _)| f)
            .unwrap_or(cx.file);
        let mut injected = Vec::new();
        for p in patterns {
            if let Pattern::Variant {
                variant, bindings, ..
            } = p
                && let Some(ev) = en.variants.iter().find(|v| v.name.name == variant.name)
            {
                for (binding, field) in bindings.iter().zip(ev.fields.iter()) {
                    let ty = self.fn_type_for_file(enum_file, &field.ty);
                    match ty {
                        Ty::Bit | Ty::Bits(_) | Ty::Signed(_) => {
                            let prev = cx.sigs.insert(binding.name.clone(), ty);
                            injected.push((binding.name.clone(), prev));
                        }
                        _ => {} // Enum/Memory/Unknown — leave as Unknown (E0807 already reported)
                    }
                }
            }
        }
        injected
    }

    /// Width-check one function body (E0804). Functions are monomorphic —
    /// param types use file consts only, so each function is checked once.
    pub(super) fn check_func_body_widths(&mut self, file: usize, func: &'a FuncDecl) {
        let env = self.file_consts[file].clone();
        let mut cx = Wcx {
            file,
            sc: Rc::new(Scope {
                names: HashMap::new(),
            }),
            env,
            sigs: HashMap::new(),
        };
        // Seed the signal environment with concrete param types.
        for param in &func.params {
            let ty = self.resolve_ty(&mut cx, &param.ty);
            cx.sigs.insert(param.name.name.clone(), ty);
        }
        let ret_ty = self.resolve_ty(&mut cx, &func.ret);
        self.check_fn_stmt_widths(&mut cx, &func.stmts, ret_ty.clone(), &func.name.name);
        // The tail is the guaranteed fallthrough — always checked, exactly
        // like every `return` expression.
        self.check_return_expr(&mut cx, &func.tail, ret_ty, &func.name.name);
    }

    /// Width-check one `fn`-body statement list. Folds `let` bindings into
    /// `cx.sigs` sequentially — a `let` bound BEFORE an `if` stays visible
    /// inside both branches and after, but a `let` bound INSIDE a branch is
    /// scoped to that branch only: `cx.sigs` is snapshotted before checking
    /// `then`, restored before checking `els` (so `then`'s bindings don't
    /// leak into `els`), and restored again after the `if` so nothing
    /// leaks into later statements or the tail. (An earlier version of this
    /// comment claimed a "flat scope model" shared with `on`-block
    /// `SeqStmt::If` as a deliberate simplification — see
    /// `check_fn_stmt_names`'s doc comment for why that claim was wrong and
    /// this was a real soundness gap, not a design choice.)
    fn check_fn_stmt_widths(
        &mut self,
        cx: &mut Wcx<'a>,
        stmts: &'a [FnStmt],
        ret_ty: Ty<'a>,
        func_name: &str,
    ) {
        for stmt in stmts {
            match stmt {
                FnStmt::Let(local) => {
                    let ty = self.infer_ty(cx, &local.value);
                    // An array-typed `let` has no single register width — it
                    // lowers to N scalar `reg`s of the ELEMENT width. Record
                    // the element width so the emitter can size each
                    // `<name>_<i>` reg (emit_verilog `render_fn_decl`).
                    let w = scalar_width(&ty);
                    // BUG-9: a `let` that shadows an existing name — an
                    // earlier `let` in this same straight-line body, or a
                    // `fn` parameter — at a DIFFERENT width can't share one
                    // Verilog `reg` declaration; the emitter used to emit
                    // two conflicting `reg` lines for the same identifier
                    // (real Verilog rejects the redeclaration). Same-width
                    // shadowing stays legal — it's the common fold/
                    // accumulator idiom (`foreach_sum.mimz`'s `let acc = acc
                    // +% v`), which the emitter safely dedupes to one `reg`.
                    if let (Some(new_w), Some(prev_ty)) = (w, cx.sigs.get(&local.name.name))
                        && let Some(prev_w) = scalar_width(prev_ty)
                        && prev_w != new_w
                    {
                        self.err(
                            cx.file,
                            local.span,
                            "E0813",
                            format!(
                                "`{}` is re-bound here at a different width ({new_w} bits) \
                                 than its earlier binding ({prev_w} bits)",
                                local.name.name
                            ),
                            format!(
                                "give this binding a different name instead of shadowing \
                                 `{}` — a `let` may re-bind a name at the SAME width \
                                 (the usual fold/accumulator pattern) but not a different one, \
                                 since both must share one fixed-width Verilog declaration",
                                local.name.name
                            ),
                        );
                    }
                    if let Some(w) = w {
                        local.inferred_width.set(Some(w));
                    }
                    cx.sigs.insert(local.name.name.clone(), ty);
                }
                FnStmt::If { cond, then, els } => {
                    self.check_cond(cx, cond);
                    let sigs_before = cx.sigs.clone();
                    self.check_fn_stmt_widths(cx, then, ret_ty.clone(), func_name);
                    if let Some(els) = els {
                        cx.sigs = sigs_before.clone();
                        self.check_fn_stmt_widths(cx, els, ret_ty.clone(), func_name);
                    }
                    cx.sigs = sigs_before;
                }
                FnStmt::Return(expr) => {
                    self.check_return_expr(cx, expr, ret_ty.clone(), func_name);
                }
                FnStmt::Loop {
                    var, lo, hi, body, ..
                } => {
                    // Bounds that do not const-eval were reported by pass 3.
                    let (Ok(lo_v), Ok(hi_v)) = (
                        consteval::eval(lo, &cx.env).map(|v| v.to_i128_saturating()),
                        consteval::eval(hi, &cx.env).map(|v| v.to_i128_saturating()),
                    ) else {
                        continue;
                    };
                    let values: Vec<i128> = if hi_v - lo_v > MAX_REPEAT_CHECKS {
                        vec![lo_v, lo_v + 1, hi_v - 1]
                    } else {
                        (lo_v..hi_v).collect()
                    };
                    let sigs_before = cx.sigs.clone();
                    for v in values {
                        let shadowed = cx
                            .env
                            .insert(var.name.clone(), consteval::ConstVal::from_i128(v));
                        cx.sigs = sigs_before.clone();
                        let before = self.diags.len();
                        self.check_fn_stmt_widths(cx, body, ret_ty.clone(), func_name);
                        self.unshadow(cx, &var.name, shadowed);
                        if self.diags.len() > before {
                            break; // one iteration's worth of errors is enough
                        }
                    }
                    cx.sigs = sigs_before;
                }
                // Same "raw body + cx.env/cx.sigs shadow" delegation as
                // `ModuleItem::ForEach`/`SeqStmt::ForEach` above, for the
                // same cross-file `'a` reason. `cx.sigs` is snapshotted and
                // restored the same way `FnStmt::Loop` above does (a `fn`
                // body's `let`-scoping means bindings made inside the loop
                // body must not leak past it) — the Elements form's `var`
                // binding is exactly such a binding (it's lowered to a real
                // `FnStmt::Let` by `ast::lower_foreach_fn`; injecting its
                // type into `cx.sigs` here has the same scoping effect
                // without needing that lowered node to exist).
                FnStmt::ForEach {
                    var, source, body, ..
                } => match source {
                    ForEachSource::Range { lo, hi } => {
                        let (Ok(lo_v), Ok(hi_v)) = (
                            consteval::eval(lo, &cx.env).map(|v| v.to_i128_saturating()),
                            consteval::eval(hi, &cx.env).map(|v| v.to_i128_saturating()),
                        ) else {
                            continue; // bounds reported by pass 3
                        };
                        let values: Vec<i128> = if hi_v - lo_v > MAX_REPEAT_CHECKS {
                            vec![lo_v, lo_v + 1, hi_v - 1]
                        } else {
                            (lo_v..hi_v).collect()
                        };
                        let sigs_before = cx.sigs.clone();
                        for v in values {
                            let shadowed = cx
                                .env
                                .insert(var.name.clone(), consteval::ConstVal::from_i128(v));
                            cx.sigs = sigs_before.clone();
                            let before = self.diags.len();
                            self.check_fn_stmt_widths(cx, body, ret_ty.clone(), func_name);
                            self.unshadow(cx, &var.name, shadowed);
                            if self.diags.len() > before {
                                break;
                            }
                        }
                        cx.sigs = sigs_before;
                    }
                    ForEachSource::Elements(arr) => {
                        let elem_ty = match cx.sigs.get(&arr.name).cloned() {
                            Some(Ty::Array {
                                elem_width,
                                elem_signed,
                                ..
                            }) => {
                                if elem_signed {
                                    Ty::Signed(elem_width)
                                } else {
                                    bits(elem_width)
                                }
                            }
                            Some(Ty::Memory { width, signed, .. }) => {
                                if signed {
                                    Ty::Signed(width)
                                } else {
                                    bits(width)
                                }
                            }
                            _ => continue, // E0417 already reported by pass 3
                        };
                        let sigs_before = cx.sigs.clone();
                        cx.sigs.insert(var.name.clone(), elem_ty);
                        self.check_fn_stmt_widths(cx, body, ret_ty.clone(), func_name);
                        cx.sigs = sigs_before;
                    }
                },
                FnStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// Like `check_return_ty`, but takes the raw expression (not a
    /// pre-inferred `Ty`) so a bundle-literal return/tail can be
    /// field-checked against `ret_ty` — `infer_ty` alone always returns
    /// `Ty::Unknown` for a `BundleLit` (it has no fixed shape without
    /// top-down context), so `check_return_ty` never got a chance today.
    fn check_return_expr(
        &mut self,
        cx: &mut Wcx<'a>,
        expr: &'a Expr,
        ret_ty: Ty<'a>,
        func_name: &str,
    ) {
        // `Ty::Bundle`'s own fields (`&'a str`/`Option<usize>`/`&'a
        // [NamedArg]`) are all `Copy`, so matching them out of `ret_ty`
        // here does not consume it — it's still available below for
        // `check_return_ty` whether or not this arm matches.
        if let Ty::Bundle {
            name,
            bfile_hint,
            args,
        } = ret_ty
            && let ExprKind::BundleLit(inits) = &expr.kind
        {
            self.check_bundle_lit(cx, name, bfile_hint, args, inits, expr.span);
            return;
        }
        let ty = self.infer_ty(cx, expr);
        self.check_return_ty(cx, expr.span, ty, ret_ty, func_name);
    }

    /// Shared E0804 check: does `ty` (a `return` expression's or the tail's
    /// inferred type) match the function's declared return type? Extracted
    /// so both `return` sites and the tail use identical logic.
    fn check_return_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        span: Span,
        ty: Ty<'a>,
        ret_ty: Ty<'a>,
        func_name: &str,
    ) {
        match (ty, ret_ty) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => {}
            (Ty::CtInt(v), t) => self.fit(cx, span, &v, t),
            (g, t) if same(&g, &t) => {}
            (g @ Ty::Bundle { .. }, t @ Ty::Bundle { .. }) => {
                match self.bundle_shape_match(cx, t, g, span) {
                    BundleShapeMatch::Compatible => {}
                    BundleShapeMatch::MissingField(field) => {
                        self.err(
                            cx.file,
                            span,
                            "E0910",
                            format!(
                                "function `{func_name}` returns a bundle missing field \
                                 `{field}`, which the declared return type requires"
                            ),
                            "structural matching allows extra fields on the returned bundle, \
                             but never fewer — add the missing field, or return a bundle that \
                             already has it",
                        );
                    }
                    BundleShapeMatch::FieldTypeMismatch {
                        field,
                        expected,
                        got,
                    } => {
                        self.err(
                            cx.file,
                            span,
                            "E0804",
                            format!(
                                "function `{func_name}` returns field `{field}` as {got}, but \
                                 the declared return type expects {expected}"
                            ),
                            "widths/types must match exactly — nothing resizes implicitly at \
                             a bundle field boundary",
                        );
                    }
                }
            }
            (g, t) => {
                self.err(
                    cx.file,
                    span,
                    "E0804",
                    format!(
                        "function `{func_name}` body is {}, but the declared return type is {}",
                        show(&g),
                        show(&t)
                    ),
                    format!(
                        "the return expression must match the declared return type exactly — \
                         use `extend`, `trunc`, or a slice to resize (spec/02 section 5); \
                         the target here is {}",
                        show(&t)
                    ),
                );
            }
        }
    }

    /// Resolve a function parameter or return type under the function's
    /// file const env. Silent — the function's own body check owns any
    /// type-resolution errors. Called by the [`ExprKind::FnCall`] width
    /// handler in `widths/expr.rs` (mirrors the port-type resolution in
    /// [`Self::check_inst_widths`] / `widths/insts.rs`).
    pub(super) fn fn_type_for_file(&mut self, ffile: usize, ty: &'a Type) -> Ty<'a> {
        let fenv = self.file_consts[ffile].clone();
        let mut fcx = Wcx {
            file: ffile,
            sc: Rc::new(Scope {
                names: HashMap::new(),
            }),
            env: fenv,
            sigs: HashMap::new(),
        };
        self.resolve_ty_silent(&mut fcx, ty)
    }
}
