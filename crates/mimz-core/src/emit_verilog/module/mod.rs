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

/// Every `cover` statement reachable from `items` — module-item level
/// directly (`ModuleItem::Cover`), `on`-block level recursed through
/// `on.body` (and any nested `if`/`loop`/`foreach` inside it) via
/// `collect_seq_covers`. Used up front, to declare every clocked cover's
/// hidden counter register before the `posedge` block that increments it
/// (GAP-6 follow-up) — the comb form declares its own counter inline
/// (Task 6) and never reaches this list. `pub(super)` since Task 8:
/// `testbench.rs` reuses it to print each cover's final hit count — the DUT module
/// itself has no Verilog-2005-legal "simulation just ended" hook, but the
/// `--emit-testbench` output's own `initial` block, ending in `$finish`,
/// does.
pub(super) fn collect_on_block_covers(items: &[ModuleItem]) -> Vec<&CoverStmt> {
    let mut out = Vec::new();
    for item in items {
        if let ModuleItem::On(on) = item {
            collect_seq_covers(&on.body, &mut out);
        }
    }
    out
}

fn collect_seq_covers<'a>(stmts: &'a [SeqStmt], out: &mut Vec<&'a CoverStmt>) {
    for s in stmts {
        match s {
            SeqStmt::Cover(c) => out.push(c),
            SeqStmt::If { then, els, .. } => {
                collect_seq_covers(then, out);
                if let Some(e) = els {
                    collect_seq_covers(e, out);
                }
            }
            SeqStmt::Loop { body, .. } => collect_seq_covers(body, out),
            SeqStmt::ForEach { body, .. } => collect_seq_covers(body, out),
            _ => {}
        }
    }
}

