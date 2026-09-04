use crate::ast::{Builtin, Expr, ExprKind};
use crate::elaborate::{Design, Signal, Width};
use crate::ir::lower;
use crate::span::Span;
use std::collections::BTreeMap;

use super::{ident, w};

/// `out y: bits[16] = extend(a, 16)` over an 8-bit unsigned input `a` — the
/// exact real-world shape `extend(1, N)`/`extend(x, N)` covers (GAP-1's own
/// measured blast radius: this is the ONLY way to size a literal).
fn extend_design(target_width: u32) -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func: Builtin::Extend,
                args: vec![
                    ident("a"),
                    Expr {
                        kind: ExprKind::Int {
                            value: (target_width as u128).into(),
                            raw: target_width.to_string(),
                        },
                        span: Span::default(),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    Design {
        module: "ext".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(target_width),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

#[test]
fn extend_grows_an_unsigned_input_by_padding_with_a_zero_constant() {
    let design = extend_design(16);
    let module = lower(&design);
    let (_, y_bits, _) = module.ports.iter().find(|(n, ..)| n == "y").unwrap();
    assert_eq!(y_bits.width(), 16);
    // Low 8 bits are `a`'s own nets (same nets, not copies) ...
    let (_, a_bits, _) = module.ports.iter().find(|(n, ..)| n == "a").unwrap();
    assert_eq!(&y_bits.0[..8], &a_bits.0[..]);
    // ... and the high 8 bits are driven by a REAL Const cell — not just
    // "validate() found nothing wrong" (validate's driven-set seeding is
    // direction-blind for ports and has no width formula for Const, so the
    // old validate-only assertion didn't actually prove this).
    let const_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, crate::ir::CellKind::Const { .. }))
        .collect();
    assert_eq!(
        const_cells.len(),
        1,
        "extend's zero-pad must be a real Const cell, not left dangling"
    );
    assert_eq!(
        const_cells[0].pins["out"].0,
        y_bits.0[8..].to_vec(),
        "the Const cell must drive exactly y's high 8 bits"
    );
    let errors = crate::ir::validate::validate(&module);
    assert_eq!(errors, Vec::new());
}

#[test]
fn extend_to_the_same_width_is_a_no_op() {
    let design = extend_design(8);
    let module = lower(&design);
    let (_, y_bits, _) = module.ports.iter().find(|(n, ..)| n == "y").unwrap();
    let (_, a_bits, _) = module.ports.iter().find(|(n, ..)| n == "a").unwrap();
    assert_eq!(y_bits, a_bits);
}

/// A value `arg_is_definitely_unsigned` cannot clear must be REFUSED, never
/// silently zero-extended. The argument here is a signed 8-bit input.
///
/// The plan's original draft of this test wrapped `a` in `signed(a)`, but
/// that shape was unreachable in Task 1: `Extend` lowers its argument
/// before it consults the guard, and `Builtin::SignedCast` had no lowering
/// until Task 2 — so it tripped the "builtin not yet lowered" catch-all
/// instead. A signed *signal* exercises the same guard on the same path.
/// See `extend_of_signed_cast_panics_loudly` below for the originally-drafted
/// `signed(a)` variant, now that `SignedCast` lowers.
#[test]
#[should_panic(expected = "cannot prove")]
fn extend_of_a_signed_value_panics_loudly_instead_of_silently_zero_extending() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func: Builtin::Extend,
                args: vec![
                    ident("a"),
                    Expr {
                        kind: ExprKind::Int {
                            value: 16u128.into(),
                            raw: "16".to_string(),
                        },
                        span: Span::default(),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "ext_signed".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: Width {
                bits: 8,
                signed: true,
            },
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(16),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    lower(&design);
}

/// The originally-drafted `signed(a)` variant of the test above — now
/// reachable since Task 2 lowers `Builtin::SignedCast` as an identity cast.
/// `extend(signed(a), 16)` must refuse just as loudly as a directly-signed
/// input. The mechanism is refusal by FALL-THROUGH, not by inspection:
/// `ExprKind::Call { func: SignedCast, .. }` matches no true-returning arm
/// in `arg_is_definitely_unsigned` (only `UnsignedCast`/`Encoding` do), so
/// it drops to the final `_ => false` catch-all and is refused immediately —
/// the inner `Ident("a")` is never reached at all. That matters: `a` here is
/// declared UNSIGNED, so an `arg_is_definitely_unsigned` that did "see
/// through" the cast to its argument would return `true` and silently
/// zero-extend a value the program explicitly asked to read as signed. If a
/// future change ever adds a `SignedCast` arm, this test is what must keep
/// failing.
#[test]
#[should_panic(expected = "cannot prove")]
fn extend_of_signed_cast_panics_loudly() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func: Builtin::Extend,
                args: vec![
                    Expr {
                        kind: ExprKind::Call {
                            func: Builtin::SignedCast,
                            args: vec![ident("a")],
                        },
                        span: Span::default(),
                    },
                    Expr {
                        kind: ExprKind::Int {
                            value: 16u128.into(),
                            raw: "16".to_string(),
                        },
                        span: Span::default(),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "ext_signed_cast".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(16),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    lower(&design);
}

#[test]
fn trunc_slices_the_low_bits() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func: Builtin::Trunc,
                args: vec![
                    ident("a"),
                    Expr {
                        kind: ExprKind::Int {
                            value: 4u128.into(),
                            raw: "4".to_string(),
                        },
                        span: Span::default(),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "trunc_mod".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(4),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);
    let (_, y_bits, _) = module.ports.iter().find(|(n, ..)| n == "y").unwrap();
    let (_, a_bits, _) = module.ports.iter().find(|(n, ..)| n == "a").unwrap();
    assert_eq!(y_bits.width(), 4);
    assert_eq!(&y_bits.0[..], &a_bits.0[..4]);
}

#[test]
fn signed_cast_is_a_pure_identity_no_new_cell() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func: Builtin::SignedCast,
                args: vec![ident("a")],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "sc".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(8),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);
    let (_, y_bits, _) = module.ports.iter().find(|(n, ..)| n == "y").unwrap();
    let (_, a_bits, _) = module.ports.iter().find(|(n, ..)| n == "a").unwrap();
    assert_eq!(y_bits, a_bits);
    assert_eq!(module.cells.len(), 0, "a pure cast must emit zero cells");
}

fn reduction_design(func: Builtin) -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func,
                args: vec![ident("a")],
            },
            span: Span::default(),
        },
    );
    Design {
        module: "red".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(1),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

#[test]
fn nand_composes_redand_then_logicnot() {
    let module = lower(&reduction_design(Builtin::Nand));
    assert_eq!(module.cells.len(), 2);
    assert!(matches!(module.cells[0].kind, crate::ir::CellKind::RedAnd));
    assert!(matches!(
        module.cells[1].kind,
        crate::ir::CellKind::LogicNot
    ));
    assert_eq!(crate::ir::validate::validate(&module), Vec::new());
}

#[test]
fn nor_composes_redor_then_logicnot() {
    let module = lower(&reduction_design(Builtin::Nor));
    assert_eq!(module.cells.len(), 2, "exactly two cells: reduce, then not");
    assert!(matches!(module.cells[0].kind, crate::ir::CellKind::RedOr));
    assert!(matches!(
        module.cells[1].kind,
        crate::ir::CellKind::LogicNot
    ));
}

#[test]
fn xnor_composes_redxor_then_logicnot() {
    let module = lower(&reduction_design(Builtin::Xnor));
    assert_eq!(module.cells.len(), 2, "exactly two cells: reduce, then not");
    assert!(matches!(module.cells[0].kind, crate::ir::CellKind::RedXor));
    assert!(matches!(
        module.cells[1].kind,
        crate::ir::CellKind::LogicNot
    ));
}

fn two_arg_design(func: Builtin) -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func,
                args: vec![ident("a"), ident("b")],
            },
            span: Span::default(),
        },
    );
    Design {
        module: "twoarg".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: w(8),
            },
            Signal {
                name: "b".into(),
                width: w(8),
            },
        ],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(8),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

