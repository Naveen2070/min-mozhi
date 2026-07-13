//! Lowers `ForEach` (module-item, `on`-block-statement, and `fn`-body-
//! statement forms) into `Repeat`/`SeqStmt::Loop`/`FnStmt::Loop`. Unlike
//! `sync_loop_lower.rs` (which the checker deliberately does NOT delegate
//! into — `SyncLoop` has genuinely new FSM semantics needing its own
//! checks), `foreach` is pure sugar with IDENTICAL semantics to a
//! hand-written `repeat`/`loop` once lowered. The checker therefore
//! delegates width/driver/clock checking by lowering and recursing into
//! each pass's existing `Repeat`/`Loop` arm — see the ForEach arms added
//! in `checker/names.rs`, `checker/widths/mod.rs`, `checker/drivers.rs`,
//! `checker/clocks.rs`, `checker/funcs.rs`. The one piece of checker logic
//! `foreach` genuinely owns is validating an Elements-form source resolves
//! to an array/`mem` type (E0417) — that's `array_like_len` returning
//! `None`.

use super::{
    Arm, Conn, ConstDecl, Expr, ExprKind, FieldInit, FnParam, FnStmt, ForEach, ForEachSource,
    Ident, Inst, LocalLet, ModuleItem, NamedArg, OnBlock, Pattern, Repeat, SeqStmt, SyncLoop, Type,
};
use crate::span::Span;
use std::cell::Cell;

/// Looks up `name` among `items`' `Port`/`Wire`/`Reg`/`Mem` declarations
/// and returns its element type and length expression if it's an array
/// or `mem` type. `None` if `name` isn't declared in `items`, or is
/// declared with a non-array/mem type.
pub fn array_like_len(name: &str, items: &[ModuleItem]) -> Option<(Type, Expr)> {
    items.iter().find_map(|it| match it {
        ModuleItem::Port {
            name: n,
            ty: Type::Array { elem, len },
            ..
        } if n.name == name => Some((elem.as_ref().clone(), len.as_ref().clone())),
        ModuleItem::Wire {
            name: n,
            ty: Type::Array { elem, len },
            ..
        } if n.name == name => Some((elem.as_ref().clone(), len.as_ref().clone())),
        ModuleItem::Reg {
            name: n,
            ty: Type::Array { elem, len },
            ..
        } if n.name == name => Some((elem.as_ref().clone(), len.as_ref().clone())),
        ModuleItem::Mem {
            name: n, ty, depth, ..
        } if n.name == name => Some((ty.clone(), depth.clone())),
        _ => None,
    })
}

/// Same as [`array_like_len`] but for a `fn` body's own parameters. A `fn`
/// declaration is always project-top-level (never nested in a module — see
/// `TopItem::Func`), so an Elements-form `foreach` inside a `fn` body has no
/// sibling module items to resolve against; the only legal source is one of
/// the `fn`'s own array-typed `FnParam`s.
pub fn array_like_len_fn(name: &str, params: &[FnParam]) -> Option<(Type, Expr)> {
    params.iter().find_map(|p| match &p.ty {
        Type::Array { elem, len } if p.name.name == name => {
            Some((elem.as_ref().clone(), len.as_ref().clone()))
        }
        _ => None,
    })
}

fn zero(span: Span) -> Expr {
    Expr {
        kind: ExprKind::Int {
            value: 0,
            raw: "0".into(),
        },
        span,
    }
}

fn ident_expr(name: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Ident(name.to_string()),
        span,
    }
}

fn index_expr(base: &str, index: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Index {
            base: Box::new(ident_expr(base, span)),
            index: Box::new(ident_expr(index, span)),
        },
        span,
    }
}

