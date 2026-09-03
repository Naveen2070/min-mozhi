//! `ir::exec` — one focused test per `CellKind` family the executor
//! evaluates. Fixtures come from the lowering tests that already build them.

use super::lower_blackbox::pll_design;
use super::lower_mem::ram_design;
use super::lower_mux::if_mux_design;
use super::lower_regs::reg_design;
use super::lower_unary_concat_slice::unary_design;
use super::{adder_design, ident};
use crate::ast::UnOp;
use crate::bits::Bits;
use crate::ir::{exec::Executor, lower};
use crate::value::Val;

fn v(bits: u128, width: u32) -> Val {
    Val::new(bits, width, false)
}

/// `wire sum = a + b`. `sum` is a WIRE, not an output port — so this also
/// covers `get_output`'s fallback from `module.ports` to named nets.
#[test]
fn executes_the_adder_module() {
    let design = adder_design();
    let module = lower(&design);
    let mut executor = Executor::new(&module);
    executor.set_input("a", v(3, 8));
    executor.set_input("b", v(4, 8));
    executor.tick();
    let sum = executor.get_output("sum");
    assert_eq!(sum.bits, Bits::Small(7));
    assert_eq!(sum.width, 9, "lossless growth: 8-bit + 8-bit is 9 bits");
}

/// `wire sum = a + 1` — the `1` lowers to a `Const` cell, so this covers
/// constant evaluation alongside `Add`.
#[test]
fn executes_a_const_cell() {
    let mut design = adder_design();
    design.comb.insert(
        "sum".to_string(),
        crate::ast::Expr {
            kind: crate::ast::ExprKind::Binary {
                op: crate::ast::BinOp::Add,
                lhs: Box::new(ident("a")),
                rhs: Box::new(crate::ast::Expr {
                    kind: crate::ast::ExprKind::Int {
                        value: Bits::Small(1),
                        raw: "1".to_string(),
                    },
                    span: crate::span::Span::default(),
                }),
            },
            span: crate::span::Span::default(),
        },
    );
    let module = lower(&design);
    let mut executor = Executor::new(&module);
    executor.set_input("a", v(7, 8));
    executor.tick();
    assert_eq!(executor.get_output("sum").bits, Bits::Small(8));
}

/// Regression: `wire w = {a, b + 1}` reuses `a`'s own nets, so `lower`'s
/// wire-naming loop stamps `"w"` onto ONLY the 9 nets of the `b + 1` half.
/// Reading `w` by scanning `module.nets` for the name would silently return
/// that half — 9 bits with the wrong value — instead of the full 17. This is
/// why `get_output` goes through `Module::signals`.
#[test]
fn reads_a_wire_that_partly_reuses_another_signals_nets() {
    use crate::ast::{BinOp, Expr, ExprKind};
    use crate::span::Span;

    let mut design = adder_design();
    design.wires[0] = crate::elaborate::Signal {
        name: "w".into(),
        width: super::w(17),
    };
    design.comb.clear();
    design.comb.insert(
        "w".to_string(),
        Expr {
            // `{a, b + 1}`: `a` (8 bits) is the MOST significant part, and
            // `b + 1` (9 bits, lossless) the least.
            kind: ExprKind::Concat(vec![
                ident("a"),
                Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(ident("b")),
                        rhs: Box::new(Expr {
                            kind: ExprKind::Int {
                                value: Bits::Small(1),
                                raw: "1".to_string(),
                            },
                            span: Span::default(),
                        }),
                    },
                    span: Span::default(),
                },
            ]),
            span: Span::default(),
        },
    );
    let module = lower(&design);
    let mut executor = Executor::new(&module);
    executor.set_input("a", v(0x5A, 8));
    executor.set_input("b", v(2, 8));
    executor.tick();

    let out = executor.get_output("w");
    assert_eq!(out.width, 17, "the WHOLE wire, not the half named `w`");
    assert_eq!(out.bits, Bits::Small((0x5A << 9) | 3));
}

/// `wire out = ~a` — a unary cell, routed through `value::unary`.
#[test]
fn executes_a_unary_not_cell() {
    let design = unary_design(UnOp::BitNot);
    let module = lower(&design);
    let mut executor = Executor::new(&module);
    executor.set_input("a", v(0x0F, 8));
    executor.tick();
    let out = executor.get_output("out");
    assert_eq!(out.bits, Bits::Small(0xF0));
    assert_eq!(out.width, 8);
}

