use super::*;
use crate::diag::Diag;

/// Builds an [`Rw`] from individual field borrows (not a `&self` method) so
/// callers can hold it alongside `&mut self.comb` in one statement.
#[allow(clippy::too_many_arguments)]
fn build_rw<'x>(
    insts: &'x HashSet<String>,
    enums: &'x HashMap<String, &'x ast::EnumDecl>,
    bundle_sigs: &'x HashSet<String>,
    consts: &'x BTreeMap<String, i128>,
    subst: &'x HashMap<String, Expr>,
    func_reg: &'x FuncRegistry<'x>,
    bundle_reg: &'x BundleRegistry<'x>,
    imports: &'x [ast::Import],
) -> Rw<'x, 'x> {
    Rw {
        insts,
        enums,
        bundle_sigs,
        consts,
        subst,
        func_reg,
        bundle_reg,
        imports,
    }
}

/// Read-only per-module elaboration context plus the thirteen mutable
/// accumulators `run_worklist` fills in.
struct Elaboration<'a> {
    reg: &'a Registry<'a>,
    extern_reg: &'a ExternRegistry<'a>,
    func_reg: &'a FuncRegistry<'a>,
    bundle_reg: &'a BundleRegistry<'a>,
    enum_reg: &'a EnumRegistry<'a>,
    file: &'a ast::File,
    m: &'a ast::Module,
    depth: u32,
    mode: SimMode,

    consts: BTreeMap<String, i128>,
    funcs: HashMap<String, FuncDecl>,
    enums: HashMap<String, &'a ast::EnumDecl>,
    insts: HashSet<String>,
    bundle_sigs: HashSet<String>,
    bundle_sig_types: HashMap<String, ast::Type>,
    /// The empty subst map `rw0()` rewrites with (a field, since `Rw::subst` must outlive each call).
    no_subst: HashMap<String, Expr>,

    // Accumulators mutated by `run_worklist`'s per-item handling.
    inputs: Vec<Signal>,
    outputs: Vec<Signal>,
    wires: Vec<Signal>,
    regs: Vec<Reg>,
    mems: Vec<Mem>,
    comb: BTreeMap<String, Expr>,
    procs: Vec<Process>,
    clocks: Vec<String>,
    resets: Vec<String>,
    /// Bit-indexed drives (`sum[i] = …`, from `repeat`), assembled into one
    /// whole-signal Concat driver after the loop.
    bit_drives: BTreeMap<String, BTreeMap<u32, Expr>>,
    flat: Flat,
    /// Combinational `assert`s (module-item form) — see [`Design::asserts`].
    asserts: Vec<AssertStmt>,
    /// Combinational `cover`s (module-item form) — see [`Design::covers`].
    covers: Vec<CoverStmt>,
}

impl<'a> Elaboration<'a> {
    /// Depth guard, const folding, and the funcs/enums/insts/bundle-signal pre-scan.
    #[allow(clippy::too_many_arguments)]
    fn new(
        reg: &'a Registry<'a>,
        extern_reg: &'a ExternRegistry<'a>,
        func_reg: &'a FuncRegistry<'a>,
        bundle_reg: &'a BundleRegistry<'a>,
        enum_reg: &'a EnumRegistry<'a>,
        file: &'a ast::File,
        m: &'a ast::Module,
        params: &BTreeMap<String, i128>,
        depth: u32,
        mode: SimMode,
    ) -> Result<Self, Box<Diag>> {
        if depth > MAX_INSTANCE_DEPTH {
            return Err(Box::new(
                Diag::new(
                    m.span,
                    format!(
                        "instance nesting exceeds {MAX_INSTANCE_DEPTH} levels in `{}` — \
                         a module likely instantiates itself (directly or in a cycle)",
                        m.name.name
                    ),
                )
                .with_code("S0119"),
            ));
        }
        // Compile-time integer environment: params (override or default), then
        // file-level and module-level consts (same order as `comb::eval_outputs`).
        let mut consts: BTreeMap<String, i128> = BTreeMap::new();
        for p in &m.params {
            let v = match params.get(&p.name.name) {
                Some(v) => *v,
                None => match &p.default {
                    Some(d) => const_eval(d, &consts)?,
                    None => {
                        return Err(Box::new(
                            Diag::new(
                                p.name.span,
                                format!(
                                    "parameter `{}` has no default — provide a value for it",
                                    p.name.name
                                ),
                            )
                            .with_code("S0121"),
                        ));
                    }
                },
            };
            consts.insert(p.name.name.clone(), v);
        }
        for item in &file.items {
            if let ast::TopItem::Const(c) = item {
                let v = const_eval(&c.value, &consts)?;
                consts.insert(c.name.name.clone(), v);
            }
        }
        for it in &m.items {
            if let ModuleItem::Const(c) = it {
                let v = const_eval(&c.value, &consts)?;
                consts.insert(c.name.name.clone(), v);
            }
        }

        // User-defined functions from ALL project files (D3: functions are
        // project-wide) — collected from `func_reg` so the kernel's expression
        // evaluator can call any fn regardless of which imported file defines it.
        // A bundle-typed parameter is pre-flattened here, ONCE, into one
        // scalar param per field (BUG-15) — see
        // `flatten_bundle_params_in_func`'s own doc for why this is done
        // once at registry-build time rather than by teaching the runtime
        // evaluator (`eval_fn_call`) about bundles directly.
        let funcs: HashMap<String, FuncDecl> = func_reg
            .values()
            .map(|f| {
                let flat =
                    flatten_bundle_params_in_func(f, bundle_reg, enum_reg, &file.imports, &consts)?;
                Ok((flat.name.name.clone(), flat))
            })
            .collect::<Result<_, Box<Diag>>>()?;

        // Enums visible to this module: file-scoped ones from `enum_reg` (spec/02
        // §1.5b — `enum Name { ... }` declared alongside the module, not inside
        // it) plus any declared INSIDE this module body (`ModuleItem::Enum`,
        // still supported), the latter taking priority on a name clash. The
        // total wire width (`inferred_total_width`) is set by the checker; falls
        // back to `clog2(count)` for tag-only enums when the checker has not run
        // (e.g. bare sim tests).
        let mut enums: HashMap<String, &'a ast::EnumDecl> = enum_reg.clone();
        for it in &m.items {
            if let ModuleItem::Enum(e) = it {
                enums.insert(e.name.name.clone(), e);
            }
        }

        // Instance names (top-level AND inside `repeat`), so `inst.port` and the
        // array form `arr[i].port` rewrite to their flat wire names.
        let mut insts: HashSet<String> = HashSet::new();
        collect_inst_names(&m.items, &consts, &mut insts);

        // Bundle-typed signal names at this module level: used by `Rw::field` to
        // rewrite `req.valid` → `req_valid` (the flat scalar name).
        // HashSet: O(1) lookup, no ordering needed.
        let mut bundle_sigs: HashSet<String> = HashSet::new();
        // Map each bundle signal name to its AST type (for O(1) lookup in Drive arm).
        let mut bundle_sig_types: HashMap<String, ast::Type> = HashMap::new();
        // Pre-scan to collect bundle signal names before building rw0 (which needs them).
        for it in &m.items {
            match it {
                ModuleItem::Port { name, ty, .. } | ModuleItem::Wire { name, ty, .. }
                    if is_bundle_ty(ty, bundle_reg, &enums) =>
                {
                    bundle_sigs.insert(name.name.clone());
                    bundle_sig_types.insert(name.name.clone(), ty.clone());
                }
                _ => {}
            }
        }

        Ok(Self {
            reg,
            extern_reg,
            func_reg,
            bundle_reg,
            enum_reg,
            file,
            m,
            depth,
            mode,
            consts,
            funcs,
            enums,
            insts,
            bundle_sigs,
            bundle_sig_types,
            no_subst: HashMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            wires: Vec::new(),
            regs: Vec::new(),
            mems: Vec::new(),
            comb: BTreeMap::new(),
            procs: Vec::new(),
            clocks: Vec::new(),
            resets: Vec::new(),
            bit_drives: BTreeMap::new(),
            flat: Flat::default(),
            asserts: Vec::new(),
            covers: Vec::new(),
        })
    }