/// Module-item-level `foreach`. `sibling_items` is the enclosing module's
/// full item list (needed to resolve an Elements-form source's length).
///
/// The Elements form does NOT synthesize a `Wire` binding for `var` — a
/// pre-existing checker rule (`no_decls_in_repeat`, E0303) unconditionally
/// rejects any declaration (`Wire`/`Reg`/`Mem`/`Const`/`Enum`) inside a
/// `Repeat`'s body (repeat unrolling would otherwise produce N colliding
/// declarations of the same name), and by the time that rule runs it's
/// looking at the LOWERED `Repeat`, indistinguishable from a hand-written
/// one. Instead, exactly like `lower_foreach_seq`, `var` is substituted
/// with `arr[idx]` throughout `fe.items` via `subst_module_item`.
pub fn lower_foreach_item(fe: &ForEach, sibling_items: &[ModuleItem]) -> Option<Vec<ModuleItem>> {
    match &fe.source {
        ForEachSource::Range { lo, hi } => Some(vec![ModuleItem::Repeat(Repeat {
            var: fe.var.clone(),
            lo: lo.clone(),
            hi: hi.clone(),
            items: fe.items.clone(),
            span: fe.span,
        })]),
        ForEachSource::Elements(arr) => {
            let (_elem_ty, len) = array_like_len(&arr.name, sibling_items)?;
            let idx_name = format!("__foreach_{}_idx", fe.var.name);
            let idx_var = Ident {
                name: idx_name.clone(),
                span: fe.span,
            };
            let replacement = index_expr(&arr.name, &idx_name, fe.span);
            let items: Vec<ModuleItem> = fe
                .items
                .iter()
                .map(|it| subst_module_item(it, &fe.var.name, &replacement))
                .collect();
            Some(vec![ModuleItem::Repeat(Repeat {
                var: idx_var,
                lo: zero(fe.span),
                hi: len,
                items,
                span: fe.span,
            })])
        }
    }
}

/// Replaces every `ExprKind::Ident(name)` read of `target` with
/// `replacement`, recursively. Used by `lower_foreach_seq` for the
/// Elements form, since `SeqStmt` (unlike `FnStmt`) has no local-binding
/// statement to introduce `var` with — see `lower_foreach_fn` for the
/// `FnStmt::Let`-based alternative that doesn't need substitution.
fn subst_expr(e: &Expr, target: &str, replacement: &Expr) -> Expr {
    let kind = match &e.kind {
        ExprKind::Ident(name) if name == target => return replacement.clone(),
        ExprKind::Int { value, raw } => ExprKind::Int {
            value: *value,
            raw: raw.clone(),
        },
        ExprKind::Bool(b) => ExprKind::Bool(*b),
        ExprKind::Ident(name) => ExprKind::Ident(name.clone()),
        ExprKind::Field { base, field } => ExprKind::Field {
            base: Box::new(subst_expr(base, target, replacement)),
            field: field.clone(),
        },
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(subst_expr(expr, target, replacement)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(subst_expr(lhs, target, replacement)),
            rhs: Box::new(subst_expr(rhs, target, replacement)),
        },
        ExprKind::IfExpr { cond, then, els } => ExprKind::IfExpr {
            cond: Box::new(subst_expr(cond, target, replacement)),
            then: Box::new(subst_expr(then, target, replacement)),
            els: Box::new(subst_expr(els, target, replacement)),
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(subst_expr(scrutinee, target, replacement)),
            arms: arms
                .iter()
                .map(|a| {
                    // A `Pattern::Variant` binding scoped to this arm's
                    // `value` shadows `target` within it — same shadowing
                    // rule `sync_loop_lower.rs`'s `rename_expr` already
                    // applies to match arms. Pass the arm through unchanged
                    // rather than substituting into a scope where `target`
                    // no longer refers to the outer binding.
                    let shadowed = a.patterns.iter().any(|p| {
                        matches!(p, Pattern::Variant { bindings, .. } if bindings.iter().any(|b| b.name == target))
                    });
                    Arm {
                        patterns: a.patterns.clone(),
                        value: if shadowed {
                            a.value.clone()
                        } else {
                            subst_expr(&a.value, target, replacement)
                        },
                    }
                })
                .collect(),
        },
        ExprKind::Concat(parts) => {
            ExprKind::Concat(parts.iter().map(|p| subst_expr(p, target, replacement)).collect())
        }
        ExprKind::Replicate { count, parts } => ExprKind::Replicate {
            count: Box::new(subst_expr(count, target, replacement)),
            parts: parts.iter().map(|p| subst_expr(p, target, replacement)).collect(),
        },
        ExprKind::Index { base, index } => ExprKind::Index {
            base: Box::new(subst_expr(base, target, replacement)),
            index: Box::new(subst_expr(index, target, replacement)),
        },
        ExprKind::Slice { base, hi, lo } => ExprKind::Slice {
            base: Box::new(subst_expr(base, target, replacement)),
            hi: Box::new(subst_expr(hi, target, replacement)),
            lo: Box::new(subst_expr(lo, target, replacement)),
        },
        ExprKind::Call { func, args } => ExprKind::Call {
            func: *func,
            args: args.iter().map(|a| subst_expr(a, target, replacement)).collect(),
        },
        ExprKind::FnCall { name, args } => ExprKind::FnCall {
            name: name.clone(),
            args: args.iter().map(|a| subst_expr(a, target, replacement)).collect(),
        },
        ExprKind::BundleLit(fields) => ExprKind::BundleLit(
            fields
                .iter()
                .map(|f| FieldInit {
                    name: f.name.clone(),
                    value: subst_expr(&f.value, target, replacement),
                    span: f.span,
                })
                .collect(),
        ),
        ExprKind::ArrayLit(elems) => {
            ExprKind::ArrayLit(elems.iter().map(|e| subst_expr(e, target, replacement)).collect())
        }
    };
    Expr { kind, span: e.span }
}

