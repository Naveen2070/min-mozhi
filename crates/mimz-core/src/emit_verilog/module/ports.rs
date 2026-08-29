use super::*;

impl<'a> Emitter<'a> {
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
                if let Some(e) = self.resolve_enum(id) {
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
    /// assignment, above). Task 2 (BUG-62(a)) adds a second:
    /// `testbench.rs::emit_testbench`, which was
    /// installing `Default::default()` — an always-empty map — for every
    /// `expect`/`Drive` in every test, so the same `None`-fallback fail-open
    /// Task 1 makes loud fired on EVERY testbench expression touching a DUT
    /// signal. `pub(in crate::emit_verilog)` (not `pub(super)`) so that
    /// caller, a sibling of `module` rather than a descendant, can reach it.
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
    pub(in crate::emit_verilog) fn build_decls(
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
    pub(super) fn insert_signal_kind(
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

    /// Resolve `id` to its `EnumDecl`: the CURRENT module's own local
    /// `enum` first, falling back to `Project::resolve_enum`'s file/
    /// project-wide table — mirrors `Checker::lookup_enum`'s exact
    /// module-scope-first algorithm (`checker/names/resolve.rs`), which
    /// the checker itself already uses for this same `Type::Named` lookup.
    ///
    /// `Project::resolve_enum` alone is wrong here: two sibling modules in
    /// one file each declaring their own `enum State { .. }` (an ordinary
    /// FSM pattern, and completely unambiguous in each module's own scope)
    /// both land in `Project::enums` under the shared key `"State"`, so
    /// `Project::resolve_enum`'s 2+-same-file-candidates check reports a
    /// false ambiguity and returns `None` for BOTH — which `resolved_kind`
    /// below then misreads as "not an enum, must be a nested bundle" and
    /// panics. Checking `self.cur_module_enums` first resolves the
    /// reference correctly before that ambiguity table is ever consulted.
    fn resolve_enum(&self, id: &QualIdent) -> Option<&'a EnumDecl> {
        self.cur_module_enums
            .get(&id.name.name)
            .copied()
            .or_else(|| self.project.resolve_enum(id))
    }

    /// Like `resolved_kind`, but against an EXPLICIT env instead of
    /// `self.env`, and returns `None` (rather than panicking) for
    /// `Bundle`/`Array` — `resolved_kind`'s own panics assume its only
    /// caller (`build_decls`'s bundle-flattening loop) never hands it a
    /// bundle/array type directly, an invariant that does NOT hold for a
    /// `fn`'s declared return type or an instance's port type (both can
    /// legitimately be `Bundle`-typed in the source, unlike an
    /// already-flattened `Port`/`Wire`/`Reg` field).
    pub(super) fn scalar_kind_in_env(
        &self,
        ty: &Type,
        env: &Env,
    ) -> Option<crate::width_rules::Kind> {
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
                let en = self.resolve_enum(id)?;
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
                if let Some(en) = self.resolve_enum(id) {
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
    /// at `hoist_pos` — see `fn module`'s own
    /// `self.out.insert_str(hoist_pos, &self.hoisted_decls)` call, a
    /// separate splice point from `fn_pos`, where the `clog2`/user-`fn`
    /// injections land) and returns the wire's name instead of
    /// `rendered_text`. Returns `rendered_text` unchanged when there is no
    /// mismatch (the common case — no new wire, no behavior change).
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
            // Position-matrix invariant (Task 4): this doc comment's own
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
            // Round-4 plan Task 9: the classifier's own `None` here means
            // "Verilog's rendered width already equals mimz's, nothing to
            // hoist toward" — the SAME claim the bare-identifier branch
            // above checks, just for a non-identifier shape. This function's
            // own doc comment already states the caller contract this
            // checks: `mimz_kind` must be the SAME `Kind` a fresh
            // `infer_kind(expr, decls, env)` call would return, not a
            // stale value threaded through from an earlier position.
            //
            // Round-6 plan Task 11 (round-6 review Part 5.3): checked
            // against the call sites rather than assumed — **this assert is
            // tautological as currently invoked, not a live check.** All
            // eleven `hoist_if_needed` call sites (`expr.rs`) compute
            // `mimz_kind` as `infer_kind(expr, &decls, &self.env)`
            // immediately before this call, with the exact same `decls`
            // and `self.env` still in scope — so the re-computation below
            // is calling the same pure function with the same arguments a
            // second time. It cannot fail today; unlike the bare-
            // identifier branch's own assert above (real: `decls` and
            // `mimz_kind` come from genuinely different sources there,
            // `build_decls` vs. the caller's own `infer_kind` call), this
            // one cannot yet catch BUG-41/BUG-42's class the way its
            // original comment claimed. Kept anyway, deliberately, as a
            // REFACTOR TRIPWIRE: the day a caller threads a `mimz_kind`
            // computed for a DIFFERENT expression, or a cached/stale one
            // from an earlier position, this starts firing — cheaper than
            // waiting for that hypothetical caller to also ship a silent
            // miscompile before anyone notices the contract broke.
            debug_assert!(
                crate::emit_verilog::kinds::infer_kind(expr, decls, &self.env) == Some(mimz_kind),
                "hoist_if_needed: classifier says nothing to compare for {expr:?}, but caller's \
                 mimz_kind {mimz_kind:?} doesn't match a fresh infer_kind computation ({:?})",
                crate::emit_verilog::kinds::infer_kind(expr, decls, &self.env),
            );
            return rendered_text;
        };
        if mimz_kind == verilog_kind {
            return rendered_text;
        }
        // Task 4 (BUG-63): Task 2 gives a `fn` body a real `cur_decls`, which means a real
        // MISMATCH — and therefore a real hoist — can now fire inside one
        // for the first time (`nand(extend(x, 8))`, a working example one
        // screen up in that plan's own repro table). A MODULE-scope
        // `wire`/`assign` pair (below) is illegal here — a `function
        // automatic` cannot reference it forward — so hoist into a
        // function-local `reg` instead, via `render_fn_operand`'s
        // buffer, which the caller (`funcs.rs`) drains into a blocking-
        // assignment statement immediately before the statement this
        // operand belongs to.
        if self.in_fn_body {
            self.fn_hoist_counter += 1;
            let name = format!("__mimz_fn_sub_{}", self.fn_hoist_counter);
            let ty = if mimz_kind.signed {
                format!("signed [{}:0]", mimz_kind.width.saturating_sub(1))
            } else {
                format!("[{}:0]", mimz_kind.width.saturating_sub(1))
            };
            self.fn_hoisted_regs
                .push_str(&format!("        reg {ty} {name};\n"));
            self.fn_hoisted_stmts
                .push(format!("{name} = {rendered_text};"));
            return name;
        }
        self.hoist_counter += 1;
        let name = format!("__mimz_sub_{}", self.hoist_counter);
        let ty = if mimz_kind.signed {
            format!("signed [{}:0]", mimz_kind.width.saturating_sub(1))
        } else {
            format!("[{}:0]", mimz_kind.width.saturating_sub(1))
        };
        self.push_hoisted_decl(&ty, &name, &rendered_text);
        name
    }

    /// Round-7 plan Task 3 (BUG-66, GAP-18): `hoist_if_needed`/
    /// `hoist_slice_base_if_needed`'s shared module-scope tail — append the
    /// `wire`/`assign` pair to whichever buffer this render site's own
    /// insertion point is safe for. `self.in_pre_decl_render` is true only
    /// while rendering `mem` init/depth, `reg` reset, or an instance port
    /// connection (`module/mod.rs`) — the three sites BUG-66 found running
    /// before `hoist_pos` is captured; everywhere else (drives, seq blocks,
    /// asserts, covers…) still goes to `hoisted_decls` exactly as before.
    fn push_hoisted_decl(&mut self, ty: &str, name: &str, rendered_text: &str) {
        let buf = if self.in_pre_decl_render {
            &mut self.pre_decl_hoisted_decls
        } else {
            &mut self.hoisted_decls
        };
        buf.push_str(&format!("    wire {ty} {name};\n"));
        buf.push_str(&format!("    assign {name} = {rendered_text};\n"));
    }

    /// Same mismatch detection, but for BUG-20's condition instead of a
    /// width mismatch: hoists whenever `rendered_text` (a slice's base)
    /// isn't already a plain identifier, since Verilog's part-select
    /// grammar only accepts one. Shares the same counter as `hoist_if_needed`
    /// (a single per-module numbering sequence for every kind of hoist, not
    /// two separate ones) and the same buffer-routing (`push_hoisted_decl`,
    /// round-7 plan Task 3).
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
        // Task 4's real fix (reg-based in-`fn` hoisting) no longer needs a
        // span here — it stopped diagnosing and started hoisting — but the
        // `hoist_unresolved`-family call sites this function shares a
        // signature shape with all have one to pass, so it stays for that
        // symmetry rather than forcing six call sites to special-case this
        // one function.
        _span: crate::span::Span,
    ) -> String {
        if super::expr::is_plain_identifier(&rendered_text) {
            return rendered_text;
        }
        // Task 4 (BUG-63) — same reasoning as `hoist_if_needed`'s own
        // `in_fn_body` branch immediately above: this is the grammar-
        // required-wire family (a slice/bit-select/`trunc` base), so
        // leaving `rendered_text` un-hoisted here isn't just a width risk,
        // it's flatly unparseable. Hoist into a function-local `reg` the
        // same way, via the same `fn_hoisted_regs`/`fn_hoisted_stmts`
        // buffer `render_fn_operand` drains.
        if self.in_fn_body {
            self.fn_hoist_counter += 1;
            let name = format!("__mimz_fn_sub_{}", self.fn_hoist_counter);
            let ty = if signed {
                format!("signed [{}:0]", width.saturating_sub(1))
            } else {
                format!("[{}:0]", width.saturating_sub(1))
            };
            self.fn_hoisted_regs
                .push_str(&format!("        reg {ty} {name};\n"));
            self.fn_hoisted_stmts
                .push(format!("{name} = {rendered_text};"));
            return name;
        }
        self.hoist_counter += 1;
        let name = format!("__mimz_sub_{}", self.hoist_counter);
        let ty = if signed {
            format!("signed [{}:0]", width.saturating_sub(1))
        } else {
            format!("[{}:0]", width.saturating_sub(1))
        };
        self.push_hoisted_decl(&ty, &name, &rendered_text);
        name
    }

    /// Task 1 (GAP-16): the
    /// single routing point every hoist call site's `None` arm goes through
    /// instead of returning `rendered_text` unchanged. `infer_kind` (or
    /// `verilog_self_determined_kind`) returning `None` at a hoist position
    /// used to be treated as "already correct, nothing to hoist toward" —
    /// BUG-62 showed that's false for a `fn`-body local, a testbench
    /// signal, or a symbolic parametric width: `decls` simply has no entry
    /// for the name, and the fallback renders text whose width mimz never
    /// checked, or — for `requires_named_wire` positions — text that
    /// real Verilog's grammar rejects outright.
    ///
    /// Every legitimately-unresolvable shape is enumerated above (a
    /// `fn`-body local, a module `parameter`, a testbench signal, a
    /// symbolic parametric width) and Tasks 2/3 close them by giving those
    /// contexts a real `decls`; after that lands this should never fire for
    /// a module- or `fn`-body expression, so the `debug_assert!` is
    /// unconditional rather than gated on a "we didn't expect this" guess —
    /// silence here is the bug this task exists to end.
    ///
    /// `requires_named_wire` is the `hoist_slice_base_if_needed`-family
    /// distinction: a slice/bit-select/`trunc` base whose Verilog grammar
    /// only accepts a plain identifier. Leaving a composite expression
    /// there unchanged doesn't just risk a wrong width, it can be
    /// unparseable — `mimz compile` must not exit 0 having written invalid
    /// Verilog, so this pushes a real `Diag` instead. A `hoist_if_needed`-
    /// family caller's fallback text is still syntactically valid Verilog
    /// (just possibly the wrong width), so it gets the assert only.
    pub(in crate::emit_verilog) fn hoist_unresolved(
        &mut self,
        expr: &Expr,
        site: &str,
        rendered_text: String,
        requires_named_wire: bool,
    ) -> String {
        // Mirrors `hoist_if_needed`/`hoist_slice_base_if_needed`'s own
        // early return, one level up: a rendered text that is ALREADY a
        // plain identifier needs no hoist regardless of whether `Kind`
        // resolved — Verilog self-determines a named signal at its own
        // declared width, and a bit-select/slice/`trunc` base's grammar
        // only ever needed a bare identifier in the first place, whether
        // or not `decls` happens to carry an entry for it. An array-typed
        // `fn` param's element access (`vals[i]` -> `vals_3`, elaborated
        // by `expr.rs`'s own `Index` arm before this is ever reached) is
        // the common legitimate case this catches: `vals` is deliberately
        // absent from `decls` (Task 2's own doc comment — it elaborates to
        // scalars, never a single `Kind`), so `infer_kind` genuinely can't
        // resolve it, but the text it already produced needs nothing more
        // done to it. Without this, EVERY array-element comparison/hoist
        // position in a `fn` body would assert, which is not what Task 1
        // means by "reaching the fallback is a bug" — reaching it for a
        // COMPOSITE (non-identifier) expression is.
        if super::expr::is_plain_identifier(&rendered_text) {
            return rendered_text;
        }
        // Task 3 (BUG-62(b)): a symbolic-width `extend(x, W)` reaching
        // THIS point (rather than being widened explicitly) means every
        // call site that knows how to do that (`try_widen_symbolic_extend`,
        // `expr.rs`) either isn't this one or couldn't resolve `x`'s own
        // `Kind` either — a residual case, not silently passed through
        // here (that would re-emit the un-widened text this task exists
        // to stop doing), so it falls through to the loud assert below
        // like everything else.
        let grammar_note = if requires_named_wire {
            " (this position's Verilog grammar only accepts a named signal — the \
              emitted text may not even parse)"
        } else {
            ""
        };
        let msg = format!(
            "hoist_unresolved: `infer_kind` returned None for {expr:?} at `{site}`{grammar_note} \
             (rendered as `{rendered_text}`) — GAP-16/BUG-62: a hoist site may not \
             silently do nothing when it cannot resolve a Kind. If this shape is \
             genuinely unresolvable, name it in this function's own doc comment \
             instead of loosening the assert."
        );
        // Round-7 plan Task 2 (review Part 4.3, BUG-67/68): this used to be
        // a bare `debug_assert!(false, ...)` reached with NO `Diag` at all
        // for a `requires_named_wire: false` site (concat/reduction/cast/
        // encoding members) — combined with `[profile.release]
        // debug-assertions = true`, a SHIPPED `mimz compile` aborted with a
        // Rust panic backtrace on a checker-clean program instead of
        // exiting non-zero with a message. `debug_assert!` alone can't tell
        // "a developer's test caught this" from "a user's release binary
        // hit this" — both have `debug_assertions` on in this workspace —
        // so the gate is `cfg!(test)` instead: true for the `cargo test`
        // binary (every `emit_src`-style unit/regression test here still
        // gets a loud, immediate panic — reaching this fallback during
        // development IS a bug), false for the actual `mimz` CLI binary in
        // EITHER profile, which now always gets a `Diag` and a clean exit.
        // `cfg!(test)` is deliberately a per-compilation constant here —
        // that's the whole mechanism (a different binary sees a different
        // constant) — not an accidentally-always-true/false condition.
        #[allow(clippy::assertions_on_constants)]
        {
            debug_assert!(!cfg!(test), "{msg}");
        }
        // Round-8 plan Task 6, item 6 (review Part 1, item 2's nit): `msg`
        // above embeds `{expr:?}`, a raw AST `Debug` dump — fine for the
        // `debug_assert!` text (a developer reading a test failure), wrong
        // for a shipped compiler's user-facing `Diag`. This mirrors `msg`'s
        // own content (site, grammar note, rendered text, GAP-16 pointer)
        // without the AST dump.
        self.err(
            expr.span,
            format!(
                "cannot determine `{rendered_text}`'s width here — GAP-16: \
                 hoist_unresolved: `infer_kind` returned `None` at `{site}`{grammar_note} \
                 (rendered as `{rendered_text}`) — a hoist site may not silently do \
                 nothing when it cannot resolve a Kind."
            ),
            "this is a compiler limitation; simplify the expression or file a bug \
             report (docs/audit/bugs.md, GAP-16)",
        );
        rendered_text
    }
}
