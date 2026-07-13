//! Pass 3 — name resolution and module-structure rules.
//!
//! For every module: build the module scope (reporting E0003 in-module
//! duplicates and E0301 reg-without-reset), then walk every expression,
//! lvalue, instantiation, and type so that **every name points at a real
//! declaration** — signals, params, consts, enums/variants, modules,
//! instance ports. Test blocks only get their header checked (module +
//! params); body checking lands with the simulator (Phase 1.5).
//!
//! Width/driver/exhaustiveness rules are NOT here — they live in the
//! later passes (widths/drivers/clocks). This pass DOES own the
//! structure rules that need only names: reg-requires-reset (E0301) and
//! instantiation completeness (E0302 — every input connected exactly
//! once; clock/reset connect implicitly by name).

use std::collections::HashMap;

use crate::ast::{
    BundleDecl, Conn, Dir, EnumDecl, Expr, ExprKind, FnParam, FnStmt, ForEachSource, FuncDecl,
    Inst, LValue, Module, ModuleItem, NamedArg, Pattern, SeqStmt, TestDecl, TopItem, Type,
};

use super::Checker;
use super::consteval::{self, Env};

/// What a name in module scope is bound to. Carries the node where it
/// helps produce a better error (enums, instances). Shared with the
/// width pass (`widths.rs`), which reuses the scopes this pass builds.
#[derive(Clone, Copy)]
pub(super) enum Bind<'a> {
    In,
    Out,
    Wire,
    Reg,
    Mem,
    Clock,
    Reset,
    Param,
    Const,
    Enum(&'a EnumDecl),
    Inst(&'a Inst),
    #[expect(dead_code)]
    Bundle(&'a BundleDecl),
}

impl Bind<'_> {
    /// Human word for error messages ("`clk` is a clock — ...").
    pub(super) fn what(&self) -> &'static str {
        match self {
            Bind::In => "an input port",
            Bind::Out => "an output port",
            Bind::Wire => "a wire",
            Bind::Reg => "a reg",
            Bind::Mem => "a memory",
            Bind::Clock => "a clock",
            Bind::Reset => "a reset",
            Bind::Param => "a parameter",
            Bind::Const => "a constant",
            Bind::Enum(_) => "an enum",
            Bind::Inst(_) => "an instance",
            Bind::Bundle(_) => "a bundle",
        }
    }
}

/// One module's name table. Built here (pass 3), then stored on the
/// `Checker` so the width pass (pass 4) resolves against the same table
/// instead of rebuilding it.
pub(super) struct Scope<'a> {
    pub(super) names: HashMap<String, Bind<'a>>,
}

