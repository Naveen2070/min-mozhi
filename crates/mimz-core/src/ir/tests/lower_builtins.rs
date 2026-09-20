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
        // `signed[8]`, not `bits[8]`: E0401 ("expected `bits[8]`, found
        // `signed[8]`") rejects an output whose declared kind disagrees with
        // its driver, so a `bits[8]` output driven by `signed(a)` is not a
        // `Design` the real pipeline can produce. It mattered once
        // `LowerCtx::resolve` started stamping the DECLARED signedness onto a
        // resolved signal (GAP-1 Task 6 round 4, F1) — which is exactly what
        // `emit_verilog`'s own `build_decls` does.
        outputs: vec![Signal {
            name: "y".into(),
            width: crate::elaborate::Width {
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

// ---------------------------------------------------------------------------
// GAP-1 Task 6 fix round: COMPUTED operands.
//
// Every test above feeds a builtin a bare signal, so every one of them reads
// a `Bits::signed` that was stamped at port-allocation time. The flag was
// never computed for a DERIVED value (`lower_binop`'s results all came back
// `signed: false`), so each Task 6 feature silently reverted to its unsigned
// reading the moment its operand was an expression rather than a port. These
// use the real source pipeline (so the checker assigns the declared types)
// and assert VALUES through the IR executor, not just a clean `validate`.
// ---------------------------------------------------------------------------

use super::lower_valid;

/// Finding 1: `extend` of a computed signed value must SIGN-extend.
/// `a - b` is `signed[9]`; with `a = 0, b = 1` it is `-1` (`0x1FF`), so a
/// 16-bit `extend` must be `0xFFFF`, not the `0x01FF` a zero-pad gives.
#[test]
fn extend_of_a_computed_signed_expression_sign_extends() {
    let src = "module M {\n  in a: signed[8]\n  in b: signed[8]\n  out o: signed[16]\n  o = extend(a - b, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0, 8, true));
    ex.set_input("b", crate::value::Val::new(1, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(0xFFFF),
        "extend(a - b, 16) over signed operands must sign-extend -1 to 0xFFFF"
    );
}

/// Finding 2: `min` over a COMPUTED signed operand must compare signed.
/// `a - b` is `signed[9]` = `-5`, `c` is `signed[9]` = `3`; an unsigned `Lt`
/// reads `-5` as `507` and wrongly picks `3`.
#[test]
fn min_over_a_computed_signed_operand_compares_signed() {
    let src = "module M {\n  in a: signed[8]\n  in b: signed[8]\n  in c: signed[9]\n  out o: signed[9]\n  o = min(a - b, c)\n}\n";
    let module = lower_valid(src);
    assert!(
        module
            .cells
            .iter()
            .any(|c| matches!(c.kind, crate::ir::CellKind::Lt { signed: true })),
        "min over a computed signed operand must use a SIGNED Lt cell"
    );
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0, 8, true));
    ex.set_input("b", crate::value::Val::new(5, 8, true));
    ex.set_input("c", crate::value::Val::new(3, 9, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(0x1FB), // -5 in 9 bits
        "min(a - b, c) must be -5, not 3"
    );
}

/// Finding 3: a LITERAL operand of `min`/`max` must be re-sized to its
/// sibling's width (the checker's `matched_ty` types it that way) AND
/// inherit its signedness — otherwise the `Lt` gets a 2-bit `Const` on an
/// 8-bit pin (`WidthMismatch`, an invalid module). `max(x, 0)` is the stock
/// clamp idiom, so both directions are covered here.
#[test]
fn min_max_with_a_literal_operand_size_and_sign_the_literal_to_its_sibling() {
    let src = "module M {\n  in a: signed[8]\n  out lo: signed[8]\n  out hi: signed[8]\n  lo = min(a, 3)\n  hi = max(a, 0)\n}\n";
    let module = lower_valid(src);
    // One `Const` per literal, not two: the foldable side is lowered once,
    // already at its sibling's width, instead of being lowered naturally and
    // then re-lowered (which left the natural-width cell orphaned).
    assert_eq!(
        module
            .cells
            .iter()
            .filter(|c| matches!(c.kind, crate::ir::CellKind::Const { .. }))
            .count(),
        2,
        "no orphaned natural-width Const cells"
    );
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFB, 8, true)); // -5
    ex.tick();
    assert_eq!(
        ex.get_output("lo").bits,
        crate::bits::Bits::Small(0xFB),
        "min(-5, 3) must be -5"
    );
    assert_eq!(
        ex.get_output("hi").bits,
        crate::bits::Bits::Small(0),
        "max(-5, 0) must clamp to 0"
    );
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(7, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("lo").bits,
        crate::bits::Bits::Small(3),
        "min(7, 3) must be 3"
    );
    assert_eq!(
        ex.get_output("hi").bits,
        crate::bits::Bits::Small(7),
        "max(7, 0) must be 7"
    );
}

/// Finding 4: `abs`/unary `-` over a COMPUTED signed operand must grow by
/// one bit, the same as over a bare signed port. `a - b` is `signed[9]`, so
/// both results are `signed[10]` — a 9-bit result is a `PortWidthMismatch`.
#[test]
fn abs_and_neg_over_a_computed_signed_operand_grow_by_one_bit() {
    let src = "module M {\n  in a: signed[8]\n  in b: signed[8]\n  out o: signed[10]\n  out n: signed[10]\n  o = abs(a - b)\n  n = -(a - b)\n}\n";
    let module = lower_valid(src);
    for name in ["o", "n"] {
        let port = module.ports.iter().find(|(p, ..)| p == name).unwrap();
        assert_eq!(port.1.width(), 10, "`{name}` must be 10 bits wide");
    }
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0, 8, true));
    ex.set_input("b", crate::value::Val::new(5, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(5),
        "abs(0 - 5) must be 5"
    );
    assert_eq!(
        ex.get_output("n").bits,
        crate::bits::Bits::Small(5),
        "-(0 - 5) must be 5"
    );
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(5, 8, true));
    ex.set_input("b", crate::value::Val::new(0, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(5),
        "abs(5 - 0) must be 5"
    );
    assert_eq!(
        ex.get_output("n").bits,
        crate::bits::Bits::Small(0x3FB), // -5 in 10 bits
        "-(5 - 0) must be -5"
    );
}

