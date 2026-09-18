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
    assert_eq!(&y_bits.nets[..8], &a_bits.nets[..]);
    // ... and the high 8 bits are driven by a REAL Const cell — not just
    // "validate() found nothing wrong" (validate's driven-set seeding is
    // direction-blind for ports and has no width formula for Const, so the
    // old validate-only assertion didn't actually prove this).
    //
    // UPDATED by GAP-1 Task 6: the zero-pad is now a single 1-bit Const
    // cell whose net is REPLICATED across every pad position (uniform with
    // the sign-extend branch's MSB-replication), not one N-bit Const cell —
    // see `Builtin::Extend`'s doc.
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
    assert_eq!(const_cells[0].pins["out"].width(), 1);
    let pad_net = const_cells[0].pins["out"].nets[0];
    assert!(
        y_bits.nets[8..].iter().all(|&n| n == pad_net),
        "the Const cell's single net must drive every one of y's high 8 bits"
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

/// UPDATED by GAP-1 Task 6: `extend` of a genuinely signed value used to be
/// REFUSED loudly (`arg_is_definitely_unsigned` couldn't clear it, and
/// `ir::Bits` had no signed bit to decide zero- vs sign-extension with). Now
/// `ir::Bits::signed` answers this directly and `extend` sign-extends for
/// real: replicate the MSB, not zero. The argument here is a signed 8-bit
/// input, `a = 0xFF` (i.e. `-1`), so a correct sign-extend produces `0xFFFF`
/// (all-ones), while an (incorrect) zero-extend would produce `0x00FF`.
#[test]
fn extend_of_a_signed_value_sign_extends_instead_of_zero_extending() {
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
    let module = lower(&design);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFF, 8, false));
    ex.tick();
    assert_eq!(
        ex.get_output("y").bits,
        crate::bits::Bits::Small(0xFFFF),
        "extend must sign-extend a: signed[8] = -1 to y: bits[16] = 0xFFFF, \
         not zero-extend it to 0x00FF"
    );
}

/// The `signed(a)` variant of the test above — `a` is declared UNSIGNED, so
/// the OLD heuristic (`arg_is_definitely_unsigned`) would have "seen
/// through" a bare `Ident` and (wrongly) zero-extended it; the cast makes it
/// definitely signed, and `Builtin::SignedCast` now marks `Bits::signed`
/// directly (GAP-1 Task 6) rather than relying on shape-recognition.
/// `extend(signed(a), 16)` must sign-extend just like a directly-signed
/// input above.
#[test]
fn extend_of_signed_cast_sign_extends() {
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
    let module = lower(&design);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFF, 8, false));
    ex.tick();
    assert_eq!(
        ex.get_output("y").bits,
        crate::bits::Bits::Small(0xFFFF),
        "signed(a) = -1 extended to 16 bits must sign-extend to 0xFFFF"
    );
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
    assert_eq!(&y_bits.nets[..], &a_bits.nets[..4]);
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
    // UPDATED by GAP-1 Task 6: `signed(x)` still repoints at `x`'s own nets
    // (allocates nothing, zero cells) — but it is no longer a byte-for-byte
    // identity on `Bits` itself, since flipping `signed` to `true` is the
    // ENTIRE point of the cast (previously inexpressible: `ir::Bits` had no
    // sign bit at all).
    assert_eq!(
        y_bits.nets, a_bits.nets,
        "same underlying nets, no new cell"
    );
    assert!(y_bits.signed, "signed(x) must mark its result as signed");
    assert!(!a_bits.signed, "`a` itself stays declared unsigned");
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
                width: Width {
                    bits: 8,
                    signed: true,
                },
            },
            Signal {
                name: "b".into(),
                width: Width {
                    bits: 8,
                    signed: true,
                },
            },
        ],
        outputs: vec![Signal {
            name: "y".into(),
            width: Width {
                bits: 8,
                signed: true,
            },
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

/// UPDATED by GAP-1 Task 6: `min`/`max` now lower for real, as `Lt` + `Mux`
/// over the checker-guaranteed-matched (here: both signed) operands. `a =
/// -5` (0xFB, signed[8]), `b = 3`: `min` must pick `a` (the smaller value
/// under a SIGNED reading — an unsigned comparison would read `a` as 251
/// and wrongly pick `b`).
#[test]
fn min_lowers_and_picks_the_smaller_signed_operand() {
    let module = lower(&two_arg_design(Builtin::Min));
    assert!(
        module
            .cells
            .iter()
            .any(|c| matches!(c.kind, crate::ir::CellKind::Lt { signed: true })),
        "min over two signed operands must use a SIGNED Lt cell"
    );
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFB, 8, true)); // -5
    ex.set_input("b", crate::value::Val::new(3, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("y").bits,
        crate::bits::Bits::Small(0xFB),
        "min(-5, 3) must be -5, not 3 (which an unsigned reading would wrongly pick)"
    );
}

/// The `max` mirror of the test above: `max(-5, 3)` must be `3`.
#[test]
fn max_lowers_and_picks_the_larger_signed_operand() {
    let module = lower(&two_arg_design(Builtin::Max));
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFB, 8, true)); // -5
    ex.set_input("b", crate::value::Val::new(3, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("y").bits,
        crate::bits::Bits::Small(3),
        "max(-5, 3) must be 3"
    );
}

fn signed_abs_design() -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Call {
                func: Builtin::Abs,
                args: vec![ident("a")],
            },
            span: Span::default(),
        },
    );
    Design {
        module: "abs_mod".to_string(),
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
            width: w(9),
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

/// UPDATED by GAP-1 Task 6: `abs` now lowers for real, as `Neg` + `Mux`
/// selected on `a`'s own sign bit. `abs(-128)` (`signed[8]`'s MIN value)
/// is the case that needs the extra growth bit: `128` does not fit back
/// into 8 bits.
#[test]
fn abs_lowers_and_negates_only_when_the_operand_is_negative() {
    let module = lower(&signed_abs_design());
    let (_, y_bits, _) = module.ports.iter().find(|(n, ..)| n == "y").unwrap();
    assert_eq!(
        y_bits.width(),
        9,
        "abs(Signed(8)) grows to 9 bits, like abs(MIN)"
    );

    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(5, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("y").bits,
        crate::bits::Bits::Small(5),
        "abs(5) must be 5 (no negation needed)"
    );

    ex.set_input("a", crate::value::Val::new(0x80, 8, true)); // -128, signed[8] MIN
    ex.tick();
    assert_eq!(
        ex.get_output("y").bits,
        crate::bits::Bits::Small(128),
        "abs(-128) must be 128, which needs the extra growth bit"
    );
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
