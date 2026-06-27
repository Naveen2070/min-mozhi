//! Elaboration (Phase 1.5, step B1): turn one AST module plus concrete
//! parameter values into a flat [`Design`] — signals with their widths folded
//! to concrete numbers, registers with their (mandatory, compile-time) reset
//! values folded, the combinational drivers, and the sequential processes.
//! The event-driven kernel (next step) interprets a `Design`; it never walks
//! the AST shape again.
//!
//! Reset is **synthesized**, exactly as the Verilog emitter does it: a `reg`
//! carries a reset value and the module declares `reset rst`, while the `on`
//! block body holds only the non-reset logic. The kernel applies `reset → the
//! folded reset value, else → the on-block result` so its results match the
//! emitted Verilog (the differential oracle).
//!
//! Full structural elaboration, mirroring the Verilog emitter so the flat
//! `Design` matches the emitted hardware: module **instances are flattened**
//! (C2, signals name-prefixed `inst.port` → `inst_port`), **`repeat` is
//! unrolled** (C3, array instances `arr__i`, bit-indexed drives assembled into a
//! Concat), and **enum-typed signals** are encoded by variant index with width
//! `clog2(variants)` (C4, variant reads/patterns → their index). Const/width
//! folding is shared with the combinational evaluator ([`super::comb`]).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::{self, Dir, Edge, Expr, ExprKind, ModuleItem, Pattern, SeqStmt, UnOp};

use super::value::{const_eval, pick_module, type_width};

/// Max `repeat` iterations the simulator will unroll — the same crate-root
/// constant the emitter uses, so a design that compiles also elaborates (the
/// simulator is the emitter's differential oracle). See [`crate::REPEAT_BUDGET`].
use crate::REPEAT_BUDGET;

/// Max instance-nesting depth the simulator will flatten. `mimz sim`/`mimz test`
/// run on the parsed AST WITHOUT the checker (which has its own recursion guard),
/// so a recursive/cyclic instantiation (`module A { let u = A() … }`, or A→B→A)
/// would otherwise recurse until the stack overflows and the process aborts. This
/// bound turns that into a clean error — the simulator's analogue of the parser's
/// `MAX_DEPTH` and the emitter's `REPEAT_BUDGET` (see SEC-6 in docs/audit).
///
/// Kept deliberately small: each level is a large `elaborate_module` +
/// `flatten_instance` stack frame, and the bound must fire well within the 1 MB
/// default main-thread stack on Windows. Real hardware nests instances only a few
/// levels deep, so 16 is generous for valid designs while staying crash-safe.
const MAX_INSTANCE_DEPTH: u32 = 16;

/// Module registry across all loaded files: name → (its file, its AST). Module
/// names are project-unique (checker-enforced), so a child instance resolves by
/// name regardless of which imported file defines it.
type Registry<'a> = HashMap<String, (&'a ast::File, &'a ast::Module)>;

fn build_registry(files: &[ast::File]) -> Registry<'_> {
    let mut reg = HashMap::new();
    for f in files {
        for it in &f.items {
            if let ast::TopItem::Module(m) = it {
                reg.insert(m.name.name.clone(), (f, m));
            }
        }
    }
    reg
}

/// A signal's concrete type after width folding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Width {
    pub bits: u32,
    pub signed: bool,
}

/// An input, output, or wire with its folded width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signal {
    pub name: String,
    pub width: Width,
}

/// A register: its width, its folded compile-time reset value (the kernel
/// masks it to `width`), and the clock whose rising edge updates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reg {
    pub name: String,
    pub width: Width,
    pub reset: i128,
    /// The clock of the `on` block that assigns this reg (empty if none does,
    /// in which case the reg simply holds its reset value forever).
    pub clock: String,
    /// The edge of the assigning `on` block (`rise`/`fall`). Defaults to `Rise`
    /// for an unassigned reg (it never ticks).
    pub edge: Edge,
}

/// A memory: an array of `depth` cells, each `width` bits, seeded to the folded
/// `init` value at construction (power-on init). Read combinationally
/// (`m[addr]`) and written on `clock`'s `edge` (`m[addr] <- v`); a memory with
/// no writing `on` block is a read-only ROM holding `init`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mem {
    pub name: String,
    pub width: Width,
    pub depth: u128,
    pub init: i128,
    /// The clock of the `on` block that writes this memory (empty if none does).
    pub clock: String,
    /// The edge of the writing `on` block (`rise`/`fall`).
    pub edge: Edge,
}

/// One sequential process — the body of an `on rise(clock)` block. The kernel
/// interprets `body` each rising edge of `clock` (after the synthesized reset
/// branch). Registers left unassigned on a path hold their current value.
#[derive(Clone, Debug)]
pub struct Process {
    pub clock: String,
    /// The edge this block triggers on (`on rise`/`on fall`).
    pub edge: Edge,
    pub body: Vec<SeqStmt>,
}