    /// A fresh no-substitution rewriter over this module's context (built on demand, not stored).
    fn rw0(&self) -> Rw<'_, '_> {
        build_rw(
            &self.insts,
            &self.enums,
            &self.bundle_sigs,
            &self.consts,
            &self.no_subst,
            self.func_reg,
            self.bundle_reg,
            &self.file.imports,
        )
    }

    /// Folded width of a type — an enum type resolves to its total wire width.
    /// Uses `inferred_total_width` when the checker has run; falls back to
    /// `clog2(variant count)` for tag-only enums without the checker.
    ///
    /// `span` (a declaration's own `Ident.span` at every call site — `ast::Type`
    /// itself carries no span) anchors the one error this can raise directly
    /// (unknown enum type) and the ones it propagates from `type_width`
    /// (`value/mod.rs`).
    fn width_of(
        &self,
        ty: &ast::Type,
        ints: &BTreeMap<String, i128>,
        span: crate::span::Span,
    ) -> Result<Width, Box<Diag>> {
        if let ast::Type::Named(n) = ty {
            let e = self.enums.get(&n.name.name).ok_or_else(|| {
                Box::new(
                    Diag::new(span, format!("unknown enum type `{}`", n.name.name))
                        .with_code("S0122"),
                )
            })?;
            // note: fallback only correct for tag-only enums (max_payload_w=0)
            let bits = e.inferred_total_width.get().unwrap_or_else(|| {
                debug_assert!(
                    e.variants.iter().all(|v| v.fields.is_empty()),
                    "tagged enum without checker run — fallback width is wrong"
                );
                clog2(e.variants.len())
            });
            Ok(Width {
                bits,
                signed: false,
            })
        } else {
            let (bits, signed) = type_width(ty, ints, span)?;
            Ok(Width { bits, signed })
        }
    }

    /// `sync loop`/`foreach` lowering; the caller keeps the returned lists alive
    /// (the worklist holds `&ModuleItem` borrows into them).
    fn lower_items(&self, expanded_items: &[ModuleItem]) -> (Vec<ModuleItem>, Vec<ModuleItem>) {
        // `sync loop` is lowered to plain Port/Reg/On/Drive items (the same
        // desugaring the Verilog emitter uses, `ast::lower_sync_loop` — Task 4)
        // BEFORE the worklist starts, so the loop never sees a `SyncLoop`
        // node. `collect_lowered_sync_loops` recurses into `const if` winning
        // branches (at any nesting depth) to find every `SyncLoop`, mirroring
        // `emit_verilog::module::flatten_items`'s recursive ConstIf/SyncLoop
        // handling — a `SyncLoop` nested inside a `const if` is
        // checker-accepted and emitter-supported, so the simulator must lower
        // it too. The main worklist loop's own `ConstIf` arm filters raw
        // `SyncLoop` items out of whichever branch it pushes, so none of them
        // reach the worklist un-lowered (the main match's fallback would
        // otherwise panic via the `unreachable!()` arm).
        let mut lowered_sync_loops: Vec<ModuleItem> = Vec::new();
        collect_lowered_sync_loops(expanded_items, &self.consts, &mut lowered_sync_loops);

        // `foreach` is pure sugar for `repeat`/bare `loop` (see
        // `ast::foreach_lower`) — lowered up front for the same reason as
        // `sync loop` above. `collect_lowered_foreach` recurses into
        // `const if` winning branches the same way
        // `collect_lowered_sync_loops` does.
        let mut lowered_foreach: Vec<ModuleItem> = Vec::new();
        collect_lowered_foreach(expanded_items, &self.consts, &mut lowered_foreach);

        (lowered_sync_loops, lowered_foreach)
    }

