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
                ModuleItem::Reset(r) => ports.push(format!("input wire {}", r.name)),
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
                _ => {}
            }
        }

        // Instances: auto-wire every child output as `{inst}_{port}`.
        for item in &m.items {
            if let ModuleItem::Inst(inst) = item {
                self.instance(inst);
            }
        }

        // Combinational drives.
        for item in &m.items {
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
                ModuleItem::Repeat(r) => {
                    self.err(
                        r.span,
                        "`repeat` is not yet supported by the Verilog emitter",
                        "compile-time unrolling lands with the const-eval pass (Phase 1 work item 4)",
                    );
                }
                _ => {}
            }
        }

        // Sequential blocks: one always per `on`, reset generated from
        // the reset values of the regs each block assigns.
        let reset_name = m.items.iter().find_map(|i| match i {
            ModuleItem::Reset(r) => Some(r.name.clone()),
            _ => None,
        });
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

                self.out
                    .push_str(&format!("    always @(posedge {}) begin\n", on.clock.name));
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

        // Substitute child param names with this instance's args inside
        // child port-width expressions.
        let args: HashMap<&str, &Expr> = inst
            .args
            .iter()
            .map(|a| (a.name.name.as_str(), &a.value))
            .collect();

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
                ModuleItem::Reset(rstp) => {
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
                        let wire_name = format!("{}_{}", inst.name.name, name.name);
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
                ModuleItem::Clock(n) | ModuleItem::Reset(n) => n.name == c.port.name,
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
            inst.name.name,
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
            }
        }
    }

    /// Render an assignment target: `name`, `name[i]`, or `name[hi:lo]`.
    fn lvalue(&mut self, lv: &LValue) -> String {
        let mut s = lv.base.name.clone();
        if let Some((first, second)) = &lv.index {
            match second {
                Some(lo) => s.push_str(&format!("[{}:{}]", self.expr(first), self.expr(lo))),
                None => s.push_str(&format!("[{}]", self.expr(first))),
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
            Type::Bits(e) | Type::Signed(e) => {
                let we = self.expr_subst(e, subst);
                format!("[({we})-1:0] ")
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
        }
    }
}
