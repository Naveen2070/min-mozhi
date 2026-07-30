use super::*;

impl<'a> Checker<'a> {
    /// Resolve every declared signal's type up front (declaration order
    /// in a module is free, so uses may precede declarations). This is
    /// where E0410 (bad width expression) fires.
    pub(super) fn collect_sigs(&mut self, cx: &mut Wcx<'a>, items: &'a [ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Port { name, ty, .. }
                | ModuleItem::Wire { name, ty, .. }
                | ModuleItem::Reg { name, ty, .. } => {
                    let t = self.resolve_ty(cx, ty);
                    cx.sigs.insert(name.name.clone(), t);
                }
                ModuleItem::Mem {
                    name, ty, depth, ..
                } => {
                    let t = match self.resolve_ty(cx, ty) {
                        Ty::Bit => Some((1, false)),
                        Ty::Bits(n) => Some((n, false)),
                        Ty::Signed(n) => Some((n, true)),
                        Ty::Unknown => None, // width error already reported
                        other => {
                            self.err(
                                cx.file,
                                name.span,
                                "E0409",
                                format!("{} cannot be a memory element type", show(&other)),
                                "memory elements are `bit`, `bits[N]`, or `signed[N]` — \
                                 store an enum's encoding as `bits[N]` for now",
                            );
                            None
                        }
                    };
                    let resolved = match (t, self.eval_depth(cx, depth)) {
                        (Some((width, signed)), Some(d)) => Ty::Memory {
                            width,
                            signed,
                            depth: d,
                        },
                        _ => Ty::Unknown,
                    };
                    cx.sigs.insert(name.name.clone(), resolved);
                }
                ModuleItem::Clock(n) => {
                    cx.sigs.insert(n.name.clone(), Ty::Clock);
                }
                ModuleItem::Reset { name: n, .. } => {
                    cx.sigs.insert(n.name.clone(), Ty::Reset);
                }
                ModuleItem::SyncLoop(sl) => {
                    let result_t = self.resolve_ty(cx, &sl.result_ty);
                    cx.sigs.insert(format!("{}_start", sl.name.name), Ty::Bit);
                    cx.sigs.insert(format!("{}_done", sl.name.name), Ty::Bit);
                    cx.sigs.insert(format!("{}_result", sl.name.name), result_t);
                    cx.sigs.insert(format!("{}_running", sl.name.name), Ty::Bit);
                }
                ModuleItem::Repeat(r) => {
                    // Types inside `repeat` resolve under a representative
                    // value (`lo`); per-iteration width EXPRESSIONS in
                    // declarations are an elaboration-slice concern.
                    let lo = consteval::eval(&r.lo, &cx.env)
                        .unwrap_or_else(|_| consteval::ConstVal::zero());
                    let shadowed = cx.env.insert(r.var.name.clone(), lo);
                    self.collect_sigs(cx, &r.items);
                    self.unshadow(cx, &r.var.name, shadowed);
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &cx.env)
                        .unwrap_or_else(|_| consteval::ConstVal::zero());
                    let branch = if !val.is_zero() {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.collect_sigs(cx, branch);
                }
                // `foreach` is pure sugar over `repeat`/`loop` (see
                // `ast::foreach_lower`'s module doc comment). This pass only
                // resolves DECLARED types (Port/Wire/Reg/Mem/Clock/Reset),
                // never the loop variable's value — and `subst_module_item`
                // never touches a declaration's `ty` field (only its
                // driving `init`/`reset`/`depth` expr), so a lowered
                // Elements-form body would resolve identical types to the
                // raw one anyway. Recurse straight into the RAW `fe.items`
                // (still borrowed from the `'a` AST arena, unlike a lowered
                // `Vec` which is a fresh, non-`'a` clone) — same "declared
                // once, raw body" treatment `names.rs`'s `collect_decls`
                // uses for its own `ForEach` arm, and for the same reason:
                // declarations inside a `foreach` body are rejected anyway
                // (E0303, `no_decls_in_repeat`), so there's nothing here
                // that substitution could ever legally affect.
                ModuleItem::ForEach(fe) => self.collect_sigs(cx, &fe.items),
                _ => {}
            }
        }
    }

    /// AST type -> pass type, under the current binding.
    pub(super) fn resolve_ty(&mut self, cx: &mut Wcx<'a>, ty: &'a Type) -> Ty<'a> {
        match ty {
            Type::Bit => Ty::Bit,
            Type::Bits(w) => match self.eval_width(cx, w) {
                Some(n) => bits(n),
                None => Ty::Unknown,
            },
            Type::Signed(w) => match self.eval_width(cx, w) {
                Some(n) => Ty::Signed(n),
                None => Ty::Unknown,
            },
            Type::Bundle { name, args } => {
                if self.bundles.contains_key(&name.name.name) {
                    Ty::Bundle {
                        name: &name.name.name,
                        bfile_hint: name.resolved_file.get(),
                        args,
                    }
                } else {
                    Ty::Unknown // E0906 already reported by an earlier pass
                }
            }
            Type::Named(n) => match self.lookup_enum(&cx.sc, &n.name.name) {
                Some(e) => Ty::Enum(e),
                // Not an enum — a bare (unparametrized) bundle name parses as
                // `Type::Named` too (only `Foo(X: N)` parses as `Type::Bundle`;
                // see `parser::items::type_`), so check `self.bundles` before
                // giving up. `args` is `&[]`: a bare name has none by
                // definition (mirrors `is_bundle_ty`'s `Type::Named` arm).
                None if self.bundles.contains_key(&n.name.name) => Ty::Bundle {
                    name: &n.name.name,
                    bfile_hint: n.resolved_file.get(),
                    args: &[],
                },
                None => Ty::Unknown, // E0103/E0906 already reported
            },
            Type::Array { elem, len } => {
                // A bundle-named element resolves to a real `Ty::Bundle` via
                // the `Type::Named`/`Type::Bundle` arms above, which on its
                // own reports no diagnostic — bundles-in-arrays are out of
                // scope (spec: "not supported in v1"), so catch it here,
                // before resolving, using the same `is_bundle_ty` check
                // `collect_sigs` already uses to detect bundle-typed signals.
                if self.is_bundle_ty(elem) {
                    let bname = ast_bundle_name(elem).unwrap_or("?");
                    self.err(
                        cx.file,
                        len.span,
                        "E0411",
                        format!("bundle `{bname}` cannot be an array element type"),
                        "array elements are `bit`, `bits[N]`, or `signed[N]` — \
                         nested arrays and enum/bundle elements are not supported in v1",
                    );
                    return Ty::Unknown;
                }
                let elem_ty = self.resolve_ty(cx, elem);
                let (elem_width, elem_signed) = match elem_ty {
                    Ty::Bit => (1, false),
                    Ty::Bits(n) => (n, false),
                    Ty::Signed(n) => (n, true),
                    Ty::Unknown => return Ty::Unknown, // element error already reported
                    other => {
                        // `Type` (the AST node) has no span of its own — `elem`
                        // is a bare `Box<Type>` (see ast/mod.rs's `Type::Array`
                        // doc comment). `len` is the only span-bearing part of
                        // this `Type::Array` node available at this call site,
                        // so it is the best available anchor for the diagnostic
                        // (mirrors E0409's approach of pointing at whatever
                        // span is actually in scope, there the memory name's).
                        self.err(
                            cx.file,
                            len.span,
                            "E0411",
                            format!("{} cannot be an array element type", show(&other)),
                            "array elements are `bit`, `bits[N]`, or `signed[N]` — \
                             nested arrays and enum/bundle elements are not supported in v1",
                        );
                        return Ty::Unknown;
                    }
                };
                match self.eval_array_len(cx, len) {
                    Some(n) => Ty::Array {
                        elem_width,
                        elem_signed,
                        len: n,
                    },
                    None => Ty::Unknown,
                }
            }
        }
    }

    /// Like [`Self::resolve_ty`] but never reports — used when resolving
    /// a CHILD module's port type at a use site (the child's own
    /// definition check is where its declaration errors belong).
    pub(super) fn resolve_ty_silent(&mut self, cx: &mut Wcx<'a>, ty: &'a Type) -> Ty<'a> {
        let before = self.diags.len();
        let t = self.resolve_ty(cx, ty);
        self.diags.truncate(before);
        t
    }

    /// Evaluate a width expression and validate the value (E0410).
    pub(super) fn eval_width(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<u128> {
        match consteval::eval(e, &cx.env).map(|v| v.to_i128_saturating()) {
            Ok(v) if (1..=MAX_WIDTH).contains(&v) => Some(v as u128),
            Ok(v) => {
                self.err(
                    cx.file,
                    e.span,
                    "E0410",
                    format!("`{v}` is not a valid width"),
                    format!(
                        "hardware needs at least one bit — a width must be between 1 \
                         and {MAX_WIDTH}"
                    ),
                );
                None
            }
            Err(d) => {
                self.diags.push(d.with_file(cx.file));
                None
            }
        }
    }

    /// Evaluate a memory depth expression and validate it (E0410). Like a
    /// width, a depth must be a positive compile-time constant within
    /// [`MAX_DEPTH`].
    fn eval_depth(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<u128> {
        match consteval::eval(e, &cx.env).map(|v| v.to_i128_saturating()) {
            Ok(v) if (1..=MAX_DEPTH).contains(&v) => Some(v as u128),
            Ok(v) => {
                self.err(
                    cx.file,
                    e.span,
                    "E0410",
                    format!("`{v}` is not a valid memory depth"),
                    format!(
                        "a memory needs at least one cell — the depth must be between 1 \
                         and {MAX_DEPTH}"
                    ),
                );
                None
            }
            Err(d) => {
                self.diags.push(d.with_file(cx.file));
                None
            }
        }
    }

    /// Evaluate an array's length expression and validate it (E0412).
    /// Like a memory depth, a length must be a positive compile-time
    /// constant. Mirrors `eval_depth` exactly (same shape, different
    /// error code/wording since an array isn't addressable the way a
    /// memory is — no MAX_DEPTH-style upper bound is enforced here since
    /// an array is fully unrolled into scalar hardware at compile time;
    /// an unreasonably large N will simply produce a lot of Verilog, the
    /// same honesty story `repeat` already tells for large bounds).
    fn eval_array_len(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<u128> {
        match consteval::eval(e, &cx.env).map(|v| v.to_i128_saturating()) {
            Ok(v) if v >= 1 => Some(v as u128),
            Ok(v) => {
                self.err(
                    cx.file,
                    e.span,
                    "E0412",
                    format!("`{v}` is not a valid array length"),
                    "an array needs at least one element — the length must be a positive compile-time constant",
                );
                None
            }
            Err(d) => {
                self.diags.push(d.with_file(cx.file));
                None
            }
        }
    }

    /// True if `ty` names a registered bundle (either `Type::Named` or parametric `Type::Bundle`).
    fn is_bundle_ty(&self, ty: &Type) -> bool {
        match ty {
            Type::Named(id) => self.bundles.contains_key(&id.name.name),
            Type::Bundle { name, .. } => self.bundles.contains_key(&name.name.name),
            _ => false,
        }
    }
}
