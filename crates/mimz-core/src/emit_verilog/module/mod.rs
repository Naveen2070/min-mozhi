//! Module-level emission: shells with parameters and ports, enum
//! localparams, declarations, instances (auto-wired outputs, implicit
//! clk/rst), combinational assigns, and always-blocks with generated reset.

use super::expr::ArrayScope;
use super::*;
use funcs::collect_assigned;

mod bundle_fields;
mod drives;
mod funcs;
mod instances;
mod ports;
mod seq;

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
        //
        // A parametric width (`reg sr: bits[WIDTH]`) is otherwise absent
        // from `self.env` when this module compiles directly (not as a
        // sub-instance) — every parameter is deliberately kept symbolic
        // in the EMITTED TEXT (real per-instance overrides), but that
        // left `sr` entirely out of `cur_decls` (`resolved_kind` needs a
        // concrete width), silently disabling every hoist a `<<` growth
        // now needs (BUG-30, `docs/audit/bugs.md` — `trunc(sr << 1,
        // WIDTH)` rendered as an illegal Verilog part-select of a
        // compound expression instead of hoisting to a wire first).
        // Binding each parameter's own DEFAULT value gives `build_decls`
        // a concrete representative width; restoring `self.env`
        // immediately after keeps every other render (which must stay
        // symbolic) unaffected.
        let params_env = self.env.clone();
        for p in &m.params {
            if let Some(d) = &p.default
                && let Ok(v) = consteval::eval(d, &self.env)
            {
                self.env.insert(p.name.name.clone(), v);
            }
        }
        self.cur_decls = self.build_decls(&flat);
        self.env = params_env;

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

        // Combinational `assert`s: one guarded `always @(*)` per statement,
        // checked every settled comb state (GAP-6). Placed after the
        // clocked always-blocks so a design's `on`-block output stays
        // textually first, matching this emitter's existing item-order
        // convention.
        for item in flat.iter() {
            if let ModuleItem::Assert(a) = item {
                let cond = self.expr(&a.cond);
                let msg = match &a.msg {
                    Some(m) => m.replace('"', "\\\""),
                    None => cond.replace('"', "\\\""),
                };
                self.out.push_str("    `ifndef SYNTHESIS\n");
                self.out
                    .push_str(&format!("    always @(*) if (!({cond})) begin\n"));
                self.out
                    .push_str(&format!("        $display(\"ASSERTION FAILED: {msg}\");\n"));
                self.out.push_str("        $fatal(1);\n");
                self.out.push_str("    end\n");
                self.out.push_str("    `endif\n");
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
}

#[cfg(test)]
mod tests;
