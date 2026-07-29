use super::*;
use crate::ast::{FnParam, FnStmt, FuncDecl};

impl<'a> Checker<'a> {
    /// Name-check a function declaration: validates param/return types (E0103)
    /// and checks all statements + the tail for unbound names (E0101).
    pub(super) fn check_func_names(&mut self, file: usize, func: &'a FuncDecl) {
        let mut env = self.file_consts[file].clone();
        let mut sc = Scope {
            names: HashMap::new(),
        };
        for param in &func.params {
            self.ty(file, &sc, &env, &param.ty);
            sc.names.insert(param.name.name.clone(), Bind::Param);
        }
        self.ty(file, &sc, &env, &func.ret);
        // `fn` declarations are project-top-level, not nested in a module
        // (see `resolve_names`'s `TopItem::Func` arm) — there is no
        // enclosing module item list to resolve an Elements-form `foreach`
        // source against. The only legal source is one of the `fn`'s own
        // array-typed params, resolved via `array_like_len_fn` inside
        // `check_fn_stmt_names` (see `FnStmt::ForEach` below).
        self.check_fn_stmt_names(file, &mut sc, &mut env, &func.params, &func.stmts);
        self.expr(file, &sc, &env, &func.tail);
    }

    /// Name-check one `fn`-body statement list, threading bindings forward
    /// sequentially — a `let` bound BEFORE an `if` (in this list or an
    /// enclosing one) stays visible inside both branches and after the
    /// `if`, exactly like ordinary sequential local scoping. A `let` bound
    /// INSIDE a branch is scoped to that branch only: it must not leak into
    /// the sibling branch's check, nor past the `if` into later statements
    /// or the tail — each branch gets its own clone of the scope-so-far, so
    /// whatever it adds is discarded once that branch's check finishes.
    /// (An earlier version of this comment claimed this mirrored `on`-block
    /// `SeqStmt::If`'s "flat, no-shadowing" model as a deliberate
    /// simplification — that claim was inaccurate: `SeqStmt` has no `Let`
    /// variant, so there was no such precedent, and letting a branch-local
    /// name leak out was a genuine soundness gap, not a stylistic choice —
    /// see the final whole-branch review that found it.)
    // Same "no `'a` needed" reasoning as `walk_items` — see its doc comment.
    fn check_fn_stmt_names(
        &mut self,
        file: usize,
        sc: &mut Scope<'a>,
        env: &mut Env,
        params: &[FnParam],
        stmts: &[FnStmt],
    ) {
        for stmt in stmts {
            match stmt {
                FnStmt::Let(local) => {
                    self.expr(file, sc, env, &local.value);
                    sc.names.insert(local.name.name.clone(), Bind::Const);
                }
                FnStmt::If { cond, then, els } => {
                    self.expr(file, sc, env, cond);
                    let mut then_sc = Scope {
                        names: sc.names.clone(),
                    };
                    self.check_fn_stmt_names(file, &mut then_sc, env, params, then);
                    if let Some(els) = els {
                        let mut els_sc = Scope {
                            names: sc.names.clone(),
                        };
                        self.check_fn_stmt_names(file, &mut els_sc, env, params, els);
                    }
                }
                FnStmt::Return(expr) => {
                    self.expr(file, sc, env, expr);
                }
                FnStmt::Loop {
                    var, lo, hi, body, ..
                } => {
                    // Same const-bound requirement as `repeat`/`SeqStmt::Loop`
                    // above — reuse `const_pos` (E0201 on a non-const bound)
                    // rather than silently defaulting to 0.
                    let lo_val = self.const_pos(file, env, lo);
                    self.const_pos(file, env, hi);
                    let shadowed = env.insert(
                        var.name.clone(),
                        consteval::ConstVal::from_i128(lo_val.unwrap_or(0)),
                    );
                    // Fresh scope clone: same branch-local-scope discipline as
                    // the `If` arm above — a `let` inside the loop body must
                    // not leak past it, same soundness rule as an if-branch.
                    let mut loop_sc = Scope {
                        names: sc.names.clone(),
                    };
                    self.check_fn_stmt_names(file, &mut loop_sc, env, params, body);
                    match shadowed {
                        Some(v) => env.insert(var.name.clone(), v),
                        None => env.remove(&var.name),
                    };
                }
                // Same "lower to `Loop`, recurse into the same fn" delegation
                // as `ModuleItem::ForEach`/`SeqStmt::ForEach` above. Unlike
                // `SeqStmt`, `FnStmt` has `Let` — the Elements form binds
                // `var` with a real `let` (see `lower_foreach_fn`'s doc
                // comment), so no substitution is needed, and (like the
                // `ModuleItem` form) there IS a synthesized declaration —
                // but `check_fn_stmt_names` has no `no_decls_in_repeat`-style
                // restriction, so that's not a concern here. `params` is the
                // enclosing `fn`'s own parameter list (a `fn` is always
                // project-top-level, so there is no sibling module item list
                // to resolve against — see `array_like_len_fn`).
                FnStmt::ForEach {
                    var,
                    source,
                    body,
                    span,
                } => {
                    if let ForEachSource::Elements(arr) = source
                        && crate::ast::array_like_len_fn(&arr.name, params).is_none()
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
                        crate::ast::lower_foreach_fn(var, source, body, *span, params)
                    else {
                        continue; // E0417 already pushed above
                    };
                    self.check_fn_stmt_names(file, sc, env, params, &lowered);
                }
                FnStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// Reject an array-typed module-level signal declaration (port, wire, or
    /// register). Array types are only supported for `fn` parameters in v0.2
    /// — module-level arrays are an explicit non-goal (would need per-element
    /// driver-uniqueness checking). This is a separate, narrowly-scoped check
    /// from `ty()` (which DOES recurse into `Type::Array`, since `fn` params
    /// legitimately use it) — only Port/Wire/Reg call this, never `fn` params.
    pub(super) fn reject_array_signal_type(
        &mut self,
        file: usize,
        ty: &Type,
        span: crate::span::Span,
        what: &str,
    ) {
        if matches!(ty, Type::Array { .. }) {
            self.err(
                file,
                span,
                "E0416",
                format!("{what} cannot be array-typed"),
                "array types are only supported for `fn` parameters in v0.2 — \
                 module-level port/wire/register arrays are not yet supported",
            );
        }
    }

    /// Validate a bundle field's type: only `bit`, `bits[N]`, `signed[N]`, and
    /// enums are allowed. Nested bundles and unknown types emit E0807 (non-concrete
    /// type); an unknown parametric bundle (`Type::Bundle` with unknown name) emits
    /// E0906. Clock/reset cannot appear here — they lex as keywords, not types.
    pub(super) fn validate_bundle_field_type(
        &mut self,
        file: usize,
        ty: &Type,
        span: crate::span::Span,
    ) {
        match ty {
            Type::Bit | Type::Bits(_) | Type::Signed(_) => {}
            Type::Named(id) => {
                if self.enums.contains_key(&id.name.name) {
                    let candidates = self.enums.get(&id.name.name).cloned();
                    self.resolve(file, candidates, id, |_| {});
                } else {
                    let msg = if self.bundles.contains_key(&id.name.name) {
                        format!("bundle field cannot be a bundle type (`{}`)", id.name.name)
                    } else {
                        format!(
                            "`{}` is not a concrete type for a bundle field",
                            id.name.name
                        )
                    };
                    self.err(
                        file,
                        span,
                        "E0807",
                        msg,
                        "bundle fields must be `bit`, `bits[N]`, `signed[N]`, or an enum — \
                         nested bundles are not supported in v0.2",
                    );
                }
            }
            Type::Bundle { name, .. } => {
                if self.bundles.contains_key(&name.name.name) {
                    let candidates = self.bundles.get(&name.name.name).cloned();
                    // Only report E0807 when `resolve` actually found the
                    // bundle it names — an ambiguous or unmatched-qualifier
                    // reference already got its own E0110/E0111 from
                    // `resolve` below, and adding E0807 on top would
                    // double-report the same bad reference.
                    if self.resolve(file, candidates, name, |_| {}).is_some() {
                        self.err(
                            file,
                            span,
                            "E0807",
                            format!(
                                "bundle field cannot be a bundle type (`{}`)",
                                name.name.name
                            ),
                            "nested bundles are not supported in v0.2 — use flat field types",
                        );
                    }
                } else {
                    self.err(
                        file,
                        name.span,
                        "E0906",
                        format!("unknown bundle type `{}`", name.name.name),
                        "declare the bundle at file level before using it as a type",
                    );
                }
            }
            Type::Array { .. } => {
                self.err(
                    file,
                    span,
                    "E0807",
                    "bundle field cannot be an array type",
                    "bundle fields must be `bit`, `bits[N]`, `signed[N]`, or an enum — \
                     arrays are not supported as bundle fields in v0.2",
                );
            }
        }
    }
}