fn subst_seq_stmt(s: &SeqStmt, target: &str, replacement: &Expr) -> SeqStmt {
    match s {
        SeqStmt::Assign { lhs, rhs } => SeqStmt::Assign {
            lhs: lhs.clone(),
            rhs: subst_expr(rhs, target, replacement),
        },
        SeqStmt::If { cond, then, els } => SeqStmt::If {
            cond: subst_expr(cond, target, replacement),
            then: then
                .iter()
                .map(|s| subst_seq_stmt(s, target, replacement))
                .collect(),
            els: els.as_ref().map(|es| {
                es.iter()
                    .map(|s| subst_seq_stmt(s, target, replacement))
                    .collect()
            }),
        },
        SeqStmt::Default { name, val, span } => SeqStmt::Default {
            name: name.clone(),
            val: subst_expr(val, target, replacement),
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
            lo: subst_expr(lo, target, replacement),
            hi: subst_expr(hi, target, replacement),
            // A nested loop's own `var` shadows `target` within its own
            // `body` — same shadowing rule `sync_loop_lower.rs`'s
            // `rename_seq_stmt` already applies to its `Loop` arm. Leave
            // `body` untouched rather than substituting into a scope where
            // `target` no longer refers to the outer binding.
            body: if var.name == target {
                body.clone()
            } else {
                body.iter()
                    .map(|s| subst_seq_stmt(s, target, replacement))
                    .collect()
            },
            span: *span,
        },
        SeqStmt::ForEach {
            var,
            source,
            body,
            span,
        } => SeqStmt::ForEach {
            var: var.clone(),
            // `source` is still evaluated in the OUTER scope (the nested
            // loop var doesn't exist yet when its own source is computed),
            // so it always substitutes regardless of shadowing.
            source: match source {
                ForEachSource::Range { lo, hi } => ForEachSource::Range {
                    lo: subst_expr(lo, target, replacement),
                    hi: subst_expr(hi, target, replacement),
                },
                ForEachSource::Elements(id) => ForEachSource::Elements(id.clone()),
            },
            // Same shadowing rule as `Loop` above: a nested `foreach`'s own
            // `var` shadows `target` within its own `body`.
            body: if var.name == target {
                body.clone()
            } else {
                body.iter()
                    .map(|s| subst_seq_stmt(s, target, replacement))
                    .collect()
            },
            span: *span,
        },
        SeqStmt::Error(sp) => SeqStmt::Error(*sp),
    }
}