// ---------------------------------------------------------------------------
// GAP-1 Task 6 fix round 2: operand SHAPES the round-1 tests never reached.
//
// Round 1 only ever built its computed operands out of binops, so two whole
// families of `Bits`-producing sites kept the wrong flag:
//   * slices and bit-selects INHERITED their base's `signed`, but
//     `width_rules::slice_result` returns `signed: false` unconditionally (it
//     exists precisely to keep BUG-21 from coming back), and
//     `checker/widths/expr/lvalue.rs` types a bit-select as `Ty::Bit`;
//   * `if`/`match` results come out of `Module::alloc_bits`, which is always
//     unsigned, even though the checker forces both branches to one `Ty`
//     (`emit_verilog/kinds.rs` derives the mux's `Kind` from its branches).
// ---------------------------------------------------------------------------

/// B1: a bit-select with a RUNTIME index lowers as `base >> i` then bit 0.
/// The shift keeps its left operand's signedness (`shift_result`), but the
/// bit-select on top of it is a bit — always unsigned. `a = 0x80`, `i = 7`
/// selects the sign bit, so `extend(a[i], 8)` is `0x01`; sign-extending it
/// (the bug) gives `0xFF`.
#[test]
fn a_runtime_bit_select_is_unsigned_even_over_a_signed_base() {
    let src = "module M {\n  in a: signed[8]\n  in i: bits[3]\n  out o: bits[8]\n  o = extend(a[i], 8)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0x80, 8, true));
    ex.set_input("i", crate::value::Val::new(7, 3, false));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(0x01),
        "a[i] is a BIT (unsigned) even when `a` is signed — extend must zero-pad"
    );
}

/// B2: a constant-index bit-select and a slice are both unsigned too, for
/// the same reason. `a = 0xFF` (i.e. `-1`): `a[7:4]` is `0x0F` and `a[7]` is
/// `0x01` once extended; inheriting `a`'s sign would give `0xFF` for both.
#[test]
fn a_slice_and_a_constant_bit_select_are_unsigned_even_over_a_signed_base() {
    let src = "module M {\n  in a: signed[8]\n  out hi: bits[8]\n  out top: bits[8]\n  hi = extend(a[7:4], 8)\n  top = extend(a[7], 8)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("hi").bits,
        crate::bits::Bits::Small(0x0F),
        "a[7:4] is unsigned bits[4] — extend must zero-pad, not sign-extend"
    );
    assert_eq!(
        ex.get_output("top").bits,
        crate::bits::Bits::Small(0x01),
        "a[7] is a BIT — extend must zero-pad"
    );
}

/// B2, the comparison shape: `min` over two slices must compare UNSIGNED.
/// `a = 0xF0`/`b = 0x70` slice to `15` and `7`; a signed 4-bit reading makes
/// `15` look like `-1` and wrongly calls it the smaller.
#[test]
fn min_over_two_slices_of_signed_bases_compares_unsigned() {
    let src = "module M {\n  in a: signed[8]\n  in b: signed[8]\n  out o: bits[4]\n  o = min(a[7:4], b[7:4])\n}\n";
    let module = lower_valid(src);
    assert!(
        module
            .cells
            .iter()
            .any(|c| matches!(c.kind, crate::ir::CellKind::Lt { signed: false })),
        "a slice is unsigned, so min over two slices must use an UNSIGNED Lt"
    );
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xF0, 8, true));
    ex.set_input("b", crate::value::Val::new(0x70, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(7),
        "min(15, 7) over unsigned 4-bit slices must be 7"
    );
}

