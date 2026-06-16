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
//! Module **instances are flattened** (C2): each child is elaborated and inlined
//! with its signals name-prefixed (`inst.port` → wire `inst_port`), mirroring the
//! Verilog emitter's instance lowering. `repeat` and enum-typed signals are still
//! rejected (C3/C4). Const/width folding is shared with the combinational
//! evaluator ([`super::comb`]).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::{self, Dir, Expr, ExprKind, ModuleItem, SeqStmt, UnOp};

use super::value::{const_eval, pick_module, type_width};

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
}

/// One sequential process — the body of an `on rise(clock)` block. The kernel
/// interprets `body` each rising edge of `clock` (after the synthesized reset
/// branch). Registers left unassigned on a path hold their current value.
#[derive(Clone, Debug)]
pub struct Process {
    pub clock: String,
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
/// imported file is available). `repeat` and enum-typed signals are still
/// rejected (C3/C4).
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
    elaborate_module(&reg, entry, m, params)
}

/// Elaborate one module (`m`, defined in `file`) under concrete `params`,
/// resolving any instantiated children through `reg`.
fn elaborate_module(
    reg: &Registry,
    file: &ast::File,
    m: &ast::Module,
    params: &BTreeMap<String, i128>,
) -> Result<Design, String> {
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

    // Instance names, so a `Field` like `add.sum` (an instance-port read) can be
    // rewritten to the flat wire name `add_sum` in the parent's own expressions.
    let insts: HashSet<String> = m
        .items
        .iter()
        .filter_map(|it| match it {
            ModuleItem::Inst(i) => Some(i.name.name.clone()),
            _ => None,
        })
        .collect();
    let fr = |e: &Expr| field_rewrite(e, &insts);

    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut wires = Vec::new();
    let mut regs = Vec::new();
    let mut comb: BTreeMap<String, Expr> = BTreeMap::new();
    let mut procs = Vec::new();
    let mut clocks = Vec::new();
    let mut resets = Vec::new();

    for it in &m.items {
        match it {
            ModuleItem::Port { dir, name, ty } => {
                let (bits, signed) = type_width(ty, &consts)?;
                let sig = Signal {
                    name: name.name.clone(),
                    width: Width { bits, signed },
                };
                match dir {
                    Dir::In => inputs.push(sig),
                    Dir::Out => outputs.push(sig),
                }
            }
            ModuleItem::Clock(n) => clocks.push(n.name.clone()),
            ModuleItem::Reset(n) => resets.push(n.name.clone()),
            ModuleItem::Wire { name, ty, init } => {
                let (bits, signed) = type_width(ty, &consts)?;
                wires.push(Signal {
                    name: name.name.clone(),
                    width: Width { bits, signed },
                });
                comb.insert(name.name.clone(), fr(init));
            }
            ModuleItem::Reg { name, ty, reset } => {
                let (bits, signed) = type_width(ty, &consts)?;
                let reset = const_eval(reset, &consts)?;
                regs.push(Reg {
                    name: name.name.clone(),
                    width: Width { bits, signed },
                    reset,
                    clock: String::new(),
                });
            }
            ModuleItem::Drive { lhs, rhs } => {
                if lhs.index.is_some() {
                    return Err(format!(
                        "driving a slice/bit of `{}` is not supported by the simulator yet — \
                         drive the whole signal",
                        lhs.base.name
                    ));
                }
                comb.insert(lhs.base.name.clone(), fr(rhs));
            }
            ModuleItem::On(on) => procs.push(Process {
                clock: on.clock.name.clone(),
                body: on
                    .body
                    .iter()
                    .map(|s| rewrite_seq(s, &fr, &|n| n.to_string()))
                    .collect(),
            }),
            // Consts are folded above; enum decls carry no runtime state.
            ModuleItem::Const(_) | ModuleItem::Enum(_) => {}
            ModuleItem::Inst(inst) => {
                let flat = flatten_instance(reg, &consts, &insts, inst)?;
                wires.extend(flat.wires);
                regs.extend(flat.regs);
                comb.extend(flat.comb);
                procs.extend(flat.procs);
            }
            ModuleItem::Repeat(_) => {
                return Err(
                    "module uses `repeat` — unrolling is not supported by the simulator yet".into(),
                );
            }
        }
    }

    // Each reg's clock is the clock of the `on` block that assigns it (the
    // checker guarantees a reg has exactly one owning block). Covers inlined
    // child regs too — their assigning proc carries the connected parent clock.
    for proc in &procs {
        for reg in &mut regs {
            if assigns(&proc.body, &reg.name) {
                reg.clock = proc.clock.clone();
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
        comb,
        procs,
        clocks,
        resets,
    })
}

/// The flat pieces one instance contributes to its parent.
struct Flat {
    wires: Vec<Signal>,
    regs: Vec<Reg>,
    comb: Vec<(String, Expr)>,
    procs: Vec<Process>,
}

/// Elaborate the child module of `inst` and inline it into the parent: every
/// child signal becomes a parent wire/reg named `{inst}_{name}`, child inputs
/// are driven by their connection expressions, and child clock/reset map to the
/// connected parent signals. Mirrors the Verilog emitter's instance lowering so
/// the simulator agrees bit-for-bit.
fn flatten_instance(
    reg: &Registry,
    parent_consts: &BTreeMap<String, i128>,
    parent_insts: &HashSet<String>,
    inst: &ast::Inst,
) -> Result<Flat, String> {
    if inst.index.is_some() {
        return Err("instance arrays (`let name[i] = …`) need `repeat`, not supported yet".into());
    }
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

    let child = elaborate_module(reg, cfile, cm, &cp)?;
    let pfx = format!("{}_", inst.name.name);
    let fr = |e: &Expr| field_rewrite(e, parent_insts);

    // Substitution for the child's own expressions: a child const folds to a
    // literal; any child signal becomes its prefixed name; a child clock/reset
    // becomes the connected parent signal.
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

    // Clock/reset: explicit connection, else the same-named parent signal.
    let mut clock_map: HashMap<String, String> = HashMap::new();
    for c in child.clocks.iter().chain(&child.resets) {
        let parent = inst
            .conns
            .iter()
            .find(|cn| cn.port.name == *c)
            .map(|cn| conn_signal_name(&fr(&cn.signal)))
            .transpose()?
            .unwrap_or_else(|| c.clone());
        subst.insert(c.clone(), ident_expr(parent.clone(), inst.span));
        clock_map.insert(c.clone(), parent);
    }

    let sb = |e: &Expr| subst_rewrite(e, &subst);
    let mut flat = Flat {
        wires: Vec::new(),
        regs: Vec::new(),
        comb: Vec::new(),
        procs: Vec::new(),
    };

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
            .push((format!("{pfx}{}", s.name), fr(&conn.signal)));
    }
    // Child outputs + wires: a parent wire driven by the child's (rewritten) logic.
    for s in child.outputs.iter().chain(&child.wires) {
        if let Some(drv) = child.comb.get(&s.name) {
            flat.wires.push(Signal {
                name: format!("{pfx}{}", s.name),
                width: s.width,
            });
            flat.comb.push((format!("{pfx}{}", s.name), sb(drv)));
        }
    }
    // Child registers (clock filled by the parent's reg-clock pass).
    for r in &child.regs {
        flat.regs.push(Reg {
            name: format!("{pfx}{}", r.name),
            width: r.width,
            reset: r.reset,
            clock: String::new(),
        });
    }
    // Child processes: prefix assigned regs, rewrite bodies, map the clock.
    for p in &child.procs {
        let clk = clock_map.get(&p.clock).cloned().unwrap_or(p.clock.clone());
        let rename = |n: &str| format!("{pfx}{n}");
        flat.procs.push(Process {
            clock: clk,
            body: p
                .body
                .iter()
                .map(|s| rewrite_seq(s, &sb, &rename))
                .collect(),
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
        Expr {
            kind: ExprKind::Int {
                value: v as u128,
                raw: v.to_string(),
            },
            span,
        }
    } else {
        Expr {
            kind: ExprKind::Unary {
                op: UnOp::Neg,
                expr: Box::new(int_expr(-v, span)),
            },
            span,
        }
    }
}

/// Rewrite `inst.port` (instance-port read) to the flat wire `inst_port` for
/// every name in `insts`; other `Field`s (enum variants) are left untouched.
fn field_rewrite(e: &Expr, insts: &HashSet<String>) -> Expr {
    map_expr(e, &|n| {
        if let ExprKind::Field { base, field } = &n.kind
            && let ExprKind::Ident(b) = &base.kind
            && insts.contains(b)
        {
            return Some(ident_expr(format!("{b}_{}", field.name), n.span));
        }
        None
    })
}

/// Rewrite child identifiers through `subst` (const → literal, signal → prefixed,
/// clock/reset → connected parent signal).
fn subst_rewrite(e: &Expr, subst: &HashMap<String, Expr>) -> Expr {
    map_expr(e, &|n| match &n.kind {
        ExprKind::Ident(name) => subst.get(name).cloned(),
        _ => None,
    })
}

/// Rebuild `e`, giving `f` first say at each node (a `Some` replaces the node and
/// stops; `None` recurses into the children).
fn map_expr(e: &Expr, f: &dyn Fn(&Expr) -> Option<Expr>) -> Expr {
    if let Some(r) = f(e) {
        return r;
    }
    let kind = match &e.kind {
        ExprKind::Int { .. } | ExprKind::Bool(_) | ExprKind::Ident(_) => e.kind.clone(),
        ExprKind::Field { base, field } => ExprKind::Field {
            base: Box::new(map_expr(base, f)),
            field: field.clone(),
        },
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(map_expr(expr, f)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(map_expr(lhs, f)),
            rhs: Box::new(map_expr(rhs, f)),
        },
        ExprKind::IfExpr { cond, then, els } => ExprKind::IfExpr {
            cond: Box::new(map_expr(cond, f)),
            then: Box::new(map_expr(then, f)),
            els: Box::new(map_expr(els, f)),
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(map_expr(scrutinee, f)),
            arms: arms
                .iter()
                .map(|a| ast::Arm {
                    patterns: a.patterns.clone(),
                    value: map_expr(&a.value, f),
                })
                .collect(),
        },
        ExprKind::Concat(parts) => ExprKind::Concat(parts.iter().map(|p| map_expr(p, f)).collect()),
        ExprKind::Index { base, index } => ExprKind::Index {
            base: Box::new(map_expr(base, f)),
            index: Box::new(map_expr(index, f)),
        },
        ExprKind::Slice { base, hi, lo } => ExprKind::Slice {
            base: Box::new(map_expr(base, f)),
            hi: Box::new(map_expr(hi, f)),
            lo: Box::new(map_expr(lo, f)),
        },
        ExprKind::Call { func, args } => ExprKind::Call {
            func: *func,
            args: args.iter().map(|a| map_expr(a, f)).collect(),
        },
    };
    Expr { kind, span: e.span }
}

/// Rewrite a sequential statement: map every expression through `f`, and rename
/// each assignment's target via `rename` (prefixing inlined child regs).
fn rewrite_seq(s: &SeqStmt, f: &dyn Fn(&Expr) -> Expr, rename: &dyn Fn(&str) -> String) -> SeqStmt {
    match s {
        SeqStmt::Assign { lhs, rhs } => SeqStmt::Assign {
            lhs: ast::LValue {
                base: ast::Ident {
                    name: rename(&lhs.base.name),
                    span: lhs.base.span,
                },
                index: lhs.index.as_ref().map(|(a, b)| (f(a), b.as_ref().map(f))),
                span: lhs.span,
            },
            rhs: f(rhs),
        },
        SeqStmt::If { cond, then, els } => SeqStmt::If {
            cond: f(cond),
            then: then.iter().map(|x| rewrite_seq(x, f, rename)).collect(),
            els: els
                .as_ref()
                .map(|e| e.iter().map(|x| rewrite_seq(x, f, rename)).collect()),
        },
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
}
