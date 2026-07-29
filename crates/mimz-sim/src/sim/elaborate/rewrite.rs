use super::*;
use mimz_core::ast::Pattern;

/// Rewrites expressions/statements during elaboration: enum-variant reads
/// (`State.Red`) → their index literal, instance-port reads (`add.sum`,
/// `fa[i].sum`) → the flat wire name, `match` variant patterns → their index
/// (tag-only) or IntMask (tagged), binding names in arm bodies → payload slice
/// expressions, plus a name substitution map (the `repeat` loop var and
/// inlined child signals). `consts` folds an array index to its concrete value.
///
/// Two lifetimes: `'d` for the module/file decls (long), `'s` for the
/// substitution map (may be a local extended map for match arm bindings).
pub(super) struct Rw<'d, 's> {
    pub(super) insts: &'d HashSet<String>,
    pub(super) enums: &'d HashMap<String, &'d ast::EnumDecl>,
    /// Bundle-typed signal names: `req.valid` → `req_valid` via `field()`.
    pub(super) bundle_sigs: &'d HashSet<String>,
    pub(super) consts: &'d BTreeMap<String, i128>,
    pub(super) subst: &'s HashMap<String, Expr>,
}

impl<'d, 's> Rw<'d, 's> {
    pub(super) fn expr(&self, e: &Expr) -> Result<Expr, String> {
        let kind = match &e.kind {
            ExprKind::Int { .. } | ExprKind::Bool(_) => e.kind.clone(),
            ExprKind::Ident(n) => {
                if let Some(r) = self.subst.get(n) {
                    return Ok(r.clone());
                }
                e.kind.clone()
            }
            ExprKind::Field { base, field } => return self.field(e, base, field),
            ExprKind::Unary { op, expr } => ExprKind::Unary {
                op: *op,
                expr: Box::new(self.expr(expr)?),
            },
            // `raw ?? 0` (unwrap form only — the operand stays scalar-typed
            // after the `??`). Rewrite to `if raw.valid { raw.data } else { 0 }`
            // as an `IfExpr` (not rendered text — this AST is what the
            // event-driven kernel interprets at runtime). Mirrors
            // `emit_verilog`'s Task 7 ternary lowering. The OR-mux form
            // (`x ?? y` where both sides — and the result — stay
            // bundle-typed) never reaches this generic scalar-expression
            // rewrite; it's handled at the bundle-field level in
            // `bundle_field_expr` (Task 10).
            ExprKind::Binary {
                op: BinOp::Coalesce,
                lhs,
                rhs,
            } => {
                let valid_field = Expr {
                    kind: ExprKind::Field {
                        base: lhs.clone(),
                        field: ast::Ident {
                            name: "valid".to_string(),
                            span: lhs.span,
                        },
                    },
                    span: lhs.span,
                };
                let data_field = Expr {
                    kind: ExprKind::Field {
                        base: lhs.clone(),
                        field: ast::Ident {
                            name: "data".to_string(),
                            span: lhs.span,
                        },
                    },
                    span: lhs.span,
                };
                return self.expr(&Expr {
                    kind: ExprKind::IfExpr {
                        cond: Box::new(valid_field),
                        then: Box::new(data_field),
                        els: rhs.clone(),
                    },
                    span: e.span,
                });
            }
            ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
                op: *op,
                lhs: Box::new(self.expr(lhs)?),
                rhs: Box::new(self.expr(rhs)?),
            },
            ExprKind::IfExpr { cond, then, els } => ExprKind::IfExpr {
                cond: Box::new(self.expr(cond)?),
                then: Box::new(self.expr(then)?),
                els: Box::new(self.expr(els)?),
            },
            ExprKind::Match { scrutinee, arms } => {
                let rw_scrutinee = self.expr(scrutinee)?;
                let rw_arms = arms
                    .iter()
                    .map(|a| {
                        // For tagged variant patterns with bindings, extract
                        // each binding as a payload slice of the scrutinee so
                        // the runtime evaluator never sees Pattern::Variant.
                        let binding_subst = self.variant_bindings(&a.patterns, &rw_scrutinee)?;
                        let rw_value = if binding_subst.is_empty() {
                            self.expr(&a.value)?
                        } else {
                            let mut ext_subst: HashMap<String, Expr> = self
                                .subst
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            ext_subst.extend(binding_subst);
                            let ext_rw = Rw {
                                insts: self.insts,
                                enums: self.enums,
                                bundle_sigs: self.bundle_sigs,
                                consts: self.consts,
                                subst: &ext_subst,
                            };
                            ext_rw.expr(&a.value)?
                        };
                        Ok::<_, String>(ast::Arm {
                            patterns: a
                                .patterns
                                .iter()
                                .map(|p| self.pattern(p))
                                .collect::<Result<_, _>>()?,
                            value: rw_value,
                        })
                    })
                    .collect::<Result<_, _>>()?;
                ExprKind::Match {
                    scrutinee: Box::new(rw_scrutinee),
                    arms: rw_arms,
                }
            }
            ExprKind::Concat(parts) => ExprKind::Concat(
                parts
                    .iter()
                    .map(|p| self.expr(p))
                    .collect::<Result<_, _>>()?,
            ),
            ExprKind::Replicate { count, parts } => ExprKind::Replicate {
                count: Box::new(self.expr(count)?),
                parts: parts
                    .iter()
                    .map(|p| self.expr(p))
                    .collect::<Result<_, _>>()?,
            },
            ExprKind::Index { base, index } => ExprKind::Index {
                base: Box::new(self.expr(base)?),
                index: Box::new(self.expr(index)?),
            },
            ExprKind::Slice { base, hi, lo } => ExprKind::Slice {
                base: Box::new(self.expr(base)?),
                hi: Box::new(self.expr(hi)?),
                lo: Box::new(self.expr(lo)?),
            },
            ExprKind::Call { func, args } => ExprKind::Call {
                func: *func,
                args: args
                    .iter()
                    .map(|a| self.expr(a))
                    .collect::<Result<_, _>>()?,
            },
            // Pass FnCall through with args rewritten (const-folded, signal-names
            // substituted). The runtime evaluator handles the call.
            ExprKind::FnCall { name, args } => ExprKind::FnCall {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|a| self.expr(a))
                    .collect::<Result<_, _>>()?,
            },
            ExprKind::BundleLit(_) => {
                return Err("bundle literal in unsupported expression position".to_string());
            }
            ExprKind::ArrayLit(elems) => ExprKind::ArrayLit(
                elems
                    .iter()
                    .map(|e| self.expr(e))
                    .collect::<Result<_, _>>()?,
            ),
            ExprKind::EnumConstruct {
                enum_name,
                variant,
                args,
            } => return self.enum_construct(enum_name, variant, args, e.span),
        };
        Ok(Expr { kind, span: e.span })
    }

    /// Lowers `ExprKind::EnumConstruct` into the same tag+payload bit
    /// layout `pattern()`/`variant_bindings()` (above) already assume —
    /// mirrors their exact `tag_w`/`max_payload_w` computation. Produces a
    /// plain `ExprKind::Int` for a tag-only (zero-arg) variant, or an
    /// `ExprKind::Concat` otherwise — both already fully evaluated by
    /// `crate::sim::value`, so no new interpreter code is needed.
    fn enum_construct(
        &self,
        enum_name: &ast::Ident,
        variant: &ast::Ident,
        args: &[Expr],
        span: mimz_core::span::Span,
    ) -> Result<Expr, String> {
        let edecl = self
            .enums
            .get(&enum_name.name)
            .ok_or_else(|| format!("unknown enum `{}`", enum_name.name))?;
        let idx = edecl
            .variants
            .iter()
            .position(|v| v.name.name == variant.name)
            .ok_or_else(|| {
                format!(
                    "enum `{}` has no variant `{}`",
                    enum_name.name, variant.name
                )
            })?;
        let total_w = edecl
            .inferred_total_width
            .get()
            .expect("checker must run before elaborate") as u128;
        let tag_w = clog2(edecl.variants.len()) as u128;
        let max_payload_w = total_w - tag_w;
        if max_payload_w == 0 {
            return Ok(int_expr(idx as i128, span));
        }
        let decl_v = &edecl.variants[idx];
        // Every part of the concat must carry its EXACT bit width: a bare
        // `ExprKind::Int` literal evaluates to its own minimal width (e.g.
        // `0` is 1 bit — see `Val::from_int`), not the tag/field/padding
        // width the layout requires, so tag, args, and padding are all
        // wrapped in `extend(_, N)` to pin their width explicitly. A
        // non-literal arg (an ident/signal) already carries its declared
        // width; `extend` to that same width is then a no-op.
        let mut parts = Vec::new();
        // `tag_w` is `clog2(variant_count)`, which floors at 1 for any
        // legal (>=1-variant) enum — this branch always taken in practice.
        // Guarded anyway as defense in depth, matching the padding guard
        // below for the symmetric zero-width case.
        if tag_w > 0 {
            parts.push(extend_to(int_expr(idx as i128, span), tag_w as u32, span));
        }
        let mut used_w = 0u128;
        for (a, field) in args.iter().zip(decl_v.fields.iter()) {
            // Identical inline match to `variant_bindings`'s own field-width
            // computation above in this same file.
            let field_w: u128 = match &field.ty {
                ast::Type::Bit => 1,
                ast::Type::Bits(e) | ast::Type::Signed(e) => {
                    const_eval(e, self.consts).unwrap_or(0) as u128
                }
                ast::Type::Named(_) | ast::Type::Bundle { .. } | ast::Type::Array { .. } => 0,
            };
            used_w += field_w;
            parts.push(extend_to(self.expr(a)?, field_w as u32, span));
        }
        let padding_w = max_payload_w - used_w;
        if padding_w > 0 {
            parts.push(extend_to(int_expr(0, span), padding_w as u32, span));
        }
        Ok(Expr {
            kind: ExprKind::Concat(parts),
            span,
        })
    }

    pub(super) fn field(&self, e: &Expr, base: &Expr, field: &ast::Ident) -> Result<Expr, String> {
        if let ExprKind::Ident(b) = &base.kind {
            // `Enum.Variant` → its index literal.
            if let Some(edecl) = self.enums.get(b) {
                let idx = edecl
                    .variants
                    .iter()
                    .position(|v| v.name.name == field.name)
                    .ok_or_else(|| format!("enum `{b}` has no variant `{}`", field.name))?;
                return Ok(int_expr(idx as i128, e.span));
            }
            // `inst.port` → flat wire `inst_port`.
            if self.insts.contains(b) {
                return Ok(ident_expr(format!("{b}_{}", field.name), e.span));
            }
            // `bundle_signal.field` → flat scalar `bundle_signal_field`.
            if self.bundle_sigs.contains(b) {
                return Ok(ident_expr(format!("{b}_{}", field.name), e.span));
            }
        }
        // `arr[i].port` → flat wire `arr__<i>_port` (the index folds here).
        if let ExprKind::Index { base: arr, index } = &base.kind
            && let ExprKind::Ident(arr_name) = &arr.kind
            && self.insts.contains(arr_name)
        {
            let n = const_eval(&self.expr(index)?, self.consts)?;
            return Ok(ident_expr(
                format!("{arr_name}__{n}_{}", field.name),
                e.span,
            ));
        }
        // Some other field access — keep the shape (recurse into the base).
        Ok(Expr {
            kind: ExprKind::Field {
                base: Box::new(self.expr(base)?),
                field: field.clone(),
            },
            span: e.span,
        })
    }

    fn pattern(&self, p: &Pattern) -> Result<Pattern, String> {
        match p {
            Pattern::Variant {
                enum_name,
                variant,
                bindings: _,
            } => {
                let edecl = self
                    .enums
                    .get(&enum_name.name)
                    .ok_or_else(|| format!("unknown enum `{}`", enum_name.name))?;
                let idx = edecl
                    .variants
                    .iter()
                    .position(|v| v.name.name == variant.name)
                    .ok_or_else(|| {
                        format!(
                            "enum `{}` has no variant `{}`",
                            enum_name.name, variant.name
                        )
                    })?;
                // note: fallback only correct for tag-only enums
                let total_w = edecl
                    .inferred_total_width
                    .get()
                    .unwrap_or_else(|| {
                        debug_assert!(
                            edecl.variants.iter().all(|v| v.fields.is_empty()),
                            "tagged enum reached pattern fallback without inferred_total_width — run checker first"
                        );
                        clog2(edecl.variants.len())
                    })
                    as u128;
                let tag_w = clog2(edecl.variants.len()) as u128;
                let max_payload_w = total_w - tag_w;
                if max_payload_w == 0 {
                    // Tag-only: simple integer comparison (existing behaviour).
                    Ok(Pattern::Int {
                        value: (idx as u128).into(),
                        raw: idx.to_string(),
                    })
                } else {
                    // Tagged: check only the tag bits (MSBs) via IntMask so
                    // payload bits are ignored by the pattern comparison.
                    let tag_val = (idx as u128) << max_payload_w;
                    let tag_mask = ((1u128 << tag_w) - 1) << max_payload_w;
                    Ok(Pattern::IntMask {
                        value: tag_val,
                        mask: tag_mask,
                        width: total_w as u32,
                        raw: idx.to_string(),
                    })
                }
            }
            other => Ok(other.clone()),
        }
    }

    /// Compute payload binding → slice-expression substitutions for the first
    /// variant pattern with bindings in `patterns`. Returns empty if no such
    /// pattern exists or the enum is tag-only.
    fn variant_bindings(
        &self,
        patterns: &[Pattern],
        scrutinee: &Expr,
    ) -> Result<HashMap<String, Expr>, String> {
        for p in patterns {
            let Pattern::Variant {
                enum_name,
                variant,
                bindings,
            } = p
            else {
                continue;
            };
            if bindings.is_empty() {
                continue;
            }
            let Some(edecl) = self.enums.get(&enum_name.name) else {
                continue;
            };
            let total_w = edecl
                .inferred_total_width
                .get()
                .unwrap_or_else(|| {
                    debug_assert!(
                        edecl.variants.iter().all(|v| v.fields.is_empty()),
                        "tagged enum reached pattern fallback without inferred_total_width — run checker first"
                    );
                    clog2(edecl.variants.len())
                }) as u128;
            let tag_w = clog2(edecl.variants.len()) as u128;
            let max_payload_w = total_w - tag_w;
            if max_payload_w == 0 {
                continue; // tag-only, no payload to extract
            }
            let Some(vdecl) = edecl.variants.iter().find(|v| v.name.name == variant.name) else {
                continue;
            };
            // Fields are packed MSB-first inside [max_payload_w-1 : 0].
            let mut cursor = max_payload_w;
            let mut out: HashMap<String, Expr> = HashMap::new();
            for (field, binding) in vdecl.fields.iter().zip(bindings.iter()) {
                let field_w: u128 = match &field.ty {
                    ast::Type::Bit => 1,
                    ast::Type::Bits(e) | ast::Type::Signed(e) => {
                        const_eval(e, self.consts).unwrap_or(0) as u128
                    }
                    ast::Type::Named(_) => 0, // E0807: already rejected by checker
                    ast::Type::Bundle { .. } => 0, // E0807 rejects bundle payload fields in enums
                    // Arrays are the same category as bundles here (not a scalar
                    // bit-vector payload field): fold to 0, matching the sibling
                    // arm exactly rather than inventing new behavior.
                    ast::Type::Array { .. } => 0,
                };
                debug_assert!(
                    field_w > 0,
                    "E0807 should have rejected zero-width payload fields before emit/sim"
                );
                if field_w == 0 {
                    continue;
                }
                let hi = cursor - 1;
                let lo = cursor - field_w;
                cursor -= field_w;
                let span = binding.span;
                // binding → scrutinee[hi:lo]
                out.insert(
                    binding.name.clone(),
                    Expr {
                        kind: ExprKind::Slice {
                            base: Box::new(scrutinee.clone()),
                            hi: Box::new(int_expr(hi as i128, span)),
                            lo: Box::new(int_expr(lo as i128, span)),
                        },
                        span,
                    },
                );
            }
            return Ok(out);
        }
        Ok(HashMap::new())
    }

    /// Rewrite a sequential statement, renaming each assignment target via
    /// `rename` (prefixing inlined child regs; identity for the parent).
    pub(super) fn seq(
        &self,
        s: &SeqStmt,
        rename: &dyn Fn(&str) -> String,
    ) -> Result<SeqStmt, String> {
        Ok(match s {
            SeqStmt::Assign { lhs, rhs } => SeqStmt::Assign {
                lhs: ast::LValue {
                    base: ast::Ident {
                        name: rename(&lhs.base.name),
                        span: lhs.base.span,
                    },
                    index: match &lhs.index {
                        Some((a, b)) => {
                            Some((self.expr(a)?, b.as_ref().map(|x| self.expr(x)).transpose()?))
                        }
                        None => None,
                    },
                    span: lhs.span,
                },
                rhs: self.expr(rhs)?,
            },
            SeqStmt::If { cond, then, els } => SeqStmt::If {
                cond: self.expr(cond)?,
                then: then
                    .iter()
                    .map(|x| self.seq(x, rename))
                    .collect::<Result<_, _>>()?,
                els: match els {
                    Some(e) => Some(
                        e.iter()
                            .map(|x| self.seq(x, rename))
                            .collect::<Result<_, _>>()?,
                    ),
                    None => None,
                },
            },
            SeqStmt::Default { name, val, span } => SeqStmt::Default {
                name: ast::Ident {
                    name: rename(&name.name),
                    span: name.span,
                },
                val: self.expr(val)?,
                span: *span,
            },
            SeqStmt::Loop {
                var,
                lo,
                hi,
                body,
                span,
            } => SeqStmt::Loop {
                var: var.clone(),
                lo: self.expr(lo)?,
                hi: self.expr(hi)?,
                body: body
                    .iter()
                    .map(|x| self.seq(x, rename))
                    .collect::<Result<_, _>>()?,
                span: *span,
            },
            // Unreachable: every `SeqStmt::ForEach` in an `on`-block body is
            // lowered before `Rw::seq` ever runs — see
            // `elaborate_module`'s `ModuleItem::On` arm.
            SeqStmt::ForEach { .. } => unreachable!(
                "ForEach is lowered before Rw::seq/assigns/run_seq ever run — see elaborate_module's ModuleItem::On arm"
            ),
            // Unreachable on the sim path (strict-parsed tree); pass through.
            SeqStmt::Error(sp) => SeqStmt::Error(*sp),
        })
    }
}