    /// Unroll one `repeat` block against `outer_consts` (the enclosing
    /// scope's const env — the module's own top-level consts for a
    /// top-level `repeat`, or the ENCLOSING repeat's own iteration env
    /// when this one is nested, so a nested loop's bounds/body can see the
    /// outer loop variable). Round-4 plan Task 8 (BUG-53's own check/sim/
    /// emit split): a `repeat` body used to be walked by a SEPARATE,
    /// hand-restricted loop that understood only `Inst`/`Drive` and
    /// rejected `Repeat`/anything else with S0125/S0126 — even though the
    /// checker's own `no_decls_in_repeat` (E0303) already treats nested
    /// `repeat`/`const if` as legal hardware generation ("`repeat` may
    /// only generate hardware... never declare it", explicitly naming
    /// nested `repeat`s as allowed). This function and
    /// [`Self::elaborate_repeat_body_item`] replace that restricted copy
    /// with one recursive walk, so the simulator actually supports what
    /// the checker's own design already committed to.
    fn elaborate_repeat(
        &mut self,
        r: &'a ast::Repeat,
        outer_consts: &BTreeMap<String, i128>,
    ) -> Result<(), Box<Diag>> {
        let lo = const_eval(&r.lo, outer_consts)?;
        let hi = const_eval(&r.hi, outer_consts)?;
        // `checked_sub`: extreme bounds (`hi - lo` past i128::MAX) must not
        // overflow-panic — treat an out-of-range span as over-budget.
        let count = hi.checked_sub(lo).unwrap_or(i128::MAX).max(0);
        if count > REPEAT_BUDGET {
            return Err(Box::new(
                Diag::new(
                    r.span,
                    format!(
                        "`repeat` would unroll {count} times, over the limit of {REPEAT_BUDGET}"
                    ),
                )
                .with_code("S0124"),
            ));
        }
        for iv in lo..hi {
            let mut ci = outer_consts.clone();
            ci.insert(r.var.name.clone(), iv);
            let subst = HashMap::from([(r.var.name.clone(), int_expr(iv, r.span))]);
            for body_it in &r.items {
                self.elaborate_repeat_body_item(body_it, &ci, &subst, r.span)?;
            }
        }
        Ok(())
    }

