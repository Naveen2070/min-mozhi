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

        // Module-level consts layer onto the file consts for the duration
        // of this module; they fold to literals wherever used (widths,
        // `repeat` bounds, indices) and emit no hardware of their own.
        let file_env = self.env.clone();
        self.env = self.eval_consts_items(&m.items, file_env.clone());
        let flat: Vec<&ModuleItem> = self.flatten_items(&m.items);

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
        for item in flat.iter().copied() {
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
            .copied()
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
        for item in flat.iter().copied() {
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
        for item in flat.iter().copied() {
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
        // ponytail: repeat-body bundle wires not tracked in bundle_sigs — checker blocks wire-in-repeat today.
        self.bundle_sigs.clear();
        for item in flat.iter().copied() {
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
        let reset_name = flat.iter().copied().find_map(|i| match i {
            ModuleItem::Reset { name: r, .. } => Some(r.name.clone()),
            _ => None,
        });
        // An async reset is added to every always-block's sensitivity list
        // (`@(… or posedge rst)`); a sync reset only acts on the clock edge.
        let async_reset = flat
            .iter()
            .copied()
            .any(|i| matches!(i, ModuleItem::Reset { is_async: true, .. }));
        let regs: HashMap<&str, &Expr> = flat
            .iter()
            .copied()
            .filter_map(|i| match i {
                ModuleItem::Reg { name, reset, .. } => Some((name.name.as_str(), reset)),
                _ => None,
            })
            .collect();

        for item in flat.iter().copied() {
            if let ModuleItem::On(on) = item {
                let mut assigned: Vec<&str> = Vec::new();
                collect_assigned(&on.body, &mut assigned);

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
                        if let Some(reset_val) = regs.get(r) {
                            let v = self.expr(reset_val);
                            self.out.push_str(&format!("            {r} <= {v};\n"));
                        }
                    }
                    self.out.push_str("        end else begin\n");
                    self.seq_stmts(&on.body, 2);
                    self.out.push_str("        end\n");
                } else {
                    self.seq_stmts(&on.body, 1);
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
        for local in fn_all_locals(&decl.stmts) {
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
                other => {
                    let pw = self.width(other);
                    s.push_str(&format!("        input {pw}{};\n", param.name.name));
                }
            }
        }
        for local in fn_all_locals(&decl.stmts) {
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
        let body_code = self.emit_fn_stmts(&decl.stmts, &tail_code, &decl.name.name, 3, &arrays);
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
    fn emit_fn_stmts(
        &mut self,
        stmts: &[FnStmt],
        rest: &str,
        fname: &str,
        indent: usize,
        arrays: &ArrayScope,
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
                out.push_str(&self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays));
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
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays);
                let then_code = self.emit_fn_stmts(then, &cont, fname, indent + 1, arrays);
                let else_code = match els {
                    Some(els) => self.emit_fn_stmts(els, &cont, fname, indent + 1, arrays),
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
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays);
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
                self.emit_fn_loop_unroll(&var.name, lo_v, hi_v, body, &cont, fname, indent, arrays)
            }
            Some((FnStmt::Error(_), tail_stmts)) => {
                self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays)
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
    ) -> String {
        if i >= hi {
            return rest.to_string();
        }
        let shadowed = self.env.insert(var.to_string(), i);
        let inner_rest =
            self.emit_fn_loop_unroll(var, i + 1, hi, body, rest, fname, indent, arrays);
        let out = self.emit_fn_stmts(body, &inner_rest, fname, indent, arrays);
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
    fn flatten_items<'a>(&self, items: &'a [ModuleItem]) -> Vec<&'a ModuleItem> {
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
                _ => out.push(item),
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
                        } else {
                            // RHS is a signal: emit signame_field = rhs_field.
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
        let Some((child_file, child)) = self.project.resolve_module_with_file(&inst.module) else {
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
            .get(&(child_file, child.name.name.clone()))
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
        for item in &child.items {
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
                ModuleItem::Port { dir, name, ty } => match dir {
                    Dir::In => {
                        let Some(conn) = inst.conns.iter().find(|c| c.port.name == name.name)
                        else {
                            self.err(
                                inst.span,
                                format!(
                                    "instance `{}` does not connect input `{}` of module `{}`",
                                    inst.name.name, name.name, child.name.name
                                ),
                                "every input must be connected: `let u = Mod() { port: signal }` (spec/02 section 1.5)",
                            );
                            continue;
                        };
                        let sig = self.expr(&conn.signal);
                        port_conns.push(format!(".{}({})", name.name, sig));
                    }
                    Dir::Out => {
                        let wire_name = format!("{}_{}", iname, name.name);
                        let w = self.width_subst(ty, &args);
                        self.out.push_str(&format!("    wire {w}{wire_name};\n"));
                        port_conns.push(format!(".{}({})", name.name, wire_name));
                    }
                },
                _ => {}
            }
        }

        // Unknown connection names → error.
        for c in &inst.conns {
            let known = child.items.iter().any(|i| match i {
                ModuleItem::Port { name, .. } => name.name == c.port.name,
                ModuleItem::Clock(n) | ModuleItem::Reset { name: n, .. } => n.name == c.port.name,
                _ => false,
            });
            if !known {
                self.err(
                    c.port.span,
                    format!("module `{}` has no port `{}`", child.name.name, c.port.name),
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
        // Must agree with the SAME `child`/`child_file` pair's declaration
        // header (`module()`, above) — same target, same emitted identifier.
        let child_verilog_name = self.project.verilog_module_name(child_file, child);
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
    /// indentation only.
    fn seq_stmts(&mut self, stmts: &[SeqStmt], depth: usize) {
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
                    self.seq_stmts(then, depth + 1);
                    if let Some(els) = els {
                        self.out.push_str(&format!("{pad}end else begin\n"));
                        self.seq_stmts(els, depth + 1);
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
                        self.seq_stmts(body, depth);
                        match shadowed {
                            Some(v) => self.env.insert(var.name.clone(), v),
                            None => self.env.remove(&var.name),
                        };
                        i += 1;
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
                if let Ok(w) = consteval::eval(e, &self.env) {
                    if w >= 1 {
                        return format!("[{}:0] ", w - 1);
                    }
                }
                // Fallback to symbolic form.
                let we = self.expr(e);
                format!("[({we})-1:0] ")
            }
            Type::Signed(e) => {
                if let Ok(w) = consteval::eval(e, &self.env) {
                    if w >= 1 {
                        return format!("signed [{}:0] ", w - 1);
                    }
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
            // Bundle types are flattened to individual signals — `width` is
            // never called on a bundle type directly in the port/wire path.
            // If it is called (e.g., from an unexpected path), treat as 0-width.
            Type::Bundle { .. } => String::new(),
            Type::Array { .. } => unreachable!(
                "array types are rejected by the checker (E0416) before reaching the \
                 emitter for anything but a `fn` parameter, which render_fn_decl handles \
                 separately without calling width()/width_subst()"
            ),
        }
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
    pub(super) fn resolve_bundle_fields(
        &self,
        bname: &QualIdent,
        args: &[NamedArg],
    ) -> Vec<(String, Type)> {
        let Some(bdecl) = self.project.resolve_bundle(bname) else {
            return vec![];
        };
        // Build param env: bundle defaults first, then call-site overrides.
        let mut param_env: HashMap<String, i128> = HashMap::new();
        for p in &bdecl.params {
            if let Some(default) = &p.default {
                if let Ok(v) = consteval::eval(default, &self.env) {
                    param_env.insert(p.name.name.clone(), v);
                }
            }
        }
        for a in args {
            if let Ok(v) = consteval::eval(&a.value, &self.env) {
                param_env.insert(a.name.name.clone(), v);
            }
        }
        // Merge param_env into env for field-type expression evaluation.
        // We do this by building a temporary Env that extends self.env.
        let mut merged_env = self.env.clone();
        for (k, v) in &param_env {
            merged_env.insert(k.clone(), *v);
        }
        // Resolve each field's type: evaluate width expressions fully to integer
        // literals using the merged env (bundle params + module consts).
        // This produces `[7:0]` rather than `[(8)-1:0]` for clean Verilog output.
        bdecl
            .fields
            .iter()
            .map(|f| {
                let resolved_ty = match &f.ty {
                    Type::Bit => Type::Bit,
                    Type::Bits(e) => {
                        let w = consteval::eval(e, &merged_env).unwrap_or_else(|_| {
                            // Fall back to substitution if eval fails (symbolic param).
                            consteval::eval(&substitute_expr(e, &merged_env), &merged_env)
                                .unwrap_or(1)
                        });
                        let lit = Expr {
                            kind: ExprKind::Int {
                                value: w as u128,
                                raw: w.to_string(),
                            },
                            span: f.span,
                        };
                        Type::Bits(Box::new(lit))
                    }
                    Type::Signed(e) => {
                        let w = consteval::eval(e, &merged_env).unwrap_or_else(|_| {
                            consteval::eval(&substitute_expr(e, &merged_env), &merged_env)
                                .unwrap_or(1)
                        });
                        let lit = Expr {
                            kind: ExprKind::Int {
                                value: w as u128,
                                raw: w.to_string(),
                            },
                            span: f.span,
                        };
                        Type::Signed(Box::new(lit))
                    }
                    // Enums and nested bundles: leave as-is (checker validates).
                    other => other.clone(),
                };
                (f.name.name.clone(), resolved_ty)
            })
            .collect()
    }
}

/// Substitute constant ident values in a type-width expression. Used by
/// `resolve_bundle_fields` to fold bundle param names (e.g. `W`) into their
/// concrete values so `bits[W]` becomes `bits[8]` in the emitted Verilog.
/// Only replaces `ExprKind::Ident` nodes that appear in `env`; leaves all
/// other nodes (arithmetic, literals) structurally identical.
fn substitute_expr(e: &Expr, env: &consteval::Env) -> Expr {
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
            } else {
                e.clone()
            }
        }
        ExprKind::Binary { op, lhs, rhs } => Expr {
            kind: ExprKind::Binary {
                op: *op,
                lhs: Box::new(substitute_expr(lhs, env)),
                rhs: Box::new(substitute_expr(rhs, env)),
            },
            span: e.span,
        },
        ExprKind::Unary { op, expr } => Expr {
            kind: ExprKind::Unary {
                op: *op,
                expr: Box::new(substitute_expr(expr, env)),
            },
            span: e.span,
        },
        ExprKind::Call { func, args } => Expr {
            kind: ExprKind::Call {
                func: *func,
                args: args.iter().map(|a| substitute_expr(a, env)).collect(),
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
/// NOTE(deferred): O(n²) — `Vec::contains` on every push. Acceptable because
/// on-blocks are small in practice (typically <10 statements). If on-blocks
/// ever grow large, switch to a `HashSet` or `IndexSet`.
fn collect_assigned<'a>(stmts: &'a [SeqStmt], out: &mut Vec<&'a str>) {
    for s in stmts {
        match s {
            SeqStmt::Assign { lhs, .. } => {
                if !out.contains(&lhs.base.name.as_str()) {
                    out.push(&lhs.base.name);
                }
            }
            SeqStmt::If { then, els, .. } => {
                collect_assigned(then, out);
                if let Some(els) = els {
                    collect_assigned(els, out);
                }
            }
            SeqStmt::Default { name, .. } => {
                if !out.contains(&name.name.as_str()) {
                    out.push(&name.name);
                }
            }
            SeqStmt::Loop { body, .. } => {
                collect_assigned(body, out);
            }
            SeqStmt::Error(_) => {} // unreachable on the codegen path
        }
    }
}

/// Collect every `Let` binding across a `fn`-body statement list, recursing
/// into BOTH arms of nested `if`s — Verilog-2005 `function` declarations
/// must all sit before `begin`, regardless of which branch actually assigns
/// them at runtime.
fn fn_all_locals(stmts: &[FnStmt]) -> Vec<&LocalLet> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            FnStmt::Let(l) => out.push(l),
            FnStmt::If { then, els, .. } => {
                out.extend(fn_all_locals(then));
                if let Some(els) = els {
                    out.extend(fn_all_locals(els));
                }
            }
            FnStmt::Loop { body, .. } => {
                out.extend(fn_all_locals(body));
            }
            FnStmt::Return(_) | FnStmt::Error(_) => {}
        }
    }
    out
}
