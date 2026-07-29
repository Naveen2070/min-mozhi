use super::*;

impl<'a> Checker<'a> {
    pub(super) fn lvalue(&mut self, file: usize, sc: &Scope<'a>, env: &Env, lv: &LValue) {
        match sc.names.get(&lv.base.name) {
            Some(Bind::Out | Bind::Wire | Bind::Reg) => {}
            Some(Bind::Mem) => {
                // A memory is addressed one cell at a time — a whole-memory
                // assignment is meaningless.
                if lv.index.is_none() {
                    self.err(
                        file,
                        lv.base.span,
                        "E0108",
                        format!("cannot assign to memory `{}` as a whole", lv.base.name),
                        "address one cell — `m[addr] <- value`",
                    );
                }
            }
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

    pub(super) fn expr(&mut self, file: usize, sc: &Scope<'a>, env: &Env, e: &Expr) {
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
                    Some(Bind::In) | Some(Bind::Out) | Some(Bind::Wire) | Some(Bind::Reg)
                    | Some(Bind::Param) => {
                        // Possible bundle field access (e.g., req.valid where req is a bundle
                        // port/signal/param). Validation deferred to the width pass where
                        // bundle types are fully resolved. `Param` included alongside the
                        // signal kinds so a bundle-typed `fn` parameter's field access
                        // (`h.valid`) isn't rejected here before the width pass — which
                        // resolves `cx.sigs` from the real bundle type — gets a chance to
                        // check it (see checker::widths::Ty::Bundle).
                    }
                    Some(b)
                        if self.lookup_enum(sc, name).is_none() && !matches!(b, Bind::Enum(_)) =>
                    {
                        let what = b.what();
                        self.err(
                            file,
                            field.span,
                            "E0105",
                            format!("`{name}` is {what} — it has no fields"),
                            "`.` reads an enum variant (`State.Red`), an \
                             instance output (`add.sum`), or a bundle field (`bus.valid`)",
                        );
                    }
                    _ => match self.lookup_enum(sc, name) {
                        Some(en) => {
                            if !en.variants.iter().any(|v| v.name.name == field.name) {
                                let list: Vec<&str> =
                                    en.variants.iter().map(|v| v.name.name.as_str()).collect();
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
                    let mut arm_sc = Scope {
                        names: sc.names.clone(),
                    };

                    // Phases 1+2: validate each sub-pattern; collect binding info.
                    // Each entry: (reporting_span, [(binding_name, type_str)]).
                    // `skip` = true on any E0806 / E0103 / unknown enum — Phases 3–5 are
                    // skipped for this arm to avoid cascading errors on bad inputs.
                    let mut skip = false;
                    let mut alt_bindings = Vec::new();

                    for p in &arm.patterns {
                        match p {
                            Pattern::Variant {
                                enum_name,
                                variant,
                                bindings,
                            } => match self.lookup_enum(sc, &enum_name.name) {
                                Some(en) => {
                                    if let Some(decl_v) =
                                        en.variants.iter().find(|v| v.name.name == variant.name)
                                    {
                                        let expected = decl_v.fields.len();
                                        let got = bindings.len();
                                        if expected != got {
                                            let help = if expected == 0 {
                                                format!(
                                                    "`{}.{}` is tag-only — remove the `(...)` binding list",
                                                    enum_name.name, variant.name
                                                )
                                            } else {
                                                format!(
                                                    "provide exactly {} binding(s) — one per payload field",
                                                    expected
                                                )
                                            };
                                            self.err(
                                                file,
                                                variant.span,
                                                "E0806",
                                                format!(
                                                    "pattern for `{}.{}` binds {} name(s) but the variant has {} field(s)",
                                                    enum_name.name, variant.name, got, expected
                                                ),
                                                help,
                                            );
                                            skip = true;
                                        } else {
                                            let pairs: Vec<(String, String)> = bindings
                                                .iter()
                                                .zip(decl_v.fields.iter())
                                                .map(|(b, f)| {
                                                    (b.name.clone(), crate::pretty::type_str(&f.ty))
                                                })
                                                .collect();
                                            alt_bindings.push((variant.span, pairs));
                                        }
                                    } else {
                                        let list: Vec<&str> = en
                                            .variants
                                            .iter()
                                            .map(|v| v.name.name.as_str())
                                            .collect();
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
                                        skip = true;
                                    }
                                }
                                None => {
                                    self.unknown(file, &enum_name.name, enum_name.span);
                                    skip = true;
                                }
                            },
                            _ => {
                                // Wildcard / Int / Bool: empty binding set.
                                // Span: arm body (no pattern span available for these variants).
                                alt_bindings.push((arm.value.span, vec![]));
                            }
                        }
                    }

                    // Error recovery or single-pattern arm: inject all valid bindings seen
                    // so far and skip intersection validation.
                    if skip || alt_bindings.len() <= 1 {
                        for (_, pairs) in &alt_bindings {
                            for (name, _) in pairs {
                                arm_sc.names.insert(name.clone(), Bind::Param);
                            }
                        }
                        self.expr(file, &arm_sc, env, &arm.value);
                        continue;
                    }

                    // Phase 3: every alternative must bind the same set of names (positionally).
                    let ref_pairs = &alt_bindings[0].1;
                    let ref_names: Vec<&str> = ref_pairs.iter().map(|(n, _)| n.as_str()).collect();
                    let mut ok = true;

                    for (curr_span, curr_pairs) in &alt_bindings[1..] {
                        let curr_names: Vec<&str> =
                            curr_pairs.iter().map(|(n, _)| n.as_str()).collect();
                        // note: positional comparison — binding order matches field declaration order; named binding syntax would need set-equality here
                        if ref_names != curr_names {
                            self.err(
                                file,
                                *curr_span,
                                "E0808",
                                "OR-pattern alternatives must bind the same variables".to_string(),
                                format!(
                                    "expected: {{{}}}; this alternative provides: {{{}}}",
                                    ref_names.join(", "),
                                    curr_names.join(", ")
                                ),
                            );
                            ok = false;
                            break;
                        }
                    }

                    if !ok {
                        self.expr(file, &arm_sc, env, &arm.value);
                        continue;
                    }

                    // Phase 4: each binding must have the same type in every alternative.
                    // Iterate names in sorted order (D5 — deterministic diagnostics).
                    let mut sorted_idx: Vec<usize> = (0..ref_pairs.len()).collect();
                    sorted_idx.sort_by_key(|&i| &ref_pairs[i].0);

                    'width: for (curr_span, curr_pairs) in &alt_bindings[1..] {
                        for &i in &sorted_idx {
                            let (ref_name, ref_ty) = &ref_pairs[i];
                            let (_, curr_ty) = &curr_pairs[i];
                            if ref_ty != curr_ty {
                                self.err(
                                    file,
                                    *curr_span,
                                    "E0808",
                                    format!(
                                        "binding `{ref_name}` has incompatible types across OR-pattern alternatives"
                                    ),
                                    format!(
                                        "first alternative: {ref_ty}; this alternative: {curr_ty}"
                                    ),
                                );
                                ok = false;
                                break 'width;
                            }
                        }
                    }

                    if !ok {
                        self.expr(file, &arm_sc, env, &arm.value);
                        continue;
                    }

                    // Phase 5: identical names and types verified — inject reference bindings.
                    for (name, _) in ref_pairs {
                        arm_sc.names.insert(name.clone(), Bind::Param);
                    }
                    self.expr(file, &arm_sc, env, &arm.value);
                }
            }
            ExprKind::Concat(parts) => {
                for p in parts {
                    self.expr(file, sc, env, p);
                }
            }
            ExprKind::Replicate { count, parts } => {
                self.expr(file, sc, env, count);
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
            ExprKind::FnCall { name, args } => {
                if let Some((_, decl)) = self.funcs.get(&name.name) {
                    let expected = decl.params.len();
                    let got = args.len();
                    if expected != got {
                        self.err(
                            file,
                            name.span,
                            "E0803",
                            format!(
                                "`{}` takes {} argument(s), got {}",
                                name.name, expected, got
                            ),
                            format!("pass exactly {} argument(s) to `{}`", expected, name.name),
                        );
                    }
                } else {
                    self.err(
                        file,
                        name.span,
                        "E1110",
                        format!("`{}` is not a function or builtin — only declared `fn`s and builtins are callable", name.name),
                        "declare a `fn` with this name, or use a builtin (`extend`, `trunc`, `signed`, `unsigned`)",
                    );
                }
                for a in args {
                    self.expr(file, sc, env, a);
                }
            }
            ExprKind::BundleLit(fields) => {
                for f in fields {
                    self.expr(file, sc, env, &f.value);
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems {
                    self.expr(file, sc, env, e);
                }
            }
            ExprKind::EnumConstruct {
                enum_name,
                variant,
                args,
            } => {
                match self.lookup_enum(sc, &enum_name.name) {
                    Some(en) => {
                        if let Some(decl_v) =
                            en.variants.iter().find(|v| v.name.name == variant.name)
                        {
                            let expected = decl_v.fields.len();
                            let got = args.len();
                            if expected != got {
                                let help = if expected == 0 {
                                    format!(
                                        "`{}.{}` is tag-only — remove the `(...)` argument list",
                                        enum_name.name, variant.name
                                    )
                                } else {
                                    format!(
                                        "provide exactly {} argument(s) — one per payload field",
                                        expected
                                    )
                                };
                                self.err(
                                    file,
                                    variant.span,
                                    "E0806",
                                    format!(
                                        "construction of `{}.{}` passes {} argument(s) but the variant has {} field(s)",
                                        enum_name.name, variant.name, got, expected
                                    ),
                                    help,
                                );
                            }
                        } else {
                            let list: Vec<&str> =
                                en.variants.iter().map(|v| v.name.name.as_str()).collect();
                            self.err(
                                file,
                                variant.span,
                                "E0103",
                                format!(
                                    "enum `{}` has no variant `{}`",
                                    enum_name.name, variant.name
                                ),
                                format!("`{}`'s variants are: {}", enum_name.name, list.join(", ")),
                            );
                        }
                    }
                    None => self.unknown(file, &enum_name.name, enum_name.span),
                }
                for a in args {
                    self.expr(file, sc, env, a);
                }
            }
        }
    }
}