/// Every `cover`'s `span.start -> ordinal rank by source position`, across
/// BOTH the module-item and `on`-block forms combined — used to name each
/// hidden hit-counter register `__cover_{ordinal}_count` instead of
/// `__cover_{span.start}_count` (GAP-6 follow-up, found via
/// `tests/translate.rs`'s pretty-print round-trip test): a raw byte offset
/// shifts on ANY reformat (pretty-print, `mimz translate`, keyword reskin)
/// even when the statement's relative position among covers is unchanged,
/// so it is not a stable register name across a semantically-identical
/// re-emit. An ordinal rank is — pretty-printing never reorders statements.
pub(super) fn build_cover_ordinals(items: &[ModuleItem]) -> HashMap<usize, usize> {
    let mut starts: Vec<usize> = items
        .iter()
        .filter_map(|it| match it {
            ModuleItem::Cover(c) => Some(c.span.start),
            _ => None,
        })
        .collect();
    starts.extend(collect_on_block_covers(items).iter().map(|c| c.span.start));
    starts.sort_unstable();
    starts
        .into_iter()
        .enumerate()
        .map(|(i, start)| (start, i))
        .collect()
}

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
        self.pre_decl_hoisted_decls.clear();
        self.in_pre_decl_render = false;
        self.declared_signal_names.clear();

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
        self.cur_decls = Rc::new(self.build_decls(&flat));
        self.env = params_env;
        self.cover_ordinals = build_cover_ordinals(&flat);

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
                ModuleItem::Clock(c) => {
                    if self.declare_signal_name(&c.name, c.span) {
                        ports.push(format!("input wire {}", c.name));
                    }
                }
                ModuleItem::Reset { name: r, .. } => {
                    if self.declare_signal_name(&r.name, r.span) {
                        ports.push(format!("input wire {}", r.name));
                    }
                }
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
                            let flat_name = format!("{}_{}", name.name, fname);
                            if self.declare_signal_name(&flat_name, name.span) {
                                ports.push(format!("{d} {w}{flat_name}"));
                            }
                        }
                    } else {
                        let w = self.width(ty);
                        if self.declare_signal_name(&name.name, name.span) {
                            ports.push(format!("{d} {w}{}", name.name));
                        }
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
                            let flat_name = format!("{}_{}", name.name, fname);
                            if self.declare_signal_name(&flat_name, name.span) {
                                let w = self.width_resolved(fty);
                                self.out.push_str(&format!("    wire {w}{flat_name};\n"));
                            }
                        }
                    } else if self.declare_signal_name(&name.name, name.span) {
                        let w = self.width(ty);
                        self.out.push_str(&format!("    wire {w}{};\n", name.name));
                    }
                }
                ModuleItem::Reg { name, ty, .. } => {
                    self.check_ascii(name);
                    if self.declare_signal_name(&name.name, name.span) {
                        let w = self.width(ty);
                        self.out.push_str(&format!("    reg {w}{};\n", name.name));
                    }
                }
                ModuleItem::Mem {
                    name, ty, depth, ..
                } => {
                    self.check_ascii(name);
                    if self.declare_signal_name(&name.name, name.span) {
                        let w = self.width(ty);
                        let d = self.expr(depth);
                        self.out
                            .push_str(&format!("    reg {w}{} [0:({d})-1];\n", name.name));
                    }
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

        // Round-8 plan Task 1 (BUG-70, GAP-18/20): declare every instance's
        // OUTPUT wire before `pre_decl_hoist_pos` is captured a few lines
        // down — strictly before ANY hoist this module can raise, not just
        // before its OWN instance's connections. `emit_instances` (below,
        // still the pass that renders connections and instantiation lines)
        // used to be the only pass over instances and interleaved each
        // instance's own output-wire declaration with the NEXT instance's
        // connection rendering; a hoist raised by instance N's connection
        // reading an EARLIER instance's output (`u1.q`) then got spliced at
        // the single, fixed `pre_decl_hoist_pos` — ahead of `u1`'s own wire,
        // which `emit_instances` had only just written a few lines into that
        // same region (BUG-70). Doing every instance's declaration walk
        // first, fully outside the `pre_decl_hoist_pos` window, restores the
        // safety argument BUG-66's own fix claims but did not actually
        // establish for this one signal class. See `declare_instance_outputs`
        // (`module/instances.rs`) for the full reasoning.
        self.repeat_budget = REPEAT_BUDGET;
        self.declare_instance_outputs(&m.items);

        // Clocked `cover`s: declare each hit-counter register up front
        // (before the `posedge` block below references it) — the comb
        // form declares its own counter inline (Task 6), this is only for
        // covers found inside an `on` block. GAP-6 follow-up.
        for c in collect_on_block_covers(&flat) {
            let name = format!("__cover_{}_count", self.cover_ordinals[&c.span.start]);
            self.out.push_str("    `ifndef SYNTHESIS\n");
            self.out.push_str(&format!("    reg [31:0] {name} = 0;\n"));
            self.out.push_str("    `endif\n");
        }

        // Round-7 plan Task 3 (BUG-66, GAP-18): the `mem` init/depth, `reg`
        // reset, and instance-port-connection renders below all run BEFORE
        // `hoist_pos` (captured after `emit_instances`, a few lines down)
        // — so a hoist raised by any of them used to be spliced in AFTER
        // its own use here. Every `wire`/`reg`/`mem` declaration is already
        // emitted above (the "Declarations" loop), so this insertion point
        // is safe for all three: `pre_decl_hoisted_decls` is spliced here,
        // strictly before `hoist_pos`'s own splice, and strictly after
        // every signal these three sites can reference (BUG-66's own
        // reproductions, and option (a)'s own safety argument, only ever
        // touch ports/parameters — already declared in the header).
        let pre_decl_hoist_pos = self.out.len();
        self.in_pre_decl_render = true;

        // Power-on init: seed every cell of each memory to its init value
        // (mirrors the simulator, which initializes all cells at construction).
        // Mandatory init value → no uninitialized state, without a per-cycle
        // reset (the `reset` line clears registers only).
        //
        // BUG-32 (docs/audit/bugs.md), Task 8: this `initial` seed is
        // simulation- and FPGA-block-RAM-only — a real ASIC has no defined power-on RAM
        // content, and an ASIC synthesis flow will not honor it. Note this
        // once, right above the first `mem`'s init, rather than silently
        // implying the value is universal (`docs/guide/04-signals.md`'s
        // own `mem` section carries the same caveat for the reader who
        // never opens the generated Verilog).
        let mut mem_note_emitted = false;
        for item in flat.iter() {
            if let ModuleItem::Mem {
                name, depth, init, ..
            } = item
            {
                if !mem_note_emitted {
                    self.out.push_str(
                        "    // NOTE: the `initial` memory-init loop(s) below are \
                         simulation/FPGA-only — an ASIC flow has no defined \
                         power-on RAM content and will not honor them. Add an \
                         explicit clocked load/reset path if this design \
                         targets ASIC.\n",
                    );
                    mem_note_emitted = true;
                }
                let pre_decl_hoists_before = self.pre_decl_hoisted_decls.len();
                let d = self.expr(depth);
                let v = self.expr(init);
                let needs_delay_guard = self.pre_decl_hoisted_decls.len() > pre_decl_hoists_before;
                let iv = format!("__mimz_{}_i", name.name);
                self.out.push_str(&format!("    integer {iv};\n"));
                // `#0`: Task 3 (BUG-66, round-7 plan) fixed the DECLARATION
                // order of a hoisted `wire`/`assign` pair this loop's own
                // `v` may reference (`push_hoisted_decl`, `module/ports.rs`)
                // — but a continuous `assign` and this `initial` block are
                // both scheduled in the SAME time-0 active region, with no
                // ordering guarantee between them. Confirmed against real
                // `iverilog`: without `#0`, a hoisted operand read X/Z here
                // (the wire hadn't propagated yet), even though the exact
                // same hoisted wire read fine from the REG-init's own
                // single-statement `initial` just below — a `for` loop
                // apparently schedules differently. `#0` defers this block
                // to time 0's INACTIVE region, strictly after every
                // active-region continuous assign has settled, with no
                // visible time advance (the emitted testbench's own first
                // read is a full `#1` later — BUG-65's fix — so this can
                // never race that).
                //
                // Round-8 plan Task 7: the race is only possible when `d`/`v`
                // actually pushed a hoisted `wire`/`assign` pair into
                // `pre_decl_hoisted_decls` — a plain literal or a reference
                // to an already-declared signal has nothing to race. Guard
                // is narrowed to that case instead of applying unconditionally.
                let delay = if needs_delay_guard { "#0 " } else { "" };
                self.out.push_str(&format!(
                    "    initial {delay}for ({iv} = 0; {iv} < ({d}); {iv} = {iv} + 1) {}[{iv}] = {v};\n",
                    name.name
                ));
            }
        }

        // BUG-65 (docs/audit/bugs.md): `ModuleItem::Reg`'s own doc comment
        // states the identical safety rule `mem`'s does — "no uninitialized
        // state" — and `mimz-sim/src/sim/kernel.rs`'s
        // `regs_init_to_their_reset_value` test confirms the KERNEL honors
        // it unconditionally: a reg holds its declared value from t=0, no
        // reset pulse required. The emitter only ever encoded that value
        // into the synchronous `if (rst)` branch below — a test/design that
        // reads a reg (or anything derived from it) before ever asserting
        // reset agrees with `mimz test` and disagrees with the emitted
        // Verilog, where the reg is a real 4-state X until reset fires.
        // Confirmed against real `iverilog`: `std/fifo.mimz`'s "starts
        // empty" test (checks `count == 0` with zero stimulus) and three
        // more shipped stdlib examples (`debouncer`, `pwm`, `uart_tx`) all
        // reported FAIL for exactly this reason. Same fix shape as the
        // `mem` loop just above, one level up: seed every reg to its
        // declared value too.
        let mut reg_note_emitted = false;
        for item in flat.iter() {
            if let ModuleItem::Reg { name, reset, .. } = item {
                if !reg_note_emitted {
                    self.out.push_str(
                        "    // NOTE (BUG-65, docs/audit/bugs.md): the `initial` \
                         register-init line(s) below are simulation/FPGA-only - an ASIC \
                         flow has no defined power-on default and will not honor them. \
                         The synchronous reset below still applies regardless.\n",
                    );
                    reg_note_emitted = true;
                }
                let pre_decl_hoists_before = self.pre_decl_hoisted_decls.len();
                let v = self.expr(reset);
                let needs_delay_guard = self.pre_decl_hoisted_decls.len() > pre_decl_hoists_before;
                // `#0`: same root cause as the `mem` loop's own `#0` above
                // (a hoisted operand `v` may reference is a continuous
                // `assign`, racing this `initial` block in the same time-0
                // active region) — applied here for parity even though
                // this single-statement form did NOT reproduce the race
                // under real `iverilog` the way the `for`-loop form did;
                // leaving the identical hazard unguarded here on nothing
                // but "it happened not to manifest for one hand-picked
                // vector" is exactly the fail-open pattern this project's
                // own audit history keeps finding (GAP-13/14).
                //
                // Round-8 plan Task 7: narrowed to fire only when `v` itself
                // pushed a hoisted `wire`/`assign` pair — same reasoning as
                // the `mem` loop above, kept per-parity rather than dropped,
                // since the hazard this guards is real even though no
                // shipped corpus file currently reaches it.
                let delay = if needs_delay_guard { "#0 " } else { "" };
                self.out
                    .push_str(&format!("    initial {delay}{} = {v};\n", name.name));
            }
        }

        // Instances: auto-wire every child output as `{inst}_{port}`.
        // `repeat` bodies are unrolled per iteration (instances first, to
        // match Verilog's declare-before-use convention).
        self.repeat_budget = REPEAT_BUDGET;
        self.emit_instances(&m.items);
        self.in_pre_decl_render = false;

        // Insertion point for every hoisted `wire`/`assign` pair
        // (`self.hoisted_decls`, filled in below by the drives/seq-block
        // rendering that follows) — BUG-44 (docs/audit/bugs.md): a hoisted
        // wire can reference a `reg`/`wire`/`mem` (declared just above) or
        // an instance's output wire (declared by `emit_instances`, just
        // above too), so it must land AFTER both, not at `fn_pos` (right
        // after the port list, before either). Icarus Verilog 14 rejects
        // forward references to those with "declaration after use" —
        // 12.0 (this codebase's own audit baseline) did not catch it,
        // which is how this went unnoticed even though the underlying
        // shape (`bug_23_wrap_under_sibling_add_inside_a_concat_matches_icarus`)
        // was already a passing, shipped regression test.
        let hoist_pos = self.out.len();

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
        //
        // ponytail: Task 8 #4 (docs/plan/v0.2-class-closure-round3.local.
        // md) — `self.expr(&a.cond)` below can hoist a width-mismatched
        // sub-expression into `self.hoisted_decls`, which splices in at
        // `hoist_pos` UNCONDITIONALLY (every hoist in the module shares
        // one buffer, one insertion point, no per-hoist provenance) — so
        // a wire that exists ONLY to feed an assert/cover condition sits
        // outside the `ifndef SYNTHESIS` guard the assert itself gets,
        // becoming dead logic in a synthesis build. Real synthesizers
        // prune unreferenced-by-hardware nets, so this is noise, not a
        // miscompile — fixing it for real means giving `hoisted_decls`
        // per-hoist provenance (which construct asked for it), a bigger
        // change than this ceiling justifies today. Upgrade path: thread
        // a `synthesis_gated: bool` through the hoist call chain if this
        // ever needs to stop being cosmetic.
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

        // Combinational `cover`s: a hidden hit-counter register, sensitized
        // on the condition wire only (never on the counter itself — a
        // naive `always @(*)` self-references its own write on the RHS of
        // `count + 1`, which pulls `count` into `@*`'s implicit sensitivity
        // list and creates a self-triggering reactive loop). GAP-6
        // follow-up. Synthesis-stripped, same convention `assert` uses.
        //
        // Task 8 #1: round 2's review claimed this "misses time zero" (a condition
        // true from the start, never toggling, never counted) — checked
        // against real `iverilog`/`vvp` before acting on it, and the claim
        // does not hold: a net's first x -> value transition (computed at
        // time 0) IS itself a change, so `always @(cond_name)` already
        // fires once. A naive `initial #0` sibling sample was tried and
        // reverted — it DOUBLE-counted the same time-0 hit. No code
        // change; pinned as `comb_cover_counts_a_condition_true_from_
        // time_zero_exactly_once` (`tests/icarus.rs`) instead.
        for item in flat.iter() {
            if let ModuleItem::Cover(c) = item {
                let ord = self.cover_ordinals[&c.span.start];
                let name = format!("__cover_{ord}_count");
                let cond_name = format!("__cover_{ord}_cond");
                let cond = self.expr(&c.cond);
                self.out.push_str("    `ifndef SYNTHESIS\n");
                self.out
                    .push_str(&format!("    wire {cond_name} = ({cond});\n"));
                self.out.push_str(&format!("    reg [31:0] {name} = 0;\n"));
                self.out.push_str(&format!(
                    "    always @({cond_name}) if ({cond_name}) {name} = {name} + 1;\n"
                ));
                self.out.push_str("    `endif\n");
            }
        }

        // Task 8 #2: `__cover_N_count` was incremented and read by nothing.
        // `final` (the natural "simulation just ended" hook) is IEEE
        // 1364-2001/SystemVerilog, not legal in the Verilog-2005 this
        // project targets throughout — confirmed the hard way, `iverilog`
        // (default, no `-g2005` even needed) rejects it outright. A DUT
        // module has no portable Verilog-2005 hook for "print when an
        // EXTERNAL testbench calls `$finish`" — the only place that
        // moment is actually known is wherever `$finish` itself is
        // written, i.e. `--emit-testbench`'s own generated `initial`
        // block, which already ends in one (`testbench.rs`).

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
        // Insert LARGEST offset first — `fn_pos <= pre_decl_hoist_pos <=
        // hoist_pos` always (nothing/`mem`+`reg`+instances/everything else
        // stand between them respectively) — so an earlier insertion never
        // shifts a not-yet-used later position out from under us. Round-7
        // plan Task 3 (BUG-66) added `pre_decl_hoist_pos` between the two
        // pre-existing splices; the ordering rule itself is unchanged.
        if !self.hoisted_decls.is_empty() {
            self.out.insert_str(hoist_pos, &self.hoisted_decls);
        }
        if !self.pre_decl_hoisted_decls.is_empty() {
            self.out
                .insert_str(pre_decl_hoist_pos, &self.pre_decl_hoisted_decls);
        }
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