/// B3: an `if`-expression's result carries its branches' signedness — the
/// checker unifies both branches to one `Ty`, so there is a single flag to
/// take. Without it `extend` zero-pads a negative value (finding 1's shape)
/// and `-`/`abs` skip their growth bit (finding 4's shape, a
/// `PortWidthMismatch` that `lower_valid` catches).
#[test]
fn an_if_expressions_result_inherits_its_branches_signedness() {
    let src = "module M {\n  in s: bits[1]\n  in a: signed[8]\n  in b: signed[8]\n  out e: signed[16]\n  out n: signed[9]\n  out v: signed[9]\n  e = extend(if s == 1 { a } else { b }, 16)\n  n = -(if s == 1 { a } else { b })\n  v = abs(if s == 1 { a } else { b })\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("s", crate::value::Val::new(1, 1, false));
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true)); // -1
    ex.set_input("b", crate::value::Val::new(0, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "extend of a signed if-expression must sign-extend -1"
    );
    assert_eq!(
        ex.get_output("n").bits,
        crate::bits::Bits::Small(1),
        "-(-1) must be 1"
    );
    assert_eq!(
        ex.get_output("v").bits,
        crate::bits::Bits::Small(1),
        "abs(-1) must be 1"
    );
}

/// B3's `match` half — `lower_match` folds its mux chain arm by arm, so the
/// flag has to survive every step, not just a single two-way mux.
#[test]
fn a_match_expressions_result_inherits_its_arms_signedness() {
    let src = "module M {\n  in s: bits[2]\n  in a: signed[8]\n  in b: signed[8]\n  out e: signed[16]\n  e = extend(match s {\n    0 => a\n    1 => b\n    _ => a\n  }, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("s", crate::value::Val::new(1, 2, false));
    ex.set_input("a", crate::value::Val::new(0, 8, true));
    ex.set_input("b", crate::value::Val::new(0xFF, 8, true)); // -1
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "extend of a signed match expression must sign-extend -1"
    );
}

// ---------------------------------------------------------------------
// Fix round 3 — the "a compile-time constant has no signedness of its
// own" family (F1-F4). Every one of these is the SAME disease: a value
// whose checker `Ty` is signed loses that flag in the IR because one
// side/branch of it is a literal, and `lower_const` builds every literal
// unsigned. See the round-3 section of
// `.superpowers/sdd/2026-09-10-ir-base-gap-closure.local/task-6-report.md`.
// ---------------------------------------------------------------------

/// F1 — an ordering comparison against a literal whose NATURAL width
/// already equals its sibling's. The literal-resize path (which stamps
/// the sibling's sign) never fires, so `cmp_signed` has to come from the
/// sibling directly rather than from "both sides are signed".
#[test]
fn a_comparison_against_a_same_width_literal_is_still_signed() {
    let src = "module M {\n  in a: signed[8]\n  out o: bit\n  o = a < -128\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(0),
        "0 < -128 is false; read unsigned it would be 0 < 128 == true"
    );
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true)); // -1
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        crate::bits::Bits::Small(0),
        "-1 < -128 is false"
    );
}

/// F2 — `if c { x } else { 0 }`, the stock clamp/default idiom. The
/// literal branch is unsigned, so a `&&` merge zeroed the whole mux's
/// flag and `extend` zero-padded a genuinely signed value.
#[test]
fn an_if_expression_with_a_literal_branch_keeps_the_other_branchs_sign() {
    let src = "module M {\n  in s: bit\n  in a: signed[8]\n  out e: signed[16]\n  \
               e = extend(if s { a } else { 0 }, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("s", crate::value::Val::new(1, 1, false));
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true)); // -1
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "the `then` branch is signed[8] = -1, so extend must sign-extend"
    );
    ex.set_input("s", crate::value::Val::new(0, 1, false));
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0),
        "the literal branch still reads 0"
    );
}

/// F2's `match` half — same shape, folded arm by arm.
#[test]
fn a_match_with_a_literal_arm_keeps_the_other_arms_sign() {
    let src = "module M {\n  in s: bits[2]\n  in a: signed[8]\n  out e: signed[16]\n  \
               e = extend(match s {\n    0 => 0\n    1 => a\n    _ => 0\n  }, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("s", crate::value::Val::new(1, 2, false));
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true)); // -1
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "a match whose other arms are literals is still signed[8]"
    );
}

