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
/// - If `expr` is a `BundleLit`, returns the matching field's value.
/// - If `expr` is an `Ident` (a bundle signal reference), returns `expr.field`
///   (dot-access, which `Rw::field` will flatten to `ident_fieldname`).
/// - Otherwise, falls back to a dot-access node.
pub(super) fn bundle_field_expr(expr: &Expr, field: &str, span: mimz_core::span::Span) -> Expr {
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
