//! Module-level emission: shells with parameters and ports, enum
//! localparams, declarations, instances (auto-wired outputs, implicit
//! clk/rst), combinational assigns, and always-blocks with generated reset.

use super::expr::ArrayScope;
use super::*;

impl Emitter<'_> {
    /// Emit one complete Verilog module. Source order inside the module
    /// body is free; output is regrouped into the conventional Verilog
    /// order: header/params/ports → enum localparams → wire/reg
    /// declarations → instances → assigns → always-blocks.
    pub(super) fn module(&mut self, m: &Module) {
        self.check_ascii(&m.name);
        self.clog2_fn_used = false;
        self.funcs_used.clear();
        self.hoist_counter = 0;
        self.hoisted_decls.clear();

        // Module-level consts layer onto the file consts for the duration
        // of this module; they fold to literals wherever used (widths,
        // `repeat` bounds, indices) and emit no hardware of their own.
        let file_env = self.env.clone();
        self.env = self.eval_consts_items(&m.items, file_env.clone());
        let flat: Vec<ModuleItem> = self.flatten_items(&m.items);
        // Task 6's "flat_items_in_scope": every hoist call site in
        // `expr.rs` needs this module's own Port/Wire/Reg `Kind`s to
        // compare against Verilog's self-determined rule. Built once
        // here (not per-expression) and read via `self.cur_decls` —
        // mirrors `bundle_sigs`' own "populate from flat items once,
        // reset per module" convention a few lines below.
        self.cur_decls = self.build_decls(&flat);

        // Parameters. The Verilog identifier is the bare name, UNLESS
        // another file also declares a module of this name — the
        // packages/namespacing same-name-across-files feature (spec/02
        // section 1.5b) — in which case it is disambiguated by declaring
        // file so two same-named modules never both emit as `module Fifo`
        // (a real Verilog toolchain rejects that outright).
        let mod_name = self.project.verilog_module_name(self.cur_file, m);
        let mut header = format!("module {}", mod_name);
        if !m.params.is_empty() {
            let ps: Vec<String> = m
                .params
                .iter()
                .map(|p| match &p.default {
                    Some(d) => format!("parameter {} = {}", p.name.name, self.expr(d)),
                    None => format!("parameter {}", p.name.name),
                })
                .collect();
            header.push_str(&format!(" #(\n    {}\n)", ps.join(",\n    ")));
        }

        // Ports: clock/reset first, then declared order. `emitting_port` makes a
        // `clog2(<param>)` port width an error — the V-2005 constant function is
        // in the body and can't reach the header port list.
        let mut ports: Vec<String> = Vec::new();
        self.emitting_port = true;
        for item in flat.iter() {
            match item {
                ModuleItem::Clock(c) => ports.push(format!("input wire {}", c.name)),
                ModuleItem::Reset { name: r, .. } => ports.push(format!("input wire {}", r.name)),
                ModuleItem::Port { dir, name, ty } => {
                    self.check_ascii(name);
                    let d = match dir {
                        Dir::In => "input wire",
                        Dir::Out => "output wire",
                    };
                    // Bundle ports flatten to one port per field.
                    let bundle_fields = match ty {
                        Type::Bundle { name: bname, args } => {
                            Some(self.resolve_bundle_fields(bname, args))
                        }
                        Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                            Some(self.resolve_bundle_fields(id, &[]))
                        }
                        _ => None,
                    };
                    if let Some(fields) = bundle_fields {
                        for (fname, fty) in &fields {
                            let w = self.width_resolved(fty);
                            ports.push(format!("{d} {w}{}_{}", name.name, fname));
                        }
                    } else {
                        let w = self.width(ty);
                        ports.push(format!("{d} {w}{}", name.name));
                    }
                }
                _ => {}
            }
        }
        self.emitting_port = false;
        header.push_str(&format!(" (\n    {}\n);\n", ports.join(",\n    ")));
        self.out.push_str(&header);
        // Insertion point for the `clog2` constant function, if a body width
        // turns out to need it (filled in just before `endmodule`).
        let fn_pos = self.out.len();

        // Enum encodings as localparams.
        let enums: Vec<&EnumDecl> = flat
            .iter()
            .filter_map(|i| match i {
                ModuleItem::Enum(e) => Some(e),
                _ => None,
            })
            .collect();
        for e in &enums {
            let total_w = e
                .inferred_total_width
                .get()
                .expect("inferred_total_width not set — checker must run before emitter")
                as u128;
            let tag_w = clog2(e.variants.len()) as u128;
            let max_payload_w = total_w - tag_w;
            for (i, v) in e.variants.iter().enumerate() {
                let i = i as u128;
                let val_str = if max_payload_w == 0 {
                    // Tag-only: unchanged (plain decimal index, no width prefix).
                    format!("{i}")
                } else {
                    // Tagged: shift tag index into MSBs, payload bits are zero.
                    let val = i << max_payload_w;
                    format!("{total_w}'h{val:x}")
                };
                self.out.push_str(&format!(
                    "    localparam [{}:0] {} = {};\n",
                    total_w - 1,
                    enum_const(&e.name.name, &v.name.name),
                    val_str
                ));
            }
        }

        // Declarations.
        for item in flat.iter() {
            match item {
                ModuleItem::Wire { name, ty, .. } => {
                    self.check_ascii(name);
                    // Bundle wires flatten to one wire per field.
                    let bundle_fields = match ty {
                        Type::Bundle { name: bname, args } => {
                            Some(self.resolve_bundle_fields(bname, args))
                        }
                        Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                            Some(self.resolve_bundle_fields(id, &[]))
                        }
                        _ => None,
                    };
                    if let Some(fields) = bundle_fields {
                        for (fname, fty) in &fields {
                            let w = self.width_resolved(fty);
                            self.out
                                .push_str(&format!("    wire {w}{}_{};\n", name.name, fname));
                        }
                    } else {
                        let w = self.width(ty);
                        self.out.push_str(&format!("    wire {w}{};\n", name.name));
                    }
                }
                ModuleItem::Reg { name, ty, .. } => {
                    self.check_ascii(name);
                    let w = self.width(ty);
                    self.out.push_str(&format!("    reg {w}{};\n", name.name));
                }
                ModuleItem::Mem {
                    name, ty, depth, ..
                } => {
                    self.check_ascii(name);
                    let w = self.width(ty);
                    let d = self.expr(depth);
                    self.out
                        .push_str(&format!("    reg {w}{} [0:({d})-1];\n", name.name));
                }
                ModuleItem::BundleDestructure { span, .. } => {
                    self.err(
                        *span,
                        "bundle destructure in module body is not yet supported by the emitter",
                        "use wire declarations with dot-access instead: `wire f: bit = bus.field`",
                    );
                }
                _ => {}
            }
        }

        // Power-on init: seed every cell of each memory to its init value
        // (mirrors the simulator, which initializes all cells at construction).
        // Mandatory init value → no uninitialized state, without a per-cycle
        // reset (the `reset` line clears registers only).
        for item in flat.iter() {
            if let ModuleItem::Mem {
                name, depth, init, ..
            } = item
            {
                let d = self.expr(depth);
                let v = self.expr(init);
                let iv = format!("__mimz_{}_i", name.name);
                self.out.push_str(&format!("    integer {iv};\n"));
                self.out.push_str(&format!(
                    "    initial for ({iv} = 0; {iv} < ({d}); {iv} = {iv} + 1) {}[{iv}] = {v};\n",
                    name.name
                ));
            }
        }

        // Instances: auto-wire every child output as `{inst}_{port}`.
        // `repeat` bodies are unrolled per iteration (instances first, to
        // match Verilog's declare-before-use convention).
        self.repeat_budget = REPEAT_BUDGET;
        self.emit_instances(&m.items);

        // Combinational drives (unrolling `repeat` the same way).
        // Pre-populate bundle_sigs so emit_drives can flatten bundle assignments.
        // Repeat-body bundle wires aren't tracked in bundle_sigs — moot for
        // now since the checker blocks wire-in-repeat outright; revisit if
        // that restriction is ever lifted.
        self.bundle_sigs.clear();
        for item in flat.iter() {
            let (sig_name, bname, args) = match item {
                ModuleItem::Port {
                    name,
                    ty: Type::Bundle { name: bn, args },
                    ..
                } => (name.name.clone(), bn.clone(), args.clone()),
                ModuleItem::Port {
                    name,
                    ty: Type::Named(id),
                    ..
                } if self.project.resolve_bundle(id).is_some() => {
                    (name.name.clone(), id.clone(), vec![])
                }
                ModuleItem::Wire {
                    name,
                    ty: Type::Bundle { name: bn, args },
                    ..
                } => (name.name.clone(), bn.clone(), args.clone()),
                ModuleItem::Wire {
                    name,
                    ty: Type::Named(id),
                    ..
                } if self.project.resolve_bundle(id).is_some() => {
                    (name.name.clone(), id.clone(), vec![])
                }
                _ => continue,
            };
            self.bundle_sigs.insert(sig_name, (bname, args));
        }
        self.repeat_budget = REPEAT_BUDGET; // reset for emit_drives pass
        self.emit_drives(&m.items);
        self.bundle_sigs.clear();

        // Sequential blocks: one always per `on`, reset generated from
        // the reset values of the regs each block assigns.
        let reset_name = flat.iter().find_map(|i| match i {
            ModuleItem::Reset { name: r, .. } => Some(r.name.clone()),
            _ => None,
        });
        // An async reset is added to every always-block's sensitivity list
        // (`@(… or posedge rst)`); a sync reset only acts on the clock edge.
        let async_reset = flat
            .iter()
            .any(|i| matches!(i, ModuleItem::Reset { is_async: true, .. }));
        let regs: HashMap<&str, &Expr> = flat
            .iter()
            .filter_map(|i| match i {
                ModuleItem::Reg { name, reset, .. } => Some((name.name.as_str(), reset)),
                _ => None,
            })
            .collect();

        for item in flat.iter() {
            if let ModuleItem::On(on) = item {
                let mut assigned: Vec<String> = Vec::new();
                collect_assigned(&on.body, &mut assigned, &flat);

                let edge = if matches!(on.edge, crate::ast::Edge::Fall) {
                    "negedge"
                } else {
                    "posedge"
                };
                // Active-high reset → `posedge rst` in the sensitivity list.
                let sens = match (&reset_name, async_reset) {
                    (Some(rst), true) => format!("{edge} {} or posedge {rst}", on.clock.name),
                    _ => format!("{edge} {}", on.clock.name),
                };
                self.out.push_str(&format!("    always @({sens}) begin\n"));
                if let Some(rst) = &reset_name {
                    self.out.push_str(&format!("        if ({rst}) begin\n"));
                    for r in &assigned {
                        if let Some(reset_val) = regs.get(r.as_str()) {
                            let v = self.expr(reset_val);
                            self.out.push_str(&format!("            {r} <= {v};\n"));
                        }
                    }
                    self.out.push_str("        end else begin\n");
                    self.seq_stmts(&on.body, 2, &flat);
                    self.out.push_str("        end\n");
                } else {
                    self.seq_stmts(&on.body, 1, &flat);
                }
                self.out.push_str("    end\n");
            }
        }

        // Inject the clog2 helper (if any body width needed it) followed by
        // any user-defined functions used by this module (in topological order:
        // callees before callers, so each function is declared before use).
        // Both must live in the module body — clog2 first so user functions
        // that happen to use clog2() in a width find it already declared.
        let fns_to_inject = self.funcs_used.clone();
        let mut user_fn_inject = String::new();
        for name in &fns_to_inject {
            if let Some(decl) = self.project.funcs.get(name.as_str()).copied() {
                user_fn_inject.push_str(&self.render_fn_decl(decl, &file_env));
            }
        }
        let mut inject = String::new();
        if self.clog2_fn_used {
            inject.push_str(CLOG2_FN);
        }
        inject.push_str(&user_fn_inject);
        inject.push_str(&self.hoisted_decls);
        if !inject.is_empty() {
            self.out.insert_str(fn_pos, &inject);
        }

        self.out.push_str("endmodule\n");

        // Peel this module's consts back off; the next module in the file
        // sees only the file-level env.
        self.env = file_env;
    }

    /// Mark `name` (and all its transitive callees) as used by the current
    /// module. Post-order DFS — callees are added before the caller — so
    /// `funcs_used` ends up in topological order ready for injection.
    /// Recursion is banned (E0805), so no cycle risk.
    pub(super) fn mark_fn_used(&mut self, name: &str) {
        if self.funcs_used.iter().any(|n| n == name) {
            return; // already enqueued (or enqueuing via a sibling path)
        }
        // Recurse into callees first (post-order).
        if let Some(decl) = self.project.funcs.get(name).copied() {
            for callee in super::fn_direct_callees(decl) {
                self.mark_fn_used(&callee);
            }
        }
        // Add self after its dependencies.
        if !self.funcs_used.iter().any(|n| n == name) {
            self.funcs_used.push(name.to_string());
        }
    }

    /// Render one user-defined function as a Verilog-2005
    /// `function automatic` block. Local `let` bindings are declared as
    /// `reg [W-1:0]` using the width inferred by the checker's width pass
    /// and stored in [`LocalLet::inferred_width`]; emitting `integer` would
    /// silently widen narrow wrapping values (e.g. an 8-bit `*%` product
    /// stored in a 32-bit `integer` would not wrap at 8 bits).
    ///
    /// Renders under the FILE-LEVEL const env (`file_env`) so file consts
    /// used in the function body fold to literals (e.g. `a >> SCALE` where
    /// `SCALE` is a file const folds to `a >> 3`), while module consts —
    /// which are not visible inside a `function automatic` body — are
    /// excluded so they cannot accidentally shadow a function parameter.
    ///
    /// `return` lowers via continuation-passing (see [`Self::emit_fn_stmts`])
    /// rather than a flat `funcname = expr;` per statement: Verilog function
    /// bodies execute sequentially with no early exit, so a naive flat
    /// lowering would let the mandatory tail's assignment silently overwrite
    /// an earlier `return` fired inside an `if` branch.
    fn render_fn_decl(&mut self, decl: &FuncDecl, file_env: &Env) -> String {
        // Replace the module env with the file-level env: module consts are
        // out of scope inside a `function automatic`, but file consts must
        // fold so uses like `a >> SCALE` emit correct literals.
        let saved_env = std::mem::replace(&mut self.env, file_env.clone());

        // Array-typed names in scope for this body: each param or `let`-bound
        // array maps to `(element_width_string, length)` so a call argument
        // referring to it by name (or an array literal passed directly) can be
        // expanded to the `<name>_<i>` scalars the callee's array param
        // elaborated into (Task 7). Never mutated after construction — an
        // array is immutable once bound (matching the `fn` purity rule).
        let mut arrays: ArrayScope = HashMap::new();
        for param in &decl.params {
            if let Type::Array { elem, len } = &param.ty {
                let n = consteval::eval(len, &self.env)
                    .expect("checker already validated this array's length")
                    as u128;
                arrays.insert(param.name.name.clone(), (self.width(elem), n));
            }
        }
        for local in fn_all_locals(&decl.stmts, &decl.params, &self.env) {
            if let ExprKind::ArrayLit(elems) = &local.value.kind {
                // Element width comes from the checker's width pass, which sets
                // `inferred_width` to the array's ELEMENT width for an
                // array-typed `let` (widths/mod.rs `FnStmt::Let` arm). Mirror
                // `self.width`'s convention: a 1-bit element has no `[..]` range.
                let ew = match local.inferred_width.get() {
                    Some(1) => String::new(),
                    Some(w) => format!("[{}:0] ", w - 1),
                    None => unreachable!(
                        "array-typed let `{}` has no element width — checker must run first",
                        local.name.name
                    ),
                };
                arrays.insert(local.name.name.clone(), (ew, elems.len() as u128));
            }
        }

        let ret_w = self.width(&decl.ret);
        let mut s = format!("    function automatic {ret_w}{};\n", decl.name.name);
        for param in &decl.params {
            match &param.ty {
                Type::Array { elem, len } => {
                    // An array parameter is never a real Verilog array port —
                    // it elaborates to N independent scalar `input` ports,
                    // named `<param>_<index>`, exactly like `repeat` elaborates
                    // to N copies of hardware rather than a real loop.
                    let n = consteval::eval(len, &self.env).expect(
                        "checker already validated this array's length is a positive compile-time constant",
                    );
                    let ew = self.width(elem);
                    for i in 0..n {
                        s.push_str(&format!("        input {ew}{}_{i};\n", param.name.name));
                    }
                }
                // BUG-10 (docs/audit/bugs.md): a bundle-typed `fn` parameter
                // is never a single scalar `input` — it flattens to one
                // `input` per field, same convention module ports/wires use
                // (module.rs:60-78, 130-140). `expr.rs`'s `Field` arm
                // already renders `u.tx` as `u_tx` unconditionally, so the
                // body needs no change — only this declaration and the
                // call-site argument expansion below (`ExprKind::FnCall`
                // in expr.rs) were missing the flatten step.
                Type::Bundle { name: bname, args } => {
                    for (fname, fty) in self.resolve_bundle_fields(bname, args) {
                        let fw = self.width_resolved(&fty);
                        s.push_str(&format!("        input {fw}{}_{fname};\n", param.name.name));
                    }
                }
                Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                    for (fname, fty) in self.resolve_bundle_fields(id, &[]) {
                        let fw = self.width_resolved(&fty);
                        s.push_str(&format!("        input {fw}{}_{fname};\n", param.name.name));
                    }
                }
                other => {
                    let pw = self.width(other);
                    s.push_str(&format!("        input {pw}{};\n", param.name.name));
                }
            }
        }
        // Names already given a `reg`/`input` declaration this function —
        // seeded with the scalar params (an array param's `<name>_<i>`
        // scalars never collide with a plain `let`'s single name). BUG-9: a
        // `let` that shadows an earlier `let` or a param — the checker
        // (E0813) now guarantees any such shadow keeps the SAME width — so
        // it's the exact same Verilog identifier and needs declaring only
        // once; the emitter used to blindly emit one `reg` line per source
        // `let`, so a shadow re-declared the same name and real Verilog
        // rejected it.
        let mut declared: std::collections::HashSet<String> = decl
            .params
            .iter()
            .filter(|p| !matches!(p.ty, Type::Array { .. }))
            .map(|p| p.name.name.clone())
            .collect();
        for local in fn_all_locals(&decl.stmts, &decl.params, &self.env) {
            // An array-typed `let` is not one sized `reg` — it lowers to N
            // scalar `reg`s named `<name>_<i>`, the same convention an array
            // param uses (built into `arrays` above).
            if let ExprKind::ArrayLit(elems) = &local.value.kind {
                let (ew, _n) = &arrays[&local.name.name];
                for i in 0..elems.len() {
                    s.push_str(&format!("        reg {ew}{}_{i};\n", local.name.name));
                }
                continue;
            }
            if !declared.insert(local.name.name.clone()) {
                continue;
            }
            let decl_line = match local.inferred_width.get() {
                Some(1) => format!("        reg {};\n", local.name.name),
                Some(w) => format!("        reg [{}:0] {};\n", w - 1, local.name.name),
                None => unreachable!(
                    "LocalLet `{}` has no inferred_width — checker must run before emitter",
                    local.name.name
                ),
            };
            s.push_str(&decl_line);
        }
        s.push_str("        begin\n");
        let tail = self.expr_subst(&decl.tail, &HashMap::new(), &arrays);
        let tail_code = format!("            {} = {};\n", decl.name.name, tail);
        let body_code = self.emit_fn_stmts(
            &decl.stmts,
            &tail_code,
            &decl.name.name,
            3,
            &arrays,
            &decl.params,
        );
        s.push_str(&body_code);
        s.push_str("        end\n");
        s.push_str("    endfunction\n");

        self.env = saved_env;
        s
    }

    /// Lower a `fn`-body statement list to Verilog, threading `rest` — the
    /// code for whatever comes after this list falls through to — as a
    /// continuation. A `return` inside an `if` branch must NOT reach
    /// `rest`: it terminates that branch's generated code outright. A
    /// branch that falls through (ends without a `return`) embeds `rest`
    /// as ITS continuation, so the code after the `if` only runs on the
    /// paths that didn't already return.
    #[allow(clippy::too_many_arguments)]
    fn emit_fn_stmts(
        &mut self,
        stmts: &[FnStmt],
        rest: &str,
        fname: &str,
        indent: usize,
        arrays: &ArrayScope,
        params: &[FnParam],
    ) -> String {
        let pad = "    ".repeat(indent);
        match stmts.split_first() {
            None => rest.to_string(),
            Some((FnStmt::Let(l), tail_stmts)) => {
                let mut out = String::new();
                if let ExprKind::ArrayLit(elems) = &l.value.kind {
                    // Array-typed `let`: assign each scalar reg `<name>_<i>`
                    // from its element (mirrors the N-reg declaration above,
                    // same `<name>_<i>` convention as an array param).
                    for (i, el) in elems.iter().enumerate() {
                        let v = self.expr_subst(el, &HashMap::new(), arrays);
                        out.push_str(&format!("{pad}{}_{i} = {v};\n", l.name.name));
                    }
                } else {
                    let v = self.expr_subst(&l.value, &HashMap::new(), arrays);
                    out.push_str(&format!("{pad}{} = {v};\n", l.name.name));
                }
                out.push_str(&self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params));
                out
            }
            Some((FnStmt::Return(e), _)) => {
                // E0812 already rejects any statement after an unconditional
                // `return` in the same block, so nothing after this one in
                // `stmts` is reachable for a program that passed the checker
                // — the continuation for a `return` is simply the return
                // value itself, never `rest`.
                let v = self.expr_subst(e, &HashMap::new(), arrays);
                format!("{pad}{fname} = {v};\n")
            }
            Some((FnStmt::If { cond, then, els }, tail_stmts)) => {
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params);
                let then_code = self.emit_fn_stmts(then, &cont, fname, indent + 1, arrays, params);
                let else_code = match els {
                    Some(els) => self.emit_fn_stmts(els, &cont, fname, indent + 1, arrays, params),
                    None => cont.clone(),
                };
                let c = self.expr_subst(cond, &HashMap::new(), arrays);
                format!(
                    "{pad}if ({c}) begin\n{then_code}{pad}end else begin\n{else_code}{pad}end\n"
                )
            }
            Some((
                FnStmt::Loop {
                    var,
                    lo,
                    hi,
                    body,
                    span,
                },
                tail_stmts,
            )) => {
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params);
                let (Some(lo_v), Some(hi_v)) = (self.eval_const(lo), self.eval_const(hi)) else {
                    return cont;
                };
                let count = (hi_v - lo_v).max(0);
                if count > self.repeat_budget {
                    self.err(
                        *span,
                        format!(
                            "`loop` would unroll {count} times, over the limit of {}",
                            crate::REPEAT_BUDGET
                        ),
                        "this is compile-time hardware generation, not a runtime loop — \
                         narrow the range (a datapath this wide is almost certainly a typo)",
                    );
                    return cont;
                }
                self.repeat_budget -= count;
                self.emit_fn_loop_unroll(
                    &var.name, lo_v, hi_v, body, &cont, fname, indent, arrays, params,
                )
            }
            // `foreach` is pure sugar over `loop` — lower on the spot and
            // splice the result in as this statement's own continuation
            // chain: `cont` is "whatever comes after the foreach" (same
            // shape `If`'s `then`/`els` branches thread through), and the
            // lowered `[Loop]` re-derives its own per-iteration flow when
            // recursed into with `cont` as ITS `rest`.
            Some((
                FnStmt::ForEach {
                    var,
                    source,
                    body,
                    span,
                },
                tail_stmts,
            )) => {
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params);
                match crate::ast::lower_foreach_fn(var, source, body, *span, params) {
                    Some(lowered) => {
                        self.emit_fn_stmts(&lowered, &cont, fname, indent, arrays, params)
                    }
                    None => cont,
                }
            }
            Some((FnStmt::Error(_), tail_stmts)) => {
                self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params)
            }
        }
    }

    /// Unroll a `FnStmt::Loop`'s body `hi - lo` times, threading each
    /// iteration's continuation to the NEXT iteration (or, for the last
    /// iteration, to the loop's own `rest`) — mirrors `emit_fn_stmts`'s own
    /// continuation-passing shape so `return`'s first-match priority holds
    /// across iterations: iteration 0's `if` only falls through to
    /// iteration 1's check when iteration 0's own condition was false,
    /// never the other way around (see the design spec's "duplicate match"
    /// requirement — this is what makes that case correct).
    #[allow(clippy::too_many_arguments)]
    fn emit_fn_loop_unroll(
        &mut self,
        var: &str,
        i: i128,
        hi: i128,
        body: &[FnStmt],
        rest: &str,
        fname: &str,
        indent: usize,
        arrays: &ArrayScope,
        params: &[FnParam],
    ) -> String {
        if i >= hi {
            return rest.to_string();
        }
        let shadowed = self.env.insert(var.to_string(), i);
        let inner_rest =
            self.emit_fn_loop_unroll(var, i + 1, hi, body, rest, fname, indent, arrays, params);
        let out = self.emit_fn_stmts(body, &inner_rest, fname, indent, arrays, params);
        match shadowed {
            Some(v) => self.env.insert(var.to_string(), v),
            None => self.env.remove(var),
        };
        out
    }

    /// Flatten `const if` nodes into the items they select, evaluating
    /// conditions against `self.env`. Items in the losing branch are dropped.
    /// Nested ConstIf is resolved recursively. Used by `module()` for loops
    /// that don't recurse.
    fn flatten_items(&self, items: &[ModuleItem]) -> Vec<ModuleItem> {
        let mut out = Vec::new();
        for item in items {
            match item {
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &self.env).unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    out.extend(self.flatten_items(branch));
                }
                ModuleItem::SyncLoop(sl) => out.extend(crate::ast::lower_sync_loop(sl)),
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                        out.extend(lowered);
                    }
                    // `None` is unreachable here — emit only ever runs on
                    // already-checked programs, where E0417 would have
                    // failed the build first.
                }
                _ => out.push(item.clone()),
            }
        }
        out
    }

    /// Like `eval_consts` but recurses into `const if` winning branches so
    /// that consts declared inside a `const if` block are folded into the env.
    fn eval_consts_items(&mut self, items: &[ModuleItem], mut base: Env) -> Env {
        for item in items {
            match item {
                ModuleItem::Const(c) => {
                    base = self.eval_consts(base, std::iter::once(c));
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &base).unwrap_or(0);
                    let branch: &[ModuleItem] = if val != 0 {
                        then
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    base = self.eval_consts_items(branch, base);
                }
                _ => {}
            }
        }
        base
    }

    /// Emit every instance in `items`, descending into `repeat` bodies and
    /// unrolling them (the loop variable is bound per iteration). Declared
    /// before drives so child-output wires exist when the drives use them.
    fn emit_instances(&mut self, items: &[ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Inst(inst) => self.instance(inst),
                ModuleItem::Repeat(r) => self.unroll(r, Self::emit_instances),
                // Unlike `sync loop` below, `foreach` is pure sugar over
                // `repeat` (see `no_decls_in_repeat`, checker/names.rs) and
                // its body may legally contain an `inst` — lower and recurse
                // the same way `flatten_items`/`emit_drives` do, or an
                // instance array written with `foreach` would silently never
                // get instantiated.
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                        self.emit_instances(&lowered);
                    }
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &self.env).unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.emit_instances(branch);
                }
                // Lowered `sync loop` items never include an `Inst` — an
                // explicit no-op arm, not a stub, so a future item added to
                // the lowering that DOES need instance emission fails loudly
                // here instead of silently falling into the wildcard below.
                ModuleItem::SyncLoop(_) => {}
                _ => {}
            }
        }
    }

    /// Emit every combinational drive in `items` (`wire` inits and `=`
    /// drives), unrolling `repeat` bodies. Indices and the loop variable
    /// fold to literals, so `sum[i] = …` becomes `assign sum[2] = …`.
    fn emit_drives(&mut self, items: &[ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Wire { name, ty, init } => {
                    // Bundle wires: emit one assign per field.
                    let binfo = match ty {
                        Type::Bundle { name: bn, args } => Some((bn.clone(), args.clone())),
                        Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                            Some((id.clone(), vec![]))
                        }
                        _ => None,
                    };
                    if let Some((bname, args)) = binfo {
                        let fields = self.resolve_bundle_fields(&bname, &args);
                        if let ExprKind::BundleLit(inits) = &init.kind {
                            let inits = inits.clone();
                            for (fname, fty) in &fields {
                                if let Some(fi) = inits.iter().find(|fi| fi.name.name == *fname) {
                                    let r = self.sized_field_expr(&fi.value, fty);
                                    self.out.push_str(&format!(
                                        "    assign {}_{} = {};\n",
                                        name.name, fname, r
                                    ));
                                }
                            }
                        } else if let ExprKind::Binary {
                            op: BinOp::Coalesce,
                            lhs: clhs,
                            rhs: crhs,
                        } = &init.kind
                        {
                            for (fname, _) in &fields {
                                let r = self.coalesce_field_expr(clhs, crhs, fname);
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {r};\n",
                                    name.name
                                ));
                            }
                        } else {
                            // RHS is a plain signal: emit signame_field = rhs_field.
                            let r = self.expr(init);
                            for (fname, _) in &fields {
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {r}_{fname};\n",
                                    name.name
                                ));
                            }
                        }
                    } else {
                        let rhs = self.expr(init);
                        self.out
                            .push_str(&format!("    assign {} = {};\n", name.name, rhs));
                    }
                }
                ModuleItem::Drive { lhs, rhs } => {
                    // If LHS is a bundle signal, flatten to one assign per field.
                    let binfo = self.bundle_sigs.get(&lhs.base.name).cloned();
                    if let Some((bname, args)) = binfo {
                        let fields = self.resolve_bundle_fields(&bname, &args);
                        if let ExprKind::BundleLit(inits) = &rhs.kind {
                            let inits = inits.clone();
                            for (fname, fty) in &fields {
                                if let Some(fi) = inits.iter().find(|fi| fi.name.name == *fname) {
                                    let r = self.sized_field_expr(&fi.value, fty);
                                    self.out.push_str(&format!(
                                        "    assign {}_{} = {};\n",
                                        lhs.base.name, fname, r
                                    ));
                                }
                            }
                        } else if let ExprKind::Binary {
                            op: BinOp::Coalesce,
                            lhs: clhs,
                            rhs: crhs,
                        } = &rhs.kind
                        {
                            for (fname, _) in &fields {
                                let r = self.coalesce_field_expr(clhs, crhs, fname);
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {r};\n",
                                    lhs.base.name
                                ));
                            }
                        } else {
                            // RHS is a bundle signal (e.g. `rsp = req`).
                            let rhs_name = match &rhs.kind {
                                ExprKind::Ident(n) => n.clone(),
                                _ => self.expr(rhs),
                            };
                            for (fname, _) in &fields {
                                self.out.push_str(&format!(
                                    "    assign {}_{fname} = {rhs_name}_{fname};\n",
                                    lhs.base.name
                                ));
                            }
                        }
                    } else {
                        let l = self.lvalue(lhs);
                        let r = self.expr(rhs);
                        self.out.push_str(&format!("    assign {l} = {r};\n"));
                    }
                }
                ModuleItem::Repeat(r) => self.unroll(r, Self::emit_drives),
                // `emit_drives` (unlike the `flat`-driven loops above) walks
                // the RAW module item list, not `flatten_items`'s output, so
                // a `sync loop` here must be lowered on the spot — lowering
                // happens once (no per-iteration substitution like `unroll`).
                ModuleItem::SyncLoop(sl) => {
                    let lowered = crate::ast::lower_sync_loop(sl);
                    self.emit_drives(&lowered);
                }
                ModuleItem::ForEach(fe) => {
                    if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                        self.emit_drives(&lowered);
                    }
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &self.env).unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.emit_drives(branch);
                }
                _ => {}
            }
        }
    }

    /// Emit one child-module instantiation. Walks the CHILD's interface
    /// (not the connection list): inputs must be connected explicitly,
    /// clock/reset fall back to same-name signals, and each output gets an
    /// auto-declared wire named `{instance}_{port}` — which is exactly what
    /// `inst.port` field-accesses render to in `expr.rs`.
    fn instance(&mut self, inst: &Inst) {
        let Some((child_file, target)) = self.project.resolve_target_with_file(&inst.module) else {
            self.err(
                inst.module.span,
                format!("unknown module `{}`", inst.module.name.name),
                "is the file that defines it imported? (`import filename` at the top — spec/02 section 1.5)",
            );
            return;
        };

        // Flat Verilog name for this instance (`fa__3` for an array element
        // inside `repeat`, plain `fa` otherwise).
        let iname = self.inst_name(inst);

        // Substitute, inside child port-width expressions: the child's own
        // consts as folded literals (the parent's Verilog knows nothing
        // about a child's `const WIDTH`, and must never fold the PARENT's
        // same-named const instead), then child param names as this
        // instance's argument expressions — params win on a name clash.
        // Negative consts stay symbolic: they cannot be a `u128` literal,
        // and a negative width is already checker-rejected (E0410).
        let child_consts: Vec<(String, Expr)> = self
            .module_envs
            .get(&(child_file, target.name().name.clone()))
            .map(|env| {
                env.iter()
                    .filter(|&(_, &v)| v >= 0)
                    .map(|(n, &v)| {
                        let kind = ExprKind::Int {
                            value: v as u128,
                            raw: v.to_string(),
                        };
                        (
                            n.clone(),
                            Expr {
                                kind,
                                span: inst.span,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut args: HashMap<&str, &Expr> =
            child_consts.iter().map(|(n, e)| (n.as_str(), e)).collect();
        for a in &inst.args {
            args.insert(a.name.name.as_str(), &a.value);
        }

        // Declare wires for child outputs, connect everything by name.
        let mut port_conns: Vec<String> = Vec::new();
        for item in target.items() {
            match item {
                ModuleItem::Clock(c) => {
                    // Implicit same-name connection (spec/02 section 1.5).
                    let sig = inst
                        .conns
                        .iter()
                        .find(|cn| cn.port.name == c.name)
                        .map(|cn| self.expr(&cn.signal))
                        .unwrap_or_else(|| c.name.clone());
                    port_conns.push(format!(".{}({})", c.name, sig));
                }
                ModuleItem::Reset { name: rstp, .. } => {
                    let sig = inst
                        .conns
                        .iter()
                        .find(|cn| cn.port.name == rstp.name)
                        .map(|cn| self.expr(&cn.signal))
                        .unwrap_or_else(|| rstp.name.clone());
                    port_conns.push(format!(".{}({})", rstp.name, sig));
                }
                ModuleItem::Port { dir, name, ty } => {
                    // Bundle ports flatten to one Verilog port per field,
                    // same convention as the module header (module.rs:60-78)
                    // and Drive-path (module.rs:762-807) — a bundle-typed
                    // port is never a single scalar Verilog port.
                    //
                    // NOTE: `args` here is this function's own instance-argument
                    // map (`HashMap<&str, &Expr>`, e.g. `{"W": &Expr(8)}` for
                    // this instantiation) — NOT the port's bundle-type args
                    // (bound as `bargs` below to avoid shadowing it). A bundle
                    // param forwarding the child's own parameter (`Handshake(W:
                    // W)`) must resolve against THIS instance's `args`, not stay
                    // symbolic — see `resolve_bundle_fields_for_instance`'s doc.
                    let bundle_fields = match ty {
                        Type::Bundle {
                            name: bname,
                            args: bargs,
                        } => Some(self.resolve_bundle_fields_for_instance(bname, bargs, &args)),
                        Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                            Some(self.resolve_bundle_fields_for_instance(id, &[], &args))
                        }
                        _ => None,
                    };
                    match dir {
                        Dir::In => {
                            let Some(conn) = inst.conns.iter().find(|c| c.port.name == name.name)
                            else {
                                self.err(
                                    inst.span,
                                    format!(
                                        "instance `{}` does not connect input `{}` of module `{}`",
                                        inst.name.name, name.name, target.name().name
                                    ),
                                    "every input must be connected: `let u = Mod() { port: signal }` (spec/02 section 1.5)",
                                );
                                continue;
                            };
                            if let Some(fields) = &bundle_fields {
                                if let ExprKind::Binary {
                                    op: BinOp::Coalesce,
                                    lhs: clhs,
                                    rhs: crhs,
                                } = &conn.signal.kind
                                {
                                    let (clhs, crhs) = (clhs.clone(), crhs.clone());
                                    for (fname, _) in fields.clone() {
                                        let r = self.coalesce_field_expr(&clhs, &crhs, &fname);
                                        port_conns.push(format!(".{}_{}({})", name.name, fname, r));
                                    }
                                } else {
                                    let sig = self.expr(&conn.signal);
                                    for (fname, _) in fields {
                                        port_conns.push(format!(
                                            ".{}_{}({}_{})",
                                            name.name, fname, sig, fname
                                        ));
                                    }
                                }
                            } else {
                                let sig = self.expr(&conn.signal);
                                port_conns.push(format!(".{}({})", name.name, sig));
                            }
                        }
                        Dir::Out => {
                            if let Some(fields) = &bundle_fields {
                                for (fname, fty) in fields {
                                    let wire_name = format!("{}_{}_{}", iname, name.name, fname);
                                    let w = self.width_resolved(fty);
                                    self.out.push_str(&format!("    wire {w}{wire_name};\n"));
                                    port_conns
                                        .push(format!(".{}_{}({})", name.name, fname, wire_name));
                                }
                            } else {
                                let wire_name = format!("{}_{}", iname, name.name);
                                let w = self.width_subst(ty, &args);
                                self.out.push_str(&format!("    wire {w}{wire_name};\n"));
                                port_conns.push(format!(".{}({})", name.name, wire_name));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Unknown connection names → error.
        for c in &inst.conns {
            let known = target.items().iter().any(|i| match i {
                ModuleItem::Port { name, .. } => name.name == c.port.name,
                ModuleItem::Clock(n) | ModuleItem::Reset { name: n, .. } => n.name == c.port.name,
                _ => false,
            });
            if !known {
                self.err(
                    c.port.span,
                    format!(
                        "module `{}` has no port `{}`",
                        target.name().name,
                        c.port.name
                    ),
                    "",
                );
            }
        }

        let params = if inst.args.is_empty() {
            String::new()
        } else {
            let ps: Vec<String> = inst
                .args
                .iter()
                .map(|a| format!(".{}({})", a.name.name, self.expr(&a.value)))
                .collect();
            format!(" #({})", ps.join(", "))
        };
        // Must agree with the SAME `target`/`child_file` pair's declaration
        // header (`module()`, above) — same target, same emitted identifier.
        // Extern targets have no per-file disambiguation: there is exactly
        // one real external module regardless of which Min-Mozhi file
        // declared the `extern module` referring to it.
        let child_verilog_name = match target {
            ModuleTarget::Real(m) => self.project.verilog_module_name(child_file, m),
            ModuleTarget::Extern(em) => em
                .verilog_name
                .clone()
                .unwrap_or_else(|| em.name.name.clone()),
        };
        self.out.push_str(&format!(
            "    {}{} {} ({});\n",
            child_verilog_name,
            params,
            iname,
            port_conns.join(", ")
        ));
    }

    /// Emit the body of an always-block. `depth` is the nesting level
    /// inside the block (0 = directly under `always`), used for
    /// indentation only. `module_items` is the enclosing module's item
    /// list — needed only to resolve a `foreach` Elements-form source
    /// (`array_like_len`); threaded through so the `ForEach` arm below can
    /// lower on the spot, same as `emit_drives`'s `SyncLoop`/`ForEach` arms.
    fn seq_stmts(&mut self, stmts: &[SeqStmt], depth: usize, module_items: &[ModuleItem]) {
        let pad = "    ".repeat(depth + 1);
        // D-DEFAULT-3: defaults first so conditional assigns override (NB last-wins)
        for s in stmts {
            if let SeqStmt::Default { name, val, .. } = s {
                let v = self.expr(val);
                self.out.push_str(&format!("{pad}{} <= {v};\n", name.name));
            }
        }
        for s in stmts {
            match s {
                SeqStmt::Assign { lhs, rhs } => {
                    let l = self.lvalue(lhs);
                    let r = self.expr(rhs);
                    self.out.push_str(&format!("{pad}{l} <= {r};\n"));
                }
                SeqStmt::If { cond, then, els } => {
                    let c = self.expr(cond);
                    self.out.push_str(&format!("{pad}if ({c}) begin\n"));
                    self.seq_stmts(then, depth + 1, module_items);
                    if let Some(els) = els {
                        self.out.push_str(&format!("{pad}end else begin\n"));
                        self.seq_stmts(els, depth + 1, module_items);
                    }
                    self.out.push_str(&format!("{pad}end\n"));
                }
                SeqStmt::Default { .. } => {} // already emitted above
                SeqStmt::Loop {
                    var,
                    lo,
                    hi,
                    body,
                    span,
                } => {
                    let (Some(lo_v), Some(hi_v)) = (self.eval_const(lo), self.eval_const(hi))
                    else {
                        continue;
                    };
                    let count = (hi_v - lo_v).max(0);
                    if count > self.repeat_budget {
                        self.err(
                            *span,
                            format!(
                                "`loop` would unroll {count} times, over the limit of {}",
                                crate::REPEAT_BUDGET
                            ),
                            "this is compile-time hardware generation, not a runtime loop — \
                             narrow the range (a datapath this wide is almost certainly a typo)",
                        );
                        continue;
                    }
                    self.repeat_budget -= count;
                    let mut i = lo_v;
                    while i < hi_v {
                        let shadowed = self.env.insert(var.name.clone(), i);
                        // Same `depth`, not `depth + 1`: unlike `SeqStmt::If`,
                        // a `loop` introduces no new Verilog block — its body
                        // is a literal textual duplicate of hand-written code,
                        // not a nested scope.
                        self.seq_stmts(body, depth, module_items);
                        match shadowed {
                            Some(v) => self.env.insert(var.name.clone(), v),
                            None => self.env.remove(&var.name),
                        };
                        i += 1;
                    }
                }
                // `foreach` is pure sugar over `loop` — lower on the spot
                // and recurse at the SAME `depth` (the lowered `Loop` arm
                // above re-derives its own per-iteration `depth`, same as
                // any hand-written `loop` would).
                SeqStmt::ForEach {
                    var,
                    source,
                    body,
                    span,
                } => {
                    if let Some(lowered) =
                        crate::ast::lower_foreach_seq(var, source, body, *span, module_items)
                    {
                        self.seq_stmts(&lowered, depth, module_items);
                    }
                }
                // Unreachable on the codegen path: `parse` rejects a tree with
                // any `Error` node, so emission never sees one.
                SeqStmt::Error(_) => {}
            }
        }
    }

    /// Render an assignment target: `name`, `name[i]`, or `name[hi:lo]`.
    /// Indices fold at compile time, so a `repeat`-driven `sum[i]` lands as
    /// `sum[2]`.
    fn lvalue(&mut self, lv: &LValue) -> String {
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
    fn width(&mut self, ty: &Type) -> String {
        self.width_subst(ty, &HashMap::new())
    }

    /// Like `width`, but for already-resolved types where the width expression
    /// is a known integer literal. Emits `[7:0]` instead of `[(8)-1:0]` by
    /// evaluating the constant at Rust time rather than leaving it symbolic.
    fn width_resolved(&mut self, ty: &Type) -> String {
        match ty {
            Type::Bit => String::new(),
            Type::Bits(e) => {
                if let Ok(w) = consteval::eval(e, &self.env)
                    && w >= 1
                {
                    return format!("[{}:0] ", w - 1);
                }
                // Fallback to symbolic form.
                let we = self.expr(e);
                format!("[({we})-1:0] ")
            }
            Type::Signed(e) => {
                if let Ok(w) = consteval::eval(e, &self.env)
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
    fn width_subst(&mut self, ty: &Type, subst: &HashMap<&str, &Expr>) -> String {
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
                width: consteval::eval(e, &self.env).ok()? as u32,
                signed: false,
            }),
            Type::Signed(e) => Some(Kind {
                width: consteval::eval(e, &self.env).ok()? as u32,
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
    pub(super) fn hoist_if_needed(
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
    pub(super) fn hoist_slice_base_if_needed(
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

    /// Emit a bundle field value expression, sized to the field's type.
    /// For integer literals, this produces `1'b1` (bit), `8'd0` (bits[8]), etc.
    /// For non-literal expressions, falls back to plain `expr()`.
    fn sized_field_expr(&mut self, e: &Expr, fty: &Type) -> String {
        if let ExprKind::Int { value, .. } = &e.kind {
            let v = *value;
            match fty {
                Type::Bit => {
                    // 1-bit field: emit as 1'b0 / 1'b1.
                    return format!("1'b{}", v & 1);
                }
                Type::Bits(w_expr) => {
                    if let Ok(w) = consteval::eval(w_expr, &self.env) {
                        let w = w as u128;
                        return format!("{w}'d{v}");
                    }
                }
                Type::Signed(w_expr) => {
                    if let Ok(w) = consteval::eval(w_expr, &self.env) {
                        let w = w as u128;
                        return format!("{w}'sd{v}");
                    }
                }
                _ => {}
            }
        }
        self.expr(e)
    }

    /// Resolve a bundle type to its fields with concrete types, substituting
    /// any bundle parameters. Returns `Vec<(field_name, resolved_type)>`.
    /// Args in the `Type::Bundle` override bundle defaults; remaining params
    /// fold using the current env.
    ///
    /// A param whose arg forwards an identifier NOT in `self.env` (the
    /// common case: a module's own `parameter`, which stays a genuine
    /// symbolic Verilog parameter and is deliberately never folded to a
    /// literal — see `module()`'s header emission) is kept symbolic rather
    /// than silently falling back to the bundle's own unrelated default.
    /// Concretely: `module Child(W: int) { in req: Handshake(W: W) }` must
    /// emit `[(W)-1:0]` (tracking Child's own parameter through Verilog's
    /// own elaboration), not a literal folded from `Handshake`'s default.
    ///
    /// Use this at a module's OWN declaration (header, wire decls) — where
    /// no per-instance argument map exists and an unresolved param must
    /// stay symbolic. At an INSTANTIATION site, use
    /// [`Self::resolve_bundle_fields_for_instance`] instead, which also
    /// resolves a forwarded param against that instance's own concrete
    /// arguments (the `Ident("W")` in `Handshake(W: W)` means something
    /// different in the parent's scope than in the child's).
    pub(super) fn resolve_bundle_fields(
        &self,
        bname: &QualIdent,
        args: &[NamedArg],
    ) -> Vec<(String, Type)> {
        self.resolve_bundle_fields_inner(bname, args, None)
    }

    /// Render one OR-mux operand's value for field `fname`. `??` chains
    /// left-associatively (`x ?? y ?? z` parses as `(x ?? y) ?? z`), so an
    /// operand here can itself be a nested `Coalesce` — in which case this
    /// recurses into [`Self::coalesce_field_expr`] to extract the same
    /// field from that sub-chain, rather than rendering the (bundle-typed)
    /// sub-expression as a plain signal and bolting `_fname` onto it.
    fn coalesce_operand_field(&mut self, operand: &Expr, fname: &str) -> String {
        if let ExprKind::Binary {
            op: BinOp::Coalesce,
            lhs,
            rhs,
        } = &operand.kind
        {
            self.coalesce_field_expr(lhs, rhs, fname)
        } else {
            let s = self.expr(operand);
            format!("{s}_{fname}")
        }
    }

    /// Render the per-field expression for extracting field `fname` from a
    /// `??` OR-mux expression (`lhs ?? rhs`, both bundle-typed): the
    /// `valid` field becomes `lhs_valid ? 1'b1 : rhs_valid`, every other
    /// field becomes `lhs_valid ? lhs_fname : rhs_fname`. `lhs`/`rhs` may
    /// themselves be nested `Coalesce` chains (`??` is left-associative and
    /// chains) — [`Self::coalesce_operand_field`] recurses through those
    /// rather than treating a bundle-typed operand as a plain signal.
    pub(super) fn coalesce_field_expr(&mut self, lhs: &Expr, rhs: &Expr, fname: &str) -> String {
        let l_valid = self.coalesce_operand_field(lhs, "valid");
        if fname == "valid" {
            let r_valid = self.coalesce_operand_field(rhs, "valid");
            format!("({l_valid} ? 1'b1 : {r_valid})")
        } else {
            let l = self.coalesce_operand_field(lhs, fname);
            let r = self.coalesce_operand_field(rhs, fname);
            format!("({l_valid} ? {l} : {r})")
        }
    }

    /// Like [`Self::resolve_bundle_fields`], but for a bundle-typed port at
    /// an instantiation site: `inst_args` is the SAME child-parameter
    /// substitution map `instance()` already builds for non-bundle port
    /// widths (see `width_subst`'s callers) — a param forwarding the
    /// child's own parameter (e.g. `Handshake(W: W)`) resolves against
    /// THIS instance's concrete argument for it, not `self.env` (the
    /// PARENT's scope, where the child's bare parameter name means
    /// nothing) and not symbolically (there is no such identifier in the
    /// parent's Verilog scope to reference).
    pub(super) fn resolve_bundle_fields_for_instance(
        &self,
        bname: &QualIdent,
        args: &[NamedArg],
        inst_args: &HashMap<&str, &Expr>,
    ) -> Vec<(String, Type)> {
        self.resolve_bundle_fields_inner(bname, args, Some(inst_args))
    }

    fn resolve_bundle_fields_inner(
        &self,
        bname: &QualIdent,
        args: &[NamedArg],
        inst_args: Option<&HashMap<&str, &Expr>>,
    ) -> Vec<(String, Type)> {
        let Some(bdecl) = self.project.resolve_bundle(bname) else {
            return vec![];
        };
        // Owned copy of inst_args's expressions, for use as a `substitute_expr`
        // symbol table (which needs owned `Expr`s, not borrowed `&Expr`s).
        let inst_args_owned: HashMap<String, Expr> = inst_args
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.to_string(), (*v).clone()))
                    .collect()
            })
            .unwrap_or_default();
        // Build the param bindings: each param is either a concrete literal
        // (`param_env`) or, when its arg forwards a symbol neither `self.env`
        // nor (at an instantiation site) `inst_args` can resolve, the
        // caller's own raw expression (`param_symbolic`) — never silently
        // defaulted when an arg was given.
        let mut param_env: HashMap<String, i128> = HashMap::new();
        let mut param_symbolic: HashMap<String, Expr> = HashMap::new();
        for p in &bdecl.params {
            let arg = args.iter().find(|a| a.name.name == p.name.name);
            if let Some(a) = arg {
                if let Ok(v) = consteval::eval(&a.value, &self.env) {
                    param_env.insert(p.name.name.clone(), v);
                    continue;
                }
                if inst_args.is_some() {
                    let substituted = substitute_expr(&a.value, &self.env, &inst_args_owned);
                    if let Ok(v) = consteval::eval(&substituted, &self.env) {
                        param_env.insert(p.name.name.clone(), v);
                        continue;
                    }
                }
                param_symbolic.insert(p.name.name.clone(), a.value.clone());
            } else if let Some(default) = &p.default
                && let Ok(v) = consteval::eval(default, &self.env)
            {
                param_env.insert(p.name.name.clone(), v);
            }
        }
        // Merge param_env into env for field-type expression evaluation.
        // We do this by building a temporary Env that extends self.env.
        let mut merged_env = self.env.clone();
        for (k, v) in &param_env {
            merged_env.insert(k.clone(), *v);
        }
        // Resolve each field's type: evaluate width expressions fully to
        // integer literals using the merged env (bundle params + module
        // consts) when possible — this produces `[7:0]` rather than
        // `[(8)-1:0]` for clean Verilog output. When a param is symbolic
        // (forwards the enclosing module's own parameter), the width stays
        // symbolic too — `width_resolved`/`width_subst` already render a
        // non-literal `Type::Bits`/`Type::Signed` correctly.
        bdecl
            .fields
            .iter()
            .map(|f| {
                let resolved_ty = match &f.ty {
                    Type::Bit => Type::Bit,
                    Type::Bits(e) => Type::Bits(Box::new(self.resolve_field_width(
                        e,
                        &merged_env,
                        &param_symbolic,
                    ))),
                    Type::Signed(e) => Type::Signed(Box::new(self.resolve_field_width(
                        e,
                        &merged_env,
                        &param_symbolic,
                    ))),
                    // Enums and nested bundles: leave as-is (checker validates).
                    other => other.clone(),
                };
                (f.name.name.clone(), resolved_ty)
            })
            .collect()
    }

    /// One bundle field's width expression, resolved as far as it can be:
    /// a literal if every identifier in it is known (env or symbolic
    /// substitution), otherwise the substituted-but-still-symbolic
    /// expression (e.g. `Ident("W")`, referencing the enclosing module's
    /// own Verilog parameter) — never a hardcoded fallback to `1`.
    fn resolve_field_width(
        &self,
        e: &Expr,
        merged_env: &consteval::Env,
        param_symbolic: &HashMap<String, Expr>,
    ) -> Expr {
        if let Ok(w) = consteval::eval(e, merged_env) {
            return Expr {
                kind: ExprKind::Int {
                    value: w as u128,
                    raw: w.to_string(),
                },
                span: e.span,
            };
        }
        let substituted = substitute_expr(e, merged_env, param_symbolic);
        match consteval::eval(&substituted, merged_env) {
            Ok(w) => Expr {
                kind: ExprKind::Int {
                    value: w as u128,
                    raw: w.to_string(),
                },
                span: e.span,
            },
            Err(_) => substituted,
        }
    }
}

/// Substitute ident values in a type-width expression: a numeric value from
/// `env` folds to a literal; a name with no numeric value but a `symbolic`
/// entry (a bundle param whose arg forwards an outer identifier `env`
/// doesn't have, e.g. the enclosing module's own `parameter`) is replaced
/// by that raw expression instead, so the width stays genuinely symbolic
/// rather than silently wrong. Anything neither map covers is left as-is.
fn substitute_expr(e: &Expr, env: &consteval::Env, symbolic: &HashMap<String, Expr>) -> Expr {
    match &e.kind {
        ExprKind::Ident(name) => {
            if let Some(&v) = env.get(name.as_str()) {
                Expr {
                    kind: ExprKind::Int {
                        value: v as u128,
                        raw: v.to_string(),
                    },
                    span: e.span,
                }
            } else if let Some(sub) = symbolic.get(name.as_str()) {
                sub.clone()
            } else {
                e.clone()
            }
        }
        ExprKind::Binary { op, lhs, rhs } => Expr {
            kind: ExprKind::Binary {
                op: *op,
                lhs: Box::new(substitute_expr(lhs, env, symbolic)),
                rhs: Box::new(substitute_expr(rhs, env, symbolic)),
            },
            span: e.span,
        },
        ExprKind::Unary { op, expr } => Expr {
            kind: ExprKind::Unary {
                op: *op,
                expr: Box::new(substitute_expr(expr, env, symbolic)),
            },
            span: e.span,
        },
        ExprKind::Call { func, args } => Expr {
            kind: ExprKind::Call {
                func: *func,
                args: args
                    .iter()
                    .map(|a| substitute_expr(a, env, symbolic))
                    .collect(),
            },
            span: e.span,
        },
        // Literals and other forms are already concrete — clone as-is.
        _ => e.clone(),
    }
}

/// Every register name assigned anywhere in this statement tree (both `if`
/// branches included), deduplicated in first-seen order. Drives the
/// generated reset branch: only the regs an `on` block writes are reset
/// in its always-block.
///
/// Owned `String`s (not `&str`) because a `foreach` arm lowers into a
/// temporary `Vec<SeqStmt>` that doesn't outlive this call — same reason
/// `fn_all_locals` returns owned `LocalLet`s instead of borrowing. Cheap:
/// `on`-block bodies are small (see the O(n²) note below).
///
/// NOTE(deferred): O(n²) — `Vec::contains` on every push. Acceptable because
/// on-blocks are small in practice (typically <10 statements). If on-blocks
/// ever grow large, switch to a `HashSet` or `IndexSet`.
fn collect_assigned(stmts: &[SeqStmt], out: &mut Vec<String>, module_items: &[ModuleItem]) {
    for s in stmts {
        match s {
            SeqStmt::Assign { lhs, .. } => {
                if !out.iter().any(|n| n == &lhs.base.name) {
                    out.push(lhs.base.name.clone());
                }
            }
            SeqStmt::If { then, els, .. } => {
                collect_assigned(then, out, module_items);
                if let Some(els) = els {
                    collect_assigned(els, out, module_items);
                }
            }
            SeqStmt::Default { name, .. } => {
                if !out.iter().any(|n| n == &name.name) {
                    out.push(name.name.clone());
                }
            }
            SeqStmt::Loop { body, .. } => {
                collect_assigned(body, out, module_items);
            }
            SeqStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                if let Some(lowered) =
                    crate::ast::lower_foreach_seq(var, source, body, *span, module_items)
                {
                    collect_assigned(&lowered, out, module_items);
                }
            }
            SeqStmt::Error(_) => {} // unreachable on the codegen path
        }
    }
}

/// Raw bit-width of a scalar element `Type` (`Bit`/`Bits`/`Signed` — the
/// only shapes an array/mem ELEMENT type can be, per the checker). Used to
/// backfill `LocalLet::inferred_width` for the synthetic `var` binding
/// `ast::lower_foreach_fn`'s Elements form produces: the checker validates
/// that binding's uses via `cx.sigs` injection instead of ever
/// constructing this node (see `checker/widths/mod.rs`'s `FnStmt::ForEach`
/// arm doc comment), so nothing else ever sets this Cell for it.
fn elem_width(ty: &Type, env: &Env) -> u32 {
    match ty {
        Type::Bit => 1,
        Type::Bits(e) | Type::Signed(e) => consteval::eval(e, env)
            .expect("checker already validated this array's element width")
            as u32,
        _ => 1, // unreachable: array/mem elements are never Array/Bundle/Named
    }
}

/// Collect every `Let` binding across a `fn`-body statement list, recursing
/// into BOTH arms of nested `if`s — Verilog-2005 `function` declarations
/// must all sit before `begin`, regardless of which branch actually assigns
/// them at runtime. `params` and `env` are needed only by the `ForEach`
/// arm: `params` to lower an Elements-form source (`ast::lower_foreach_fn`
/// needs the enclosing `fn`'s own array-typed parameters — a `fn` is
/// always project-top-level, so there's no module to resolve against),
/// `env` to backfill the synthesized binding's `inferred_width` via
/// `elem_width`. Returns owned `LocalLet`s (not `&LocalLet`) because a
/// lowered `foreach` produces a temporary `Vec<FnStmt>` that doesn't
/// outlive this call — same reason `collect_assigned` returns owned
/// `String`s instead of borrowing.
fn fn_all_locals(stmts: &[FnStmt], params: &[FnParam], env: &Env) -> Vec<LocalLet> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            FnStmt::Let(l) => out.push(l.clone()),
            FnStmt::If { then, els, .. } => {
                out.extend(fn_all_locals(then, params, env));
                if let Some(els) = els {
                    out.extend(fn_all_locals(els, params, env));
                }
            }
            FnStmt::Loop { body, .. } => {
                out.extend(fn_all_locals(body, params, env));
            }
            FnStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                if let Some(lowered) =
                    crate::ast::lower_foreach_fn(var, source, body, *span, params)
                {
                    // The Elements form's synthesized `var` binding (see
                    // `lower_foreach_fn`) never gets its `inferred_width`
                    // set by the checker — backfill it here from the
                    // array's element type before it's collected below.
                    if let ForEachSource::Elements(arr) = source
                        && let Some((elem_ty, _)) = crate::ast::array_like_len_fn(&arr.name, params)
                        && let FnStmt::Loop { body: inner, .. } = &lowered[0]
                        && let Some(FnStmt::Let(synth)) = inner.first()
                    {
                        synth.inferred_width.set(Some(elem_width(&elem_ty, env)));
                    }
                    out.extend(fn_all_locals(&lowered, params, env));
                }
            }
            FnStmt::Return(_) | FnStmt::Error(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;
    use crate::{lexer, parser};

    /// Parse + emit one self-contained source (no imports) to Verilog text.
    /// Mirrors `emit_verilog::mod::tests::emit_src` — duplicated locally so
    /// this test stays inside `module.rs` per Task 9's scoping.
    fn emit_src(src: &str) -> String {
        let files = [parser::parse(lexer::lex(src).unwrap()).unwrap()];
        let project = Project::from_files(&files).unwrap();
        emit(&project, &files).expect("emit should succeed")
    }

    /// Minimal `Emitter` for unit-level tests that need to call a `&self`/
    /// `&mut self` method directly without driving the whole `emit()`
    /// pipeline. There is no existing helper for this anywhere in
    /// `emit_verilog` (every other inline test here and in `mod.rs` goes
    /// through `emit_src`/`emit()`) — this mirrors `emit()`'s own `Emitter`
    /// struct literal (`emit_verilog/mod.rs`) field-for-field, the one
    /// other place this struct gets constructed.
    fn test_emitter<'a>(project: &'a Project<'a>) -> Emitter<'a> {
        Emitter {
            project,
            out: String::new(),
            diags: Vec::new(),
            cur_file: 0,
            env: Env::new(),
            module_envs: HashMap::new(),
            repeat_budget: REPEAT_BUDGET,
            clog2_fn_used: false,
            emitting_port: false,
            funcs_used: Vec::new(),
            bundle_sigs: HashMap::new(),
            hoist_counter: 0,
            hoisted_decls: String::new(),
            cur_decls: HashMap::new(),
        }
    }

    /// Smoke test: build_decls's own logic is exercised indirectly through
    /// a normal compile (Port/Wire declarations still render), before
    /// Step 3's direct unit test below checks its return value.
    #[test]
    fn build_decls_resolves_port_and_wire_widths() {
        let src = "module M {\n  in p0: bits[8]\n  in p1: signed[4]\n  \
                    wire w: bits[3] = 0\n  out y: bit\n  y = p0[0:0]\n}\n";
        let v = emit_src(src);
        assert!(v.contains("input"));
        assert!(v.contains("wire"));
    }

    #[test]
    fn build_decls_maps_names_to_kinds() {
        let files = [parser::parse(lexer::lex("module M {}\n").unwrap()).unwrap()];
        let project = Project::from_files(&files).unwrap();
        let emitter = test_emitter(&project);
        let flat = vec![
            ModuleItem::Port {
                dir: Dir::In,
                name: Ident {
                    name: "p0".to_string(),
                    span: Span::new(0, 0),
                },
                ty: Type::Bits(Box::new(Expr {
                    kind: ExprKind::Int {
                        value: 8,
                        raw: "8".to_string(),
                    },
                    span: Span::new(0, 0),
                })),
            },
            ModuleItem::Port {
                dir: Dir::In,
                name: Ident {
                    name: "p1".to_string(),
                    span: Span::new(0, 0),
                },
                ty: Type::Signed(Box::new(Expr {
                    kind: ExprKind::Int {
                        value: 4,
                        raw: "4".to_string(),
                    },
                    span: Span::new(0, 0),
                })),
            },
        ];
        let decls = emitter.build_decls(&flat);
        assert_eq!(
            decls["p0"],
            crate::width_rules::Kind {
                width: 8,
                signed: false
            }
        );
        assert_eq!(
            decls["p1"],
            crate::width_rules::Kind {
                width: 4,
                signed: true
            }
        );
    }

    /// Proves `sync loop` actually desugars in the emitter: its 4 ports, 4
    /// regs, `on`-block FSM, and 3 output drives all reach real Verilog
    /// through the existing Port/Reg/On/Drive codegen — not silently
    /// dropped, which was the bug before `flatten_items` called
    /// `lower_sync_loop`.
    #[test]
    fn sync_loop_emits_fsm_and_ports() {
        let src = "module Search {\n  clock clk\n  reset rst\n  mem m: bits[8][8] = 0\n  in key: bits[8]\n  sync loop find_first on rise(clk) (i: 0..8) -> result: signed[4] = 0 - 1 {\n    if m[i] == key { result <- i }\n  }\n}\n";
        let v = emit_src(src);
        // Ports (4): _start in, _done/_result/_running out.
        assert!(
            v.contains("input wire find_first_start"),
            "start port missing:\n{v}"
        );
        assert!(
            v.contains("output wire find_first_done"),
            "done port missing:\n{v}"
        );
        assert!(
            v.contains("output wire signed [(4)-1:0] find_first_result"),
            "signed result port missing or wrongly formatted:\n{v}"
        );
        assert!(
            v.contains("output wire find_first_running"),
            "running port missing:\n{v}"
        );
        // Counter reg: bits[clog2(8)] = bits[3] -> "[(3)-1:0]" (same folded-
        // literal-in-parens convention as the existing clog2-port-width test).
        assert!(
            v.contains("reg [(3)-1:0] find_first_cnt;"),
            "counter reg missing:\n{v}"
        );
        // FSM always-block, clocked on `clk`.
        assert!(
            v.contains("always @(posedge clk"),
            "always block missing:\n{v}"
        );
        // The 3 generated output drives.
        assert!(
            v.contains("assign find_first_done = find_first_done_r;"),
            "done drive missing:\n{v}"
        );
        assert!(
            v.contains("assign find_first_result = find_first_acc;"),
            "result drive missing:\n{v}"
        );
        assert!(
            v.contains("assign find_first_running = find_first_running_r;"),
            "running drive missing:\n{v}"
        );
    }
}