/// A fully elaborated single module: a flat signal/process graph with all
/// parameters and widths folded to concrete values.
#[derive(Clone, Debug)]
pub struct Design {
    pub module: String,
    /// Folded compile-time integers (params + consts) — for the const
    /// expressions (indices, slice bounds) the kernel still evaluates.
    pub consts: BTreeMap<String, i128>,
    pub inputs: Vec<Signal>,
    pub outputs: Vec<Signal>,
    pub wires: Vec<Signal>,
    pub regs: Vec<Reg>,
    /// Memories (RAM/register arrays), seeded at construction.
    pub mems: Vec<Mem>,
    /// Combinational drivers: signal name → driving expression. Covers wire
    /// `init` and `out = expr` drives (outputs and wires only; never regs).
    pub comb: BTreeMap<String, Expr>,
    /// Sequential processes, one per `on` block.
    pub procs: Vec<Process>,
    /// Declared clock signal names.
    pub clocks: Vec<String>,
    /// Declared reset signal names (synchronous, active-high).
    pub resets: Vec<String>,
}

/// Elaborate `module` (or the file's only module when `module` is `None`) into a
/// flat [`Design`]. Single-file entry point: a module that instantiates a
/// sub-module defined in ANOTHER file needs [`elaborate_project`] (so the
/// imported file is available). Handles instances, `repeat`, and enum signals.
pub fn elaborate(
    file: &ast::File,
    module: Option<&str>,
    params: &BTreeMap<String, i128>,
) -> Result<Design, String> {
    elaborate_project(std::slice::from_ref(file), module, params)
}

/// Elaborate the entry module across a loaded project (`files[0]` is the entry,
/// the rest are its imports — the order [`crate::project::load_project`] returns).
/// Instances are **flattened**: each child is elaborated and inlined into the
/// parent with its signals name-prefixed (`inst.port` → wire `inst_port`,
/// matching the Verilog emitter), so the flat [`Design`] the kernel runs is
/// equivalent to the emitted Verilog.
pub fn elaborate_project(
    files: &[ast::File],
    module: Option<&str>,
    params: &BTreeMap<String, i128>,
) -> Result<Design, String> {
    let reg = build_registry(files);
    let entry = files.first().ok_or("no files to elaborate")?;
    let m = pick_module(entry, module)?;
    elaborate_module(&reg, entry, m, params, 0)
}