/// Same substitution as `subst_expr`/`subst_seq_stmt`, exhaustive over all
/// `ModuleItem` variants. Used by `lower_foreach_item`'s Elements form (see
/// its doc comment for why substitution replaces the earlier `Wire`-binding
/// approach). Declarations (`Wire`/`Reg`/`Mem`/`Const`) still get their
/// driving `Expr` substituted even though the checker's `no_decls_in_repeat`
/// (E0303) forbids them appearing inside `fe.items` in practice — this
/// function makes no assumption about what's already been checked.
fn subst_module_item(it: &ModuleItem, target: &str, replacement: &Expr) -> ModuleItem {
    match it {
        ModuleItem::Port { dir, name, ty } => ModuleItem::Port {
            dir: *dir,
            name: name.clone(),
            ty: ty.clone(),
        },
        ModuleItem::Clock(id) => ModuleItem::Clock(id.clone()),
        ModuleItem::Reset { name, is_async } => ModuleItem::Reset {
            name: name.clone(),
            is_async: *is_async,
        },
        ModuleItem::Wire { name, ty, init } => ModuleItem::Wire {
            name: name.clone(),
            ty: ty.clone(),
            init: subst_expr(init, target, replacement),
        },
        ModuleItem::Reg { name, ty, reset } => ModuleItem::Reg {
            name: name.clone(),
            ty: ty.clone(),
            reset: subst_expr(reset, target, replacement),
        },
        ModuleItem::Mem {
            name,
            ty,
            depth,
            init,
        } => ModuleItem::Mem {
            name: name.clone(),
            ty: ty.clone(),
            depth: subst_expr(depth, target, replacement),
            init: subst_expr(init, target, replacement),
        },
        ModuleItem::Const(c) => ModuleItem::Const(ConstDecl {
            name: c.name.clone(),
            ty: c.ty,
            value: subst_expr(&c.value, target, replacement),
        }),
        ModuleItem::Enum(e) => ModuleItem::Enum(e.clone()),
        ModuleItem::Inst(inst) => ModuleItem::Inst(Inst {
            name: inst.name.clone(),
            index: inst
                .index
                .as_ref()
                .map(|e| subst_expr(e, target, replacement)),
            module: inst.module.clone(),
            args: inst
                .args
                .iter()
                .map(|a| NamedArg {
                    name: a.name.clone(),
                    value: subst_expr(&a.value, target, replacement),
                })
                .collect(),
            conns: inst
                .conns
                .iter()
                .map(|c| Conn {
                    port: c.port.clone(),
                    signal: subst_expr(&c.signal, target, replacement),
                })
                .collect(),
            span: inst.span,
        }),
        ModuleItem::On(on) => ModuleItem::On(OnBlock {
            clock: on.clock.clone(),
            edge: on.edge,
            body: on
                .body
                .iter()
                .map(|s| subst_seq_stmt(s, target, replacement))
                .collect(),
            span: on.span,
        }),
        ModuleItem::Drive { lhs, rhs } => ModuleItem::Drive {
            lhs: lhs.clone(),
            rhs: subst_expr(rhs, target, replacement),
        },
        ModuleItem::Repeat(r) => ModuleItem::Repeat(Repeat {
            var: r.var.clone(),
            lo: subst_expr(&r.lo, target, replacement),
            hi: subst_expr(&r.hi, target, replacement),
            // A nested `repeat`'s own `var` shadows `target` within its own
            // `items` — same shadowing rule as `subst_seq_stmt`'s `Loop` arm.
            items: if r.var.name == target {
                r.items.clone()
            } else {
                r.items
                    .iter()
                    .map(|it| subst_module_item(it, target, replacement))
                    .collect()
            },
            span: r.span,
        }),
        ModuleItem::ForEach(inner) => ModuleItem::ForEach(ForEach {
            var: inner.var.clone(),
            // `source` is evaluated in the OUTER scope (the nested
            // foreach's own var doesn't exist yet when its source is
            // computed), so it always substitutes regardless of shadowing.
            source: match &inner.source {
                ForEachSource::Range { lo, hi } => ForEachSource::Range {
                    lo: subst_expr(lo, target, replacement),
                    hi: subst_expr(hi, target, replacement),
                },
                ForEachSource::Elements(id) => ForEachSource::Elements(id.clone()),
            },
            // Same shadowing rule as `Repeat` above.
            items: if inner.var.name == target {
                inner.items.clone()
            } else {
                inner
                    .items
                    .iter()
                    .map(|it| subst_module_item(it, target, replacement))
                    .collect()
            },
            span: inner.span,
        }),
        // A `sync loop` nested inside a `foreach` Elements-form body is
        // legal (`no_decls_in_repeat`/E0303 allows `SyncLoop` inside a
        // `Repeat`/`ForEach` body) and can reference the outer `foreach`
        // var — substitute into its const-eval'd bounds/init and into
        // `body`, with the same shadowing rule as `Repeat`/`ForEach` above:
        // `sl.var`/`sl.result_name` are new bindings introduced by the sync
        // loop itself, so either shadowing `target` means `body` must NOT
        // be substituted (it refers to the sync loop's own var/result, not
        // the outer foreach var). `name`/`clock`/`var`/`result_name` are
        // Ident fields naming/declaring something, never a value read, so
        // (like `Repeat.var`/`ForEach.var` above) they are cloned, not
        // substituted.
        ModuleItem::SyncLoop(sl) => ModuleItem::SyncLoop(Box::new(SyncLoop {
            name: sl.name.clone(),
            clock: sl.clock.clone(),
            edge: sl.edge,
            var: sl.var.clone(),
            lo: subst_expr(&sl.lo, target, replacement),
            hi: subst_expr(&sl.hi, target, replacement),
            result_name: sl.result_name.clone(),
            result_ty: sl.result_ty.clone(),
            result_init: subst_expr(&sl.result_init, target, replacement),
            body: if sl.var.name == target || sl.result_name.name == target {
                sl.body.clone()
            } else {
                sl.body
                    .iter()
                    .map(|s| subst_seq_stmt(s, target, replacement))
                    .collect()
            },
            span: sl.span,
        })),
        ModuleItem::ConstIf {
            cond,
            then,
            els,
            span,
        } => ModuleItem::ConstIf {
            cond: subst_expr(cond, target, replacement),
            then: then
                .iter()
                .map(|it| subst_module_item(it, target, replacement))
                .collect(),
            els: els.as_ref().map(|es| {
                es.iter()
                    .map(|it| subst_module_item(it, target, replacement))
                    .collect()
            }),
            span: *span,
        },
        ModuleItem::BundleDestructure {
            bindings,
            expr,
            span,
        } => ModuleItem::BundleDestructure {
            bindings: bindings.clone(),
            expr: subst_expr(expr, target, replacement),
            span: *span,
        },
        ModuleItem::Error(sp) => ModuleItem::Error(*sp),
    }
}