/// `out = if sel { a } else { b }`, both selector values exercised.
#[test]
fn executes_a_mux_cell() {
    let design = if_mux_design();
    let module = lower(&design);
    let mut executor = Executor::new(&module);
    executor.set_input("a", v(0xAA, 8));
    executor.set_input("b", v(0x55, 8));

    executor.set_input("sel", v(1, 1));
    executor.tick();
    assert_eq!(executor.get_output("out").bits, Bits::Small(0xAA));

    executor.set_input("sel", v(0, 1));
    executor.tick();
    assert_eq!(executor.get_output("out").bits, Bits::Small(0x55));
}

/// A `Dff`'s Q only moves at a clock edge: driving `d` alone changes
/// nothing until `tick`.
#[test]
fn executes_a_register_across_two_ticks() {
    let design = reg_design();
    let module = lower(&design);
    let mut executor = Executor::new(&module);

    assert_eq!(
        executor.get_output("q").bits,
        Bits::Small(0),
        "power-on Q, readable before the first edge"
    );

    executor.set_input("d", v(5, 8));
    assert_eq!(
        executor.get_output("q").bits,
        Bits::Small(0),
        "driving `d` does not move `q` — only a clock edge does"
    );

    executor.tick();
    assert_eq!(executor.get_output("q").bits, Bits::Small(5));

    executor.set_input("d", v(9, 8));
    executor.tick();
    assert_eq!(executor.get_output("q").bits, Bits::Small(9));
}

/// `rd = ram[addr]` with an unconditional `ram[addr] <- din` on `rise(clk)`.
/// The read port's nets carry the MEMORY's name (`lower` names `rdata` after
/// the memory), so `get_output("ram")` is how a read is observed.
#[test]
fn a_memory_write_commits_only_at_a_clock_edge() {
    let design = ram_design();
    let module = lower(&design);
    let mut executor = Executor::new(&module);

    executor.set_input("addr", v(2, 2));
    executor.set_input("din", v(0x11, 8));
    executor.tick();
    assert_eq!(executor.get_output("ram").bits, Bits::Small(0x11));

    // A new word is presented on `din` but no edge has happened yet.
    executor.set_input("din", v(0x22, 8));
    assert_eq!(
        executor.get_output("ram").bits,
        Bits::Small(0x11),
        "the pending write is not applied until the clock edge"
    );

    executor.tick();
    assert_eq!(executor.get_output("ram").bits, Bits::Small(0x22));
}

/// A clockless memory (a ROM: `wen` tied low, no `clock` pin at all). Every
/// address reads the `init` seed, because nothing can ever write it.
#[test]
fn a_never_written_memory_cell_reads_its_init_seed() {
    let mut design = ram_design();
    design.mems[0].clock = String::new();
    design.procs.clear();
    design.clocks.clear();
    design.inputs.retain(|s| s.name != "clk" && s.name != "din");
    let module = lower(&design);
    let mut executor = Executor::new(&module);

    executor.set_input("addr", v(3, 2));
    executor.tick();
    assert_eq!(
        executor.get_output("ram").bits,
        Bits::Small(0xA5),
        "an unwritten cell reads the memory's power-on seed, not zero"
    );
}

/// An extern instance's output is driverless by design. It needs no arm in
/// `eval_comb_cell` at all: its nets are never written, and `get_bits`
/// reports an undriven net as a whole-value `Val::unknown`.
#[test]
fn a_blackbox_output_reads_as_unknown() {
    let design = pll_design();
    let module = lower(&design);
    let mut executor = Executor::new(&module);
    executor.set_input("clk", v(1, 1));
    executor.tick();

    assert!(
        executor.get_output("u_clk_out").unknown,
        "an unconnected extern output is X, not a real value of 0"
    );
    // The instance's input side is a plain alias of `clk` (its comb driver is
    // `Ident("clk")`, so lowering repoints rather than allocating it any nets
    // of its own) — addressable anyway, because `Module::signals` records the
    // Bits it resolved to rather than relying on a net carrying its name.
    assert!(!executor.get_output("u_clk_in").unknown);
}
