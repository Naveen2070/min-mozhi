use super::*;

impl<'a> Checker<'a> {
    /// Walk a module body, checking every width-bearing position.
    pub(super) fn walk_width_items(
        &mut self,
        cx: &mut Wcx<'a>,
        items: &'a [ModuleItem],
        found: &mut Vec<Config>,
    ) {
        for item in items {
            match item {
                ModuleItem::Wire { name, init, .. } => {
                    let expected = cx.sigs.get(&name.name).cloned().unwrap_or(Ty::Unknown);
                    self.check_expr(cx, init, expected);
                }
                ModuleItem::Reg { name, reset, .. } => {
                    let expected = cx.sigs.get(&name.name).cloned().unwrap_or(Ty::Unknown);
                    self.check_expr(cx, reset, expected);
                }
                ModuleItem::SyncLoop(sl) => {
                    let result_t = cx
                        .sigs
                        .get(&format!("{}_result", sl.name.name))
                        .cloned()
                        .unwrap_or(Ty::Unknown);
                    self.check_expr(cx, &sl.result_init, result_t.clone());
                    // Bounds that do not const-eval were already reported by
                    // pass 3 (names.rs) — nothing more to check here. `lo`
                    // isn't used in the width formula below (see the comment
                    // there), but must still const-eval — same skip-if-either-
                    // fails behavior as before Finding 2's fix.
                    let (Ok(_lo), Ok(hi)) = (
                        consteval::eval(&sl.lo, &cx.env).map(|v| v.to_i128_saturating()),
                        consteval::eval(&sl.hi, &cx.env).map(|v| v.to_i128_saturating()),
                    ) else {
                        continue;
                    };
                    // Counter width: `clog2(hi)`, NOT `clog2(hi - lo)` — the
                    // physical `_cnt` register (`ast::lower_sync_loop`) holds
                    // the LIVE INDEX VALUE (`lo..hi-1`), not the iteration
                    // count, so it must be wide enough for `hi - 1`, the
                    // largest value it ever holds, regardless of `lo`. Using
                    // `hi - lo` here under-sizes the body's view of the loop
                    // variable whenever `lo != 0` (final whole-branch review
                    // Finding 2 — mirrors the lowering fix already applied in
                    // `ast::sync_loop_lower`, see
                    // `counter_width_is_clog2_hi_not_clog2_range_when_lo_nonzero`).
                    // This one is a real runtime signal (unlike an ordinary
                    // compile-time `repeat`/`loop` var) — shadow it (and the
                    // accumulator name) in `cx.sigs` for the body walk,
                    // unrolled exactly once (the body is emitted/simulated
                    // once, never per-iteration, unlike `Repeat`/`Loop`).
                    let var_t = bits(consteval::clog2_bits(hi.max(1) as u128) as u128);
                    let shadowed_var = cx.sigs.insert(sl.var.name.clone(), var_t);
                    let shadowed_result = cx.sigs.insert(sl.result_name.name.clone(), result_t);
                    self.seq_width_stmts(cx, &sl.body);
                    match shadowed_var {
                        Some(t) => {
                            cx.sigs.insert(sl.var.name.clone(), t);
                        }
                        None => {
                            cx.sigs.remove(&sl.var.name);
                        }
                    }
                    match shadowed_result {
                        Some(t) => {
                            cx.sigs.insert(sl.result_name.name.clone(), t);
                        }
                        None => {
                            cx.sigs.remove(&sl.result_name.name);
                        }
                    }
                }
                ModuleItem::Mem { name, init, .. } => {
                    // The init value seeds every cell, so it is checked against
                    // the element type.
                    let expected = match cx.sigs.get(&name.name) {
                        Some(Ty::Memory { width, signed, .. }) => {
                            if *signed {
                                Ty::Signed(*width)
                            } else {
                                bits(*width)
                            }
                        }
                        _ => Ty::Unknown,
                    };
                    self.check_expr(cx, init, expected);
                }
                ModuleItem::Drive { lhs, rhs } => {
                    // `lhs.index` on a bundle port is invalid and caught by
                    // an earlier pass; the `cx.sigs` lookup by base name
                    // below is safe regardless, since it only looks at
                    // `lhs.base`, never `lhs.index`.
                    let lhs_bundle = cx.sigs.get(&lhs.base.name).cloned();
                    if let Some(Ty::Bundle {
                        name: l,
                        bfile_hint,
                        args,
                    }) = lhs_bundle
                    {
                        // LHS is bundle-typed: dispatch by RHS shape.
                        match &rhs.kind {
                            ExprKind::BundleLit(inits) => {
                                self.check_bundle_lit(cx, l, bfile_hint, args, inits, rhs.span);
                            }
                            ExprKind::Ident(rhs_sig) => {
                                // Structural type check (feature 2.9): `l`'s fields
                                // must all exist in the RHS bundle's fields with an
                                // exactly-matching type. Same-name bundles never
                                // reach the mismatch arms below (trivially their
                                // own required-fields subset).
                                if let Some(rhs_ty @ Ty::Bundle { .. }) =
                                    cx.sigs.get(rhs_sig.as_str()).cloned()
                                {
                                    let required = Ty::Bundle {
                                        name: l,
                                        bfile_hint,
                                        args,
                                    };
                                    match self.bundle_shape_match(cx, required, rhs_ty, rhs.span) {
                                        BundleShapeMatch::Compatible => {}
                                        BundleShapeMatch::MissingField(field) => {
                                            self.err(
                                                cx.file,
                                                rhs.span,
                                                "E0910",
                                                format!(
                                                    "the connected bundle is missing field `{field}`, which `{l}` requires"
                                                ),
                                                "structural matching allows extra fields on the \
                                                 provided bundle, but never fewer — add the \
                                                 missing field, or connect a bundle that has it",
                                            );
                                        }
                                        BundleShapeMatch::FieldTypeMismatch {
                                            field,
                                            expected,
                                            got,
                                        } => {
                                            self.err(
                                                cx.file,
                                                rhs.span,
                                                "E0907",
                                                format!(
                                                    "field `{field}`: expected {expected}, got {got}"
                                                ),
                                                "widths/types must match exactly — nothing \
                                                 resizes implicitly at a bundle field boundary",
                                            );
                                        }
                                    }
                                }
                            }
                            _ => {
                                // Non-literal, non-ident RHS assigned to a bundle port.
                                // Recurse for inner errors; no scalar type to check against.
                                let _ = self.infer_ty(cx, rhs);
                            }
                        }
                    } else {
                        let expected = self.lvalue_ty(cx, lhs);
                        self.check_expr(cx, rhs, expected);
                    }
                }
                ModuleItem::On(on) => self.seq_width_stmts(cx, &on.body),
                ModuleItem::Inst(inst) => self.check_inst_widths(cx, inst, found),
                ModuleItem::Repeat(r) => {
                    // Bounds that do not const-eval were reported by pass 3.
                    let (Ok(lo), Ok(hi)) = (
                        consteval::eval(&r.lo, &cx.env).map(|v| v.to_i128_saturating()),
                        consteval::eval(&r.hi, &cx.env).map(|v| v.to_i128_saturating()),
                    ) else {
                        continue;
                    };
                    let values: Vec<i128> = if hi - lo > MAX_REPEAT_CHECKS {
                        vec![lo, lo + 1, hi - 1]
                    } else {
                        (lo..hi).collect()
                    };
                    for v in values {
                        let shadowed = cx
                            .env
                            .insert(r.var.name.clone(), consteval::ConstVal::from_i128(v));
                        let before = self.diags.len();
                        self.walk_width_items(cx, &r.items, found);
                        self.unshadow(cx, &r.var.name, shadowed);
                        if self.diags.len() > before {
                            break; // one iteration's worth of errors is enough
                        }
                    }
                }
                ModuleItem::Enum(e) => {
                    let (tag_w, max_payload_w) = self.enum_tag_and_payload_widths(cx, e);
                    let total_w = if max_payload_w == 0 {
                        tag_w
                    } else {
                        tag_w + max_payload_w
                    };
                    e.inferred_total_width.set(Some(total_w as u32));
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &cx.env)
                        .map(|v| v.to_i128_saturating())
                        .unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.walk_width_items(cx, branch, found);
                }
                // Unlike `drivers.rs`/`clocks.rs`, this pass cannot delegate
                // by calling `ast::lower_foreach_item` and recursing into the
                // lowered `Repeat`: `lower_foreach_item` always returns a
                // freshly cloned/substituted `Vec<ModuleItem>` (never `'a`,
                // even for the Range form — see its doc comment), but
                // `walk_width_items` threads `&'a Expr`/`&'a Type` into
                // `check_expr`/`infer_ty` (`widths/expr.rs`, outside this
                // file's scope) via every item it walks — a lowered, owned
                // Vec cannot satisfy that bound without leaking it (E0597
                // confirmed this empirically; see task-6-report.md).
                //
                // Instead this arm walks the RAW (still-`'a`, AST-arena-
                // owned) `fe.items` directly, reusing this file's own
                // existing shadow/restore idioms: the Range form is
                // semantically identical to `ModuleItem::Repeat` above, so
                // it duplicates that exact per-iteration `cx.env` shadow;
                // the Elements form doesn't need the lowered
                // `arr[__foreach_v_idx]` substitution at all for WIDTH
                // purposes (only for driver/clock EDGE tracking) — `var`'s
                // type never varies per iteration, so it's enough to inject
                // `var`'s element type into `cx.sigs` once (the exact
                // pattern `inject_arm_bindings`, below, already uses for
                // match-arm payload bindings) and walk the body a single
                // time.
                ModuleItem::ForEach(fe) => match &fe.source {
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
                        for v in values {
                            let shadowed = cx
                                .env
                                .insert(fe.var.name.clone(), consteval::ConstVal::from_i128(v));
                            let before = self.diags.len();
                            self.walk_width_items(cx, &fe.items, found);
                            self.unshadow(cx, &fe.var.name, shadowed);
                            if self.diags.len() > before {
                                break;
                            }
                        }
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
                            // Not an array/mem: E0417 already reported by
                            // pass 3 (names.rs) — silently skip.
                            _ => continue,
                        };
                        let shadowed = cx.sigs.insert(fe.var.name.clone(), elem_ty);
                        self.walk_width_items(cx, &fe.items, found);
                        match shadowed {
                            Some(t) => {
                                cx.sigs.insert(fe.var.name.clone(), t);
                            }
                            None => {
                                cx.sigs.remove(&fe.var.name);
                            }
                        }
                    }
                },
                ModuleItem::Port { .. }
                | ModuleItem::Clock(_)
                | ModuleItem::Reset { .. }
                | ModuleItem::Const(_)
                | ModuleItem::Error(_) => {}
                ModuleItem::BundleDestructure {
                    bindings,
                    expr,
                    span,
                } => {
                    // E0903: duplicate binding names in the destructure pattern.
                    let mut seen: HashMap<&str, Span> = HashMap::new();
                    for b in bindings {
                        if seen.insert(b.name.as_str(), b.span).is_some() {
                            self.err(
                                cx.file,
                                b.span,
                                "E0903",
                                format!("duplicate binding `{}` in bundle destructure", b.name),
                                "each field can only be bound once in a destructure",
                            );
                        }
                    }
                    // E0907: verify expr is actually bundle-typed (Ty::Unknown for non-bundles
                    // produces no further diagnostic; pass 3 already reported unknown names).
                    let _ = self.infer_ty(cx, expr);
                    let _ = span; // span available for future E0907-on-destructure diagnostics
                }
                ModuleItem::Assert(a) => {
                    self.check_cond(cx, &a.cond);
                }
            }
        }
    }

    fn seq_width_stmts(&mut self, cx: &mut Wcx<'a>, stmts: &'a [SeqStmt]) {
        for s in stmts {
            match s {
                SeqStmt::Assign { lhs, rhs } => {
                    let expected = self.lvalue_ty(cx, lhs);
                    self.check_expr(cx, rhs, expected);
                }
                SeqStmt::If { cond, then, els } => {
                    self.check_cond(cx, cond);
                    self.seq_width_stmts(cx, then);
                    if let Some(els) = els {
                        self.seq_width_stmts(cx, els);
                    }
                }
                SeqStmt::Default { name, val, .. } => {
                    let expected = cx.sigs.get(&name.name).cloned().unwrap_or(Ty::Unknown);
                    self.check_expr(cx, val, expected);
                }
                SeqStmt::Loop {
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
                    for v in values {
                        let shadowed = cx
                            .env
                            .insert(var.name.clone(), consteval::ConstVal::from_i128(v));
                        let before = self.diags.len();
                        self.seq_width_stmts(cx, body);
                        self.unshadow(cx, &var.name, shadowed);
                        if self.diags.len() > before {
                            break;
                        }
                    }
                }
                // Same "raw body + cx.env/cx.sigs shadow" delegation as
                // `ModuleItem::ForEach` above, for the same cross-file
                // `'a` reason (`check_expr` lives in `widths/expr.rs`).
                SeqStmt::ForEach {
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
                        for v in values {
                            let shadowed = cx
                                .env
                                .insert(var.name.clone(), consteval::ConstVal::from_i128(v));
                            let before = self.diags.len();
                            self.seq_width_stmts(cx, body);
                            self.unshadow(cx, &var.name, shadowed);
                            if self.diags.len() > before {
                                break;
                            }
                        }
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
                        let shadowed = cx.sigs.insert(var.name.clone(), elem_ty);
                        self.seq_width_stmts(cx, body);
                        match shadowed {
                            Some(t) => {
                                cx.sigs.insert(var.name.clone(), t);
                            }
                            None => {
                                cx.sigs.remove(&var.name);
                            }
                        }
                    }
                },
                SeqStmt::Assert(a) => {
                    self.check_cond(cx, &a.cond);
                }
                SeqStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// Compute `(tag_width, max_payload_width)` for an enum decl, emitting
    /// E0807 for any payload field whose type is not a concrete bit-vector.
    /// D4: tag_w = clog2(variant_count).max(1); D6: tag-only variants contribute 0 payload.
    pub(super) fn enum_tag_and_payload_widths(
        &mut self,
        cx: &mut Wcx<'a>,
        decl: &'a EnumDecl,
    ) -> (u128, u128) {
        let tag_w = consteval::clog2_bits(decl.variants.len() as u128).max(1) as u128;
        let max_payload = decl
            .variants
            .iter()
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| match self.resolve_ty(cx, &f.ty) {
                        Ty::Bit => 1u128,
                        Ty::Bits(n) | Ty::Signed(n) => n,
                        Ty::Enum(_) | Ty::Memory { .. } => {
                            self.err(
                                cx.file,
                                f.span,
                                "E0807",
                                format!(
                                    "payload field `{}` must be a bit-vector type \
                                     (`bit`, `bits[N]`, `signed[N]`)",
                                    f.name.name
                                ),
                                "enum and memory types cannot be payload fields — \
                                 encode the value as `bits[N]` manually",
                            );
                            0
                        }
                        Ty::Array { .. } => {
                            self.err(
                                cx.file,
                                f.span,
                                "E0807",
                                format!(
                                    "payload field `{}` must be a bit-vector type \
                                     (`bit`, `bits[N]`, `signed[N]`)",
                                    f.name.name
                                ),
                                "arrays cannot be payload fields either — \
                                 encode the value as `bits[N]` manually",
                            );
                            0
                        }
                        _ => 0, // Unknown: E0103 already reported
                    })
                    .sum::<u128>()
            })
            .max()
            .unwrap_or(0);
        (tag_w, max_payload)
    }
}