    /// One `repeat`-body item, against the current (possibly nested) loop
    /// env `ci`/`subst`. `enclosing_span` is the nearest enclosing
    /// `repeat`'s own span, used only for this function's own S0126 —
    /// every OTHER item kind that could reach it is already rejected by
    /// the checker's `no_decls_in_repeat` (E0303) before the simulator
    /// ever runs, so a real per-item span here would be unreachable
    /// dead code, same reasoning the `ConstIf` arm's own `unwrap_or(0)`
    /// already documents for a non-const condition.
    ///
    /// `Inst`'s array-index naming folds `inst.index` against `ci` —
    /// BUG-53's exact fix (`docs/audit/bugs.md`), mirrored here from the
    /// emitter's own `inst_name` (`emit_verilog/mod.rs`) rather than the
    /// pre-fix assumption that the index expression IS the loop counter
    /// `iv`, which silently mis-keyed every array-instance whose index
    /// wasn't the bare loop variable (an offset like `i + 1`, or a nested
    /// loop's own variable) — the exact "offset index" shape BUG-53 filed.
    fn elaborate_repeat_body_item(
        &mut self,
        body_it: &'a ModuleItem,
        ci: &BTreeMap<String, i128>,
        subst: &HashMap<String, Expr>,
        enclosing_span: crate::span::Span,
    ) -> Result<(), Box<Diag>> {
        match body_it {
            ModuleItem::Inst(inst) => {
                let iname = match &inst.index {
                    Some(idx) => {
                        let n = const_eval(idx, ci)?;
                        format!("{}__{}", inst.name.name, n)
                    }
                    None => inst.name.name.clone(),
                };
                let f = flatten_instance(
                    self.reg,
                    self.extern_reg,
                    self.func_reg,
                    self.bundle_reg,
                    self.enum_reg,
                    &self.file.imports,
                    ci,
                    &self.insts,
                    &self.enums,
                    &self.bundle_sigs,
                    subst,
                    inst,
                    &iname,
                    self.depth,
                    self.mode,
                )?;
                self.flat.absorb(f);
            }
            ModuleItem::Drive { lhs, rhs } => {
                let rwi = build_rw(
                    &self.insts,
                    &self.enums,
                    &self.bundle_sigs,
                    ci,
                    subst,
                    self.func_reg,
                    self.bundle_reg,
                    &self.file.imports,
                );
                record_drive(lhs, rhs, &rwi, ci, &mut self.comb, &mut self.bit_drives)?;
            }
            ModuleItem::Repeat(nested) => self.elaborate_repeat(nested, ci)?,
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                let val = const_eval(cond, ci)?;
                let branch: &'a [ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                for it in branch {
                    self.elaborate_repeat_body_item(it, ci, subst, enclosing_span)?;
                }
            }
            _ => {
                return Err(Box::new(
                    Diag::new(
                        enclosing_span,
                        "a `repeat` body may only contain instances, drives, nested \
                         `repeat`s, or `const if` — this item should already have been \
                         rejected by the checker (E0303)",
                    )
                    .with_code("S0126"),
                ));
            }
        }
        Ok(())
    }

    /// Pops one `ModuleItem` at a time, folding it into the accumulators.
    /// `work`/`expanded_items` are parameters, not fields (the `ConstIf`
    /// arm's `work.extend(...)` push-back needs an unaliased `Vec`).
    fn run_worklist(
        &mut self,
        mut work: Vec<&'a ModuleItem>,
        expanded_items: &'a [ModuleItem],
    ) -> Result<(), Box<Diag>> {
        while let Some(it) = work.pop() {
            match it {
                ModuleItem::Port { dir, name, ty } => {
                    // Bundle port → N scalar signals named `portname_fieldname`.
                    if let Some((bname, args)) = bundle_type_info(ty, self.bundle_reg, &self.enums)
                    {
                        let fields = resolve_bundle_fields_sim(
                            self.bundle_reg,
                            &self.file.imports,
                            &bname,
                            &args,
                            &self.consts,
                        )?;
                        for (fname, fwidth) in fields {
                            let flat_name = format!("{}_{}", name.name, fname);
                            let sig = Signal {
                                name: flat_name,
                                width: fwidth,
                            };
                            match dir {
                                Dir::In => self.inputs.push(sig),
                                Dir::Out => self.outputs.push(sig),
                            }
                        }
                    } else {
                        let width = self.width_of(ty, &self.consts, name.span)?;
                        let sig = Signal {
                            name: name.name.clone(),
                            width,
                        };
                        match dir {
                            Dir::In => self.inputs.push(sig),
                            Dir::Out => self.outputs.push(sig),
                        }
                    }
                }
                ModuleItem::Clock(n) => self.clocks.push(n.name.clone()),
                // The cycle-based kernel applies reset at the clock edge; an async
                // reset is observationally identical at the per-cycle sample points
                // (sub-cycle timing is out of the kernel's model), so `is_async` is
                // an emitter-only distinction and the sim just records the name. The
                // path to modeling sub-cycle reset timing is the three-tier fidelity
                // roadmap in docs/plan/phase-1.5-simulator.md (currently Tier 3:
                // delegate timing-faithful runs to the Verilog/Icarus oracle).
                ModuleItem::Reset { name: n, .. } => self.resets.push(n.name.clone()),
                ModuleItem::Wire { name, ty, init } => {
                    // Bundle wire → N scalar wires, each driven by the corresponding
                    // field of the bundle init expression (must be a BundleLit).
                    if let Some((bname, args)) = bundle_type_info(ty, self.bundle_reg, &self.enums)
                    {
                        let fields = resolve_bundle_fields_sim(
                            self.bundle_reg,
                            &self.file.imports,
                            &bname,
                            &args,
                            &self.consts,
                        )?;
                        for (fname, fwidth) in fields {
                            let flat_name = format!("{}_{}", name.name, fname);
                            self.wires.push(Signal {
                                name: flat_name.clone(),
                                width: fwidth,
                            });
                            // The driver for each scalar field comes from the BundleLit init.
                            let field_init = bundle_field_expr(init, &fname, init.span);
                            let driver = self.rw0().expr(&field_init)?;
                            self.comb.insert(flat_name, driver);
                        }
                    } else {
                        let width = self.width_of(ty, &self.consts, name.span)?;
                        self.wires.push(Signal {
                            name: name.name.clone(),
                            width,
                        });
                        let driver = self.rw0().expr(init)?;
                        self.comb.insert(name.name.clone(), driver);
                    }
                }
                ModuleItem::Reg { name, ty, reset } => {
                    let width = self.width_of(ty, &self.consts, name.span)?;
                    let reset_expr = self.rw0().expr(reset)?;
                    let reset = const_eval_wide(&reset_expr, &self.consts)?;
                    self.regs.push(Reg {
                        name: name.name.clone(),
                        width,
                        reset,
                        clock: String::new(),
                        edge: Edge::Rise,
                    });
                }
                ModuleItem::Mem {
                    name,
                    ty,
                    depth,
                    init,
                } => {
                    let width = self.width_of(ty, &self.consts, name.span)?;
                    // The sim runs WITHOUT the checker, so guard the depth here too.
                    let depth_expr = self.rw0().expr(depth)?;
                    let d = const_eval(&depth_expr, &self.consts)?;
                    let depth = u128::try_from(d).ok().filter(|d| *d >= 1).ok_or_else(|| {
                        Box::new(
                            Diag::new(
                                depth.span,
                                format!("memory `{}` has a non-positive depth ({d})", name.name),
                            )
                            .with_code("S0123"),
                        )
                    })?;
                    let init_expr = self.rw0().expr(init)?;
                    let init = const_eval_wide(&init_expr, &self.consts)?;
                    self.mems.push(Mem {
                        name: name.name.clone(),
                        width,
                        depth,
                        init,
                        clock: String::new(),
                        edge: Edge::Rise,
                    });
                }
                ModuleItem::Drive { lhs, rhs } => {
                    // Bundle drive: `rsp = req` where `rsp` is a bundle signal.
                    // Expand to one scalar drive per field: `rsp_valid = req_valid`, etc.
                    if self.bundle_sigs.contains(&lhs.base.name) && lhs.index.is_none() {
                        // O(1) lookup: bundle type from the pre-scan map.
                        if let Some(ty) = self.bundle_sig_types.get(&lhs.base.name)
                            && let Some((bname, args)) =
                                bundle_type_info(ty, self.bundle_reg, &self.enums)
                        {
                            let fields = resolve_bundle_fields_sim(
                                self.bundle_reg,
                                &self.file.imports,
                                &bname,
                                &args,
                                &self.consts,
                            )?;
                            for (fname, _) in &fields {
                                let flat_lhs = format!("{}_{}", lhs.base.name, fname);
                                // The RHS field: either `rhs.fname` (if rhs is a bundle ident)
                                // or a BundleLit field extraction.
                                let field_rhs = bundle_field_expr(rhs, fname, rhs.span);
                                let driver = self.rw0().expr(&field_rhs)?;
                                self.comb.insert(flat_lhs, driver);
                            }
                        } else {
                            record_drive(
                                lhs,
                                rhs,
                                &build_rw(
                                    &self.insts,
                                    &self.enums,
                                    &self.bundle_sigs,
                                    &self.consts,
                                    &self.no_subst,
                                    self.func_reg,
                                    self.bundle_reg,
                                    &self.file.imports,
                                ),
                                &self.consts,
                                &mut self.comb,
                                &mut self.bit_drives,
                            )?;
                        }
                    } else {
                        record_drive(
                            lhs,
                            rhs,
                            &build_rw(
                                &self.insts,
                                &self.enums,
                                &self.bundle_sigs,
                                &self.consts,
                                &self.no_subst,
                                self.func_reg,
                                self.bundle_reg,
                                &self.file.imports,
                            ),
                            &self.consts,
                            &mut self.comb,
                            &mut self.bit_drives,
                        )?;
                    }
                }
                ModuleItem::On(on) => {
                    // `foreach` inside an `on`-block body is pure sugar over
                    // `loop` (see `ast::foreach_lower`) — lowered here, before
                    // `Rw::seq` ever runs, so `Rw::seq`/`assigns`/the kernel's
                    // `run_seq` never see a raw `SeqStmt::ForEach` node.
                    let lowered_body = lower_foreach_in_seq(&on.body, expanded_items);
                    let process = Process {
                        clock: on.clock.name.clone(),
                        edge: on.edge,
                        body: lowered_body
                            .iter()
                            .map(|s| self.rw0().seq(s, &|n| n.to_string()))
                            .collect::<Result<_, _>>()?,
                    };
                    self.procs.push(process)
                }
                // Consts are folded above; enum decls become the encoding above.
                ModuleItem::Const(_) | ModuleItem::Enum(_) => {}
                // Unreachable: elaboration runs on a strict-parsed tree, which
                // carries no `Error` placeholder.
                ModuleItem::Error(_) => {}
                // Unreachable: every `SyncLoop` is lowered and filtered out of
                // `expanded_items` before the worklist runs (see
                // `elaborate_module`'s `lower_items` call) — the match arm
                // exists only so a future refactor that breaks that filter
                // fails loudly instead of silently dropping the construct.
                ModuleItem::SyncLoop(_) => {
                    unreachable!(
                        "SyncLoop is lowered before the worklist runs — see elaborate_module's lower_items call"
                    )
                }
                // Unreachable: every `ForEach` is lowered (to `Repeat`) and
                // filtered out of `expanded_items` before the worklist runs,
                // same as `SyncLoop` just above.
                ModuleItem::ForEach(_) => {
                    unreachable!(
                        "ForEach is lowered before the worklist runs — see elaborate_module's lower_items call"
                    )
                }
                ModuleItem::Inst(inst) => {
                    let f = flatten_instance(
                        self.reg,
                        self.extern_reg,
                        self.func_reg,
                        self.bundle_reg,
                        self.enum_reg,
                        &self.file.imports,
                        &self.consts,
                        &self.insts,
                        &self.enums,
                        &self.bundle_sigs,
                        &self.no_subst,
                        inst,
                        &inst.name.name,
                        self.depth,
                        self.mode,
                    )?;
                    self.flat.absorb(f);
                }
                ModuleItem::Repeat(r) => {
                    let base = self.consts.clone();
                    self.elaborate_repeat(r, &base)?;
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    // NOTE(deferred): `unwrap_or(0)` here is a safe latent fallback —
                    // the checker rejects non-const `const if` conditions (E0811) before
                    // anything reaches the simulator, so `const_eval` always returns Ok
                    // in practice. If the simulator ever runs without the checker
                    // (e.g. a direct API consumer), a non-const condition silently takes
                    // the `then` branch. Fix: thread the error upward instead of defaulting.
                    let val = const_eval(cond, &self.consts).unwrap_or(0);
                    let branch: &[ModuleItem] = if val != 0 {
                        then
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    // Any `SyncLoop`/`ForEach` in this branch was already lowered
                    // (into `lowered_sync_loops`/`lowered_foreach`) by
                    // `lower_items` above, before the worklist started — filter
                    // the raw nodes back out here so neither is pushed a second
                    // time un-lowered (which would hit the `unreachable!()` arm
                    // above — there is no `ModuleItem::ForEach` match arm).
                    work.extend(
                        branch
                            .iter()
                            .filter(|it| {
                                !matches!(it, ModuleItem::SyncLoop(_) | ModuleItem::ForEach(_))
                            })
                            .rev(),
                    );
                }
                ModuleItem::BundleDestructure { span, .. } => {
                    return Err(Box::new(
                        Diag::new(
                            *span,
                            "bundle destructure in module body is not yet supported by the simulator",
                        )
                        .with_code("S0127"),
                    ));
                }
                ModuleItem::Assert(a) => {
                    self.asserts.push(a.clone());
                }
                ModuleItem::Cover(c) => {
                    self.covers.push(c.clone());
                }
            }
        }
        Ok(())
    }

    /// Post-loop merge, bit-drive Concat assembly, clock/edge propagation, final `Design`.
    fn finish(self) -> Result<Design, Box<Diag>> {
        let Elaboration {
            m,
            consts,
            funcs,
            inputs,
            outputs,
            mut wires,
            mut regs,
            mut mems,
            mut comb,
            mut procs,
            clocks,
            resets,
            bit_drives,
            flat,
            asserts,
            covers,
            ..
        } = self;

        // Merge inlined-instance pieces, then assemble bit-indexed drives into one
        // whole-signal Concat (widest bit first, Verilog concat order).
        let unknown_signals: HashSet<String> = flat.unknown.iter().cloned().collect();
        let extern_instances = flat.extern_instances;
        wires.extend(flat.wires);
        regs.extend(flat.regs);
        // Merge instance drivers, erroring on a name collision instead of silently
        // overwriting (a parent signal named like a flattened `inst_port` wire).
        for (name, driver) in flat.comb {
            if comb.insert(name.clone(), driver).is_some() {
                return Err(Box::new(
                    Diag::new(
                        m.span,
                        format!(
                            "flattened instance signal `{name}` collides with an existing signal in `{}`",
                            m.name.name
                        ),
                    )
                    .with_code("S0128"),
                ));
            }
        }
        procs.extend(flat.procs);
        mems.extend(flat.mems);
        for (sig, bits) in bit_drives {
            let width = inputs
                .iter()
                .chain(&outputs)
                .chain(&wires)
                .find(|s| s.name == sig)
                .map(|s| s.width.bits)
                .ok_or_else(|| {
                    Box::new(
                        Diag::new(
                            m.span,
                            format!("bit-driven signal `{sig}` has no declaration"),
                        )
                        .with_code("S0129"),
                    )
                })?;
            let mut parts = Vec::with_capacity(width as usize);
            for b in (0..width).rev() {
                let e = bits.get(&b).ok_or_else(|| {
                    Box::new(
                        Diag::new(m.span, format!("signal `{sig}` bit {b} is not driven"))
                            .with_code("S0130"),
                    )
                })?;
                parts.push(e.clone());
            }
            let span = parts.first().map(|e| e.span).unwrap_or(m.span);
            comb.insert(
                sig,
                Expr {
                    kind: ExprKind::Concat(parts),
                    span,
                },
            );
        }

        // Each reg's clock is the clock of the `on` block that assigns it (the
        // checker guarantees a reg has exactly one owning block). Covers inlined
        // child regs too — their assigning proc carries the connected parent clock.
        for proc in &procs {
            for reg in &mut regs {
                if assigns(&proc.body, &reg.name) {
                    reg.clock = proc.clock.clone();
                    reg.edge = proc.edge;
                }
            }
            for mem in &mut mems {
                if assigns(&proc.body, &mem.name) {
                    mem.clock = proc.clock.clone();
                    mem.edge = proc.edge;
                }
            }
        }

        Ok(Design {
            module: m.name.name.clone(),
            consts,
            inputs,
            outputs,
            wires,
            regs,
            mems,
            comb,
            procs,
            clocks,
            resets,
            funcs,
            unknown_signals,
            extern_instances,
            asserts,
            covers,
        })
    }
}

