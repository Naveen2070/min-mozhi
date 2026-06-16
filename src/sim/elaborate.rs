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
//! Single-module only for now: `let` instances and `repeat` are rejected with a
//! clear message (instance elaboration is a later step). Const/width folding is
//! shared with the combinational evaluator ([`super::comb`]).

use std::collections::BTreeMap;

use crate::ast::{self, Dir, Expr, ModuleItem, SeqStmt};

use super::comb::{const_eval, pick_module, type_width};

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
/// flat [`Design`], folding widths and the reset values under `params` (a
/// parameter not in `params` uses its declared default; one with neither is an
/// error). Instances, `repeat`, and enum-typed signals are not yet handled and
/// return a descriptive error.
pub fn elaborate(
    file: &ast::File,
    module: Option<&str>,
    params: &BTreeMap<String, i128>,
) -> Result<Design, String> {
    let m = pick_module(file, module)?;

    // Structural items the simulator does not elaborate yet (a later step).
    for it in &m.items {
        match it {
            ModuleItem::Inst(_) => {
                return Err("module instantiates a sub-module — the simulator does not \
                            elaborate instances yet (single-module for now)"
                    .into());
            }
            ModuleItem::Repeat(_) => {
                return Err(
                    "module uses `repeat` — unrolling is not supported by the simulator yet".into(),
                );
            }
            _ => {}
        }
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
                comb.insert(name.name.clone(), init.clone());
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
                comb.insert(lhs.base.name.clone(), rhs.clone());
            }
            ModuleItem::On(on) => procs.push(Process {
                clock: on.clock.name.clone(),
                body: on.body.clone(),
            }),
            // Consts are folded above; enum decls carry no runtime state.
            ModuleItem::Const(_) | ModuleItem::Enum(_) => {}
            // Rejected above.
            ModuleItem::Inst(_) | ModuleItem::Repeat(_) => unreachable!(),
        }
    }

    // Each reg's clock is the clock of the `on` block that assigns it (the
    // checker guarantees a reg has exactly one owning block).
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
    fn rejects_instances_for_now() {
        let err = elaborate(
            &parse(
                "module Top {\n  clock clk\n  reset rst\n  out y: bits[8]\n  \
                 let u = Counter() { clk: clk, rst: rst }\n  y = 0\n}\n",
            ),
            None,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("instances"), "got: {err}");
    }
}