impl<'a> Checker<'a> {
    pub(super) fn resolve_names(&mut self) {
        let files = self.files;
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                match item {
                    TopItem::Module(m) => self.check_module(file, m),
                    TopItem::Test(t) => self.check_test(file, t),
                    TopItem::Const(_) => {} // earlier passes
                    TopItem::Bundle(b) => {
                        for field in &b.fields {
                            self.validate_bundle_field_type(file, &field.ty, field.span);
                        }
                    }
                    TopItem::Enum(e) => {
                        let env = self.file_consts[file].clone();
                        let sc = Scope {
                            names: HashMap::new(),
                        };
                        for v in &e.variants {
                            for field in &v.fields {
                                self.ty(file, &sc, &env, &field.ty);
                            }
                        }
                    }
                    TopItem::Error(_) => {} // parse-recovery placeholder
                    TopItem::Func(f) => self.check_func_names(file, f),
                }
            }
        }
    }

    fn check_module(&mut self, file: usize, m: &'a Module) {
        let mut sc = Scope {
            names: HashMap::new(),
        };
        for p in &m.params {
            self.declare(file, &mut sc, &p.name, Bind::Param);
        }
        // Build env BEFORE collect_decls so ConstIf conditions can be evaluated
        // during declaration scanning (spec D-CONSTIF-4: losing branch is fully discarded).
        let mut env = self.file_consts[file].clone();
        for item in &m.items {
            if let ModuleItem::Const(c) = item {
                match consteval::eval(&c.value, &env) {
                    Ok(v) => {
                        env.insert(c.name.name.clone(), v);
                    }
                    Err(d) => self.diags.push(d.with_file(file)),
                }
            }
        }

        self.collect_decls(file, &mut sc, &env, &m.items);

        // E0301 — registers load their reset value on reset, so a module
        // with regs and no `reset` line has unreachable initialization.
        let has_reg = sc.names.values().any(|b| matches!(b, Bind::Reg));
        let has_reset = sc.names.values().any(|b| matches!(b, Bind::Reset));
        if has_reg && !has_reset {
            self.err(
                file,
                m.name.span,
                "E0301",
                format!("module `{}` has registers but no `reset`", m.name.name),
                "every reg declares a reset value, and that value is loaded when \
                 reset is asserted — add a `reset rst` line (spec/02 section 1.2)",
            );
        }

        self.walk_items(file, &sc, &mut env, &m.items);

        // Hand the scope to the width and driver passes, keyed by (file,
        // name) — same-named modules from different files are legal (spec/02
        // section 1.5b), so no "canonical owner" disambiguation is needed:
        // each module gets its own scope under its own file.
        self.scopes
            .insert((file, m.name.name.clone()), std::rc::Rc::new(sc));
    }

    /// Declarations, recursively through `repeat` and `const if` bodies (declaration
    /// order in a module is free; `repeat` instantiates arrays but the
    /// names are declared once; `const if` losing branch is fully discarded).
    fn collect_decls(
        &mut self,
        file: usize,
        sc: &mut Scope<'a>,
        env: &Env,
        items: &'a [ModuleItem],
    ) {
        for item in items {
            match item {
                ModuleItem::Port { dir, name, .. } => {
                    let bind = if *dir == Dir::In { Bind::In } else { Bind::Out };
                    self.declare(file, sc, name, bind);
                }
                ModuleItem::Clock(n) => self.declare(file, sc, n, Bind::Clock),
                ModuleItem::Reset { name: n, .. } => self.declare(file, sc, n, Bind::Reset),
                ModuleItem::Wire { name, .. } => self.declare(file, sc, name, Bind::Wire),
                ModuleItem::Reg { name, .. } => self.declare(file, sc, name, Bind::Reg),
                ModuleItem::Mem { name, .. } => self.declare(file, sc, name, Bind::Mem),
                ModuleItem::Const(c) => self.declare(file, sc, &c.name, Bind::Const),
                ModuleItem::Enum(e) => self.declare(file, sc, &e.name, Bind::Enum(e)),
                ModuleItem::Inst(i) => self.declare(file, sc, &i.name, Bind::Inst(i)),
                ModuleItem::Repeat(r) => self.collect_decls(file, sc, env, &r.items),
                // Same "declared once, raw body" treatment as `Repeat` above —
                // this recurses into the RAW (unlowered) `fe.items`, not the
                // lowered `Repeat`, mirroring `Repeat`'s own comment: whatever
                // this foreach body's items directly declare gets picked up
                // once, without per-iteration substitution (substitution only
                // matters to elaboration/width checks, not to name collection).
                ModuleItem::ForEach(fe) => self.collect_decls(file, sc, env, &fe.items),
                ModuleItem::SyncLoop(sl) => {
                    // A sync loop namespaces 4 generated signals off its own
                    // name — declare them here so the existing E0003 check
                    // (in `declare`, below) catches a collision with a
                    // user-declared signal or another sync loop's generated
                    // names, same as any other declaration.
                    let mk = |suffix: &str| crate::ast::Ident {
                        name: format!("{}_{suffix}", sl.name.name),
                        span: sl.name.span,
                    };
                    self.declare(file, sc, &mk("start"), Bind::In);
                    self.declare(file, sc, &mk("done"), Bind::Out);
                    self.declare(file, sc, &mk("result"), Bind::Out);
                    self.declare(file, sc, &mk("running"), Bind::Out);
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, env).unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.collect_decls(file, sc, env, branch);
                }
                ModuleItem::On(_) | ModuleItem::Drive { .. } | ModuleItem::Error(_) => {}
                ModuleItem::BundleDestructure { .. } => {} // checker stub (T5)
            }
        }
    }

    fn declare(
        &mut self,
        file: usize,
        sc: &mut Scope<'a>,
        name: &crate::ast::Ident,
        bind: Bind<'a>,
    ) {
        if let Some(prev) = sc.names.get(&name.name) {
            let what = prev.what();
            self.err(
                file,
                name.span,
                "E0003",
                format!("`{}` is declared twice in this module", name.name),
                format!(
                    "there is already {what} named `{}` — pick a different name",
                    name.name
                ),
            );
        } else {
            sc.names.insert(name.name.clone(), bind);
        }
    }

    // `items` deliberately has NO `'a` bound (unlike `collect_decls`'s
    // `items: &'a [ModuleItem]`) — this fn only ever reads through `items`
    // for the duration of this call (via `ty`/`expr`/`lvalue`/`check_inst`,
    // none of which stash anything long-lived either); the only thing that
    // genuinely needs `'a` is `Scope<'a>` itself (`Bind::Inst`/`Bind::Enum`),
    // built once by `collect_decls` over the REAL AST and reused by later
    // passes. That independence is what lets a `ForEach` arm recurse here
    // with a freshly lowered, locally-owned `Vec<ModuleItem>` (see that arm
    // below) without needing to leak it to manufacture a fake `'a`.
    fn walk_items(&mut self, file: usize, sc: &Scope<'a>, env: &mut Env, items: &[ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Port { ty, name, .. } => {
                    self.ty(file, sc, env, ty);
                    self.reject_array_signal_type(file, ty, name.span, "a port");
                }
                ModuleItem::Wire { ty, init, name, .. } => {
                    self.ty(file, sc, env, ty);
                    self.reject_array_signal_type(file, ty, name.span, "a wire");
                    self.expr(file, sc, env, init);
                }
                ModuleItem::Reg { ty, reset, name, .. } => {
                    self.ty(file, sc, env, ty);
                    self.reject_array_signal_type(file, ty, name.span, "a register");
                    self.expr(file, sc, env, reset);
                }
                ModuleItem::Mem {
                    ty, depth, init, ..
                } => {
                    self.ty(file, sc, env, ty);
                    self.expr(file, sc, env, depth);
                    self.expr(file, sc, env, init);
                }
                ModuleItem::Inst(i) => self.check_inst(file, sc, env, i),
                ModuleItem::On(on) => {
                    match sc.names.get(&on.clock.name) {
                        Some(Bind::Clock) => {}
                        Some(b) => {
                            let what = b.what();
                            self.err(
                                file,
                                on.clock.span,
                                "E0109",
                                format!("`{}` is {what}, not a clock", on.clock.name),
                                "`on rise(...)` takes a clock — declare one with \
                                 `clock clk` (spec/02 section 1.2)",
                            );
                        }
                        None => self.unknown(file, &on.clock.name, on.clock.span),
                    }
                    // E0810: each reg may have at most one `default` per `on` block
                    let mut seen_defaults: std::collections::HashSet<&str> = Default::default();
                    for stmt in &on.body {
                        if let SeqStmt::Default { name, span, .. } = stmt
                            && !seen_defaults.insert(name.name.as_str()) {
                                self.err(
                                    file,
                                    *span,
                                    "E0810",
                                    format!(
                                        "duplicate `default` for `{}` in this `on` block",
                                        name.name
                                    ),
                                    "each reg may have at most one `default` per `on` block",
                                );
                            }
                    }
                    self.seq_stmts(file, sc, env, items, &on.body);
                }
                ModuleItem::Drive { lhs, rhs } => {
                    self.lvalue(file, sc, env, lhs);
                    self.expr(file, sc, env, rhs);
                }
                ModuleItem::Repeat(r) => {
                    self.no_decls_in_repeat(file, &r.items);
                    let lo = self.const_pos(file, env, &r.lo);
                    self.const_pos(file, env, &r.hi);
                    // The loop variable is a compile-time int inside the
                    // body. Its per-iteration values matter only to
                    // elaboration (later slice) — names resolve the same
                    // for every iteration, so one walk with `lo` suffices.
                    let shadowed = env.insert(r.var.name.clone(), lo.unwrap_or(0));
                    self.walk_items(file, sc, env, &r.items);
                    match shadowed {
                        Some(v) => env.insert(r.var.name.clone(), v),
                        None => env.remove(&r.var.name),
                    };
                }
                // `foreach` is pure sugar over `repeat`/`loop` — the ONLY
                // checker logic it genuinely owns is validating an
                // Elements-form source resolves to an array/mem type
                // (E0417); everything else (bound const-ness, name
                // resolution, `no_decls_in_repeat`, ...) is inherited for
                // free by lowering to `Repeat` and recursing into the SAME
                // `walk_items` this arm itself lives in — hitting the
                // `ModuleItem::Repeat` arm above on the next pass.
                ModuleItem::ForEach(fe) => {
                    if let ForEachSource::Elements(arr) = &fe.source
                        && crate::ast::array_like_len(&arr.name, items).is_none()
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
                                fe.var.name, arr.name, arr.name, fe.var.name
                            ),
                        );
                        continue;
                    }
                    let Some(lowered) = crate::ast::lower_foreach_item(fe, items) else {
                        continue; // E0417 already pushed above
                    };
                    // `lowered` is a fresh owned `Vec` (a clone of `fe.items`
                    // with `fe.var` substituted), not part of the `'a` AST
                    // arena — but `walk_items` doesn't need `'a` for `items`
                    // (see the fn's own doc comment), so this borrows fine
                    // for just the duration of this recursive call.
                    self.walk_items(file, sc, env, &lowered);
                }
                ModuleItem::SyncLoop(sl) => {
                    match sc.names.get(&sl.clock.name) {
                        Some(Bind::Clock) => {}
                        Some(b) => {
                            let what = b.what();
                            self.err(
                                file,
                                sl.clock.span,
                                "E0109",
                                format!("`{}` is {what}, not a clock", sl.clock.name),
                                "a sync loop's `on rise(...)`/`on fall(...)` clause takes a \
                                 clock — declare one with `clock clk` (spec/02 section 1.2)",
                            );
                        }
                        None => self.unknown(file, &sl.clock.name, sl.clock.span),
                    }
                    self.ty(file, sc, env, &sl.result_ty);
                    self.expr(file, sc, env, &sl.result_init);
                    let lo_val = self.const_pos(file, env, &sl.lo);
                    self.const_pos(file, env, &sl.hi);
                    // `var` is a runtime counter, read-only inside the body —
                    // the generated FSM owns incrementing it (see
                    // `ast::sync_loop_lower`) — so, same as `Repeat`'s
                    // compile-time loop var above, one representative `env`
                    // entry is enough for `expr()`'s name lookup to resolve
                    // it; per-iteration values don't matter to name
                    // resolution.
                    //
                    // `result_name` differs: the body legitimately assigns to
                    // it (`result <- ...` accumulates every cycle — it lowers
                    // to a real reg, `<name>_acc`). `lvalue()` only allows
                    // Out/Wire/Reg targets found in `sc.names`, so an
                    // `env`-only entry would make every real sync-loop body
                    // fail with a spurious "cannot assign to constant"
                    // (E0108). Give it a real (body-local) `Bind::Reg` entry
                    // instead, via the same clone-and-extend scope idiom
                    // `ExprKind::Match`'s per-arm bindings already use above.
                    let shadowed_var = env.insert(sl.var.name.clone(), lo_val.unwrap_or(0));
                    let mut body_sc = Scope {
                        names: sc.names.clone(),
                    };
                    body_sc.names.insert(sl.result_name.name.clone(), Bind::Reg);
                    self.seq_stmts(file, &body_sc, env, items, &sl.body);
                    match shadowed_var {
                        Some(v) => env.insert(sl.var.name.clone(), v),
                        None => env.remove(&sl.var.name),
                    };
                }
                ModuleItem::Enum(e) => {
                    for v in &e.variants {
                        for field in &v.fields {
                            self.ty(file, sc, env, &field.ty);
                        }
                    }
                }
                ModuleItem::ConstIf { cond, then, els, span } => {
                    match consteval::eval(cond, env) {
                        Ok(val) => {
                            let branch = if val != 0 {
                                then.as_slice()
                            } else {
                                els.as_deref().unwrap_or(&[])
                            };
                            self.walk_items(file, sc, env, branch);
                        }
                        Err(_) => {
                            self.err(
                                file,
                                *span,
                                "E0811",
                                "`const if` condition is not a compile-time constant",
                                "use only module parameters, consts, literals, and \
                                 arithmetic/comparison; runtime signals like ports are \
                                 not allowed",
                            );
                        }
                    }
                }
                ModuleItem::Clock(_)
                | ModuleItem::Reset { .. }
                | ModuleItem::Const(_) // evaluated in check_module
                | ModuleItem::Error(_) => {}
                ModuleItem::BundleDestructure { expr, .. } => {
                    self.expr(file, sc, env, expr);
                }
            }
        }
    }

    // Same "no `'a` needed" reasoning as `walk_items` — see its doc comment.
    fn seq_stmts(
        &mut self,
        file: usize,
        sc: &Scope<'a>,
        env: &mut Env,
        module_items: &[ModuleItem],
        stmts: &[SeqStmt],
    ) {
        for s in stmts {
            match s {
                SeqStmt::Assign { lhs, rhs } => {
                    self.lvalue(file, sc, env, lhs);
                    self.expr(file, sc, env, rhs);
                }
                SeqStmt::If { cond, then, els } => {
                    self.expr(file, sc, env, cond);
                    self.seq_stmts(file, sc, env, module_items, then);
                    if let Some(els) = els {
                        self.seq_stmts(file, sc, env, module_items, els);
                    }
                }
                SeqStmt::Default { name, val, span } => {
                    match sc.names.get(&name.name) {
                        Some(Bind::Reg) => {}
                        Some(_) => self.err(
                            file,
                            *span,
                            "E0809",
                            format!("`default` target `{}` is not a reg", name.name),
                            "only `reg` signals can have default assignments; \
                             wires are always driven combinationally",
                        ),
                        None => self.unknown(file, &name.name, name.span),
                    }
                    self.expr(file, sc, env, val);
                }
                SeqStmt::Loop {
                    var, lo, hi, body, ..
                } => {
                    // `loop` unrolls at compile time, same as `ModuleItem::Repeat`
                    // — its bounds must const-evaluate, so reuse `const_pos`
                    // (which reports E0201, `repeat`'s own bound-checking path)
                    // instead of silently defaulting a non-const bound to 0.
                    let lo_val = self.const_pos(file, env, lo);
                    self.const_pos(file, env, hi);
                    // The loop variable is a compile-time int inside the body,
                    // same one-representative-walk model as `ModuleItem::Repeat`
                    // (per-iteration values matter only to elaboration, not name
                    // resolution).
                    let shadowed = env.insert(var.name.clone(), lo_val.unwrap_or(0));
                    self.seq_stmts(file, sc, env, module_items, body);
                    match shadowed {
                        Some(v) => env.insert(var.name.clone(), v),
                        None => env.remove(&var.name),
                    };
                }
                // Same "lower to `Loop`, recurse into the same fn" delegation
                // as `ModuleItem::ForEach` above — see that arm's comment.
                // `SeqStmt` has no local-binding statement, so the Elements
                // form substitutes `var` throughout `body` instead of
                // introducing a new binding (see `lower_foreach_seq`'s doc
                // comment) — no synthesized declaration, so (unlike the
                // `ModuleItem` form) there's no `no_decls_in_repeat`-style
                // concern here.
                SeqStmt::ForEach {
                    var,
                    source,
                    body,
                    span,
                } => {
                    if let ForEachSource::Elements(arr) = source
                        && crate::ast::array_like_len(&arr.name, module_items).is_none()
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
                        crate::ast::lower_foreach_seq(var, source, body, *span, module_items)
                    else {
                        continue; // E0417 already pushed above
                    };
                    self.seq_stmts(file, sc, env, module_items, &lowered);
                }
                SeqStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// E0303 — a `repeat` body may only generate hardware (drives,
    /// instances, nested `repeat`s), never declare it. A declaration
    /// inside `repeat` would mean N copies of the same name — there is no
    /// such thing; declare the signal once outside and drive bit `i`
    /// inside. Reports each offending item at its own span (the immediate
    /// level only; nested `repeat`s are checked when the walk reaches
    /// them).
    fn no_decls_in_repeat(&mut self, file: usize, items: &[ModuleItem]) {
        for item in items {
            let (span, what) = match item {
                ModuleItem::Drive { .. }
                | ModuleItem::Inst(_)
                | ModuleItem::Repeat(_)
                | ModuleItem::ForEach(_)
                | ModuleItem::SyncLoop(_)
                | ModuleItem::ConstIf { .. }
                | ModuleItem::Error(_) => continue,
                ModuleItem::Port { name, .. } => (name.span, "an input/output port"),
                ModuleItem::Wire { name, .. } => (name.span, "a wire"),
                ModuleItem::Reg { name, .. } => (name.span, "a register"),
                ModuleItem::Mem { name, .. } => (name.span, "a memory"),
                ModuleItem::Clock(n) => (n.span, "a clock"),
                ModuleItem::Reset { name: n, .. } => (n.span, "a reset"),
                ModuleItem::Const(c) => (c.name.span, "a const"),
                ModuleItem::Enum(e) => (e.name.span, "an enum"),
                ModuleItem::On(on) => (on.span, "an `on` block"),
                ModuleItem::BundleDestructure { span, .. } => (*span, "a bundle destructure"),
            };
            self.err(
                file,
                span,
                "E0303",
                format!("`repeat` cannot contain {what}"),
                "`repeat` unrolls at compile time — it may only generate \
                 hardware (drives, instances, nested `repeat`s). Declare \
                 the signal once outside the loop and drive bit `i` inside \
                 (spec/02 section 1.6).",
            );
        }
    }

    /// A position that must const-evaluate today (`repeat` bounds).
    /// Returns the value if it did.
    fn const_pos(&mut self, file: usize, env: &Env, e: &Expr) -> Option<i128> {
        match consteval::eval(e, env) {
            Ok(v) => Some(v),
            Err(d) => {
                self.diags.push(d.with_file(file));
                None
            }
        }
    }

    fn ty(&mut self, file: usize, sc: &Scope<'a>, env: &Env, ty: &Type) {
        match ty {
            Type::Bit => {}
            Type::Bits(w) | Type::Signed(w) => self.expr(file, sc, env, w),
            Type::Bundle { name, .. } => {
                let candidates = self.bundles.get(&name.name.name).cloned();
                self.resolve(file, candidates, name, |ck| {
                    ck.err(
                        file,
                        name.span,
                        "E0906",
                        format!("unknown bundle type `{}`", name.name.name),
                        "declare the bundle at file level before using it as a type",
                    );
                });
            }
            Type::Named(n) => {
                // Module-scope enum shadows any project-wide import — unchanged
                // behavior. Only once that's ruled out do we resolve against
                // the project tables (enum first, then bundle — same
                // "enum OR bundle" disjunction as before, now going through
                // `resolve` so an ambiguous/qualified reference gets its own
                // E0110/E0111 instead of silently picking the first file).
                let sc_enum = matches!(sc.names.get(&n.name.name), Some(Bind::Enum(_)));
                if !sc_enum {
                    if self.enums.contains_key(&n.name.name) {
                        let candidates = self.enums.get(&n.name.name).cloned();
                        self.resolve(file, candidates, n, |_| {});
                    } else if self.bundles.contains_key(&n.name.name) {
                        let candidates = self.bundles.get(&n.name.name).cloned();
                        self.resolve(file, candidates, n, |_| {});
                    } else {
                        self.err(
                            file,
                            n.span,
                            "E0103",
                            format!("unknown type `{}`", n.name.name),
                            format!(
                                "named types are `enum` or `bundle` declarations — declare \
                                 `enum {} {{ ... }}` or `bundle {} {{ ... }}` at file level, \
                                 or import the file that does",
                                n.name.name, n.name.name
                            ),
                        );
                    }
                }
            }
            Type::Array { elem, len } => {
                self.ty(file, sc, env, elem);
                self.expr(file, sc, env, len);
            }
        }
    }

    /// Enum lookup: module scope first, then file-level enums project-wide.
    pub(super) fn lookup_enum(&self, sc: &Scope<'a>, name: &str) -> Option<&'a EnumDecl> {
        if let Some(Bind::Enum(e)) = sc.names.get(name).copied() {
            return Some(e);
        }
        self.enums
            .get(name)
            .and_then(|v| v.first())
            .map(|&(_, e)| e)
    }

    /// Resolve a possibly-namespaced reference against the caller's
    /// already-looked-up candidate bucket (`table.get(&q.name.name).cloned()`
    /// — cloning just the one bucket, not the whole project-wide multimap,
    /// sidesteps the borrow conflict between holding a `&self.modules`
    /// borrow and calling `self.err`/`unknown` below, which need `&mut
    /// self`). `unknown` is called (and its diagnostic emitted) when there
    /// are 0 candidates — same behavior/codes as before this feature.
    /// Returns `None` on 0, ambiguous-bare, or unmatched-qualifier; `Some` on
    /// exactly 1 candidate or a qualifier that matches exactly one.
    fn resolve<'b, T>(
        &mut self,
        file: usize,
        candidates: Option<Vec<(usize, &'b T)>>,
        q: &'b crate::ast::QualIdent,
        unknown: impl FnOnce(&mut Self),
    ) -> Option<&'b T> {
        let Some(candidates) = candidates else {
            unknown(self);
            return None;
        };
        if q.is_bare() {
            match candidates.as_slice() {
                [] => unreachable!(
                    "empty Vec is never inserted — symbols.rs always pushes at least one"
                ),
                [(f, only)] => {
                    q.resolved_file.set(Some(*f));
                    Some(*only)
                }
                _ => {
                    let files: Vec<String> = candidates
                        .iter()
                        .map(|&(f, _)| format!("file {f}"))
                        .collect();
                    self.err(
                        file,
                        q.span,
                        "E0110",
                        format!(
                            "`{}` is ambiguous — declared in {} different files",
                            q.name.name,
                            candidates.len()
                        ),
                        format!(
                            "qualify with the import path to pick one, e.g. `a.b.{}` \
                             (candidates: {})",
                            q.name.name,
                            files.join(", ")
                        ),
                    );
                    None
                }
            }
        } else {
            // The actual disambiguation mechanism (spec/02 section 1.5b,
            // design doc §4.4): match this reference's `.path` against THIS
            // file's own `import` statements, caching the target file onto
            // `q.resolved_file` (a `Cell`) so every later pass that reads
            // the same Cell — `drivers.rs`, `widths/*.rs`, and
            // `emit_verilog::Project` (which runs on these SAME `ast::File`/
            // `QualIdent` instances after the checker, per
            // `commands/compile.rs`) — gets the answer for free.
            q.resolve_via_imports(&self.files[file].imports);
            let Some(target_file) = q.resolved_file.get() else {
                self.err(
                    file,
                    q.span,
                    "E0111",
                    format!(
                        "the path in `{}` doesn't match any `import` in this file",
                        q.to_dotted()
                    ),
                    "check the import path segments, or drop the qualifier if \
                     the bare name is unambiguous",
                );
                return None;
            };
            match candidates.iter().find(|&&(f, _)| f == target_file) {
                Some(&(_, t)) => Some(t),
                None => {
                    self.err(
                        file,
                        q.span,
                        "E0111",
                        format!(
                            "the file imported as `{}` doesn't declare `{}`",
                            q.path
                                .iter()
                                .map(|s| s.name.as_str())
                                .collect::<Vec<_>>()
                                .join("."),
                            q.name.name
                        ),
                        "the import resolves to a real file, but that file has no \
                         declaration by this name — check the spelling, or declare \
                         it there",
                    );
                    None
                }
            }
        }
    }

    fn check_inst(&mut self, file: usize, sc: &Scope<'a>, env: &Env, inst: &Inst) {
        if let Some(idx) = &inst.index {
            self.expr(file, sc, env, idx);
        }
        let candidates = self.modules.get(&inst.module.name.name).cloned();
        let target = self.resolve(file, candidates, &inst.module, |ck| {
            ck.err(
                file,
                inst.module.span,
                "E0102",
                format!(
                    "no module named `{}` in this project",
                    inst.module.name.name
                ),
                "check the spelling, or add the `import` that brings it in \
                 (spec/02 section 1.5)",
            );
        });
        let Some(target) = target else {
            // Still resolve the argument/connection expressions.
            for NamedArg { value, .. } in &inst.args {
                self.expr(file, sc, env, value);
            }
            for Conn { signal, .. } in &inst.conns {
                self.expr(file, sc, env, signal);
            }
            return;
        };

        let params: Vec<&str> = target.params.iter().map(|p| p.name.name.as_str()).collect();
        for NamedArg { name, value } in &inst.args {
            if !params.contains(&name.name.as_str()) {
                let available = if params.is_empty() {
                    format!("`{}` takes no parameters", target.name.name)
                } else {
                    format!(
                        "`{}`'s parameters are: {}",
                        target.name.name,
                        params.join(", ")
                    )
                };
                self.err(
                    file,
                    name.span,
                    "E0106",
                    format!("`{}` has no parameter `{}`", target.name.name, name.name),
                    available,
                );
            }
            self.expr(file, sc, env, value);
        }

        // Data inputs must each be connected exactly once (E0302);
        // clock/reset ports may be omitted — they connect implicitly by
        // name (the emitter's rule, spec/02 section 1.5).
        let mut ins: Vec<&str> = Vec::new();
        let mut implicit: Vec<&str> = Vec::new();
        let mut outputs: Vec<&str> = Vec::new();
        for item in &target.items {
            match item {
                ModuleItem::Port {
                    dir: Dir::In, name, ..
                } => ins.push(&name.name),
                ModuleItem::Port {
                    dir: Dir::Out,
                    name,
                    ..
                } => outputs.push(&name.name),
                ModuleItem::Clock(n) | ModuleItem::Reset { name: n, .. } => implicit.push(&n.name),
                _ => {}
            }
        }
        let mut connected: Vec<&str> = Vec::new();
        for Conn { port, signal } in &inst.conns {
            if outputs.contains(&port.name.as_str()) {
                self.err(
                    file,
                    port.span,
                    "E0107",
                    format!("`{}` is an output of `{}`", port.name, target.name.name),
                    format!(
                        "outputs are not connected here — read them with \
                         `{}.{}` (spec/02 section 1.5)",
                        inst.name.name, port.name
                    ),
                );
            } else if !ins.contains(&port.name.as_str()) && !implicit.contains(&port.name.as_str())
            {
                let mut all = ins.clone();
                all.extend(&implicit);
                self.err(
                    file,
                    port.span,
                    "E0107",
                    format!("`{}` has no input named `{}`", target.name.name, port.name),
                    format!("`{}`'s inputs are: {}", target.name.name, all.join(", ")),
                );
            } else if connected.contains(&port.name.as_str()) {
                self.err(
                    file,
                    port.span,
                    "E0302",
                    format!("input `{}` is connected twice", port.name),
                    "every input is connected exactly once — delete the \
                     duplicate connection",
                );
            } else {
                connected.push(&port.name);
            }
            self.expr(file, sc, env, signal);
        }
        let missing: Vec<&str> = ins
            .iter()
            .copied()
            .filter(|i| !connected.contains(i))
            .collect();
        if !missing.is_empty() {
            self.err(
                file,
                inst.name.span,
                "E0302",
                format!(
                    "`{}` leaves input{} `{}` unconnected",
                    target.name.name,
                    if missing.len() == 1 { "" } else { "s" },
                    missing.join("`, `")
                ),
                "every input of an instance must be connected — hardware \
                 has no default arguments (clock/reset connect implicitly \
                 by name and may be omitted)",
            );
        }
    }

    fn check_test(&mut self, file: usize, t: &'a TestDecl) {
        let candidates = self.modules.get(&t.module.name.name).cloned();
        let target = self.resolve(file, candidates, &t.module, |ck| {
            ck.err(
                file,
                t.module.span,
                "E0102",
                format!("no module named `{}` in this project", t.module.name.name),
                "check the spelling, or add the `import` that brings it in \
                 (spec/02 section 1.5)",
            );
        });
        let Some(target) = target else {
            return;
        };
        let params: Vec<&str> = target.params.iter().map(|p| p.name.name.as_str()).collect();
        for NamedArg { name, .. } in &t.args {
            if !params.contains(&name.name.as_str()) {
                self.err(
                    file,
                    name.span,
                    "E0106",
                    format!("`{}` has no parameter `{}`", target.name.name, name.name),
                    "test headers set the module's compile-time parameters only",
                );
            }
        }
        // Test BODIES are checked when the simulator lands (Phase 1.5) —
        // they reference the module's ports, which needs port typing.
    }

    fn lvalue(&mut self, file: usize, sc: &Scope<'a>, env: &Env, lv: &LValue) {
        match sc.names.get(&lv.base.name) {
            Some(Bind::Out | Bind::Wire | Bind::Reg) => {}
            Some(Bind::Mem) => {
                // A memory is addressed one cell at a time — a whole-memory
                // assignment is meaningless.
                if lv.index.is_none() {
                    self.err(
                        file,
                        lv.base.span,
                        "E0108",
                        format!("cannot assign to memory `{}` as a whole", lv.base.name),
                        "address one cell — `m[addr] <- value`",
                    );
                }
            }
            Some(Bind::In) => self.err(
                file,
                lv.base.span,
                "E0108",
                format!("cannot assign to input port `{}`", lv.base.name),
                "inputs are driven by the parent module — to compute a local \
                 value, declare a `wire`",
            ),
            Some(b) => {
                let what = b.what();
                self.err(
                    file,
                    lv.base.span,
                    "E0108",
                    format!("cannot assign to `{}` — it is {what}", lv.base.name),
                    "only output ports, wires (at their declaration), and regs \
                     (inside `on`) can be assigned",
                );
            }
            None if env.contains_key(&lv.base.name) => self.err(
                file,
                lv.base.span,
                "E0108",
                format!("cannot assign to `{}` — it is a constant", lv.base.name),
                "consts are compile-time values; use a `wire` or `reg` for \
                 something that varies",
            ),
            None => self.unknown(file, &lv.base.name, lv.base.span),
        }
        if let Some((i, hi)) = &lv.index {
            self.expr(file, sc, env, i);
            if let Some(hi) = hi {
                self.expr(file, sc, env, hi);
            }
        }
    }

    fn expr(&mut self, file: usize, sc: &Scope<'a>, env: &Env, e: &Expr) {
        match &e.kind {
            ExprKind::Int { .. } | ExprKind::Bool(_) => {}
            ExprKind::Ident(name) => {
                if !sc.names.contains_key(name) && !env.contains_key(name) {
                    self.unknown(file, name, e.span);
                }
            }
            ExprKind::Field { base, field } => {
                // `blink[i].out` — unwrap the instance-array index.
                let core = match &base.kind {
                    ExprKind::Index { base: b, index } if matches!(b.kind, ExprKind::Ident(_)) => {
                        self.expr(file, sc, env, index);
                        b
                    }
                    _ => base,
                };
                let ExprKind::Ident(name) = &core.kind else {
                    self.err(
                        file,
                        field.span,
                        "E0105",
                        "only enums and instances have fields",
                        "`.` reads an enum variant (`State.Red`) or an instance \
                         output (`add.sum`)",
                    );
                    self.expr(file, sc, env, base);
                    return;
                };
                match sc.names.get(name).copied() {
                    Some(Bind::Inst(inst)) => self.inst_output(file, inst, field),
                    Some(Bind::In) | Some(Bind::Out) | Some(Bind::Wire) | Some(Bind::Reg)
                    | Some(Bind::Param) => {
                        // Possible bundle field access (e.g., req.valid where req is a bundle
                        // port/signal/param). Validation deferred to the width pass where
                        // bundle types are fully resolved. `Param` included alongside the
                        // signal kinds so a bundle-typed `fn` parameter's field access
                        // (`h.valid`) isn't rejected here before the width pass — which
                        // resolves `cx.sigs` from the real bundle type — gets a chance to
                        // check it (see checker::widths::Ty::Bundle).
                    }
                    Some(b)
                        if self.lookup_enum(sc, name).is_none() && !matches!(b, Bind::Enum(_)) =>
                    {
                        let what = b.what();
                        self.err(
                            file,
                            field.span,
                            "E0105",
                            format!("`{name}` is {what} — it has no fields"),
                            "`.` reads an enum variant (`State.Red`), an \
                             instance output (`add.sum`), or a bundle field (`bus.valid`)",
                        );
                    }
                    _ => match self.lookup_enum(sc, name) {
                        Some(en) => {
                            if !en.variants.iter().any(|v| v.name.name == field.name) {
                                let list: Vec<&str> =
                                    en.variants.iter().map(|v| v.name.name.as_str()).collect();
                                self.err(
                                    file,
                                    field.span,
                                    "E0103",
                                    format!("enum `{name}` has no variant `{}`", field.name),
                                    format!("`{name}`'s variants are: {}", list.join(", ")),
                                );
                            }
                        }
                        None => self.unknown(file, name, core.span),
                    },
                }
            }
            ExprKind::Unary { expr, .. } => self.expr(file, sc, env, expr),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(file, sc, env, lhs);
                self.expr(file, sc, env, rhs);
            }
            ExprKind::IfExpr { cond, then, els } => {
                self.expr(file, sc, env, cond);
                self.expr(file, sc, env, then);
                self.expr(file, sc, env, els);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(file, sc, env, scrutinee);
                for arm in arms {
                    let mut arm_sc = Scope {
                        names: sc.names.clone(),
                    };

                    // Phases 1+2: validate each sub-pattern; collect binding info.
                    // Each entry: (reporting_span, [(binding_name, type_str)]).
                    // `skip` = true on any E0806 / E0103 / unknown enum — Phases 3–5 are
                    // skipped for this arm to avoid cascading errors on bad inputs.
                    let mut skip = false;
                    let mut alt_bindings = Vec::new();

                    for p in &arm.patterns {
                        match p {
                            Pattern::Variant {
                                enum_name,
                                variant,
                                bindings,
                            } => match self.lookup_enum(sc, &enum_name.name) {
                                Some(en) => {
                                    if let Some(decl_v) =
                                        en.variants.iter().find(|v| v.name.name == variant.name)
                                    {
                                        let expected = decl_v.fields.len();
                                        let got = bindings.len();
                                        if expected != got {
                                            let help = if expected == 0 {
                                                format!(
                                                    "`{}.{}` is tag-only — remove the `(...)` binding list",
                                                    enum_name.name, variant.name
                                                )
                                            } else {
                                                format!(
                                                    "provide exactly {} binding(s) — one per payload field",
                                                    expected
                                                )
                                            };
                                            self.err(
                                                file,
                                                variant.span,
                                                "E0806",
                                                format!(
                                                    "pattern for `{}.{}` binds {} name(s) but the variant has {} field(s)",
                                                    enum_name.name, variant.name, got, expected
                                                ),
                                                help,
                                            );
                                            skip = true;
                                        } else {
                                            let pairs: Vec<(String, String)> = bindings
                                                .iter()
                                                .zip(decl_v.fields.iter())
                                                .map(|(b, f)| {
                                                    (b.name.clone(), crate::pretty::type_str(&f.ty))
                                                })
                                                .collect();
                                            alt_bindings.push((variant.span, pairs));
                                        }
                                    } else {
                                        let list: Vec<&str> = en
                                            .variants
                                            .iter()
                                            .map(|v| v.name.name.as_str())
                                            .collect();
                                        self.err(
                                            file,
                                            variant.span,
                                            "E0103",
                                            format!(
                                                "enum `{}` has no variant `{}`",
                                                enum_name.name, variant.name
                                            ),
                                            format!(
                                                "`{}`'s variants are: {}",
                                                enum_name.name,
                                                list.join(", ")
                                            ),
                                        );
                                        skip = true;
                                    }
                                }
                                None => {
                                    self.unknown(file, &enum_name.name, enum_name.span);
                                    skip = true;
                                }
                            },
                            _ => {
                                // Wildcard / Int / Bool: empty binding set.
                                // Span: arm body (no pattern span available for these variants).
                                alt_bindings.push((arm.value.span, vec![]));
                            }
                        }
                    }

                    // Error recovery or single-pattern arm: inject all valid bindings seen
                    // so far and skip intersection validation.
                    if skip || alt_bindings.len() <= 1 {
                        for (_, pairs) in &alt_bindings {
                            for (name, _) in pairs {
                                arm_sc.names.insert(name.clone(), Bind::Param);
                            }
                        }
                        self.expr(file, &arm_sc, env, &arm.value);
                        continue;
                    }

                    // Phase 3: every alternative must bind the same set of names (positionally).
                    let ref_pairs = &alt_bindings[0].1;
                    let ref_names: Vec<&str> = ref_pairs.iter().map(|(n, _)| n.as_str()).collect();
                    let mut ok = true;

                    for (curr_span, curr_pairs) in &alt_bindings[1..] {
                        let curr_names: Vec<&str> =
                            curr_pairs.iter().map(|(n, _)| n.as_str()).collect();
                        // note: positional comparison — binding order matches field declaration order; named binding syntax would need set-equality here
                        if ref_names != curr_names {
                            self.err(
                                file,
                                *curr_span,
                                "E0808",
                                "OR-pattern alternatives must bind the same variables".to_string(),
                                format!(
                                    "expected: {{{}}}; this alternative provides: {{{}}}",
                                    ref_names.join(", "),
                                    curr_names.join(", ")
                                ),
                            );
                            ok = false;
                            break;
                        }
                    }

                    if !ok {
                        self.expr(file, &arm_sc, env, &arm.value);
                        continue;
                    }

                    // Phase 4: each binding must have the same type in every alternative.
                    // Iterate names in sorted order (D5 — deterministic diagnostics).
                    let mut sorted_idx: Vec<usize> = (0..ref_pairs.len()).collect();
                    sorted_idx.sort_by_key(|&i| &ref_pairs[i].0);

                    'width: for (curr_span, curr_pairs) in &alt_bindings[1..] {
                        for &i in &sorted_idx {
                            let (ref_name, ref_ty) = &ref_pairs[i];
                            let (_, curr_ty) = &curr_pairs[i];
                            if ref_ty != curr_ty {
                                self.err(
                                    file,
                                    *curr_span,
                                    "E0808",
                                    format!(
                                        "binding `{ref_name}` has incompatible types across OR-pattern alternatives"
                                    ),
                                    format!(
                                        "first alternative: {ref_ty}; this alternative: {curr_ty}"
                                    ),
                                );
                                ok = false;
                                break 'width;
                            }
                        }
                    }

                    if !ok {
                        self.expr(file, &arm_sc, env, &arm.value);
                        continue;
                    }

                    // Phase 5: identical names and types verified — inject reference bindings.
                    for (name, _) in ref_pairs {
                        arm_sc.names.insert(name.clone(), Bind::Param);
                    }
                    self.expr(file, &arm_sc, env, &arm.value);
                }
            }
            ExprKind::Concat(parts) => {
                for p in parts {
                    self.expr(file, sc, env, p);
                }
            }
            ExprKind::Replicate { count, parts } => {
                self.expr(file, sc, env, count);
                for p in parts {
                    self.expr(file, sc, env, p);
                }
            }
            ExprKind::Index { base, index } => {
                self.expr(file, sc, env, base);
                self.expr(file, sc, env, index);
            }
            ExprKind::Slice { base, hi, lo } => {
                self.expr(file, sc, env, base);
                self.expr(file, sc, env, hi);
                self.expr(file, sc, env, lo);
            }
            ExprKind::Call { args, .. } => {
                for a in args {
                    self.expr(file, sc, env, a);
                }
            }
            ExprKind::FnCall { name, args } => {
                if let Some((_, decl)) = self.funcs.get(&name.name) {
                    let expected = decl.params.len();
                    let got = args.len();
                    if expected != got {
                        self.err(
                            file,
                            name.span,
                            "E0803",
                            format!(
                                "`{}` takes {} argument(s), got {}",
                                name.name, expected, got
                            ),
                            format!("pass exactly {} argument(s) to `{}`", expected, name.name),
                        );
                    }
                } else {
                    self.err(
                        file,
                        name.span,
                        "E1110",
                        format!("`{}` is not a function or builtin — only declared `fn`s and builtins are callable", name.name),
                        "declare a `fn` with this name, or use a builtin (`extend`, `trunc`, `signed`, `unsigned`)",
                    );
                }
                for a in args {
                    self.expr(file, sc, env, a);
                }
            }
            ExprKind::BundleLit(fields) => {
                for f in fields {
                    self.expr(file, sc, env, &f.value);
                }
            }
            ExprKind::ArrayLit(elems) => {
                for e in elems {
                    self.expr(file, sc, env, e);
                }
            }
        }
    }

    /// `inst.field` — the field must be an OUTPUT port of the target
    /// module (inputs are connected at instantiation, not read back).
    fn inst_output(&mut self, file: usize, inst: &'a Inst, field: &crate::ast::Ident) {
        // Item order within a module is free (`collect_decls`), so an
        // earlier item may reference this instance's output before
        // `check_inst` has run for it — `resolved_file` may still be
        // unset. Re-resolve independently rather than depending on that
        // ordering, but discard any diagnostics the probe pushes: if
        // `check_inst` already ran (and already reported E0102/E0110/E0111
        // for this same `inst.module`), we'd otherwise double-report.
        let before = self.diags.len();
        let candidates = self.modules.get(&inst.module.name.name).cloned();
        let target = self.resolve(file, candidates, &inst.module, |_| {});
        self.diags.truncate(before);
        let Some(target) = target else {
            return; // unknown/ambiguous/unmatched module already reported at the `let`
        };
        let mut outputs: Vec<&str> = Vec::new();
        let mut is_input = false;
        for item in &target.items {
            match item {
                ModuleItem::Port {
                    dir: Dir::Out,
                    name,
                    ..
                } => outputs.push(&name.name),
                ModuleItem::Port {
                    dir: Dir::In, name, ..
                } => {
                    is_input |= name.name == field.name;
                }
                _ => {}
            }
        }
        if outputs.contains(&field.name.as_str()) {
            return;
        }
        let help = if is_input {
            format!(
                "`{}` is an input of `{}` — inputs are connected at the `let`, \
                 only outputs are read with `.`",
                field.name, target.name.name
            )
        } else if outputs.is_empty() {
            format!("`{}` has no outputs", target.name.name)
        } else {
            format!(
                "`{}`'s outputs are: {}",
                target.name.name,
                outputs.join(", ")
            )
        };
        self.err(
            file,
            field.span,
            "E0104",
            format!(
                "`{}` has no output named `{}`",
                target.name.name, field.name
            ),
            help,
        );
    }

    /// Name-check a function declaration: validates param/return types (E0103)
    /// and checks all statements + the tail for unbound names (E0101).
    fn check_func_names(&mut self, file: usize, func: &'a FuncDecl) {
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
                    let shadowed = env.insert(var.name.clone(), lo_val.unwrap_or(0));
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
    fn reject_array_signal_type(
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
    fn validate_bundle_field_type(&mut self, file: usize, ty: &Type, span: crate::span::Span) {
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

    fn unknown(&mut self, file: usize, name: &str, span: crate::span::Span) {
        self.err(
            file,
            span,
            "E0101",
            format!("unknown name `{name}`"),
            "nothing with this name is declared in this module — check the \
             spelling, or declare it as a port, wire, reg, or const",
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::{checker::check, diag::Diag, lexer, parser};

    /// Parse + run the full checker; panics if it doesn't parse (this file's
    /// other checker tests live in `checker::tests`, which does the same
    /// via its own private `parse`/`errs` helpers — this test lives here
    /// instead, self-contained, so this commit touches only `names.rs`).
    fn diags_for(src: &str) -> Vec<Diag> {
        let toks = lexer::lex(src).expect("lexes");
        let file = parser::parse(toks).expect("parses");
        check(&[file]).expect_err("expected checker errors")
    }

    #[test]
    fn sync_loop_generated_name_collision_is_e0003() {
        let src = "module M {\n  clock clk\n  in find_first_start: bit\n  sync loop find_first on rise(clk) (i: 0..4) -> result: bit = 0 {\n    result <- 1\n  }\n}\n";
        let diags = diags_for(src);
        assert!(diags.iter().any(|d| d.code == Some("E0003")));
    }
}