/// Elaborate one module (`m`, defined in `file`) under concrete `params`,
/// resolving any instantiated children through `reg`. User-defined functions
/// from ALL project files are supplied via `func_reg` (D3: functions are
/// project-wide) so a fn declared in an imported file is available here.
/// Delegates to [`Elaboration`]: `new`/`lower_items`/`run_worklist`/`finish`.
#[allow(clippy::too_many_arguments)]
pub(super) fn elaborate_module(
    reg: &Registry,
    extern_reg: &ExternRegistry,
    func_reg: &FuncRegistry<'_>,
    bundle_reg: &BundleRegistry<'_>,
    enum_reg: &EnumRegistry<'_>,
    file: &ast::File,
    m: &ast::Module,
    params: &BTreeMap<String, i128>,
    depth: u32,
    mode: SimMode,
) -> Result<Design, Box<Diag>> {
    let mut e = Elaboration::new(
        reg, extern_reg, func_reg, bundle_reg, enum_reg, file, m, params, depth, mode,
    )?;

    // `sync.double_flop`/`sync.pulse` call sites are expanded to plain
    // Reg/On/Wire items BEFORE `sync loop`/`foreach` lowering runs, exactly
    // mirroring `emit_verilog::module::flatten_items`'s own ordering (Task
    // 5) — the two expansions are orthogonal (this one never touches
    // `SyncLoop`/`ForEach` items), so order between them doesn't matter in
    // practice, but running it first keeps both passes' item-list
    // references consistent below. `expanded_items` must outlive the whole
    // function since `work` holds `&ModuleItem` borrows into it, same as
    // `lowered_sync_loops`/`lowered_foreach` below.
    let expanded_items: Vec<ModuleItem> = crate::ast::expand_sync_prims(&m.items);
    let (lowered_sync_loops, lowered_foreach) = e.lower_items(&expanded_items);

    let work: Vec<&ModuleItem> = expanded_items
        .iter()
        .filter(|it| !matches!(it, ModuleItem::SyncLoop(_) | ModuleItem::ForEach(_)))
        .rev()
        .chain(lowered_sync_loops.iter().rev())
        .chain(lowered_foreach.iter().rev())
        .collect();
    e.run_worklist(work, &expanded_items)?;
    e.finish()
}