/// Elaborate one module (`m`, defined in `file`) under concrete `params`,
/// resolving any instantiated children through `reg`.
fn elaborate_module(
    reg: &Registry,
    file: &ast::File,
    m: &ast::Module,
    params: &BTreeMap<String, i128>,
    depth: u32,
) -> Result<Design, String> {
    // Guard against recursive/cyclic instantiation (the checker would catch it,
    // but `mimz sim`/`test` skip the checker) — bound the stack, fail cleanly.
    if depth > MAX_INSTANCE_DEPTH {
        return Err(format!(
            "instance nesting exceeds {MAX_INSTANCE_DEPTH} levels in `{}` — \
             a module likely instantiates itself (directly or in a cycle)",
            m.name.name
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
                    return Err(format!(
                        "parameter `{}` has no default — provide a value for it",
                        p.name.name
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

    // Module-level enums: name → ordered variant names. A variant encodes as its
    // index, width `clog2(count)` — exactly the Verilog emitter's encoding.
    let enums: HashMap<String, Vec<String>> = m
        .items
        .iter()
        .filter_map(|it| match it {
            ModuleItem::Enum(e) => Some((
                e.name.name.clone(),
                e.variants.iter().map(|v| v.name.clone()).collect(),
            )),
            _ => None,
        })
        .collect();

    // Instance names (top-level AND inside `repeat`), so `inst.port` and the
    // array form `arr[i].port` rewrite to their flat wire names.
    let mut insts: HashSet<String> = HashSet::new();
    collect_inst_names(&m.items, &mut insts);

    // Folded width of a type — an enum type resolves to `clog2(variants)`.
    let width_of = |ty: &ast::Type, ints: &BTreeMap<String, i128>| -> Result<Width, String> {
        if let ast::Type::Named(n) = ty {
            let vs = enums
                .get(&n.name)
                .ok_or_else(|| format!("unknown enum type `{}`", n.name))?;
            Ok(Width {
                bits: clog2(vs.len()),
                signed: false,
            })
        } else {
            let (bits, signed) = type_width(ty, ints)?;
            Ok(Width { bits, signed })
        }
    };

    let no_subst: HashMap<String, Expr> = HashMap::new();
    let rw0 = Rw {
        insts: &insts,
        enums: &enums,
        consts: &consts,
        subst: &no_subst,
    };

    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut wires = Vec::new();
    let mut regs = Vec::new();
    let mut mems = Vec::new();
    let mut comb: BTreeMap<String, Expr> = BTreeMap::new();
    let mut procs = Vec::new();
    let mut clocks = Vec::new();
    let mut resets = Vec::new();
    // Bit-indexed drives (`sum[i] = …`, from `repeat`), assembled into one
    // whole-signal Concat driver after the loop.
    let mut bit_drives: BTreeMap<String, BTreeMap<u32, Expr>> = BTreeMap::new();
    let mut flat = Flat::default();

    for it in &m.items {
        match it {
            ModuleItem::Port { dir, name, ty } => {
                let sig = Signal {
                    name: name.name.clone(),
                    width: width_of(ty, &consts)?,
                };
                match dir {
                    Dir::In => inputs.push(sig),
                    Dir::Out => outputs.push(sig),
                }
            }
            ModuleItem::Clock(n) => clocks.push(n.name.clone()),
            // The cycle-based kernel applies reset at the clock edge; an async
            // reset is observationally identical at the per-cycle sample points
            // (sub-cycle timing is out of the kernel's model), so `is_async` is
            // an emitter-only distinction and the sim just records the name. The
            // path to modeling sub-cycle reset timing is the three-tier fidelity
            // roadmap in docs/plan/phase-1.5-simulator.md (currently Tier 3:
            // delegate timing-faithful runs to the Verilog/Icarus oracle).
            ModuleItem::Reset { name: n, .. } => resets.push(n.name.clone()),
            ModuleItem::Wire { name, ty, init } => {
                wires.push(Signal {
                    name: name.name.clone(),
                    width: width_of(ty, &consts)?,
                });
                comb.insert(name.name.clone(), rw0.expr(init)?);
            }
            ModuleItem::Reg { name, ty, reset } => {
                let width = width_of(ty, &consts)?;
                let reset = const_eval(&rw0.expr(reset)?, &consts)?;
                regs.push(Reg {
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
                let width = width_of(ty, &consts)?;
                // The sim runs WITHOUT the checker, so guard the depth here too.
                let d = const_eval(&rw0.expr(depth)?, &consts)?;
                let depth = u128::try_from(d).ok().filter(|d| *d >= 1).ok_or_else(|| {
                    format!("memory `{}` has a non-positive depth ({d})", name.name)
                })?;
                let init = const_eval(&rw0.expr(init)?, &consts)?;
                mems.push(Mem {
                    name: name.name.clone(),
                    width,
                    depth,
                    init,
                    clock: String::new(),
                    edge: Edge::Rise,
                });
            }
            ModuleItem::Drive { lhs, rhs } => {
                record_drive(lhs, rhs, &rw0, &consts, &mut comb, &mut bit_drives)?
            }
            ModuleItem::On(on) => procs.push(Process {
                clock: on.clock.name.clone(),
                edge: on.edge,
                body: on
                    .body
                    .iter()
                    .map(|s| rw0.seq(s, &|n| n.to_string()))
                    .collect::<Result<_, _>>()?,
            }),
            // Consts are folded above; enum decls become the encoding above.
            ModuleItem::Const(_) | ModuleItem::Enum(_) => {}
            // Unreachable: elaboration runs on a strict-parsed tree, which
            // carries no `Error` placeholder.
            ModuleItem::Error(_) => {}
            ModuleItem::Inst(inst) => {
                let f = flatten_instance(
                    reg,
                    &consts,
                    &insts,
                    &enums,
                    &no_subst,
                    inst,
                    &inst.name.name,
                    depth,
                )?;
                flat.absorb(f);
            }
            ModuleItem::Repeat(r) => {
                let lo = const_eval(&r.lo, &consts)?;
                let hi = const_eval(&r.hi, &consts)?;
                // `checked_sub`: extreme bounds (`hi - lo` past i128::MAX) must not
                // overflow-panic — treat an out-of-range span as over-budget.
                let count = hi.checked_sub(lo).unwrap_or(i128::MAX).max(0);
                if count > REPEAT_BUDGET {
                    return Err(format!(
                        "`repeat` would unroll {count} times, over the limit of {REPEAT_BUDGET}"
                    ));
                }
                for iv in lo..hi {
                    let mut ci = consts.clone();
                    ci.insert(r.var.name.clone(), iv);
                    let subst = HashMap::from([(r.var.name.clone(), int_expr(iv, r.span))]);
                    let rwi = Rw {
                        insts: &insts,
                        enums: &enums,
                        consts: &ci,
                        subst: &subst,
                    };
                    for body_it in &r.items {
                        match body_it {
                            ModuleItem::Inst(inst) => {
                                let iname = match &inst.index {
                                    Some(_) => format!("{}__{}", inst.name.name, iv),
                                    None => inst.name.name.clone(),
                                };
                                let f = flatten_instance(
                                    reg, &ci, &insts, &enums, &subst, inst, &iname, depth,
                                )?;
                                flat.absorb(f);
                            }
                            ModuleItem::Drive { lhs, rhs } => {
                                record_drive(lhs, rhs, &rwi, &ci, &mut comb, &mut bit_drives)?
                            }
                            ModuleItem::Repeat(_) => {
                                return Err(
                                    "nested `repeat` is not supported by the simulator yet".into(),
                                );
                            }
                            _ => {
                                return Err(
                                    "a `repeat` body may only contain instances and drives".into(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Merge inlined-instance pieces, then assemble bit-indexed drives into one
    // whole-signal Concat (widest bit first, Verilog concat order).
    wires.extend(flat.wires);
    regs.extend(flat.regs);
    // Merge instance drivers, erroring on a name collision instead of silently
    // overwriting (a parent signal named like a flattened `inst_port` wire).
    for (name, driver) in flat.comb {
        if comb.insert(name.clone(), driver).is_some() {
            return Err(format!(
                "flattened instance signal `{name}` collides with an existing signal in `{}`",
                m.name.name
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
            .ok_or_else(|| format!("bit-driven signal `{sig}` has no declaration"))?;
        let mut parts = Vec::with_capacity(width as usize);
        for b in (0..width).rev() {
            let e = bits
                .get(&b)
                .ok_or_else(|| format!("signal `{sig}` bit {b} is not driven"))?;
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
    })
}

/// The flat pieces one instance contributes to its parent.
#[derive(Default)]
struct Flat {
    wires: Vec<Signal>,
    regs: Vec<Reg>,
    mems: Vec<Mem>,
    comb: Vec<(String, Expr)>,
    procs: Vec<Process>,
}

impl Flat {
    fn absorb(&mut self, other: Flat) {
        self.wires.extend(other.wires);
        self.regs.extend(other.regs);
        self.mems.extend(other.mems);
        self.comb.extend(other.comb);
        self.procs.extend(other.procs);
    }
}

/// Elaborate the child module of `inst` and inline it into the parent: every
/// child signal becomes a parent wire/reg named `{inst}_{name}`, child inputs
/// are driven by their connection expressions, and child clock/reset map to the
/// connected parent signals. Mirrors the Verilog emitter's instance lowering so
/// the simulator agrees bit-for-bit.
#[allow(clippy::too_many_arguments)]
fn flatten_instance(
    reg: &Registry,
    parent_consts: &BTreeMap<String, i128>,
    parent_insts: &HashSet<String>,
    parent_enums: &HashMap<String, Vec<String>>,
    parent_subst: &HashMap<String, Expr>,
    inst: &ast::Inst,
    iname: &str,
    depth: u32,
) -> Result<Flat, String> {
    let (cfile, cm) = *reg.get(&inst.module.name).ok_or_else(|| {
        format!(
            "instance `{}` uses unknown module `{}` — is the file that defines it imported?",
            inst.name.name, inst.module.name
        )
    })?;

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

    let child = elaborate_module(reg, cfile, cm, &cp, depth + 1)?;
    let pfx = format!("{iname}_");

    // Parent-context rewriter for connection expressions: folds the `repeat`
    // loop var and resolves nested `arr[i-1].port` reads.
    let prw = Rw {
        insts: parent_insts,
        enums: parent_enums,
        consts: parent_consts,
        subst: parent_subst,
    };

    // The child body is already flat (no `Field`/enum nodes survive its own
    // elaboration), so a subst-only rewriter suffices: child const → literal,
    // child signal → prefixed name, child clock/reset → connected parent signal.
    let no_insts = HashSet::new();
    let no_enums = HashMap::new();
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
    for s in child.outputs.iter().chain(&child.wires) {
        if let Some(drv) = child.comb.get(&s.name) {
            flat.wires.push(Signal {
                name: format!("{pfx}{}", s.name),
                width: s.width,
            });
            flat.comb.push((format!("{pfx}{}", s.name), crw.expr(drv)?));
        }
    }
    // Child registers (clock filled by the parent's reg-clock pass).
    for r in &child.regs {
        flat.regs.push(Reg {
            name: format!("{pfx}{}", r.name),
            width: r.width,
            reset: r.reset,
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
            init: mem.init,
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

/// A clock/reset connection must be a plain signal name.
fn conn_signal_name(e: &Expr) -> Result<String, String> {
    match &e.kind {
        ExprKind::Ident(n) => Ok(n.clone()),
        _ => Err("a clock/reset connection must be a plain signal name".into()),
    }
}

fn ident_expr(name: String, span: crate::span::Span) -> Expr {
    Expr {
        kind: ExprKind::Ident(name),
        span,
    }
}

fn int_expr(v: i128, span: crate::span::Span) -> Expr {
    if v >= 0 {
        return Expr {
            kind: ExprKind::Int {
                value: v as u128,
                raw: v.to_string(),
            },
            span,
        };
    }
    // Negative: emit `-<magnitude>`. Use `unsigned_abs` (not `-v`) so the one
    // value whose magnitude does not fit `i128` — `i128::MIN`, magnitude 2^127 —
    // is representable in the `u128` literal instead of overflow-panicking the
    // negation. `i128::MIN` is reachable on the unchecked sim path: a child
    // const can evaluate to it via checked arithmetic (e.g. `(-i128::MAX) - 1`),
    // and every flattened const passes through here.
    let mag = v.unsigned_abs();
    Expr {
        kind: ExprKind::Unary {
            op: UnOp::Neg,
            expr: Box::new(Expr {
                kind: ExprKind::Int {
                    value: mag,
                    raw: mag.to_string(),
                },
                span,
            }),
        },
        span,
    }
}

/// Collect every instance name (top-level and inside `repeat`), so an
/// instance-port read resolves whether the instance is plain or an array.
fn collect_inst_names(items: &[ModuleItem], out: &mut HashSet<String>) {
    for it in items {
        match it {
            ModuleItem::Inst(i) => {
                out.insert(i.name.name.clone());
            }
            ModuleItem::Repeat(r) => collect_inst_names(&r.items, out),
            _ => {}
        }
    }
}

/// Record one combinational drive. A whole-signal drive becomes a `comb` entry;
/// a bit-indexed drive (`sum[i] = …`, from `repeat`) is collected per bit and
/// assembled into a Concat after the item loop. Slice drives are not yet handled.
fn record_drive(
    lhs: &ast::LValue,
    rhs: &Expr,
    rw: &Rw,
    consts: &BTreeMap<String, i128>,
    comb: &mut BTreeMap<String, Expr>,
    bit_drives: &mut BTreeMap<String, BTreeMap<u32, Expr>>,
) -> Result<(), String> {
    match &lhs.index {
        None => {
            comb.insert(lhs.base.name.clone(), rw.expr(rhs)?);
        }
        Some((idx, None)) => {
            let bit = const_eval(&rw.expr(idx)?, consts)?;
            // Bound to the evaluator's max width BEFORE `as u32`, so an oversized
            // index can't silently truncate into a valid bit.
            if !(0..128).contains(&bit) {
                return Err(format!(
                    "bit index {bit} driving `{}` is out of range (0..128)",
                    lhs.base.name
                ));
            }
            bit_drives
                .entry(lhs.base.name.clone())
                .or_default()
                .insert(bit as u32, rw.expr(rhs)?);
        }
        Some((_, Some(_))) => {
            return Err(format!(
                "driving a slice of `{}` is not supported by the simulator yet",
                lhs.base.name
            ));
        }
    }
    Ok(())
}

/// `clog2` matching the Verilog emitter and the `clog2` const-builtin: the bit
/// width of an `n`-variant enum encoding (one source of truth, so they agree).
fn clog2(n: usize) -> u32 {
    crate::checker::consteval::clog2_bits(n as u128)
}

/// Rewrites expressions/statements during elaboration: enum-variant reads
/// (`State.Red`) → their index literal, instance-port reads (`add.sum`,
/// `fa[i].sum`) → the flat wire name, `match` variant patterns → their index,
/// plus a name substitution map (the `repeat` loop var and inlined child
/// signals). `consts` folds an array index to its concrete value.
struct Rw<'a> {
    insts: &'a HashSet<String>,
    enums: &'a HashMap<String, Vec<String>>,
    consts: &'a BTreeMap<String, i128>,
    subst: &'a HashMap<String, Expr>,
}

impl Rw<'_> {
    fn expr(&self, e: &Expr) -> Result<Expr, String> {
        let kind = match &e.kind {
            ExprKind::Int { .. } | ExprKind::Bool(_) => e.kind.clone(),
            ExprKind::Ident(n) => {
                if let Some(r) = self.subst.get(n) {
                    return Ok(r.clone());
                }
                e.kind.clone()
            }
            ExprKind::Field { base, field } => return self.field(e, base, field),
            ExprKind::Unary { op, expr } => ExprKind::Unary {
                op: *op,
                expr: Box::new(self.expr(expr)?),
            },
            ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
                op: *op,
                lhs: Box::new(self.expr(lhs)?),
                rhs: Box::new(self.expr(rhs)?),
            },
            ExprKind::IfExpr { cond, then, els } => ExprKind::IfExpr {
                cond: Box::new(self.expr(cond)?),
                then: Box::new(self.expr(then)?),
                els: Box::new(self.expr(els)?),
            },
            ExprKind::Match { scrutinee, arms } => ExprKind::Match {
                scrutinee: Box::new(self.expr(scrutinee)?),
                arms: arms
                    .iter()
                    .map(|a| {
                        Ok::<_, String>(ast::Arm {
                            patterns: a
                                .patterns
                                .iter()
                                .map(|p| self.pattern(p))
                                .collect::<Result<_, _>>()?,
                            value: self.expr(&a.value)?,
                        })
                    })
                    .collect::<Result<_, _>>()?,
            },
            ExprKind::Concat(parts) => ExprKind::Concat(
                parts
                    .iter()
                    .map(|p| self.expr(p))
                    .collect::<Result<_, _>>()?,
            ),
            ExprKind::Replicate { count, parts } => ExprKind::Replicate {
                count: Box::new(self.expr(count)?),
                parts: parts
                    .iter()
                    .map(|p| self.expr(p))
                    .collect::<Result<_, _>>()?,
            },
            ExprKind::Index { base, index } => ExprKind::Index {
                base: Box::new(self.expr(base)?),
                index: Box::new(self.expr(index)?),
            },
            ExprKind::Slice { base, hi, lo } => ExprKind::Slice {
                base: Box::new(self.expr(base)?),
                hi: Box::new(self.expr(hi)?),
                lo: Box::new(self.expr(lo)?),
            },
            ExprKind::Call { func, args } => ExprKind::Call {
                func: *func,
                args: args
                    .iter()
                    .map(|a| self.expr(a))
                    .collect::<Result<_, _>>()?,
            },
            // ponytail: temporary arm — FnCall elaborator lands in a later task
            ExprKind::FnCall { .. } => {
                return Err("user-defined functions are not yet supported by the simulator".into());
            }
        };
        Ok(Expr { kind, span: e.span })
    }

    fn field(&self, e: &Expr, base: &Expr, field: &ast::Ident) -> Result<Expr, String> {
        if let ExprKind::Ident(b) = &base.kind {
            // `Enum.Variant` → its index literal.
            if let Some(vs) = self.enums.get(b) {
                let idx = vs
                    .iter()
                    .position(|v| v == &field.name)
                    .ok_or_else(|| format!("enum `{b}` has no variant `{}`", field.name))?;
                return Ok(int_expr(idx as i128, e.span));
            }
            // `inst.port` → flat wire `inst_port`.
            if self.insts.contains(b) {
                return Ok(ident_expr(format!("{b}_{}", field.name), e.span));
            }
        }
        // `arr[i].port` → flat wire `arr__<i>_port` (the index folds here).
        if let ExprKind::Index { base: arr, index } = &base.kind
            && let ExprKind::Ident(arr_name) = &arr.kind
            && self.insts.contains(arr_name)
        {
            let n = const_eval(&self.expr(index)?, self.consts)?;
            return Ok(ident_expr(
                format!("{arr_name}__{n}_{}", field.name),
                e.span,
            ));
        }
        // Some other field access — keep the shape (recurse into the base).
        Ok(Expr {
            kind: ExprKind::Field {
                base: Box::new(self.expr(base)?),
                field: field.clone(),
            },
            span: e.span,
        })
    }

    fn pattern(&self, p: &Pattern) -> Result<Pattern, String> {
        match p {
            Pattern::Variant { enum_name, variant } => {
                let vs = self
                    .enums
                    .get(&enum_name.name)
                    .ok_or_else(|| format!("unknown enum `{}`", enum_name.name))?;
                let idx = vs.iter().position(|v| v == &variant.name).ok_or_else(|| {
                    format!(
                        "enum `{}` has no variant `{}`",
                        enum_name.name, variant.name
                    )
                })?;
                Ok(Pattern::Int {
                    value: idx as u128,
                    raw: idx.to_string(),
                })
            }
            other => Ok(other.clone()),
        }
    }

    /// Rewrite a sequential statement, renaming each assignment target via
    /// `rename` (prefixing inlined child regs; identity for the parent).
    fn seq(&self, s: &SeqStmt, rename: &dyn Fn(&str) -> String) -> Result<SeqStmt, String> {
        Ok(match s {
            SeqStmt::Assign { lhs, rhs } => SeqStmt::Assign {
                lhs: ast::LValue {
                    base: ast::Ident {
                        name: rename(&lhs.base.name),
                        span: lhs.base.span,
                    },
                    index: match &lhs.index {
                        Some((a, b)) => {
                            Some((self.expr(a)?, b.as_ref().map(|x| self.expr(x)).transpose()?))
                        }
                        None => None,
                    },
                    span: lhs.span,
                },
                rhs: self.expr(rhs)?,
            },
            SeqStmt::If { cond, then, els } => SeqStmt::If {
                cond: self.expr(cond)?,
                then: then
                    .iter()
                    .map(|x| self.seq(x, rename))
                    .collect::<Result<_, _>>()?,
                els: match els {
                    Some(e) => Some(
                        e.iter()
                            .map(|x| self.seq(x, rename))
                            .collect::<Result<_, _>>()?,
                    ),
                    None => None,
                },
            },
            // Unreachable on the sim path (strict-parsed tree); pass through.
            SeqStmt::Error(sp) => SeqStmt::Error(*sp),
        })
    }
}

/// Does this sequential body assign register `name` on any path (including
/// inside `if`/`else`)?
fn assigns(body: &[SeqStmt], name: &str) -> bool {
    body.iter().any(|s| match s {
        SeqStmt::Assign { lhs, .. } => lhs.base.name == name,
        SeqStmt::If { then, els, .. } => {
            assigns(then, name) || els.as_deref().is_some_and(|e| assigns(e, name))
        }
        SeqStmt::Error(_) => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ast::File {
        crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses")
    }

    fn design(src: &str) -> Design {
        elaborate(&parse(src), None, &BTreeMap::new()).expect("elaborates")
    }

    const COUNTER: &str = "module Counter(WIDTH: int = 8) {\n  \
        clock clk\n  reset rst\n  out count: bits[WIDTH]\n  \
        reg value: bits[WIDTH] = 0\n  on rise(clk) { value <- value +% 1 }\n  \
        count = value\n}\n";

    #[test]
    fn elaborates_the_counter() {
        let d = design(COUNTER);
        assert_eq!(d.module, "Counter");
        assert_eq!(d.consts["WIDTH"], 8);
        assert_eq!(d.inputs, vec![]);
        assert_eq!(
            d.outputs,
            vec![Signal {
                name: "count".into(),
                width: Width {
                    bits: 8,
                    signed: false
                }
            }]
        );
        assert_eq!(
            d.regs,
            vec![Reg {
                name: "value".into(),
                width: Width {
                    bits: 8,
                    signed: false
                },
                reset: 0,
                clock: "clk".into(),
                edge: Edge::Rise,
            }]
        );
        assert!(d.comb.contains_key("count")); // count = value
        assert_eq!(d.clocks, vec!["clk".to_string()]);
        assert_eq!(d.resets, vec!["rst".to_string()]);
        assert_eq!(d.procs.len(), 1);
        assert_eq!(d.procs[0].clock, "clk");
    }

    #[test]
    fn param_override_folds_widths() {
        let d = elaborate(
            &parse(COUNTER),
            None,
            &BTreeMap::from([("WIDTH".into(), 4)]),
        )
        .expect("elaborates");
        assert_eq!(d.consts["WIDTH"], 4);
        assert_eq!(d.outputs[0].width.bits, 4);
        assert_eq!(d.regs[0].width.bits, 4);
    }

    #[test]
    fn elaborates_a_combinational_module() {
        // No clock/reset/reg → empty sequential parts, comb drivers only.
        let d = design(
            "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[9]\n  y = a + b\n}\n",
        );
        assert_eq!(d.inputs.len(), 2);
        assert_eq!(d.outputs.len(), 1);
        assert!(d.regs.is_empty());
        assert!(d.procs.is_empty());
        assert!(d.clocks.is_empty());
        assert!(d.resets.is_empty());
        assert!(d.comb.contains_key("y"));
    }

    #[test]
    fn reg_takes_a_nonzero_folded_reset_value() {
        let d = design(
            "module R {\n  clock clk\n  reset rst\n  out y: bits[8]\n  \
             reg r: bits[8] = 5\n  on rise(clk) { r <- r +% 1 }\n  y = r\n}\n",
        );
        assert_eq!(d.regs[0].reset, 5);
        assert_eq!(d.regs[0].clock, "clk");
    }

    #[test]
    fn flattens_a_same_file_instance() {
        // `Top` instantiates a combinational `Add`; the child's signals inline as
        // `u_a`/`u_b`/`u_s`, and the parent's `u.s` field-read becomes `u_s`.
        let d = elaborate(
            &parse(
                "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out s: bits[9]\n  \
                 s = a + b\n}\n\
                 module Top {\n  in x: bits[8]\n  in y: bits[8]\n  out t: bits[9]\n  \
                 let u = Add() { a: x, b: y }\n  t = u.s\n}\n",
            ),
            Some("Top"),
            &BTreeMap::new(),
        )
        .expect("flattens");
        assert_eq!(d.module, "Top");
        let wire_names: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
        assert!(wire_names.contains(&"u_a"), "wires: {wire_names:?}");
        assert!(wire_names.contains(&"u_b"), "wires: {wire_names:?}");
        assert!(wire_names.contains(&"u_s"), "wires: {wire_names:?}");
        // `t = u.s` → `t = u_s`; child output `u_s` is driven by `u_a + u_b`.
        assert!(d.comb.contains_key("t"));
        assert!(d.comb.contains_key("u_s"));
        assert!(d.regs.is_empty() && d.procs.is_empty());
    }

    #[test]
    fn rejects_unknown_instance_module() {
        let err = elaborate(
            &parse(
                "module Top {\n  out y: bits[8]\n  \
                 let u = Missing() { }\n  y = 0\n}\n",
            ),
            None,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("unknown module"), "got: {err}");
    }

    #[test]
    fn unrolls_repeat_with_instance_array_and_bit_drives() {
        // `repeat` inlines one `Xor` per bit; `s[i] = fa[i].o` collects bit drives
        // that assemble into a whole-signal Concat.
        let d = elaborate(
            &parse(
                "module Xor {\n  in a: bit\n  in b: bit\n  out o: bit\n  o = a ^ b\n}\n\
                 module R {\n  in x: bits[2]\n  in y: bits[2]\n  out s: bits[2]\n  \
                 repeat i: 0..2 {\n    let fa[i] = Xor() { a: x[i], b: y[i] }\n    \
                 s[i] = fa[i].o\n  }\n}\n",
            ),
            Some("R"),
            &BTreeMap::new(),
        )
        .expect("unrolls");
        let wires: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
        assert!(wires.contains(&"fa__0_o"), "wires: {wires:?}");
        assert!(wires.contains(&"fa__1_o"), "wires: {wires:?}");
        // `s` assembled from its per-bit drives.
        assert!(
            matches!(d.comb["s"].kind, ExprKind::Concat(_)),
            "s not a concat"
        );
    }

    #[test]
    fn elaborates_an_enum_signal_and_match() {
        // `reg st: S` width = clog2(2) = 1; `S.A` reset = 0; the match over the
        // enum elaborates (variant patterns rewritten to their indices).
        let d = design(
            "module FSM {\n  clock clk\n  reset rst\n  out o: bit\n  \
             enum S { A, B }\n  reg st: S = S.A\n  \
             on rise(clk) { st <- match st { S.A => S.B\n S.B => S.A } }\n  \
             o = st == S.B\n}\n",
        );
        let st = d.regs.iter().find(|r| r.name == "st").expect("reg st");
        assert_eq!(st.width.bits, 1);
        assert_eq!(st.reset, 0);
        assert!(d.comb.contains_key("o"));
    }

    // ---- C1–C4 hardening regressions (SEC-6 / the 2026 audit) ----

    #[test]
    fn recursive_instantiation_errors_not_overflows() {
        // SIM-1: `mimz sim`/`test` skip the checker, so a self-instantiating
        // module must error on the depth bound, not recurse into a stack overflow.
        let err = elaborate(
            &parse("module A {\n  out y: bits[8]\n  let u = A() { }\n  y = 0\n}\n"),
            None,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("nesting"), "got: {err}");
    }

    #[test]
    fn extreme_repeat_bounds_error_not_overflow() {
        // SIM-2: `hi - lo` past i128::MAX must be a clean over-budget error, not an
        // overflow panic.
        let big = "100000000000000000000000000000000000000"; // ~1e38, fits i128
        let src = format!(
            "module R {{\n  out y: bit\n  repeat i: -{big}..{big} {{\n    y[i] = 0\n  }}\n}}\n"
        );
        let err = elaborate(&parse(&src), None, &BTreeMap::new()).unwrap_err();
        assert!(err.contains("unroll"), "got: {err}");
    }

    #[test]
    fn an_out_of_range_bit_index_errors() {
        // SIM-3: a bit index past 128 must error, not truncate via `as u32`.
        let err = elaborate(
            &parse("module R {\n  out y: bits[4]\n  y[200] = 0\n}\n"),
            None,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn a_flatten_name_collision_errors() {
        // SIM-4: a parent signal named like a flattened `inst_port` wire must error
        // instead of silently overwriting.
        let err = elaborate(
            &parse(
                "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out s: bits[9]\n  \
                 s = a + b\n}\n\
                 module Top {\n  in x: bits[8]\n  out t: bits[9]\n  wire u_s: bits[9] = 0\n  \
                 let u = Add() { a: x, b: x }\n  t = u_s\n}\n",
            ),
            Some("Top"),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("collides"), "got: {err}");
    }

    #[test]
    fn an_i128_min_const_elaborates_without_overflow() {
        // SIM-5: a flattened child const that evaluates to i128::MIN must not
        // overflow-panic the negation in `int_expr`. i128::MAX is
        // 170141183460469231731687303715884105727, so `-MAX - 1` is i128::MIN,
        // reachable via checked arithmetic even on the checker-skipping sim path.
        let res = elaborate(
            &parse(
                "module Child {\n  \
                 const M: int = -170141183460469231731687303715884105727 - 1\n  \
                 out y: bit\n  y = 0\n}\n\
                 module Top {\n  out t: bit\n  let u = Child() { }\n  t = u_y\n}\n",
            ),
            Some("Top"),
            &BTreeMap::new(),
        );
        assert!(
            res.is_ok(),
            "i128::MIN const should elaborate, got: {res:?}"
        );
    }
}
