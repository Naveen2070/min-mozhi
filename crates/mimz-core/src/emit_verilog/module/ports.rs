use super::*;

impl Emitter<'_> {
    /// Render an assignment target: `name`, `name[i]`, or `name[hi:lo]`.
    /// Indices fold at compile time, so a `repeat`-driven `sum[i]` lands as
    /// `sum[2]`.
    pub(super) fn lvalue(&mut self, lv: &LValue) -> String {
        let mut s = lv.base.name.clone();
        if let Some((first, second)) = &lv.index {
            let empty = HashMap::new();
            let no_arrays = ArrayScope::new();
            match second {
                Some(lo) => s.push_str(&format!(
                    "[{}:{}]",
                    self.index_expr(first, &empty, &no_arrays),
                    self.index_expr(lo, &empty, &no_arrays)
                )),
                None => s.push_str(&format!("[{}]", self.index_expr(first, &empty, &no_arrays))),
            }
        }
        s
    }

    /// Verilog range like `[WIDTH-1:0] ` (with trailing space), or "" for bit.
    pub(super) fn width(&mut self, ty: &Type) -> String {
        self.width_subst(ty, &HashMap::new())
    }

    /// Like `width`, but for already-resolved types where the width expression
    /// is a known integer literal. Emits `[7:0]` instead of `[(8)-1:0]` by
    /// evaluating the constant at Rust time rather than leaving it symbolic.
    pub(super) fn width_resolved(&mut self, ty: &Type) -> String {
        match ty {
            Type::Bit => String::new(),
            Type::Bits(e) => {
                if let Ok(w) = consteval::eval(e, &self.env).map(|v| v.to_i128_saturating())
                    && w >= 1
                {
                    return format!("[{}:0] ", w - 1);
                }
                // Fallback to symbolic form.
                let we = self.expr(e);
                format!("[({we})-1:0] ")
            }
            Type::Signed(e) => {
                if let Ok(w) = consteval::eval(e, &self.env).map(|v| v.to_i128_saturating())
                    && w >= 1
                {
                    return format!("signed [{}:0] ", w - 1);
                }
                let we = self.expr(e);
                format!("signed [({we})-1:0] ")
            }
            _ => self.width(ty),
        }
    }

    /// Like [`Self::width`], but with child-module parameter names replaced
    /// by the instantiating module's argument expressions — used when
    /// declaring auto-wires for a child instance's outputs.
    pub(super) fn width_subst(&mut self, ty: &Type, subst: &HashMap<&str, &Expr>) -> String {
        match ty {
            Type::Bit => String::new(),
            Type::Bits(e) => {
                let we = self.expr_subst(e, subst, &ArrayScope::new());
                format!("[({we})-1:0] ")
            }
            Type::Signed(e) => {
                // Declared `signed` so Verilog's native two's-complement
                // semantics apply: assignments SIGN-extend and comparisons
                // are signed. Sound because the checker forbids
                // signed/unsigned mixing inside one expression (E0403).
                let we = self.expr_subst(e, subst, &ArrayScope::new());
                format!("signed [({we})-1:0] ")
            }
            Type::Named(id) => {
                if let Some(e) = self.project.resolve_enum(id) {
                    let w = e
                        .inferred_total_width
                        .get()
                        .expect("inferred_total_width not set — checker must run before emitter");
                    format!("[{}:0] ", w - 1)
                } else if self.project.resolve_bundle(id).is_some() {
                    // BUG-10 (docs/audit/bugs.md): a bare bundle name reaches
                    // here ONLY via a `fn` return type — module ports/wires
                    // and `fn` params flatten a bundle to per-field signals
                    // BEFORE ever calling width()/width_subst() (see
                    // render_fn_decl's own Type::Bundle/Type::Named arms
                    // above it). A Verilog `function` can only return one
                    // value, so there is no flattening strategy for a
                    // bundle-typed return — reject with a real diagnostic
                    // instead of the misleading "not a declared enum"
                    // message this used to fall through to.
                    self.err(
                        id.span,
                        format!(
                            "`fn` cannot return a bundle-typed value (`{}`)",
                            id.name.name
                        ),
                        "a Verilog `function` can only return one value, and there is no \
                         flattening strategy for a bundle-typed return (unlike a bundle-typed \
                         param, which flattens to one input per field) — return an individual \
                         field instead, or restructure as separate `fn`s",
                    );
                    String::new()
                } else {
                    self.err(
                        id.span,
                        format!(
                            "unknown type `{}` — not a built-in and not a declared enum",
                            id.name.name
                        ),
                        "",
                    );
                    String::new()
                }
            }
            // BUG-10 (docs/audit/bugs.md): the parametric form of a
            // bundle-typed `fn` return (`Foo(W: 8)`) reaches here for the
            // exact same reason the bare form reaches the `Type::Named` arm
            // above — every OTHER caller (module ports/wires, `fn` params)
            // flattens a bundle to per-field signals before ever calling
            // width()/width_subst(). This used to silently return an empty
            // (0-width) string here, producing invalid Verilog with no
            // diagnostic at all — worse than the bare form's at least-an-
            // error behavior. Same fix, same message.
            Type::Bundle { name, .. } => {
                self.err(
                    name.span,
                    format!(
                        "`fn` cannot return a bundle-typed value (`{}`)",
                        name.name.name
                    ),
                    "a Verilog `function` can only return one value, and there is no \
                     flattening strategy for a bundle-typed return (unlike a bundle-typed \
                     param, which flattens to one input per field) — return an individual \
                     field instead, or restructure as separate `fn`s",
                );
                String::new()
            }
            Type::Array { .. } => unreachable!(
                "array types are rejected by the checker (E0416) before reaching the \
                 emitter for anything but a `fn` parameter, which render_fn_decl handles \
                 separately without calling width()/width_subst()"
            ),
        }
    }

    /// Every `Port`/`Wire`/`Reg` name in `flat` (the current module's own
    /// flattened item list, already produced by `flatten_items` before this
    /// runs), mapped to its resolved `Kind`. Mirrors `width_subst`'s own
    /// `Type` resolution exactly (same `consteval::eval` against `self.env`,
    /// same `EnumDecl.inferred_total_width` source for `Type::Named`), just
    /// producing a `Kind` instead of a declaration-text fragment.
    ///
    /// A bundle-typed `Port`/`Wire` is never a single scalar Verilog signal
    /// (same convention the ports/wires-declaration loops and `emit_drives`
    /// already follow) — `flat` never pre-expands one to per-field scalars
    /// (`flatten_items` only lowers `const if`/`sync loop`/`foreach`, not
    /// bundles), so this calls `resolve_bundle_fields` itself, the same way
    /// every other bundle-aware renderer in this file does, and inserts one
    /// `{name}_{field}` entry per field — the exact identifier
    /// `expr.rs::Field`'s rendering (`base_field`) and the ports/wires
    /// loops' own declarations already use for it.
    ///
    /// Task 6 adds the real caller (`module()`'s own `self.cur_decls`
    /// assignment, above).
    pub(super) fn build_decls(
        &self,
        flat: &[ModuleItem],
    ) -> HashMap<String, crate::width_rules::Kind> {
        let mut decls = HashMap::new();
        for item in flat {
            let (name, ty) = match item {
                ModuleItem::Port { name, ty, .. } => (name, ty),
                ModuleItem::Wire { name, ty, .. } => (name, ty),
                ModuleItem::Reg { name, ty, .. } => (name, ty),
                _ => continue,
            };
            let bundle_fields = match ty {
                Type::Bundle {
                    name: bname,
                    args: bargs,
                } => Some(self.resolve_bundle_fields(bname, bargs)),
                Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                    Some(self.resolve_bundle_fields(id, &[]))
                }
                _ => None,
            };
            if let Some(fields) = bundle_fields {
                for (fname, fty) in &fields {
                    if let Some(k) = self.resolved_kind(fty) {
                        decls.insert(format!("{}_{}", name.name, fname), k);
                    }
                }
            } else if let Some(k) = self.resolved_kind(ty) {
                decls.insert(name.name.clone(), k);
            }
        }
        decls
    }

    /// Resolve a scalar (never `Bundle`/`Array` — `build_decls` above
    /// flattens those to per-field scalars before this ever sees them) type
    /// to its `Kind`. A bundle-typed field reaching the `Type::Named` arm's
    /// else-branch would mean a NESTED bundle field — not currently
    /// supported by any bundle-aware renderer in this file, so this panics
    /// rather than silently falling back, same as the rest of this file's
    /// convention for a genuinely-unhandled shape.
    fn resolved_kind(&self, ty: &Type) -> Option<crate::width_rules::Kind> {
        use crate::width_rules::Kind;
        match ty {
            Type::Bit => Some(Kind {
                width: 1,
                signed: false,
            }),
            Type::Bits(e) => Some(Kind {
                width: consteval::eval(e, &self.env).ok()?.to_i128_saturating() as u32,
                signed: false,
            }),
            Type::Signed(e) => Some(Kind {
                width: consteval::eval(e, &self.env).ok()?.to_i128_saturating() as u32,
                signed: true,
            }),
            Type::Named(id) => {
                if let Some(en) = self.project.resolve_enum(id) {
                    Some(Kind {
                        width: en.inferred_total_width.get().expect(
                            "inferred_total_width not set — checker must run before emitter",
                        ),
                        signed: false,
                    })
                } else {
                    panic!(
                        "build_decls: `{}` is bundle-typed — nested bundle fields are not \
                         supported by build_decls",
                        id.name.name
                    )
                }
            }
            Type::Bundle { name, .. } => panic!(
                "build_decls: bundle field `{}` is itself bundle-typed — nested bundles are \
                 not supported by build_decls",
                name.name.name
            ),
            Type::Array { .. } => unreachable!(
                "array types are rejected by the checker (E0416) before reaching the emitter \
                 for anything but a `fn` parameter, which never reaches build_decls"
            ),
        }
    }

    /// Compares `expr`'s mimz-computed `Kind` against what Verilog would
    /// self-determine for it in a self-determined position (Stage 4,
    /// Phase A1b). On a mismatch, hoists `rendered_text` into a fresh
    /// `wire`/`assign` pair (appended to `self.hoisted_decls`, inserted
    /// at `fn_pos` alongside the existing `clog2`/user-`fn` injections
    /// — see `fn module`'s own `self.out.insert_str(fn_pos, &inject)`
    /// call) and returns the wire's name instead of `rendered_text`.
    /// Returns `rendered_text` unchanged when there is no mismatch (the
    /// common case — no new wire, no behavior change).
    ///
    /// Callers (`expr.rs`) must only reach this when `expr::kind_is_inferrable`
    /// has already confirmed `infer_kind` can resolve `expr` against `decls`
    /// without panicking — this function does not re-check that itself.
    pub(in crate::emit_verilog) fn hoist_if_needed(
        &mut self,
        expr: &Expr,
        rendered_text: String,
        decls: &HashMap<String, crate::width_rules::Kind>,
    ) -> String {
        // Same early-return `hoist_slice_base_if_needed` already uses: a
        // rendered text that is ALREADY a plain identifier is either a bare
        // `Ident` (whose declared `Kind` in `decls` trivially equals its own
        // `mimz_kind` — nothing to compare) or the name of a wire a prior
        // hoist (`hoist_width_effect_operand`) just created, sized to
        // exactly THIS expression's own `infer_kind` — Verilog self-
        // determines an identifier at its declared width, so that width
        // already IS `mimz_kind` regardless of what `expr`'s own AST shape
        // is. Skipping here avoids a double-hoist at the four call sites
        // (`Concat`/`Replicate`/`SignedCast`/`UnsignedCast`) where both
        // `hoist_width_effect_operand` and this function run on the same
        // operand — see BUG-23's double-hoist finding (docs/audit/bugs.md).
        if super::expr::is_plain_identifier(&rendered_text) {
            return rendered_text;
        }
        use crate::emit_verilog::kinds::infer_kind;
        use crate::emit_verilog::self_determined::verilog_self_determined_kind;

        let mimz_kind = infer_kind(expr, decls);
        let Some(verilog_kind) = verilog_self_determined_kind(expr, decls) else {
            return rendered_text;
        };
        if mimz_kind == verilog_kind {
            return rendered_text;
        }
        self.hoist_counter += 1;
        let name = format!("__mimz_sub_{}", self.hoist_counter);
        let ty = if mimz_kind.signed {
            format!("signed [{}:0]", mimz_kind.width.saturating_sub(1))
        } else {
            format!("[{}:0]", mimz_kind.width.saturating_sub(1))
        };
        self.hoisted_decls
            .push_str(&format!("    wire {ty} {name};\n"));
        self.hoisted_decls
            .push_str(&format!("    assign {name} = {rendered_text};\n"));
        name
    }

    /// Same mismatch detection, but for BUG-20's condition instead of a
    /// width mismatch: hoists whenever `rendered_text` (a slice's base)
    /// isn't already a plain identifier, since Verilog's part-select
    /// grammar only accepts one. Shares the same counter/buffer as
    /// `hoist_if_needed` (a single per-module numbering sequence for
    /// every kind of hoist, not two separate ones).
    ///
    /// `signed` picks the declared wire's own signedness, mirroring
    /// `hoist_if_needed`'s `ty` computation exactly — needed by BUG-23's
    /// wrap-operand hoist (`hoist_width_effect_operand`), whose hoisted
    /// operand can itself be signed; the BUG-20 slice-base caller always
    /// passes `false`, since a part-select's result is unsigned
    /// regardless of the base's own declared signedness.
    pub(in crate::emit_verilog) fn hoist_slice_base_if_needed(
        &mut self,
        rendered_text: String,
        width: u32,
        signed: bool,
    ) -> String {
        if super::expr::is_plain_identifier(&rendered_text) {
            return rendered_text;
        }
        self.hoist_counter += 1;
        let name = format!("__mimz_sub_{}", self.hoist_counter);
        let ty = if signed {
            format!("signed [{}:0]", width.saturating_sub(1))
        } else {
            format!("[{}:0]", width.saturating_sub(1))
        };
        self.hoisted_decls
            .push_str(&format!("    wire {ty} {name};\n"));
        self.hoisted_decls
            .push_str(&format!("    assign {name} = {rendered_text};\n"));
        name
    }
}
