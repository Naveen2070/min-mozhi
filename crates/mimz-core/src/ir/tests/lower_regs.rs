use super::{ident, w};
use crate::ast::{Edge, Ident, LValue, SeqStmt};
use crate::checker::consteval::ConstVal;
use crate::elaborate::{Design, Process, Reg, Signal};
use crate::ir::{Cell, CellKind, Module, lower};
use crate::span::Span;
use std::collections::BTreeMap;

fn lvalue(name: &str) -> LValue {
    LValue {
        base: Ident {
            name: name.to_string(),
            span: Span::default(),
        },
        index: None,
        span: Span::default(),
    }
}

fn find_port<'a>(module: &'a Module, name: &str) -> &'a crate::ir::Bits {
    &module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .expect("port")
        .1
}

fn find_dff(module: &Module) -> &Cell {
    let dffs: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Dff { .. }))
        .collect();
    assert_eq!(dffs.len(), 1, "expected exactly one Dff cell");
    dffs[0]
}

/// Base design shared by both tests: one 8-bit input "d", a 1-bit clock
/// "clk", and a register "q" driven by an `on rise(clk)` block assigning
/// `q <- d`. Callers customize `regs[0].reset` and `resets`.
fn reg_design() -> Design {
    Design {
        module: "regger".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "d".into(),
                width: w(8),
            },
            Signal {
                name: "clk".into(),
                width: w(1),
            },
        ],
        outputs: vec![],
        wires: vec![],
        regs: vec![Reg {
            name: "q".into(),
            width: w(8),
            reset: ConstVal {
                bits: crate::bits::Bits::Small(0),
                width: 8,
                signed: false,
            },
            clock: "clk".into(),
            edge: Edge::Rise,
        }],
        mems: vec![],
        comb: BTreeMap::new(),
        procs: vec![Process {
            clock: "clk".into(),
            edge: Edge::Rise,
            body: vec![SeqStmt::Assign {
                lhs: lvalue("q"),
                rhs: ident("d"),
            }],
        }],
        clocks: vec!["clk".into()],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        asserts: vec![],
        covers: vec![],
    }
}

#[test]
fn lowers_a_register_with_no_reset_to_a_dff_cell() {
    let design = reg_design();
    let module = lower(&design);

    let dff = find_dff(&module);
    let CellKind::Dff { clock, edge } = &dff.kind else {
        unreachable!()
    };
    assert_eq!(*edge, Edge::Rise);

    let clk_bits = find_port(&module, "clk");
    assert_eq!(clk_bits.width(), 1);
    assert_eq!(*clock, clk_bits.0[0]);

    assert_eq!(dff.pins["q"].width(), 8);

    let d_bits = find_port(&module, "d");
    assert_eq!(
        dff.pins["d"], *d_bits,
        "d input feeds the Dff directly (no reset mux)"
    );
}

#[test]
fn lowers_a_register_with_synchronous_reset_to_a_muxed_dff() {
    let mut design = reg_design();
    design.resets = vec!["rst".to_string()];
    design.inputs.push(Signal {
        name: "rst".into(),
        width: w(1),
    });
    design.regs[0].reset = ConstVal {
        bits: crate::bits::Bits::Small(5),
        width: 8,
        signed: false,
    };
    let module = lower(&design);

    let dff = find_dff(&module);
    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(mux_cells.len(), 1);
    let mux = mux_cells[0];
    assert_eq!(
        mux.pins["out"], dff.pins["d"],
        "the mux feeds the Dff's d pin"
    );

    let rst_bits = find_port(&module, "rst");
    assert_eq!(mux.pins["sel"], *rst_bits);

    let d_bits = find_port(&module, "d");
    assert_eq!(mux.pins["b"], *d_bits, "b is the process-body result");

    let const_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Const { .. }))
        .collect();
    let reset_const = const_cells
        .iter()
        .find(|c| c.pins["out"] == mux.pins["a"])
        .expect("mux's `a` traces to a Const cell (the folded reset value)");
    let CellKind::Const { value } = &reset_const.kind else {
        unreachable!()
    };
    assert_eq!(value.bits, crate::bits::Bits::Small(5));
}

/// Regression: `lower_seq_stmts` walks the WHOLE process body once per
/// register, so a second register assigned in only one branch of an `if`
/// used to leak its name into `then_env` during the *first* register's walk
/// — and the branch merge's `env[&name]` then panicked on a key that was
/// never pre-seeded. Both `Default` and `Assign` now only track names the
/// caller pre-seeded.
#[test]
fn a_second_register_assigned_only_inside_an_if_does_not_disturb_the_first() {
    let mut design = reg_design();
    design.inputs.push(Signal {
        name: "en".into(),
        width: w(1),
    });
    let b = Reg {
        name: "qb".into(),
        ..design.regs[0].clone()
    };
    design.regs.push(b);
    design.procs[0].body = vec![
        SeqStmt::Assign {
            lhs: lvalue("q"),
            rhs: ident("d"),
        },
        SeqStmt::If {
            cond: ident("en"),
            then: vec![SeqStmt::Assign {
                lhs: lvalue("qb"),
                rhs: ident("d"),
            }],
            els: None,
        },
    ];

    let module = lower(&design); // must not panic

    let dffs: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Dff { .. }))
        .collect();
    assert_eq!(dffs.len(), 2);
    let q_dff = dffs
        .iter()
        .find(|c| module.nets[c.pins["q"].0[0].0 as usize].name.as_deref() == Some("q"))
        .expect("one Dff drives `q`");
    assert_eq!(
        q_dff.pins["d"],
        *find_port(&module, "d"),
        "`q`'s D input is plain `d` — unaffected by `qb`'s conditional assign"
    );
}

#[test]
fn lowers_default_seq_stmt_to_a_const_driven_dff() {
    let mut design = reg_design();
    // Replace the process body with a bare `default q <- 0` and no
    // conditional assign at all (D-DEFAULT-3 coverage: Default must not be
    // silently dropped).
    design.procs[0].body = vec![SeqStmt::Default {
        name: Ident {
            name: "q".to_string(),
            span: Span::default(),
        },
        val: crate::ast::Expr {
            kind: crate::ast::ExprKind::Int {
                value: crate::bits::Bits::Small(0),
                raw: "0".to_string(),
            },
            span: Span::default(),
        },
        span: Span::default(),
    }];
    let module = lower(&design);

    let dff = find_dff(&module);
    let const_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Const { .. }))
        .collect();
    let default_const = const_cells
        .iter()
        .find(|c| c.pins["out"] == dff.pins["d"])
        .expect("Dff's `d` pin traces to a Const cell fed by the `default` statement");
    let CellKind::Const { value } = &default_const.kind else {
        unreachable!()
    };
    assert_eq!(value.bits, crate::bits::Bits::Small(0));
}
