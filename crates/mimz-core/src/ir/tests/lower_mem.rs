use super::{ident, w};
use crate::ast::{Edge, Expr, ExprKind, Ident, LValue, SeqStmt};
use crate::checker::consteval::ConstVal;
use crate::elaborate::{Design, Mem, Process, Signal};
use crate::ir::{Cell, CellKind, Module, lower};
use crate::span::Span;
use std::collections::BTreeMap;

fn find_port<'a>(module: &'a Module, name: &str) -> &'a crate::ir::Bits {
    &module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .expect("port")
        .1
}

fn find_mem(module: &Module) -> &Cell {
    let mems: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Mem { .. }))
        .collect();
    assert_eq!(mems.len(), 1, "expected exactly one Mem cell");
    mems[0]
}

/// Traces a pin back to the `Const` cell driving it, asserting there is one.
fn const_behind<'a>(module: &'a Module, pin: &crate::ir::Bits) -> &'a ConstVal {
    let cell = module
        .cells
        .iter()
        .find(|c| matches!(c.kind, CellKind::Const { .. }) && c.pins["out"] == *pin)
        .expect("pin traces to a Const cell");
    let CellKind::Const { value } = &cell.kind else {
        unreachable!()
    };
    value
}

/// `ram[addr]` — the dual-use `ExprKind::Index` form that means a full-width
/// memory-word read when `base` names a `design.mems` entry.
fn mem_read(name: &str, addr: &str) -> Expr {
    Expr {
        kind: ExprKind::Index {
            base: Box::new(ident(name)),
            index: Box::new(ident(addr)),
        },
        span: Span::default(),
    }
}

/// `ram[0]` — a read whose index is a literal, so each lowering of it
/// allocates a fresh `Const` cell rather than resolving to a memoized signal.
fn mem_read_lit(name: &str, index: u128) -> Expr {
    Expr {
        kind: ExprKind::Index {
            base: Box::new(ident(name)),
            index: Box::new(Expr {
                kind: ExprKind::Int {
                    value: crate::bits::Bits::Small(index),
                    raw: index.to_string(),
                },
                span: Span::default(),
            }),
        },
        span: Span::default(),
    }
}

/// A plain (unindexed) assignment target, for the registers some of these
/// designs drive alongside the memory.
fn reg_lvalue(name: &str) -> LValue {
    LValue {
        base: Ident {
            name: name.to_string(),
            span: Span::default(),
        },
        index: None,
        span: Span::default(),
    }
}

/// `ram[addr] <- ...` — the matching write-side `LValue`.
fn mem_lvalue(name: &str, addr: &str) -> LValue {
    LValue {
        base: Ident {
            name: name.to_string(),
            span: Span::default(),
        },
        index: Some((ident(addr), None)),
        span: Span::default(),
    }
}

/// A non-trivial power-on seed, so `init`'s assertions can't be satisfied by
/// a lowering that hardcodes zero.
fn seed() -> ConstVal {
    ConstVal {
        bits: crate::bits::Bits::Small(0xA5),
        width: 8,
        signed: false,
    }
}