#[test]
#[should_panic(expected = "does not lower")]
fn min_is_refused_loudly() {
    lower(&two_arg_design(Builtin::Min));
}

#[test]
#[should_panic(expected = "does not lower")]
fn max_is_refused_loudly() {
    lower(&two_arg_design(Builtin::Max));
}

#[test]
#[should_panic(expected = "does not lower")]
fn abs_is_refused_loudly() {
    lower(&reduction_design(Builtin::Abs));
}

/// `a = 0b0000_0001` (exactly one bit set) is chosen specifically because
/// `RedAnd`/`RedOr`/`RedXor` disagree on it (0, 1, 1 respectively) — a
/// composition that accidentally wired the wrong reduction cell to a given
/// builtin would compute a DIFFERENT wrong value here, not coincidentally
/// the right one, unlike an all-zeros or all-ones input where several wrong
/// wirings still happen to agree with the right answer.
#[test]
fn nand_executes_to_the_negated_and_reduction() {
    let module = lower(&reduction_design(Builtin::Nand));
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0b0000_0001, 8, false));
    ex.tick();
    assert_eq!(ex.get_output("y").bits, crate::bits::Bits::Small(1));
}

#[test]
fn nor_executes_to_the_negated_or_reduction() {
    let module = lower(&reduction_design(Builtin::Nor));
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0b0000_0001, 8, false));
    ex.tick();
    assert_eq!(ex.get_output("y").bits, crate::bits::Bits::Small(0));
}

#[test]
fn xnor_executes_to_the_negated_xor_reduction() {
    let module = lower(&reduction_design(Builtin::Xnor));
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0b0000_0001, 8, false));
    ex.tick();
    assert_eq!(ex.get_output("y").bits, crate::bits::Bits::Small(0));
}
