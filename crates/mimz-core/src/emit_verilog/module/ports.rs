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
    ///
    /// BUG-41 (docs/audit/bugs.md): also inserts every fact
    /// `kinds::infer_kind`'s `Index`/`FnCall`/`Field` arms need but cannot
    /// reach through `decls`' plain namespace alone — a memory's element
    /// `Kind` (under `kinds::mem_elem_decl_key`), a non-array instance's
    /// output-port `Kind`s (under the exact `{inst}_{port}` name
    /// `expr.rs`'s own `Field` rendering already produces — no reserved
    /// prefix needed, a real Verilog signal is never ALSO named that by
    /// coincidence any more than it collides with an existing hoisted
    /// `__mimz_sub_N` wire), and every `fn`'s declared return `Kind`
    /// (under `kinds::fn_ret_decl_key`, independent of any one call's
    /// arguments). Keeping `infer_kind`'s own signature at `(expr, decls)`
    /// — no second map, no `&Project` parameter — means every caller in
    /// `expr.rs` stays exactly as simple as it already was.
    pub(super) fn build_decls(
        &self,
        flat: &[ModuleItem],
    ) -> HashMap<String, crate::width_rules::Kind> {
        use crate::emit_verilog::kinds::{fn_ret_decl_key, mem_elem_decl_key};

        let mut decls = HashMap::new();
        for item in flat {
            match item {
                ModuleItem::Port { name, ty, .. }
                | ModuleItem::Wire { name, ty, .. }
                | ModuleItem::Reg { name, ty, .. } => {
                    self.insert_signal_kind(&mut decls, name, ty);
                }
                ModuleItem::Mem { name, ty, .. } => {
                    if let Some(k) = self.scalar_kind_in_env(ty, &self.env) {
                        decls.insert(mem_elem_decl_key(&name.name), k);
                    }
                }
                ModuleItem::Inst(inst) if inst.index.is_none() => {
                    self.insert_instance_output_kinds(&mut decls, inst);
                }
                // Array instances (`let s[i] = Sub() { ... }` inside
                // `repeat`) — BUG-48 (`docs/audit/bugs.md`): `s[0].q`
                // renders through `Field { base: Index }`, a shape
                // `infer_kind`'s `Field` arm now also resolves, but only
                // if `decls` actually holds the key it looks up. This was
                // the missing half.
                ModuleItem::Repeat(r) => {
                    self.insert_repeat_instance_output_kinds(&mut decls, r);
                }
                _ => {}
            }
        }
        for (fname, decl) in &self.project.funcs {
            if let Some(k) = self.scalar_kind_in_env(&decl.ret, &self.env) {
                decls.insert(fn_ret_decl_key(fname), k);
            }
        }
        decls
    }

    /// The bundle-or-scalar `Port`/`Wire`/`Reg` insertion `build_decls`
    /// used to do inline, factored out unchanged so `build_decls` itself
    /// can also handle `Mem`/`Inst` items without the loop body growing a
    /// second incompatible shape.
    fn insert_signal_kind(
        &self,
        decls: &mut HashMap<String, crate::width_rules::Kind>,
        name: &Ident,
        ty: &Type,
    ) {
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

    /// Every output port `Kind` of a non-array-instance `inst`, keyed
    /// `{inst_name}_{port_name}` (matching `expr.rs::Field`'s own
    /// rendering) — BUG-41's instance-port case. Resolves the child's
    /// width expressions against an env layering the child's OWN file/
    /// module consts (`self.module_envs`, the same source `instance()`
    /// itself uses) under THIS instance's actual parameter arguments
    /// (folded against the CURRENT module's `self.env`) — best-effort:
    /// an argument or child const that doesn't fold to a literal here
    /// (a genuinely symbolic parametric width) just means that one port
    /// stays absent from `decls`, the same safe "not resolvable" outcome
    /// as every other shape this module's facts can't cover. Skips a
    /// `Bundle`-typed output port — `scalar_kind_in_env` returns `None`
    /// for it, same as any other unresolvable type.
    fn insert_instance_output_kinds(
        &self,
        decls: &mut HashMap<String, crate::width_rules::Kind>,
        inst: &Inst,
    ) {
        self.insert_instance_output_kinds_keyed(decls, inst, &self.env, &inst.name.name);
    }

    /// Every output port `Kind` of one `repeat`-body instance PER
    /// iteration, keyed `{inst}__{n}_{port}` — matching `expr.rs`'s own
    /// array-instance `Field` rendering (`fa[i].port` → `fa__<i>_port`)
    /// — BUG-48's fix. `lo`/`hi` must fold against `self.env` the same
    /// way `unroll` (`mod.rs`) folds them for real emission; an
    /// unfoldable bound just leaves these entries absent, the same
    /// best-effort fallback every other `build_decls` fact uses. Caps at
    /// the checker-enforced `repeat` bound already validated before this
    /// (an already-checked program), so no separate budget guard is
    /// needed here the way `unroll` needs one for its own error path.
    ///
    /// BUG-53 (`docs/audit/bugs.md`): `n` must be the instance's own
    /// `index` EXPRESSION, folded against this iteration's env — the same
    /// authority `inst_name` (`emit_verilog/mod.rs`) uses to render the
    /// real Verilog name — not the bare loop counter `i`. An offset index
    /// (`let s[i + 1] = ...`) diverges from `i` the moment it isn't the
    /// identity; using `i` directly keyed a `decls` entry `expr.rs` never
    /// reads and left the one it DOES read absent. The walk also recurses
    /// into nested `Repeat`/`ConstIf`/`ForEach` inside the body, mirroring
    /// `emit_instances`'s own traversal (`module/instances.rs`) — the
    /// original only scanned `r.items` one level deep, so a nested
    /// `repeat`/`const if` instance was never keyed at all.
    fn insert_repeat_instance_output_kinds(
        &self,
        decls: &mut HashMap<String, crate::width_rules::Kind>,
        r: &Repeat,
    ) {
        self.insert_repeat_instance_output_kinds_in(decls, r, &self.env);
    }

    /// `insert_repeat_instance_output_kinds`'s actual body, taking the
    /// enclosing scope's env explicitly so a NESTED `repeat`'s own bounds
    /// fold against the outer loop variable, not just the module-level one.
    fn insert_repeat_instance_output_kinds_in(
        &self,
        decls: &mut HashMap<String, crate::width_rules::Kind>,
        r: &Repeat,
        outer_env: &Env,
    ) {
        let Some(lo) = consteval::eval(&r.lo, outer_env)
            .ok()
            .map(|v| v.to_i128_saturating())
        else {
            return;
        };
        let Some(hi) = consteval::eval(&r.hi, outer_env)
            .ok()
            .map(|v| v.to_i128_saturating())
        else {
            return;
        };
        let mut i = lo;
        while i < hi {
            let mut env = outer_env.clone();
            env.insert(r.var.name.clone(), consteval::ConstVal::from_i128(i));
            self.insert_array_instance_output_kinds(decls, &r.items, &env);
            i += 1;
        }
    }

    /// Walk `items` (one `repeat` iteration's body) keying every `Inst`
    /// found by its own folded `index` against `env`, recursing into
    /// nested `Repeat`/`ConstIf`/`ForEach` the same way `emit_instances`
    /// (`module/instances.rs`) does for real emission — BUG-53's second
    /// half. `Repeat`/`ConstIf` recurse directly; `ForEach` lowers first
    /// (`ast::lower_foreach_item`, same call shape `emit_instances` uses,
    /// `items` itself as the sibling context it needs) then recurses into
    /// the result.
    fn insert_array_instance_output_kinds(
        &self,
        decls: &mut HashMap<String, crate::width_rules::Kind>,
        items: &[ModuleItem],
        env: &Env,
    ) {
        for item in items {
            match item {
                ModuleItem::Inst(inst) => {
                    let key = match &inst.index {
                        Some(idx) => {
                            let Some(n) = consteval::eval(idx, env)
                                .ok()
                                .map(|v| v.to_i128_saturating())
                            else {
                                continue;
                            };
                            format!("{}__{n}", inst.name.name)
                        }
                        None => inst.name.name.clone(),
                    };
                    self.insert_instance_output_kinds_keyed(decls, inst, env, &key);
                }
                ModuleItem::Repeat(nested) => {
                    self.insert_repeat_instance_output_kinds_in(decls, nested, env);
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, env)
                        .map(|v| v.to_i128_saturating())
                        .unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.insert_array_instance_output_kinds(decls, branch, env);
                }
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                        self.insert_array_instance_output_kinds(decls, &lowered, env);
                    }
                }
                _ => {}
            }
        }
    }

    /// Shared by both callers above: every output port `Kind` of `inst`,
    /// resolved against `parent_env` (the instantiating scope's own env —
    /// `self.env` for a plain instance, one `repeat` iteration's env with
    /// the loop var bound for an array element), stored under
    /// `{key_prefix}_{port}`.
    fn insert_instance_output_kinds_keyed(
        &self,
        decls: &mut HashMap<String, crate::width_rules::Kind>,
        inst: &Inst,
        parent_env: &Env,
        key_prefix: &str,
    ) {
        let Some((child_file, target)) = self.project.resolve_target_with_file(&inst.module) else {
            return;
        };
        let mut env: Env = self
            .module_envs
            .get(&(child_file, target.name().name.clone()))
            .cloned()
            .unwrap_or_default();
        for a in &inst.args {
            if let Ok(v) = consteval::eval(&a.value, parent_env) {
                env.insert(a.name.name.clone(), v);
            }
        }
        for item in target.items() {
            if let ModuleItem::Port {
                dir: Dir::Out,
                name,
                ty,
            } = item
                && let Some(k) = self.scalar_kind_in_env(ty, &env)
            {
                decls.insert(format!("{key_prefix}_{}", name.name), k);
            }
        }
    }

    /// Like `resolved_kind`, but against an EXPLICIT env instead of
    /// `self.env`, and returns `None` (rather than panicking) for
    /// `Bundle`/`Array` — `resolved_kind`'s own panics assume its only
    /// caller (`build_decls`'s bundle-flattening loop) never hands it a
    /// bundle/array type directly, an invariant that does NOT hold for a
    /// `fn`'s declared return type or an instance's port type (both can
    /// legitimately be `Bundle`-typed in the source, unlike an
    /// already-flattened `Port`/`Wire`/`Reg` field).
    fn scalar_kind_in_env(&self, ty: &Type, env: &Env) -> Option<crate::width_rules::Kind> {
        use crate::width_rules::Kind;
        match ty {
            Type::Bit => Some(Kind {
                width: 1,
                signed: false,
            }),
            Type::Bits(e) => Some(Kind {
                width: consteval::eval(e, env).ok()?.to_i128_saturating() as u32,
                signed: false,
            }),
            Type::Signed(e) => Some(Kind {
                width: consteval::eval(e, env).ok()?.to_i128_saturating() as u32,
                signed: true,
            }),
            Type::Named(id) => {
                let en = self.project.resolve_enum(id)?;
                Some(Kind {
                    width: en.inferred_total_width.get()?,
                    signed: false,
                })
            }
            Type::Bundle { .. } | Type::Array { .. } => None,
        }
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
    /// Callers (`expr.rs`) must only reach this with the `Kind`
    /// `kinds::infer_kind(expr, decls)` already returned `Some` of — BUG-41
    /// (`docs/audit/bugs.md`): `infer_kind` itself is now the one and only
    /// gate (no separate `kind_is_inferrable` to keep in sync by hand), so
    /// every call site matches on its `Option` first and only reaches this
    /// function in the `Some` arm, passing that same `Kind` through as
    /// `mimz_kind` instead of making this function re-derive (and
    /// potentially re-fail to derive) it.
    pub(in crate::emit_verilog) fn hoist_if_needed(
        &mut self,
        expr: &Expr,
        rendered_text: String,
        mimz_kind: crate::width_rules::Kind,
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
            // Position-matrix invariant (Task 4, `docs/plan/v0.2-
            // correctness-remediation.local.md`): this doc comment's own
            // claim — a bare identifier's DECLARED `Kind` (when it names a
            // real signal; a previously hoisted `__mimz_sub_N` wire is
            // absent from `decls` and passes trivially) equals what the
            // caller computed as `mimz_kind` for this position — used to
            // go unverified. Checking it here turns every example,
            // regression, and fuzz run that touches a bare-identifier
            // self-determined operand into a free assertion against a
            // future `infer_kind`/`build_decls` drift, the same class of
            // defect BUG-41/BUG-42 shipped as a silent miscompile instead.
            debug_assert!(
                decls.get(&rendered_text).is_none_or(|k| *k == mimz_kind),
                "hoist_if_needed: `{rendered_text}` declared as {:?} but caller computed {mimz_kind:?}",
                decls.get(&rendered_text),
            );
            return rendered_text;
        }
        use crate::emit_verilog::self_determined::verilog_self_determined_kind;

        let Some(verilog_kind) = verilog_self_determined_kind(expr, decls, &self.env) else {
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