/// Collect every instance name (top-level, inside `repeat`, and inside a
/// `const if` nested at any depth — round-4 plan Task 8, BUG-53's own
/// check/sim/emit split: this used to recurse into `Repeat` but not
/// `ConstIf`, so an array instance declared behind a `const if` inside a
/// `repeat` was never registered here, and `Rw::field`/`Rw::index` — which
/// consult this set to tell an instance-port read apart from an ordinary
/// signal — fell through to a different, unrelated error path instead of
/// resolving it), so an instance-port read resolves whether the instance is
/// plain or an array. Mirrors `collect_lowered_sync_loops`'s own
/// `const_eval(cond, consts)` resolution, so a branch that resolves one way
/// there resolves the same way here.
fn collect_inst_names(
    items: &[ModuleItem],
    consts: &BTreeMap<String, i128>,
    out: &mut HashSet<String>,
) {
    for it in items {
        match it {
            ModuleItem::Inst(i) => {
                out.insert(i.name.name.clone());
            }
            ModuleItem::Repeat(r) => collect_inst_names(&r.items, consts, out),
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                // Same fallback as every other pre-worklist ConstIf scan in
                // this file — the checker rejects a non-const condition
                // (E0811) before this ever runs in practice.
                let val = const_eval(cond, consts).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                collect_inst_names(branch, consts, out);
            }
            _ => {}
        }
    }
}

