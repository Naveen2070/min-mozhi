use super::*;

/// Returns true if `ty` is a bundle type (either `Type::Bundle` or a
/// `Type::Named` that names a registered bundle — not an enum).
pub(super) fn is_bundle_ty(
    ty: &ast::Type,
    bundle_reg: &BundleRegistry<'_>,
    enums: &HashMap<String, &ast::EnumDecl>,
) -> bool {
    match ty {
        ast::Type::Bundle { .. } => true,
        ast::Type::Named(id) => {
            bundle_reg.contains_key(&id.name.name) && !enums.contains_key(&id.name.name)
        }
        _ => false,
    }
}

/// Extract `(bundle_qual_ident, args)` from a bundle type, or `None` for
/// non-bundle types. Returns the full `QualIdent` (not just the bare name)
/// so the caller can resolve it against a same-named bundle in another
/// file via [`resolve_bundle`] instead of collapsing to a bare-name lookup.
pub(super) fn bundle_type_info(
    ty: &ast::Type,
    bundle_reg: &BundleRegistry<'_>,
    enums: &HashMap<String, &ast::EnumDecl>,
) -> Option<(ast::QualIdent, Vec<NamedArg>)> {
    match ty {
        ast::Type::Bundle { name, args } => Some((name.clone(), args.clone())),
        ast::Type::Named(id)
            if bundle_reg.contains_key(&id.name.name) && !enums.contains_key(&id.name.name) =>
        {
            Some((id.clone(), vec![]))
        }
        _ => None,
    }
}

/// Extract the expression for a named field from a bundle expression.
/// - If `expr` is a bundle-typed `Coalesce` (`lhs ?? rhs`), builds the OR-mux
///   form: an `if lhs.valid { … } else { … }` node per field (see the match
///   arm below).
/// - If `expr` is a `BundleLit`, returns the matching field's value.
/// - If `expr` is an `Ident` (a bundle signal reference), returns `expr.field`
///   (dot-access, which `Rw::field` will flatten to `ident_fieldname`).
/// - Otherwise, falls back to a dot-access node.
pub(super) fn bundle_field_expr(expr: &Expr, field: &str, span: crate::span::Span) -> Expr {
    // OR-mux form: `lhs ?? rhs` where both operands (and the result) stay
    // bundle-typed. `merged.valid = lhs.valid || rhs.valid` (built as
    // `if lhs.valid { true } else { rhs.valid }`); every other field is
    // `if lhs.valid { lhs.field } else { rhs.field }`. Extracted per-field
    // by RECURSING into `bundle_field_expr` for `lhs`/`rhs` rather than
    // wrapping them in a bare `Field` node — `??` is left-associative and
    // chains (`x ?? y ?? z` parses as `Coalesce(Coalesce(x, y), z)`), so
    // `lhs` can itself be a `Coalesce` node: a bundle-typed compound
    // expression, not a plain signal reference. Recursing here re-enters
    // this same match and expands the nested chain correctly; wrapping it
    // in `Field { base: lhs, field }` instead would hand a `Coalesce` base
    // to `Rw::field`'s fallback, which recurses through `Rw::expr`'s
    // generic (unwrap-form) `Coalesce` arm — the wrong semantics for a
    // still-bundle-typed nested operand. Mirrors the fix applied to
    // `emit_verilog`'s `coalesce_field_expr` in Task 8's review.
    if let ExprKind::Binary {
        op: BinOp::Coalesce,
        lhs,
        rhs,
    } = &expr.kind
    {
        let cond = bundle_field_expr(lhs, "valid", span);
        let (then, els) = if field == "valid" {
            (
                Expr {
                    kind: ExprKind::Bool(true),
                    span,
                },
                bundle_field_expr(rhs, "valid", span),
            )
        } else {
            (
                bundle_field_expr(lhs, field, span),
                bundle_field_expr(rhs, field, span),
            )
        };
        return Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            span,
        };
    }
    if let ExprKind::BundleLit(inits) = &expr.kind {
        if let Some(fi) = inits.iter().find(|fi| fi.name.name == field) {
            return fi.value.clone();
        } else {
            unreachable!(
                "BundleLit missing field `{}` — checker should have rejected this",
                field
            )
        }
    }
    // RHS is a bundle ident or other expr: emit `expr.field` — Rw::field will flatten it.
    Expr {
        kind: ExprKind::Field {
            base: Box::new(expr.clone()),
            field: ast::Ident {
                name: field.to_string(),
                span,
            },
        },
        span,
    }
}

