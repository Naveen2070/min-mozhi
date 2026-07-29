use super::module::elaborate_module;
use super::registry::resolve_target;
use super::*;

/// The flat pieces one instance contributes to its parent.
#[derive(Default)]
pub(super) struct Flat {
    pub(super) wires: Vec<Signal>,
    pub(super) regs: Vec<Reg>,
    pub(super) mems: Vec<Mem>,
    pub(super) comb: Vec<(String, Expr)>,
    pub(super) procs: Vec<Process>,
    /// Names of driverless signals (extern-instance outputs in `warn`
    /// [`SimMode`]) — see `Design::unknown_signals`.
    pub(super) unknown: Vec<String>,
}

impl Flat {
    pub(super) fn absorb(&mut self, other: Flat) {
        self.wires.extend(other.wires);
        self.regs.extend(other.regs);
        self.mems.extend(other.mems);
        self.comb.extend(other.comb);
        self.procs.extend(other.procs);
        self.unknown.extend(other.unknown);
    }
}

/// Elaborate the child module of `inst` and inline it into the parent: every
/// child signal becomes a parent wire/reg named `{inst}_{name}`, child inputs
/// are driven by their connection expressions, and child clock/reset map to the
/// connected parent signals. Mirrors the Verilog emitter's instance lowering so
/// the simulator agrees bit-for-bit.
#[allow(clippy::too_many_arguments)]
pub(super) fn flatten_instance(
    reg: &Registry,
    extern_reg: &ExternRegistry,
    func_reg: &FuncRegistry<'_>,
    bundle_reg: &BundleRegistry<'_>,
    enum_reg: &EnumRegistry<'_>,
    parent_imports: &[ast::Import],
    parent_consts: &BTreeMap<String, i128>,
    parent_insts: &HashSet<String>,
    parent_enums: &HashMap<String, &ast::EnumDecl>,
    parent_subst: &HashMap<String, Expr>,
    inst: &ast::Inst,
    iname: &str,
    depth: u32,
    mode: SimMode,
) -> Result<Flat, String> {
    let (cfile, cm) = match resolve_target(reg, extern_reg, parent_imports, &inst.module)
        .map_err(|e| format!("instance `{}` {e}", inst.name.name))?
    {
        (Some(f), ast::ModuleTarget::Real(m)) => (f, m),
        (None, ast::ModuleTarget::Extern(em)) => {
            return flatten_extern_instance(em, parent_consts, inst, iname, mode);
        }
        (_, target) => unreachable!(
            "resolve_target always pairs ModuleTarget::Real with Some(file) and \
             ModuleTarget::Extern with None — got is_extern={}",
            target.is_extern()
        ),
    };

    // Child parameter bindings: an explicit `arg` (evaluated in the PARENT's
    // consts) wins; otherwise the child default (in the child's own consts).
    let mut cp: BTreeMap<String, i128> = BTreeMap::new();
    for p in &cm.params {
        let v = if let Some(a) = inst.args.iter().find(|a| a.name.name == p.name.name) {
            const_eval(&a.value, parent_consts)?
        } else if let Some(d) = &p.default {
            const_eval(d, &cp)?
        } else {
            return Err(format!(
                "instance `{}`: parameter `{}` has no value",
                inst.name.name, p.name.name
            ));
        };
        cp.insert(p.name.name.clone(), v);
    }

    let child = elaborate_module(
        reg,
        extern_reg,
        func_reg,
        bundle_reg,
        enum_reg,
        cfile,
        cm,
        &cp,
        depth + 1,
        mode,
    )?;
    let pfx = format!("{iname}_");

    // Parent-context rewriter for connection expressions: folds the `repeat`
    // loop var and resolves nested `arr[i-1].port` reads.
    // Empty bundle_sigs: the child's own signals are already flattened to
    // scalars by this point, so there's no dot-access left to rewrite.
    let no_bundle_sigs: HashSet<String> = HashSet::new();
    let prw = Rw {
        insts: parent_insts,
        enums: parent_enums,
        bundle_sigs: &no_bundle_sigs,
        consts: parent_consts,
        subst: parent_subst,
    };

    // The child body is already flat (no `Field`/enum nodes survive its own
    // elaboration), so a subst-only rewriter suffices: child const → literal,
    // child signal → prefixed name, child clock/reset → connected parent signal.
    let no_insts: HashSet<String> = HashSet::new();
    let no_enums: HashMap<String, &ast::EnumDecl> = HashMap::new();
    let mut subst: HashMap<String, Expr> = HashMap::new();
    for (n, &v) in &child.consts {
        subst.insert(n.clone(), int_expr(v, inst.span));
    }
    for s in child
        .inputs
        .iter()
        .chain(&child.outputs)
        .chain(&child.wires)
    {
        subst.insert(
            s.name.clone(),
            ident_expr(format!("{pfx}{}", s.name), inst.span),
        );
    }
    for r in &child.regs {
        subst.insert(
            r.name.clone(),
            ident_expr(format!("{pfx}{}", r.name), inst.span),
        );
    }
    for mem in &child.mems {
        subst.insert(
            mem.name.clone(),
            ident_expr(format!("{pfx}{}", mem.name), inst.span),
        );
    }

    // Clock/reset: explicit connection, else the same-named parent signal.
    let mut clock_map: HashMap<String, String> = HashMap::new();
    for c in child.clocks.iter().chain(&child.resets) {
        let parent = inst
            .conns
            .iter()
            .find(|cn| cn.port.name == *c)
            .map(|cn| conn_signal_name(&prw.expr(&cn.signal)?))
            .transpose()?
            .unwrap_or_else(|| c.clone());
        subst.insert(c.clone(), ident_expr(parent.clone(), inst.span));
        clock_map.insert(c.clone(), parent);
    }

    let crw = Rw {
        insts: &no_insts,
        enums: &no_enums,
        bundle_sigs: &no_bundle_sigs,
        consts: &child.consts,
        subst: &subst,
    };
    let mut flat = Flat::default();

    // Child inputs: a parent wire driven by the (required) connection.
    for s in &child.inputs {
        let conn = inst
            .conns
            .iter()
            .find(|cn| cn.port.name == s.name)
            .ok_or_else(|| {
                format!(
                    "instance `{}`: input `{}` of `{}` is not connected",
                    inst.name.name, s.name, cm.name.name
                )
            })?;
        flat.wires.push(Signal {
            name: format!("{pfx}{}", s.name),
            width: s.width,
        });
        flat.comb
            .push((format!("{pfx}{}", s.name), prw.expr(&conn.signal)?));
    }
    // Child outputs + wires: a parent wire driven by the child's (rewritten) logic.
    // A child wire/output with no `comb` driver is, by construction, one the
    // child itself marked as unknown-tainted (an extern-instance output read
    // through, however many levels deep) — copy it into the parent's own
    // `unknown` set (not just drop it) so a grandparent that re-exposes it
    // still finds a live wire + taint marker instead of a dangling name.
    for s in child.outputs.iter().chain(&child.wires) {
        if let Some(drv) = child.comb.get(&s.name) {
            flat.wires.push(Signal {
                name: format!("{pfx}{}", s.name),
                width: s.width,
            });
            flat.comb.push((format!("{pfx}{}", s.name), crw.expr(drv)?));
        } else if child.unknown_signals.contains(&s.name) {
            let flat_name = format!("{pfx}{}", s.name);
            flat.wires.push(Signal {
                name: flat_name.clone(),
                width: s.width,
            });
            flat.unknown.push(flat_name);
        }
    }
    // Child registers (clock filled by the parent's reg-clock pass).
    for r in &child.regs {
        flat.regs.push(Reg {
            name: format!("{pfx}{}", r.name),
            width: r.width,
            reset: r.reset.clone(),
            clock: String::new(),
            edge: r.edge,
        });
    }
    // Child memories (clock filled by the parent's clock-binding pass).
    for mem in &child.mems {
        flat.mems.push(Mem {
            name: format!("{pfx}{}", mem.name),
            width: mem.width,
            depth: mem.depth,
            init: mem.init.clone(),
            clock: String::new(),
            edge: mem.edge,
        });
    }
    // Child processes: prefix assigned regs, rewrite bodies, map the clock.
    for p in &child.procs {
        let clk = clock_map
            .get(&p.clock)
            .cloned()
            .unwrap_or_else(|| p.clock.clone());
        let rename = |n: &str| format!("{pfx}{n}");
        flat.procs.push(Process {
            clock: clk,
            edge: p.edge,
            body: p
                .body
                .iter()
                .map(|s| crw.seq(s, &rename))
                .collect::<Result<_, _>>()?,
        });
    }
    Ok(flat)
}