/// Recursively find every `SyncLoop` reachable from `items` — including one
/// nested inside a `const if`'s winning branch, at any nesting depth — and
/// push its lowering (`ast::lower_sync_loop`) onto `out`. Mirrors
/// `emit_verilog::module::flatten_items`'s recursive `ConstIf`/`SyncLoop`
/// handling: same `const_eval(cond, consts)` resolution the main worklist
/// loop's own `ConstIf` arm uses, so a branch that resolves one way here
/// resolves the same way there. Called once, before the worklist starts —
/// see `elaborate_module`'s `lowered_sync_loops`.
fn collect_lowered_sync_loops(
    items: &[ModuleItem],
    consts: &BTreeMap<String, i128>,
    out: &mut Vec<ModuleItem>,
) {
    for it in items {
        match it {
            ModuleItem::SyncLoop(sl) => out.extend(ast::lower_sync_loop(sl)),
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                // Same fallback as the main worklist loop's ConstIf arm — the
                // checker rejects a non-const condition (E0811) before this
                // ever runs in practice.
                let val = const_eval(cond, consts).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                collect_lowered_sync_loops(branch, consts, out);
            }
            _ => {}
        }
    }
}

/// Recursively find every `ForEach` reachable from `items` — including one
/// nested inside a `const if`'s winning branch, at any nesting depth — and
/// push its lowering (`ast::lower_foreach_item`) onto `out`. Mirrors
/// `collect_lowered_sync_loops` exactly, for the same reason: `foreach` is
/// pure sugar over `repeat`/bare `loop` (`ast::foreach_lower`'s doc comment),
/// so the simulator lowers it the same way the emitter does. Called once,
/// before the worklist starts — see `elaborate_module`'s `lowered_foreach`.
fn collect_lowered_foreach(
    items: &[ModuleItem],
    consts: &BTreeMap<String, i128>,
    out: &mut Vec<ModuleItem>,
) {
    for it in items {
        match it {
            ModuleItem::ForEach(fe) => {
                // `None` here means the Elements-form source name didn't
                // resolve to an array/mem — the checker rejects this
                // (E0417) before `mimz build`/`mimz test` ever reach here,
                // but `mimz sim`/`mimz test` can run unchecked programs, so
                // this is a reachable path in this file. We silently drop
                // the loop rather than lowering it; any signal the dropped
                // body would have driven surfaces as a driverless-signal
                // read error downstream instead of a crash or wrong output.
                if let Some(lowered) = ast::lower_foreach_item(fe, items) {
                    out.extend(lowered);
                }
            }
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                // Same fallback as the main worklist loop's ConstIf arm — the
                // checker rejects a non-const `const if` condition (E0811)
                // before this ever runs in practice.
                let val = const_eval(cond, consts).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                collect_lowered_foreach(branch, consts, out);
            }
            _ => {}
        }
    }
}

/// Recursively lower every `SeqStmt::ForEach` reachable in `stmts` — inside
/// `If` branches and `Loop` bodies too, at any nesting depth — into its
/// `SeqStmt::Loop` equivalent via `ast::lower_foreach_seq`. Called once, on
/// an `on`-block's raw body, before it becomes a `Process` — so `Rw::seq`,
/// `assigns`, and the kernel's `run_seq` never see a raw `ForEach` node
/// (mirrors why `ModuleItem::ForEach` is pre-lowered before the worklist
/// starts, Task 8). An Elements-form source that doesn't resolve (`None`)
/// silently drops that `foreach`'s statements, matching this file's other
/// `None`-handling for the same construct (see `collect_lowered_foreach`).
fn lower_foreach_in_seq(stmts: &[SeqStmt], module_items: &[ModuleItem]) -> Vec<SeqStmt> {
    let mut out = Vec::new();
    for s in stmts {
        match s {
            SeqStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                if let Some(lowered) =
                    ast::lower_foreach_seq(var, source, body, *span, module_items)
                {
                    out.extend(lower_foreach_in_seq(&lowered, module_items));
                }
            }
            SeqStmt::If { cond, then, els } => {
                out.push(SeqStmt::If {
                    cond: cond.clone(),
                    then: lower_foreach_in_seq(then, module_items),
                    els: els.as_ref().map(|e| lower_foreach_in_seq(e, module_items)),
                });
            }
            SeqStmt::Loop {
                var,
                lo,
                hi,
                body,
                span,
            } => {
                out.push(SeqStmt::Loop {
                    var: var.clone(),
                    lo: lo.clone(),
                    hi: hi.clone(),
                    body: lower_foreach_in_seq(body, module_items),
                    span: *span,
                });
            }
            other => out.push(other.clone()),
        }
    }
    out
}