/// F3 — a `fn` whose result is effectively a literal. There is no
/// sibling to inherit from here: the signedness is the fn's DECLARED
/// return type, which `lower_fn_stmts` never stamped.
#[test]
fn a_fn_returning_a_literal_carries_its_declared_return_signedness() {
    let src = "fn lo(x: signed[8]) -> signed[8] {\n  if x < 0 {\n    return -1\n  }\n  \
               x\n}\n\nmodule M {\n  in a: signed[8]\n  out e: signed[16]\n  \
               e = extend(lo(a), 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xF0, 8, true)); // -16 -> -1
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "the fn declares `-> signed[8]`, so its literal `-1` return is signed"
    );
    ex.set_input("a", crate::value::Val::new(5, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(5),
        "the non-literal path still works"
    );
}

/// F4 — a signed memory's read value. `checker/widths/expr/lvalue.rs`
/// types `m[addr]` as `Ty::Signed(width)` when the memory's element type
/// is signed, and `emit_verilog/kinds.rs` agrees; the IR read port was
/// always allocated unsigned.
#[test]
fn a_signed_memorys_read_value_is_signed() {
    let src = "module M {\n  clock clk\n  in we: bit\n  in addr: bits[2]\n  \
               in wdata: signed[8]\n  out e: signed[16]\n  mem m: signed[8][4] = 0\n  \
               on rise(clk) {\n    if we {\n      m[addr] <- wdata\n    }\n  }\n  \
               e = extend(m[addr], 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("we", crate::value::Val::new(1, 1, false));
    ex.set_input("addr", crate::value::Val::new(2, 2, false));
    ex.set_input("wdata", crate::value::Val::new(0xFF, 8, true)); // -1
    ex.tick();
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "a signed[8] memory word reads back signed, so extend sign-extends -1"
    );
}

/// Beyond the reported findings: a mux nested inside another mux, both
/// with a literal branch. The flag has to survive every level.
#[test]
fn a_nested_literal_branch_mux_stays_signed_at_every_level() {
    let src = "module M {\n  in s: bit\n  in t: bit\n  in a: signed[8]\n  out e: signed[16]\n  \
               e = extend(if s { if t { a } else { 0 } } else { 0 }, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("s", crate::value::Val::new(1, 1, false));
    ex.set_input("t", crate::value::Val::new(1, 1, false));
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true));
    ex.tick();
    assert_eq!(ex.get_output("e").bits, crate::bits::Bits::Small(0xFFFF));
}

/// F8 — found by probing beyond the reported findings, and broken the same
/// way F3 was: a `fn` ARGUMENT that is a compile-time constant took neither
/// the parameter's declared width nor its signedness, so `ext(-1)` bound a
/// 1-bit `1` to a `signed[8]` parameter.
#[test]
fn a_literal_fn_argument_takes_the_parameters_declared_width_and_sign() {
    let src = "fn ext(x: signed[8]) -> signed[16] {\n  extend(x, 16)\n}\n\n\
               module M {\n  in a: bit\n  out e: signed[16]\n  e = ext(-1)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0, 1, false));
    ex.tick();
    assert_eq!(ex.get_output("e").bits, crate::bits::Bits::Small(0xFFFF));
}

/// Beyond the reported findings (checked, already correct): reading an
/// element out of a signed-element array parameter.
#[test]
fn an_array_element_read_keeps_the_elements_signedness() {
    let src = "fn pick(vals: signed[8][4], i: bits[2]) -> signed[16] {\n  extend(vals[i], 16)\n}\n\n\
               module M {\n  in a: signed[8]\n  in b: signed[8]\n  in i: bits[2]\n  \
               out e: signed[16]\n  e = pick([a, b, a, b], i)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true));
    ex.set_input("b", crate::value::Val::new(1, 8, true));
    ex.set_input("i", crate::value::Val::new(0, 2, false));
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "vals[0] = -1"
    );
}

/// `min`/`max` against a literal: the comparison was already signed (round
/// 1), but the RESULT went through `push_mux_cell`, whose `&&` let the
/// literal operand veto it — the clamp idiom, same disease as F2.
#[test]
fn min_max_against_a_literal_produce_a_signed_result() {
    let src = "module M {\n  in a: signed[8]\n  out e: signed[16]\n  out f: signed[16]\n  \
               e = extend(min(a, 0), 16)\n  f = extend(max(a, 0), 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = crate::ir::exec::Executor::new(&module);
    ex.set_input("a", crate::value::Val::new(0xFF, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        crate::bits::Bits::Small(0xFFFF),
        "min(-1,0) = -1, sign-extended"
    );
    assert_eq!(
        ex.get_output("f").bits,
        crate::bits::Bits::Small(0),
        "max(-1,0) = 0"
    );
}