/// Pre-flatten every bundle-typed parameter of `f` into one scalar
/// `FnParam` per field (named `<param>_<field>`, its width folded to a
/// concrete literal from THIS bundle's own resolved fields — never
/// symbolic, since a `fn`'s own const environment is file-scope only,
/// spec D8) and rewrite every `param.field` read in the body to the
/// matching flat name (BUG-15). Built ONCE here, not per-call, so
/// `eval_fn_call`'s existing param-binding loop (scalar + array cases
/// only — no bundle case needed) and `Rw::expand_fn_call_args`'s
/// call-site expansion naturally agree on both field order and the local
/// names bound under, without threading any bundle-registry knowledge
/// into the runtime evaluator itself.
pub(super) fn flatten_bundle_params_in_func(
    f: &FuncDecl,
    bundle_reg: &BundleRegistry<'_>,
    enums: &HashMap<String, &ast::EnumDecl>,
    imports: &[ast::Import],
    consts: &BTreeMap<String, i128>,
) -> Result<FuncDecl, Box<Diag>> {
    let mut bundle_params: HashSet<String> = HashSet::new();
    let mut new_params: Vec<ast::FnParam> = Vec::new();
    for p in &f.params {
        match bundle_type_info(&p.ty, bundle_reg, enums) {
            Some((bname, bargs)) => {
                bundle_params.insert(p.name.name.clone());
                let fields =
                    resolve_bundle_fields_sim(bundle_reg, imports, &bname, &bargs, consts)?;
                for (fname, w) in fields {
                    let ty = if w.signed {
                        ast::Type::Signed(Box::new(int_expr(w.bits as i128, p.span)))
                    } else {
                        ast::Type::Bits(Box::new(int_expr(w.bits as i128, p.span)))
                    };
                    new_params.push(ast::FnParam {
                        name: ast::Ident {
                            name: format!("{}_{fname}", p.name.name),
                            span: p.span,
                        },
                        ty,
                        span: p.span,
                    });
                }
            }
            None => new_params.push(p.clone()),
        }
    }
    if bundle_params.is_empty() {
        // Common case: no bundle-typed param — skip the body walk below.
        return Ok(f.clone());
    }
    Ok(FuncDecl {
        name: f.name.clone(),
        params: new_params,
        ret: f.ret.clone(),
        stmts: flatten_bundle_refs_stmts(&f.stmts, &bundle_params),
        tail: flatten_bundle_refs_expr(&f.tail, &bundle_params),
        span: f.span,
    })
}

fn flatten_bundle_refs_stmts(
    stmts: &[ast::FnStmt],
    bundle_params: &HashSet<String>,
) -> Vec<ast::FnStmt> {
    stmts
        .iter()
        .map(|s| flatten_bundle_refs_stmt(s, bundle_params))
        .collect()
}

fn flatten_bundle_refs_stmt(s: &ast::FnStmt, bundle_params: &HashSet<String>) -> ast::FnStmt {
    match s {
        ast::FnStmt::Let(local) => ast::FnStmt::Let(ast::LocalLet {
            name: local.name.clone(),
            value: flatten_bundle_refs_expr(&local.value, bundle_params),
            span: local.span,
            inferred_width: local.inferred_width.clone(),
        }),
        ast::FnStmt::If { cond, then, els } => ast::FnStmt::If {
            cond: flatten_bundle_refs_expr(cond, bundle_params),
            then: flatten_bundle_refs_stmts(then, bundle_params),
            els: els
                .as_ref()
                .map(|e| flatten_bundle_refs_stmts(e, bundle_params)),
        },
        ast::FnStmt::Return(e) => ast::FnStmt::Return(flatten_bundle_refs_expr(e, bundle_params)),
        ast::FnStmt::Loop {
            var,
            lo,
            hi,
            body,
            span,
        } => ast::FnStmt::Loop {
            var: var.clone(),
            lo: flatten_bundle_refs_expr(lo, bundle_params),
            hi: flatten_bundle_refs_expr(hi, bundle_params),
            body: flatten_bundle_refs_stmts(body, bundle_params),
            span: *span,
        },
        ast::FnStmt::ForEach {
            var,
            source,
            body,
            span,
        } => ast::FnStmt::ForEach {
            var: var.clone(),
            source: source.clone(),
            body: flatten_bundle_refs_stmts(body, bundle_params),
            span: *span,
        },
        ast::FnStmt::Error(sp) => ast::FnStmt::Error(*sp),
    }
}