/// Record one combinational drive. A whole-signal drive becomes a `comb`
/// entry; a bit-indexed drive (`sum[i] = …`, from `repeat`) OR a slice
/// drive (`sig[hi:lo] = …`, BUG-17) is collected per bit and assembled
/// into a Concat after the item loop — a slice drive just expands to one
/// bit-drive entry per bit position, each reading the matching bit
/// straight back off the RHS (`sig[hi:lo] = rhs` behaves exactly like
/// writing `sig[hi] = rhs[hi-lo]; …; sig[lo] = rhs[0];` one bit at a
/// time), so it reuses the exact same per-bit assembly path the
/// bit-indexed case already has — no separate range-aware structure
/// needed. The one exception: a compile-time-constant RHS (common after a
/// `foreach` unroll substitutes its loop variable with a literal, e.g.
/// `lamps[i*8+7:i*8] = i*2`) pulls bits from the FOLDED VALUE instead of
/// indexing the raw expression — see the inner comment for why.
fn record_drive(
    lhs: &ast::LValue,
    rhs: &Expr,
    rw: &Rw,
    consts: &BTreeMap<String, i128>,
    comb: &mut BTreeMap<String, Expr>,
    bit_drives: &mut BTreeMap<String, BTreeMap<u32, Expr>>,
) -> Result<(), Box<Diag>> {
    match &lhs.index {
        None => {
            comb.insert(lhs.base.name.clone(), rw.expr(rhs)?);
        }
        Some((idx, None)) => {
            let idx_expr = rw.expr(idx)?;
            let bit = const_eval(&idx_expr, consts)?;
            // Bound to the evaluator's max width BEFORE `as u32`, so an oversized
            // index can't silently truncate into a valid bit.
            if !(0..128).contains(&bit) {
                return Err(Box::new(
                    Diag::new(
                        idx.span,
                        format!(
                            "bit index {bit} driving `{}` is out of range (0..128)",
                            lhs.base.name
                        ),
                    )
                    .with_code("S0134"),
                ));
            }
            bit_drives
                .entry(lhs.base.name.clone())
                .or_default()
                .insert(bit as u32, rw.expr(rhs)?);
        }
        Some((hi_e, Some(lo_e))) => {
            let hi_expr = rw.expr(hi_e)?;
            let hi = const_eval(&hi_expr, consts)?;
            let lo_expr = rw.expr(lo_e)?;
            let lo = const_eval(&lo_expr, consts)?;
            if !(0..128).contains(&hi) || !(0..128).contains(&lo) {
                return Err(Box::new(
                    Diag::new(
                        hi_e.span.join(lo_e.span),
                        format!(
                            "slice bound driving `{}` is out of range (0..128)",
                            lhs.base.name
                        ),
                    )
                    .with_code("S0135"),
                ));
            }
            if hi < lo {
                return Err(Box::new(
                    Diag::new(
                        hi_e.span.join(lo_e.span),
                        format!(
                            "slice bounds driving `{}` are reversed (write `[hi:lo]`, \
                             most significant bit first)",
                            lhs.base.name
                        ),
                    )
                    .with_code("S0136"),
                ));
            }
            let rhs_expr = rw.expr(rhs)?;
            let span = rhs_expr.span;
            let entry = bit_drives.entry(lhs.base.name.clone()).or_default();
            // A compile-time-constant RHS (e.g. `i * 2` after a `foreach`
            // unroll substitutes `i` with a literal) adapts to the slice's
            // width the same way any other CtInt assignment does (the
            // checker's own `fit` check only verifies the VALUE fits,
            // never that the expression's own "natural" width already
            // equals `hi - lo + 1`) — pull each target bit straight from
            // the folded value (arithmetic shift, so a negative constant
            // sign-extends correctly into a wider slice) instead of
            // indexing into the raw expression, which may be narrower.
            if let Ok(v) = const_eval(&rhs_expr, consts) {
                for b in lo..=hi {
                    let bit = ((v >> (b - lo)) & 1) as u128;
                    entry.insert(
                        b as u32,
                        Expr {
                            kind: ExprKind::Int {
                                value: bit.into(),
                                raw: bit.to_string(),
                            },
                            span,
                        },
                    );
                }
            } else {
                for b in lo..=hi {
                    let rhs_bit = (b - lo) as u128;
                    let sel = Expr {
                        kind: ExprKind::Index {
                            base: Box::new(rhs_expr.clone()),
                            index: Box::new(Expr {
                                kind: ExprKind::Int {
                                    value: rhs_bit.into(),
                                    raw: rhs_bit.to_string(),
                                },
                                span,
                            }),
                        },
                        span,
                    };
                    entry.insert(b as u32, sel);
                }
            }
        }
    }
    Ok(())
}
