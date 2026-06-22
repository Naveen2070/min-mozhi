//! Module-level emission: shells with parameters and ports, enum
//! localparams, declarations, instances (auto-wired outputs, implicit
//! clk/rst), combinational assigns, and always-blocks with generated reset.

use super::*;

impl Emitter<'_> {
    /// Emit one complete Verilog module. Source order inside the module
    /// body is free; output is regrouped into the conventional Verilog
    /// order: header/params/ports → enum localparams → wire/reg
    /// declarations → instances → assigns → always-blocks.
    pub(super) fn module(&mut self, m: &Module) {
        self.check_ascii(&m.name);

        // Module-level consts layer onto the file consts for the duration
        // of this module; they fold to literals wherever used (widths,
        // `repeat` bounds, indices) and emit no hardware of their own.
        let file_env = self.env.clone();
        self.env = self.eval_consts(
            file_env.clone(),
            m.items.iter().filter_map(|i| match i {
                ModuleItem::Const(c) => Some(c),
                _ => None,
            }),
        );

        // Parameters.
        let mut header = format!("module {}", m.name.name);
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

        // Ports: clock/reset first, then declared order.
        let mut ports: Vec<String> = Vec::new();
        for item in &m.items {
            match item {
                ModuleItem::Clock(c) => ports.push(format!("input wire {}", c.name)),
                ModuleItem::Reset { name: r, .. } => ports.push(format!("input wire {}", r.name)),
                ModuleItem::Port { dir, name, ty } => {
                    self.check_ascii(name);
                    let w = self.width(ty);
                    let d = match dir {
                        Dir::In => "input wire",
                        Dir::Out => "output wire",
                    };
                    ports.push(format!("{d} {w}{}", name.name));
                }
                _ => {}
            }
        }
        header.push_str(&format!(" (\n    {}\n);\n", ports.join(",\n    ")));
        self.out.push_str(&header);

        // Enum encodings as localparams.
        let enums: Vec<&EnumDecl> = m
            .items
            .iter()
            .filter_map(|i| match i {
                ModuleItem::Enum(e) => Some(e),
                _ => None,
            })
            .collect();
        for e in &enums {
            let w = clog2(e.variants.len());
            for (i, v) in e.variants.iter().enumerate() {
                self.out.push_str(&format!(
                    "    localparam [{}:0] {} = {};\n",
                    w - 1,
                    enum_const(&e.name.name, &v.name),
                    i
                ));
            }
        }

        // Declarations.
        for item in &m.items {
            match item {
                ModuleItem::Wire { name, ty, .. } => {
                    self.check_ascii(name);
                    let w = self.width(ty);
                    self.out.push_str(&format!("    wire {w}{};\n", name.name));
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
                _ => {}
            }
        }

        // Power-on init: seed every cell of each memory to its init value
        // (mirrors the simulator, which initializes all cells at construction).
        // Mandatory init value → no uninitialized state, without a per-cycle
        // reset (the `reset` line clears registers only).
        for item in &m.items {
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
        self.repeat_budget = REPEAT_BUDGET;
        self.emit_drives(&m.items);

        // Sequential blocks: one always per `on`, reset generated from
        // the reset values of the regs each block assigns.
        let reset_name = m.items.iter().find_map(|i| match i {
            ModuleItem::Reset { name: r, .. } => Some(r.name.clone()),
            _ => None,
        });
        // An async reset is added to every always-block's sensitivity list
        // (`@(… or posedge rst)`); a sync reset only acts on the clock edge.
        let async_reset = m
            .items
            .iter()
            .any(|i| matches!(i, ModuleItem::Reset { is_async: true, .. }));
        let regs: HashMap<&str, &Expr> = m
            .items
            .iter()
            .filter_map(|i| match i {
                ModuleItem::Reg { name, reset, .. } => Some((name.name.as_str(), reset)),
                _ => None,
            })
            .collect();

        for item in &m.items {
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

        self.out.push_str("endmodule\n");

        // Peel this module's consts back off; the next module in the file
        // sees only the file-level env.
        self.env = file_env;
    }

    /// Emit every instance in `items`, descending into `repeat` bodies and
    /// unrolling them (the loop variable is bound per iteration). Declared
    /// before drives so child-output wires exist when the drives use them.
    fn emit_instances(&mut self, items: &[ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Inst(inst) => self.instance(inst),
                ModuleItem::Repeat(r) => self.unroll(r, Self::emit_instances),
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
                ModuleItem::Wire { name, init, .. } => {
                    let rhs = self.expr(init);
                    self.out
                        .push_str(&format!("    assign {} = {};\n", name.name, rhs));
                }
                ModuleItem::Drive { lhs, rhs } => {
                    let l = self.lvalue(lhs);
                    let r = self.expr(rhs);
                    self.out.push_str(&format!("    assign {l} = {r};\n"));
                }
                ModuleItem::Repeat(r) => self.unroll(r, Self::emit_drives),
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
        let Some(child) = self.project.modules.get(&inst.module.name).copied() else {
            self.err(
                inst.module.span,
                format!("unknown module `{}`", inst.module.name),
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
            .get(&child.name.name)
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
        self.out.push_str(&format!(
            "    {}{} {} ({});\n",
            child.name.name,
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
            match second {
                Some(lo) => s.push_str(&format!(
                    "[{}:{}]",
                    self.index_expr(first, &empty),
                    self.index_expr(lo, &empty)
                )),
                None => s.push_str(&format!("[{}]", self.index_expr(first, &empty))),
            }
        }
        s
    }

    /// Verilog range like `[WIDTH-1:0] ` (with trailing space), or "" for bit.
    fn width(&mut self, ty: &Type) -> String {
        self.width_subst(ty, &HashMap::new())
    }

    /// Like [`Self::width`], but with child-module parameter names replaced
    /// by the instantiating module's argument expressions — used when
    /// declaring auto-wires for a child instance's outputs.
    fn width_subst(&mut self, ty: &Type, subst: &HashMap<&str, &Expr>) -> String {
        match ty {
            Type::Bit => String::new(),
            Type::Bits(e) => {
                let we = self.expr_subst(e, subst);
                format!("[({we})-1:0] ")
            }
            Type::Signed(e) => {
                // Declared `signed` so Verilog's native two's-complement
                // semantics apply: assignments SIGN-extend and comparisons
                // are signed. Sound because the checker forbids
                // signed/unsigned mixing inside one expression (E0403).
                let we = self.expr_subst(e, subst);
                format!("signed [({we})-1:0] ")
            }
            Type::Named(id) => {
                if let Some(e) = self.project.enums.get(&id.name) {
                    let w = clog2(e.variants.len());
                    format!("[{}:0] ", w - 1)
                } else {
                    self.err(
                        id.span,
                        format!(
                            "unknown type `{}` — not a built-in and not a declared enum",
                            id.name
                        ),
                        "",
                    );
                    String::new()
                }
            }
        }
    }
}

/// Every register name assigned anywhere in this statement tree (both `if`
/// branches included), deduplicated in first-seen order. Drives the
/// generated reset branch: only the regs an `on` block writes are reset
/// in its always-block.
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
            SeqStmt::Error(_) => {} // unreachable on the codegen path
        }
    }
}