/// Recursively replace `Field { base: Ident(p), field }` with
/// `Ident("{p}_{field}")` wherever `p` is in `bundle_params` — the
/// fn-body counterpart of `Rw::field`'s `bundle_sigs` rule, scoped to the
/// callee's own bundle-typed parameters. Every other node is walked
/// structurally and left otherwise unchanged.
fn flatten_bundle_refs_expr(e: &Expr, bundle_params: &HashSet<String>) -> Expr {
    if let ExprKind::Field { base, field } = &e.kind
        && let ExprKind::Ident(b) = &base.kind
        && bundle_params.contains(b)
    {
        return ident_expr(format!("{b}_{}", field.name), e.span);
    }
    let kind = match &e.kind {
        ExprKind::Int { value, raw } => ExprKind::Int {
            value: value.clone(),
            raw: raw.clone(),
        },
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Ident(name) => ExprKind::Ident(name.clone()),
        ExprKind::Field { base, field } => ExprKind::Field {
            base: Box::new(flatten_bundle_refs_expr(base, bundle_params)),
            field: field.clone(),
        },
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(flatten_bundle_refs_expr(expr, bundle_params)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(flatten_bundle_refs_expr(lhs, bundle_params)),
            rhs: Box::new(flatten_bundle_refs_expr(rhs, bundle_params)),
        },
        ExprKind::IfExpr { cond, then, els } => ExprKind::IfExpr {
            cond: Box::new(flatten_bundle_refs_expr(cond, bundle_params)),
            then: Box::new(flatten_bundle_refs_expr(then, bundle_params)),
            els: Box::new(flatten_bundle_refs_expr(els, bundle_params)),
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(flatten_bundle_refs_expr(scrutinee, bundle_params)),
            arms: arms
                .iter()
                .map(|a| ast::Arm {
                    patterns: a.patterns.clone(),
                    value: flatten_bundle_refs_expr(&a.value, bundle_params),
                })
                .collect(),
        },
        ExprKind::Concat(parts) => ExprKind::Concat(
            parts
                .iter()
                .map(|p| flatten_bundle_refs_expr(p, bundle_params))
                .collect(),
        ),
        ExprKind::Replicate { count, parts } => ExprKind::Replicate {
            count: Box::new(flatten_bundle_refs_expr(count, bundle_params)),
            parts: parts
                .iter()
                .map(|p| flatten_bundle_refs_expr(p, bundle_params))
                .collect(),
        },
        ExprKind::Index { base, index } => ExprKind::Index {
            base: Box::new(flatten_bundle_refs_expr(base, bundle_params)),
            index: Box::new(flatten_bundle_refs_expr(index, bundle_params)),
        },
        ExprKind::Slice { base, hi, lo } => ExprKind::Slice {
            base: Box::new(flatten_bundle_refs_expr(base, bundle_params)),
            hi: Box::new(flatten_bundle_refs_expr(hi, bundle_params)),
            lo: Box::new(flatten_bundle_refs_expr(lo, bundle_params)),
        },
        ExprKind::Call { func, args } => ExprKind::Call {
            func: *func,
            args: args
                .iter()
                .map(|a| flatten_bundle_refs_expr(a, bundle_params))
                .collect(),
        },
        ExprKind::FnCall { name, args } => ExprKind::FnCall {
            name: name.clone(),
            args: args
                .iter()
                .map(|a| flatten_bundle_refs_expr(a, bundle_params))
                .collect(),
        },
        ExprKind::BundleLit(fields) => ExprKind::BundleLit(
            fields
                .iter()
                .map(|f| ast::FieldInit {
                    name: f.name.clone(),
                    value: flatten_bundle_refs_expr(&f.value, bundle_params),
                    span: f.span,
                })
                .collect(),
        ),
        ExprKind::ArrayLit(elems) => ExprKind::ArrayLit(
            elems
                .iter()
                .map(|e| flatten_bundle_refs_expr(e, bundle_params))
                .collect(),
        ),
        ExprKind::EnumConstruct {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumConstruct {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: args
                .iter()
                .map(|a| flatten_bundle_refs_expr(a, bundle_params))
                .collect(),
        },
    };
    Expr { kind, span: e.span }
}