/// Statement-level `foreach` inside an `on` block. `module_items` is the
/// enclosing module's full item list. Returns a single-element
/// `Vec<SeqStmt>` wrapping a `SeqStmt::Loop` — a `Vec` return (not a bare
/// `SeqStmt`) mirrors `lower_foreach_item`'s shape and lets a future
/// multi-statement lowering extend without a signature change.
pub fn lower_foreach_seq(
    var: &Ident,
    source: &ForEachSource,
    body: &[SeqStmt],
    span: Span,
    module_items: &[ModuleItem],
) -> Option<Vec<SeqStmt>> {
    match source {
        ForEachSource::Range { lo, hi } => Some(vec![SeqStmt::Loop {
            var: var.clone(),
            lo: lo.clone(),
            hi: hi.clone(),
            body: body.to_vec(),
            span,
        }]),
        ForEachSource::Elements(arr) => {
            let (_elem_ty, len) = array_like_len(&arr.name, module_items)?;
            let idx_name = format!("__foreach_{}_idx", var.name);
            let idx_var = Ident {
                name: idx_name.clone(),
                span,
            };
            let replacement = index_expr(&arr.name, &idx_name, span);
            let substituted: Vec<SeqStmt> = body
                .iter()
                .map(|s| subst_seq_stmt(s, &var.name, &replacement))
                .collect();
            Some(vec![SeqStmt::Loop {
                var: idx_var,
                lo: zero(span),
                hi: len,
                body: substituted,
                span,
            }])
        }
    }
}