/// One 8-bit x 4-word memory `ram`, read combinationally into wire `rd` and
/// written unconditionally on `rise(clk)`. `depth: 4` -> a 2-bit address.
fn ram_design() -> Design {
    let mut comb = BTreeMap::new();
    comb.insert("rd".to_string(), mem_read("ram", "addr"));
    Design {
        module: "rammer".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "clk".into(),
                width: w(1),
            },
            Signal {
                name: "addr".into(),
                width: w(2),
            },
            Signal {
                name: "din".into(),
                width: w(8),
            },
        ],
        outputs: vec![],
        wires: vec![Signal {
            name: "rd".into(),
            width: w(8),
        }],
        regs: vec![],
        mems: vec![Mem {
            name: "ram".into(),
            width: w(8),
            depth: 4,
            init: seed(),
            clock: "clk".into(),
            edge: Edge::Rise,
        }],
        comb,
        procs: vec![Process {
            clock: "clk".into(),
            edge: Edge::Rise,
            body: vec![SeqStmt::Assign {
                lhs: mem_lvalue("ram", "addr"),
                rhs: ident("din"),
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
fn lowers_a_read_write_memory_to_one_mem_cell() {
    let design = ram_design();
    let module = lower(&design);

    let mem = find_mem(&module);
    let CellKind::Mem { depth, init } = &mem.kind else {
        unreachable!()
    };
    assert_eq!(*depth, 4);
    assert_eq!(*init, seed(), "power-on seed rides on the cell");

    // `raddr` and `waddr` are independent pins. Here both sides happen to
    // name the same signal, so both land on it — but neither is derived from
    // the other, and no reconciling mux exists.
    assert_eq!(mem.pins["raddr"], *find_port(&module, "addr"));
    assert_eq!(mem.pins["waddr"], *find_port(&module, "addr"));
    assert_eq!(mem.pins["raddr"].width(), 2, "clog2(4) address");

    assert_eq!(mem.pins["rdata"].width(), 8);
    assert_eq!(mem.pins["wdata"], *find_port(&module, "din"));

    // Unconditional write => write-enable folds to constant 1.
    assert_eq!(mem.pins["wen"].width(), 1);
    assert_eq!(
        const_behind(&module, &mem.pins["wen"]).bits,
        crate::bits::Bits::Small(1)
    );

    assert_eq!(mem.pins["clock"], *find_port(&module, "clk"));
}

#[test]
fn conditional_write_gates_the_write_enable_with_a_mux() {
    let mut design = ram_design();
    design.inputs.push(Signal {
        name: "we".into(),
        width: w(1),
    });
    design.procs[0].body = vec![SeqStmt::If {
        cond: ident("we"),
        then: vec![SeqStmt::Assign {
            lhs: mem_lvalue("ram", "addr"),
            rhs: ident("din"),
        }],
        els: None,
    }];
    let module = lower(&design);

    let mem = find_mem(&module);
    let wen_mux = module
        .cells
        .iter()
        .find(|c| c.kind == CellKind::Mux && c.pins["out"] == mem.pins["wen"])
        .expect("wen is Mux(we, 1, 0)");
    assert_eq!(wen_mux.pins["sel"], *find_port(&module, "we"));
    assert_eq!(
        const_behind(&module, &wen_mux.pins["a"]).bits,
        crate::bits::Bits::Small(1)
    );
    assert_eq!(
        const_behind(&module, &wen_mux.pins["b"]).bits,
        crate::bits::Bits::Small(0)
    );
}

/// The canonical register-file shape: `rd = ram[ra]` alongside
/// `on rise(clk) { if we { ram[wa] <- din } }`. Read and write addresses are
/// genuinely different signals and must reach the cell as independent pins —
/// the simulator kernel reads the PRE-tick array, so a same-cycle read must
/// not follow the write address when `we` is high.
#[test]
fn read_and_write_addresses_stay_independent() {
    let mut design = ram_design();
    design.inputs.push(Signal {
        name: "wa".into(),
        width: w(2),
    });
    design.inputs.push(Signal {
        name: "we".into(),
        width: w(1),
    });
    // `rd = ram[addr]` is already in the fixture; only the write moves to `wa`.
    design.procs[0].body = vec![SeqStmt::If {
        cond: ident("we"),
        then: vec![SeqStmt::Assign {
            lhs: mem_lvalue("ram", "wa"),
            rhs: ident("din"),
        }],
        els: None,
    }];
    let module = lower(&design);

    let mem = find_mem(&module);
    assert_eq!(
        mem.pins["raddr"],
        *find_port(&module, "addr"),
        "the read address is the read expression's own index, untouched by `we`"
    );
    assert_ne!(mem.pins["raddr"], mem.pins["waddr"]);

    // `waddr` is `Mux(we, wa, 0)` — the write address gated by the `if`.
    let waddr_mux = module
        .cells
        .iter()
        .find(|c| c.kind == CellKind::Mux && c.pins["out"] == mem.pins["waddr"])
        .expect("waddr is Mux(we, wa, 0)");
    assert_eq!(waddr_mux.pins["sel"], *find_port(&module, "we"));
    assert_eq!(waddr_mux.pins["a"], *find_port(&module, "wa"));

    // No cell anywhere reconciles the two addresses against each other.
    assert!(
        !module
            .cells
            .iter()
            .any(|c| c.kind == CellKind::Mux && c.pins["out"] == mem.pins["raddr"]),
        "nothing muxes the read address"
    );
}

/// A read reachable ONLY from inside the memory's own writing process
/// (`ram[wa] <- ram[addr]`). The register pass never walks `MemWrite`
/// statements, so this is the first place the read is lowered — the write
/// pass must therefore run before any cell is emitted.
#[test]
fn a_read_inside_the_write_process_still_reaches_the_cell() {
    let mut design = ram_design();
    design.wires.clear();
    design.comb.clear();
    design.inputs.push(Signal {
        name: "wa".into(),
        width: w(2),
    });
    design.procs[0].body = vec![SeqStmt::Assign {
        lhs: mem_lvalue("ram", "wa"),
        rhs: mem_read("ram", "addr"),
    }];
    let module = lower(&design);

    let mem = find_mem(&module);
    assert_eq!(
        mem.pins["raddr"],
        *find_port(&module, "addr"),
        "the read address is not lost to a placeholder constant"
    );
    assert_eq!(
        mem.pins["wdata"], mem.pins["rdata"],
        "the word being written IS the word being read back"
    );
    assert_eq!(mem.pins["waddr"], *find_port(&module, "wa"));
}

/// `ram` as a ROM plus two registers `q`/`qb` on one process, so the shared
/// body gets walked once per register and nothing else walks it.
fn two_reg_rom_design() -> Design {
    let mut design = ram_design();
    design.mems[0].clock = String::new();
    design.wires.clear();
    design.comb.clear();
    for name in ["q", "qb"] {
        design.regs.push(crate::elaborate::Reg {
            name: name.into(),
            width: w(8),
            reset: ConstVal {
                bits: crate::bits::Bits::Small(0),
                width: 8,
                signed: false,
            },
            clock: "clk".into(),
            edge: Edge::Rise,
        });
    }
    design
}

fn two_reg_assigns() -> Vec<SeqStmt> {
    ["q", "qb"]
        .into_iter()
        .map(|n| SeqStmt::Assign {
            lhs: reg_lvalue(n),
            rhs: ident("din"),
        })
        .collect()
}

/// Counts `Const` cells holding `value` — a discriminating value that nothing
/// else in the fixture produces makes "was this lowered once or twice?"
/// directly observable.
fn count_consts(module: &Module, value: u128) -> usize {
    module
        .cells
        .iter()
        .filter(|c| {
            matches!(&c.kind, CellKind::Const { value: v }
                if v.bits == crate::bits::Bits::Small(value))
        })
        .count()
}

/// `lower_seq_stmts` walks one shared process body once per target, and
/// `SeqStmt::If`'s condition is lowered on every one of those walks. A read
/// with a literal index allocates a fresh `Const` for that literal each time,
/// so comparing lowered nets alone would flag the SAME source-level read as
/// "two different addresses". One read site must stay one read site however
/// many targets the process drives.
#[test]
fn one_read_site_survives_being_walked_once_per_target() {
    let mut design = two_reg_rom_design();
    design.procs[0].body = vec![SeqStmt::If {
        cond: mem_read_lit("ram", 3),
        then: two_reg_assigns(),
        els: None,
    }];

    let module = lower(&design); // must not panic

    let mem = find_mem(&module);
    let dffs: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Dff { .. }))
        .collect();
    assert_eq!(dffs.len(), 2);
    for dff in &dffs {
        let mux = module
            .cells
            .iter()
            .find(|c| c.kind == CellKind::Mux && c.pins["out"] == dff.pins["d"])
            .expect("each register's D is Mux(cond, din, hold)");
        assert_eq!(
            mux.pins["sel"], mem.pins["rdata"],
            "both walks resolved to the SAME read port, not two rival ones"
        );
        assert_eq!(mux.pins["a"], *find_port(&module, "din"));
    }
}

/// Same shape as above, but the shared condition goes through a `fn` call —
/// `fn f(x) { ram[3] }`. A fn body lowers with `locals: Some(..)`, so nothing
/// inside it is memoized; the protection has to come from the CALL node
/// itself being memoized, so the second per-target walk never re-inlines the
/// body at all.
#[test]
fn a_fn_call_in_a_shared_condition_is_inlined_once_per_source_site() {
    let mut design = two_reg_rom_design();
    design.funcs.insert(
        "f".to_string(),
        crate::ast::FuncDecl {
            name: Ident {
                name: "f".to_string(),
                span: Span::default(),
            },
            params: vec![crate::ast::FnParam {
                name: Ident {
                    name: "x".to_string(),
                    span: Span::default(),
                },
                ty: crate::ast::Type::Bit,
                span: Span::default(),
            }],
            ret: crate::ast::Type::Bit,
            stmts: vec![],
            tail: mem_read_lit("ram", 3),
            span: Span::default(),
        },
    );
    design.procs[0].body = vec![SeqStmt::If {
        cond: Expr {
            kind: ExprKind::FnCall {
                name: Ident {
                    name: "f".to_string(),
                    span: Span::default(),
                },
                args: vec![ident("din")],
            },
            span: Span::default(),
        },
        then: two_reg_assigns(),
        els: None,
    }];

    let module = lower(&design); // must not panic

    // Exactly ONE Const cell for the literal index: proof the second walk
    // reused the call's result instead of re-inlining `f`'s body.
    assert_eq!(
        count_consts(&module, 3),
        1,
        "`ram[3]`'s literal index is lowered once, not once per target walk"
    );
    assert_eq!(find_mem(&module).pins["raddr"].width(), 2);
}

/// One read port only. A second read at a different address must panic
/// rather than silently reuse the first read's address, matching how every
/// other out-of-scope form in `lower.rs` fails.
#[test]
#[should_panic(expected = "exactly one read port per memory")]
fn a_second_read_at_a_different_address_panics() {
    let mut design = ram_design();
    design.inputs.push(Signal {
        name: "ra2".into(),
        width: w(2),
    });
    design.wires.push(Signal {
        name: "rd2".into(),
        width: w(8),
    });
    design
        .comb
        .insert("rd2".to_string(), mem_read("ram", "ra2"));
    lower(&design);
}

#[test]
fn lowers_a_clockless_memory_to_a_rom_with_wen_tied_low() {
    let mut design = ram_design();
    design.mems[0].clock = String::new();
    design.procs.clear();
    design.clocks.clear();
    design.inputs.retain(|s| s.name != "clk" && s.name != "din");
    let module = lower(&design);

    let mem = find_mem(&module);
    let CellKind::Mem { init, .. } = &mem.kind else {
        unreachable!()
    };
    assert_eq!(*init, seed(), "a ROM's contents are entirely its seed");
    assert_eq!(mem.pins["raddr"], *find_port(&module, "addr"));
    assert_eq!(mem.pins["rdata"].width(), 8);
    assert_eq!(
        const_behind(&module, &mem.pins["wen"]).bits,
        crate::bits::Bits::Small(0),
        "a ROM's write-enable is tied to constant 0"
    );
    assert_eq!(mem.pins["wdata"].width(), 8);
    assert_eq!(mem.pins["waddr"].width(), 2);
    assert!(
        !mem.pins.contains_key("clock"),
        "a ROM has no clock signal at all, so no clock pin is invented"
    );
}