/// Handle an extern-module instance: it has no body, so there's nothing to
/// recursively elaborate. `strict` mode refuses to simulate around missing
/// hardware behavior; `warn` mode lowers every output port to an
/// unconstrained (`Val::unknown`) read and prints one warning per instance
/// (this function runs exactly once per `Inst` node during elaboration, so
/// "once per distinct instance" falls out for free — no dedup bookkeeping
/// needed).
fn flatten_extern_instance(
    em: &ast::ExternModule,
    parent_consts: &BTreeMap<String, i128>,
    inst: &ast::Inst,
    iname: &str,
    mode: SimMode,
) -> Result<Flat, String> {
    if mode == SimMode::Strict {
        return Err(format!(
            "instance `{}` instantiates extern module `{}` — no simulation model; \
             extern modules are Verilog-emission only (pass a `warn`-mode config to \
             simulate around it)",
            inst.name.name, em.name.name
        ));
    }
    eprintln!(
        "warning: instance `{}` instantiates extern module `{}` — its output(s) \
         are unconstrained (X) in simulation; only Verilog emission models its real \
         behavior",
        inst.name.name, em.name.name
    );

    // Child parameter bindings — same precedence as a real module's instance
    // (an explicit `arg` wins, else the extern's own default), needed to
    // fold a param-dependent output width (e.g. `out y: bits[WIDTH]`).
    let mut cp: BTreeMap<String, i128> = BTreeMap::new();
    for p in &em.params {
        let v = if let Some(a) = inst.args.iter().find(|a| a.name.name == p.name.name) {
            const_eval(&a.value, parent_consts)?
        } else if let Some(d) = &p.default {
            const_eval(d, &cp)?
        } else {
            return Err(format!(
                "instance `{}`: parameter `{}` has no value",
                inst.name.name, p.name.name
            ));
        };
        cp.insert(p.name.name.clone(), v);
    }

    let pfx = format!("{iname}_");
    let mut flat = Flat::default();
    // Extern ports are scalar-only (bit/bits[N]/signed[N]) — the checker
    // enforces this on the declaration (Task 3), so `type_width` (the same
    // width-resolution helper the real child-elaboration path uses) never
    // hits its enum/bundle/array error arms here.
    for it in &em.items {
        if let ModuleItem::Port {
            dir: Dir::Out,
            name,
            ty,
        } = it
        {
            let (bits, signed) = type_width(ty, &cp)?;
            let flat_name = format!("{pfx}{}", name.name);
            flat.wires.push(Signal {
                name: flat_name.clone(),
                width: Width { bits, signed },
            });
            flat.unknown.push(flat_name);
        }
    }
    Ok(flat)
}