/// Statement-level `foreach` inside a `fn` body. Unlike `lower_foreach_seq`
/// (which must substitute `var` throughout the body because `SeqStmt` has
/// no local-binding statement), `FnStmt` has `Let(LocalLet)` — the
/// Elements form binds `var` with a real `let`, and never needs to touch
/// `body`.
///
/// `params` is the enclosing `fn`'s own parameter list — NOT sibling module
/// items. A `fn` declaration is always project-top-level (see
/// `TopItem::Func`), so there is no enclosing module to resolve an
/// Elements-form source against; the only legal source is one of the `fn`'s
/// own array-typed `FnParam`s (see `array_like_len_fn`).
pub fn lower_foreach_fn(
    var: &Ident,
    source: &ForEachSource,
    body: &[FnStmt],
    span: Span,
    params: &[FnParam],
) -> Option<Vec<FnStmt>> {
    match source {
        ForEachSource::Range { lo, hi } => Some(vec![FnStmt::Loop {
            var: var.clone(),
            lo: lo.clone(),
            hi: hi.clone(),
            body: body.to_vec(),
            span,
        }]),
        ForEachSource::Elements(arr) => {
            let (_elem_ty, len) = array_like_len_fn(&arr.name, params)?;
            let idx_name = format!("__foreach_{}_idx", var.name);
            let idx_var = Ident {
                name: idx_name.clone(),
                span,
            };
            let binding = FnStmt::Let(LocalLet {
                name: var.clone(),
                value: index_expr(&arr.name, &idx_name, span),
                span,
                inferred_width: Cell::new(None),
            });
            let mut new_body = vec![binding];
            new_body.extend(body.iter().cloned());
            Some(vec![FnStmt::Loop {
                var: idx_var,
                lo: zero(span),
                hi: len,
                body: new_body,
                span,
            }])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Edge, LValue};
    use crate::span::Span;

    fn sp() -> Span {
        Span::new(0, 0)
    }

    fn id(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: sp(),
        }
    }

    #[test]
    fn range_form_lowers_to_repeat_unchanged() {
        let fe = ForEach {
            var: id("i"),
            source: ForEachSource::Range {
                lo: super::zero(sp()),
                hi: Expr {
                    kind: ExprKind::Int {
                        value: 4,
                        raw: "4".into(),
                    },
                    span: sp(),
                },
            },
            items: vec![],
            span: sp(),
        };
        let lowered = lower_foreach_item(&fe, &[]).expect("range form never needs sibling_items");
        assert_eq!(lowered.len(), 1);
        assert!(matches!(&lowered[0], ModuleItem::Repeat(r) if r.var.name == "i"));
    }

    #[test]
    fn elements_form_resolves_array_port_length() {
        let sibling_items = vec![ModuleItem::Port {
            dir: super::super::Dir::In,
            name: id("values"),
            ty: Type::Array {
                elem: Box::new(Type::Bits(Box::new(Expr {
                    kind: ExprKind::Int {
                        value: 8,
                        raw: "8".into(),
                    },
                    span: sp(),
                }))),
                len: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: 8,
                        raw: "8".into(),
                    },
                    span: sp(),
                }),
            },
        }];
        let fe = ForEach {
            var: id("v"),
            source: ForEachSource::Elements(id("values")),
            // `sum = v` — a `Drive` item referencing the bound element
            // value, to prove substitution (not a synthesized `Wire`
            // binding — see `lower_foreach_item`'s doc comment for why:
            // a declaration inside a lowered `Repeat`'s items is rejected
            // by the checker's `no_decls_in_repeat`, E0303).
            items: vec![ModuleItem::Drive {
                lhs: LValue {
                    base: id("sum"),
                    index: None,
                    span: sp(),
                },
                rhs: Expr {
                    kind: ExprKind::Ident("v".into()),
                    span: sp(),
                },
            }],
            span: sp(),
        };
        let lowered = lower_foreach_item(&fe, &sibling_items).expect("array port must resolve");
        let ModuleItem::Repeat(r) = &lowered[0] else {
            panic!("expected Repeat")
        };
        assert!(
            r.items
                .iter()
                .all(|it| !matches!(it, ModuleItem::Wire { .. })),
            "Elements form must not synthesize a Wire declaration (E0303)"
        );
        let ModuleItem::Drive { rhs, .. } = &r.items[0] else {
            panic!("expected the Drive item, substituted")
        };
        assert!(
            matches!(&rhs.kind, ExprKind::Index { .. }),
            "`v` must be substituted with `values[__foreach_v_idx]`, got {rhs:?}"
        );
    }

    #[test]
    fn elements_form_on_undeclared_name_returns_none() {
        let fe = ForEach {
            var: id("v"),
            source: ForEachSource::Elements(id("nope")),
            items: vec![],
            span: sp(),
        };
        assert!(lower_foreach_item(&fe, &[]).is_none());
    }

    #[test]
    fn seq_elements_form_substitutes_var_with_index_expr() {
        let sibling_items = vec![ModuleItem::Mem {
            name: id("m"),
            ty: Type::Bits(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            })),
            depth: Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            },
            init: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
        }];
        let body = vec![SeqStmt::Assign {
            lhs: LValue {
                base: id("acc"),
                index: None,
                span: sp(),
            },
            rhs: Expr {
                kind: ExprKind::Ident("v".into()),
                span: sp(),
            },
        }];
        let lowered = lower_foreach_seq(
            &id("v"),
            &ForEachSource::Elements(id("m")),
            &body,
            sp(),
            &sibling_items,
        )
        .expect("mem must resolve");
        let SeqStmt::Loop { body: inner, .. } = &lowered[0] else {
            panic!("expected Loop")
        };
        let SeqStmt::Assign { rhs, .. } = &inner[0] else {
            panic!("expected Assign")
        };
        assert!(
            matches!(&rhs.kind, ExprKind::Index { .. }),
            "`v` must be substituted with `m[__foreach_v_idx]`, got {rhs:?}"
        );
    }

    /// Regression: `foreach v in arr { loop v: 0..4 { acc <- v } }` — the
    /// inner `loop`'s own `v` must shadow the outer `foreach`'s `v` within
    /// its own body. Before the fix, `subst_seq_stmt` recursed into the
    /// nested `Loop`'s body unconditionally and rewrote the INNER loop's
    /// own `v` reads to the outer array-index expression too — silently
    /// wrong hardware, no compile error. Mirrors
    /// `sync_loop_lower.rs`'s `rename_expr_match_arm_binding_shadows_accumulator_name`.
    #[test]
    fn loop_var_shadowing_outer_foreach_var_is_not_substituted() {
        let sibling_items = vec![ModuleItem::Mem {
            name: id("arr"),
            ty: Type::Bits(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            })),
            depth: Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            },
            init: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
        }];
        let inner_loop = SeqStmt::Loop {
            var: id("v"),
            lo: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
            hi: Expr {
                kind: ExprKind::Int {
                    value: 4,
                    raw: "4".into(),
                },
                span: sp(),
            },
            body: vec![SeqStmt::Assign {
                lhs: LValue {
                    base: id("acc"),
                    index: None,
                    span: sp(),
                },
                rhs: Expr {
                    kind: ExprKind::Ident("v".into()),
                    span: sp(),
                },
            }],
            span: sp(),
        };
        let lowered = lower_foreach_seq(
            &id("v"),
            &ForEachSource::Elements(id("arr")),
            &[inner_loop],
            sp(),
            &sibling_items,
        )
        .expect("mem must resolve");
        let SeqStmt::Loop {
            body: outer_body, ..
        } = &lowered[0]
        else {
            panic!("expected outer Loop")
        };
        let SeqStmt::Loop {
            body: inner_body, ..
        } = &outer_body[0]
        else {
            panic!("expected inner Loop preserved")
        };
        let SeqStmt::Assign { rhs, .. } = &inner_body[0] else {
            panic!("expected Assign")
        };
        assert!(
            matches!(&rhs.kind, ExprKind::Ident(n) if n == "v"),
            "inner loop's own `v` must shadow the outer foreach var and stay unsubstituted, got {rhs:?}"
        );
    }

    /// Regression: a `Pattern::Variant` binding scoped to one match arm's
    /// `value` shadows the substitution target within that arm only — the
    /// other arm (whose pattern binds nothing) must still substitute. Mirrors
    /// `sync_loop_lower.rs`'s `rename_expr_match_arm_binding_shadows_accumulator_name`.
    #[test]
    fn subst_expr_match_arm_binding_shadows_target() {
        let replacement = Expr {
            kind: ExprKind::Ident("REPL".into()),
            span: sp(),
        };
        let e = Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(Expr {
                    kind: ExprKind::Ident("x".into()),
                    span: sp(),
                }),
                arms: vec![
                    Arm {
                        // `E.Tag(v)` — binds a fresh local `v`, shadowing the
                        // substitution target within this arm's value only.
                        patterns: vec![Pattern::Variant {
                            enum_name: id("E"),
                            variant: id("Tag"),
                            bindings: vec![id("v")],
                        }],
                        value: Expr {
                            kind: ExprKind::Ident("v".into()),
                            span: sp(),
                        },
                    },
                    Arm {
                        patterns: vec![Pattern::Wildcard],
                        value: Expr {
                            kind: ExprKind::Ident("v".into()),
                            span: sp(),
                        },
                    },
                ],
            },
            span: sp(),
        };
        let result = subst_expr(&e, "v", &replacement);
        let ExprKind::Match { arms, .. } = &result.kind else {
            panic!("expected Match")
        };
        assert!(
            matches!(&arms[0].value.kind, ExprKind::Ident(n) if n == "v"),
            "pattern-bound `v` must not be substituted, got {:?}",
            arms[0].value
        );
        assert!(
            matches!(&arms[1].value.kind, ExprKind::Ident(n) if n == "REPL"),
            "unshadowed `v` must be substituted, got {:?}",
            arms[1].value
        );
    }

    /// Regression: `foreach v in arr { repeat v: 0..4 { d = v } }` — a
    /// nested `repeat`'s own `var` shadows the outer `foreach`'s `var`
    /// within its own `items`, same shadowing rule as the `SeqStmt::Loop`
    /// case above, now for `subst_module_item`.
    #[test]
    fn nested_repeat_var_shadowing_outer_foreach_var_is_not_substituted() {
        let sibling_items = vec![ModuleItem::Mem {
            name: id("arr"),
            ty: Type::Bits(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            })),
            depth: Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            },
            init: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
        }];
        let inner_repeat = ModuleItem::Repeat(Repeat {
            var: id("v"),
            lo: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
            hi: Expr {
                kind: ExprKind::Int {
                    value: 4,
                    raw: "4".into(),
                },
                span: sp(),
            },
            items: vec![ModuleItem::Drive {
                lhs: LValue {
                    base: id("d"),
                    index: None,
                    span: sp(),
                },
                rhs: Expr {
                    kind: ExprKind::Ident("v".into()),
                    span: sp(),
                },
            }],
            span: sp(),
        });
        let fe = ForEach {
            var: id("v"),
            source: ForEachSource::Elements(id("arr")),
            items: vec![inner_repeat],
            span: sp(),
        };
        let lowered = lower_foreach_item(&fe, &sibling_items).expect("mem must resolve");
        let ModuleItem::Repeat(outer) = &lowered[0] else {
            panic!("expected outer Repeat")
        };
        let ModuleItem::Repeat(inner) = &outer.items[0] else {
            panic!("expected inner Repeat preserved")
        };
        let ModuleItem::Drive { rhs, .. } = &inner.items[0] else {
            panic!("expected Drive")
        };
        assert!(
            matches!(&rhs.kind, ExprKind::Ident(n) if n == "v"),
            "inner repeat's own `v` must shadow the outer foreach var and stay unsubstituted, got {rhs:?}"
        );
    }

    /// Regression: `foreach v in arr { sync loop foo on rise(clk) (i: 0..4)
    /// -> result: bits[8] = 0 { result <- v } }` — a nested `sync loop` is
    /// legal inside a `foreach`/`repeat` body (`no_decls_in_repeat`/E0303
    /// exempts it) and can reference the outer `foreach` var; the fix
    /// substitutes into the sync loop's `body` (and would into
    /// `lo`/`hi`/`result_init` too), not just clone it through unchanged.
    #[test]
    fn nested_sync_loop_body_substitutes_outer_foreach_var() {
        let sibling_items = vec![ModuleItem::Mem {
            name: id("arr"),
            ty: Type::Bits(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            })),
            depth: Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            },
            init: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
        }];
        let inner_sync_loop = ModuleItem::SyncLoop(Box::new(SyncLoop {
            name: id("foo"),
            clock: id("clk"),
            edge: Edge::Rise,
            var: id("i"),
            lo: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
            hi: Expr {
                kind: ExprKind::Int {
                    value: 4,
                    raw: "4".into(),
                },
                span: sp(),
            },
            result_name: id("result"),
            result_ty: Type::Bits(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp(),
            })),
            result_init: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp(),
            },
            body: vec![SeqStmt::Assign {
                lhs: LValue {
                    base: id("result"),
                    index: None,
                    span: sp(),
                },
                rhs: Expr {
                    kind: ExprKind::Ident("v".into()),
                    span: sp(),
                },
            }],
            span: sp(),
        }));
        let fe = ForEach {
            var: id("v"),
            source: ForEachSource::Elements(id("arr")),
            items: vec![inner_sync_loop],
            span: sp(),
        };
        let lowered = lower_foreach_item(&fe, &sibling_items).expect("mem must resolve");
        let ModuleItem::Repeat(outer) = &lowered[0] else {
            panic!("expected outer Repeat")
        };
        let ModuleItem::SyncLoop(sl) = &outer.items[0] else {
            panic!("expected SyncLoop preserved")
        };
        let SeqStmt::Assign { rhs, .. } = &sl.body[0] else {
            panic!("expected Assign")
        };
        assert!(
            matches!(&rhs.kind, ExprKind::Index { base, .. } if matches!(&base.kind, ExprKind::Ident(n) if n == "arr")),
            "outer foreach var `v` inside a nested sync loop's body must be substituted with `arr[idx]`, got {rhs:?}"
        );
    }

    /// Regression (Bug 2): `foreach x in arr { ... }` inside a `fn` body,
    /// where `arr` is one of the `fn`'s own array-typed parameters — a `fn`
    /// is always project-top-level, so there is no module context for the
    /// Elements form to resolve against; it must resolve via `FnParam`s.
    #[test]
    fn fn_elements_form_resolves_via_own_param_and_binds_with_let() {
        let params = vec![FnParam {
            name: id("arr"),
            ty: Type::Array {
                elem: Box::new(Type::Bits(Box::new(Expr {
                    kind: ExprKind::Int {
                        value: 8,
                        raw: "8".into(),
                    },
                    span: sp(),
                }))),
                len: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: 4,
                        raw: "4".into(),
                    },
                    span: sp(),
                }),
            },
            span: sp(),
        }];
        let body = vec![FnStmt::Return(Expr {
            kind: ExprKind::Ident("x".into()),
            span: sp(),
        })];
        let lowered = lower_foreach_fn(
            &id("x"),
            &ForEachSource::Elements(id("arr")),
            &body,
            sp(),
            &params,
        )
        .expect("array fn param must resolve");
        let FnStmt::Loop { body: inner, .. } = &lowered[0] else {
            panic!("expected Loop")
        };
        let FnStmt::Let(let_stmt) = &inner[0] else {
            panic!("expected the synthesized Let binding for `x`")
        };
        assert_eq!(let_stmt.name.name, "x");
        assert!(
            matches!(&let_stmt.value.kind, ExprKind::Index { .. }),
            "`x` must bind to `arr[__foreach_x_idx]`, got {:?}",
            let_stmt.value
        );
        assert!(
            matches!(&inner[1], FnStmt::Return(_)),
            "original body must follow the Let binding"
        );
    }

    #[test]
    fn fn_elements_form_on_undeclared_param_returns_none() {
        assert!(
            lower_foreach_fn(
                &id("x"),
                &ForEachSource::Elements(id("nope")),
                &[],
                sp(),
                &[]
            )
            .is_none()
        );
    }
}
