//! Emitter-local `Kind` inference — Stage 4, Phase A1b
//! (`docs/superpowers/specs/2026-07-19-emitter-self-determined-position-design.local.md`).
//!
//! Computes mimz's own width/signedness for an expression DIRECTLY from
//! the AST — no dependency on any prior pass having annotated anything.
//! This is deliberate: the checker and the emitter do not walk the same
//! `Expr` instances for `foreach`/`sync_loop`-lowered content (each of
//! the ~6 call sites across the codebase that call
//! `ast::lower_foreach_item`/`ast::lower_sync_loop` produces its OWN
//! fresh `Expr` tree), so an annotation set by one pass cannot reliably
//! reach another. Computing `Kind` fresh, every time, from the AST
//! alone works identically for top-level module items and for whatever
//! this emitter just produced via its own lowering calls.
//!
//! Calls into `mimz_core::width_rules` for every operator family it
//! already covers (shift, slice, lossless growth, width-matching) — this
//! is a second CALL SITE into those shared rules, not a second
//! implementation of them. Only `concat`/`replicate` get their own
//! logic here, since those stay checker-only (no simulator counterpart
//! to unify against, so no Class-A drift risk either way).

use std::collections::HashMap;

use crate::ast::{BinOp, Builtin, Expr, ExprKind};
use crate::width_rules::Kind;

/// mimz's own width/signedness for `expr`, computed directly from the
/// AST. `decls` maps every signal name declared in the CURRENT module
/// (port/wire/register) to its already-resolved `Kind` — built once per
/// module by `build_decls` (Task 2), reused across every expression in
/// that module's body.
///
/// Only handles the `ExprKind`s that can actually reach a Verilog
/// self-determined position (per mimz's own type rules): literals,
/// identifiers, unary, binary, concat, replicate, slice, the four
/// builtins. Bundles/enums/memories/arrays/match/`??` never appear
/// inside a concat/replicate/comparison/cast/slice-base position, so
/// this function does not need to handle them.
///
pub(crate) fn infer_kind(expr: &Expr, decls: &HashMap<String, Kind>) -> Kind {
    match &expr.kind {
        ExprKind::Int { value, .. } => Kind {
            width: min_width_for(*value),
            signed: false,
        },
        ExprKind::Bool(_) => Kind {
            width: 1,
            signed: false,
        },
        ExprKind::Ident(name) => *decls
            .get(name)
            .unwrap_or_else(|| panic!("emit_verilog::kinds::infer_kind: `{name}` not in decls")),
        ExprKind::Unary { op: _, expr: inner } => infer_kind(inner, decls),
        ExprKind::Binary { op, lhs, rhs } => infer_binary(*op, lhs, rhs, decls),
        ExprKind::Concat(parts) => Kind {
            width: parts.iter().map(|p| infer_kind(p, decls).width).sum(),
            signed: false,
        },
        ExprKind::Replicate { count: _, parts } => {
            // `count` is always a compile-time constant per the checker's
            // own `replicate_ty` — this function only needs the INNER
            // concat's width times some multiplier; the multiplier
            // itself isn't needed for this phase's self-determined check
            // (replication's own count position is checked separately,
            // see `self_determined.rs`), so this returns the inner
            // concat's per-iteration width, matching what a caller needs
            // when checking a replication's REPEATED PART, not the whole
            // replication's total width.
            Kind {
                width: parts.iter().map(|p| infer_kind(p, decls).width).sum(),
                signed: false,
            }
        }
        ExprKind::Slice { hi, lo, .. } => {
            // Slice bounds are always compile-time constant per the
            // checker's own `slice_ty` — this function doesn't need
            // `base`'s own Kind to compute the slice RESULT's width,
            // only `hi`/`lo`'s folded values. `slice_result` itself
            // needs `base_width` only to validate range, which this
            // call site doesn't need to re-validate (the checker already
            // did) — pass `u32::MAX` as a base_width wide enough that
            // the out-of-range branch never fires here.
            let hi_v = const_fold(hi);
            let lo_v = const_fold(lo);
            crate::width_rules::slice_result(u32::MAX, hi_v, lo_v)
                .expect("checker already validated this slice's bounds")
        }
        ExprKind::Call { func, args } => infer_call(*func, args, decls),
        other => panic!(
            "emit_verilog::kinds::infer_kind: {other:?} cannot appear in a \
             self-determined position (mimz's own type rules forbid it there)"
        ),
    }
}

/// The minimal bit width needed to represent `value` as an unsigned
/// literal (at least 1 bit) — mirrors the checker's own `min_bits`
/// (`checker/widths/mod.rs`), reimplemented here since that function is
/// `pub(super)`-scoped to the checker module tree, not reachable from
/// `emit_verilog`; this is a one-line arithmetic helper, not a rule with
/// any drift risk (there is nothing to "get wrong" differently between
/// two copies of `128 - value.leading_zeros()`).
fn min_width_for(value: u128) -> u32 {
    if value == 0 {
        1
    } else {
        (128 - value.leading_zeros()).max(1)
    }
}

/// True iff `kind` is a bare integer literal — the checker's untyped
/// `Ty::CtInt`, which adapts to a sized sibling operand rather than
/// carrying a fixed width of its own (see `infer_binary`'s matched-width
/// arm). Deliberately narrow: a literal wrapped in `Unary` (e.g. a
/// negated constant) is NOT recognized here — that shape doesn't appear
/// in this function's one caller's reproduced crash, and widening this
/// check to cover it would mean re-deriving how far the checker's own
/// const-folding reaches, which this file's doc comment already commits
/// to leaving to `mimz-core`'s checker rather than re-implementing here.
fn is_bare_int(kind: &ExprKind) -> bool {
    matches!(kind, ExprKind::Int { .. })
}

/// Fold a compile-time-constant expression (a slice bound, a replication
/// count) to its `u32` value. Slice bounds/replication counts are
/// guaranteed compile-time constant by the checker (`slice_ty`/
/// `replicate_ty`) before this code ever runs, so only the literal case
/// needs handling — anything else is a structurally-impossible input at
/// this call site.
fn const_fold(expr: &Expr) -> u32 {
    match &expr.kind {
        ExprKind::Int { value, .. } => *value as u32,
        other => panic!(
            "emit_verilog::kinds::const_fold: {other:?} is not a literal — \
             the checker guarantees slice bounds/replication counts are \
             compile-time constant before emission"
        ),
    }
}

fn infer_binary(op: BinOp, lhs: &Expr, rhs: &Expr, decls: &HashMap<String, Kind>) -> Kind {
    let l = infer_kind(lhs, decls);
    match op {
        BinOp::Shl | BinOp::Shr => {
            let r = infer_kind(rhs, decls);
            crate::width_rules::shift_result(l, r)
                .expect("checker already validated this shift's operand kinds")
        }
        BinOp::Add | BinOp::Sub => {
            let r = infer_kind(rhs, decls);
            crate::width_rules::lossless_result(l, r, false)
                .expect("checker already validated this operator's operand kinds")
        }
        BinOp::Mul => {
            let r = infer_kind(rhs, decls);
            crate::width_rules::lossless_result(l, r, true)
                .expect("checker already validated this operator's operand kinds")
        }
        BinOp::AddWrap
        | BinOp::SubWrap
        | BinOp::MulWrap
        | BinOp::BitAnd
        | BinOp::BitOr
        | BinOp::BitXor => {
            let r = infer_kind(rhs, decls);
            // A bare integer literal is the checker's `Ty::CtInt` — untyped,
            // adapting to a sized sibling operand's width/signedness rather
            // than carrying its own minimal natural width (`checker::widths::
            // ops::matched_ty`'s `(Ty::CtInt(v), t) | (t, Ty::CtInt(v))`
            // arms, which return the SIZED side's type unconditionally, no
            // equality check). `infer_kind`'s own `Int` arm has no such
            // context, so without this, `cnt +% 1` (`cnt: bits[26]`) — a
            // perfectly ordinary, checker-valid program — infers the
            // literal `1` at its own 1-bit width and panics the
            // `matched_result` fallback below on a mismatch the checker
            // never considered one.
            match (is_bare_int(&lhs.kind), is_bare_int(&rhs.kind)) {
                (true, false) => r,
                (false, true) => l,
                _ => crate::width_rules::matched_result(l, r)
                    .expect("checker already validated this operator's operand kinds"),
            }
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Kind {
            width: 1,
            signed: false,
        },
        BinOp::LogicAnd | BinOp::LogicOr => Kind {
            width: 1,
            signed: false,
        },
        BinOp::Coalesce => {
            panic!(
                "emit_verilog::kinds::infer_binary: `??` cannot appear in a self-determined position"
            )
        }
    }
}

fn infer_call(func: Builtin, args: &[Expr], decls: &HashMap<String, Kind>) -> Kind {
    match func {
        Builtin::Extend | Builtin::Trunc => {
            let n = const_fold(&args[1]);
            let base_signed = infer_kind(&args[0], decls).signed;
            Kind {
                width: n,
                signed: base_signed,
            }
        }
        Builtin::SignedCast => Kind {
            width: infer_kind(&args[0], decls).width,
            signed: true,
        },
        Builtin::UnsignedCast => Kind {
            width: infer_kind(&args[0], decls).width,
            signed: false,
        },
        other => panic!(
            "emit_verilog::kinds::infer_call: {other:?} cannot appear in a \
             self-determined position"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn ident(name: &str) -> Expr {
        Expr {
            kind: ExprKind::Ident(name.to_string()),
            span: Span::new(0, 0),
        }
    }

    fn int(value: u128) -> Expr {
        Expr {
            kind: ExprKind::Int {
                value,
                raw: value.to_string(),
            },
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn ident_looks_up_declared_kind() {
        let mut decls = HashMap::new();
        decls.insert(
            "p0".to_string(),
            Kind {
                width: 8,
                signed: false,
            },
        );
        let e = ident("p0");
        assert_eq!(
            infer_kind(&e, &decls),
            Kind {
                width: 8,
                signed: false
            }
        );
    }

    #[test]
    fn literal_gets_its_minimal_width() {
        let decls = HashMap::new();
        assert_eq!(
            infer_kind(&int(3), &decls),
            Kind {
                width: 2,
                signed: false
            }
        );
        assert_eq!(
            infer_kind(&int(0), &decls),
            Kind {
                width: 1,
                signed: false
            }
        );
    }

    #[test]
    fn concat_sums_part_widths() {
        let decls = HashMap::new();
        let e = Expr {
            kind: ExprKind::Concat(vec![int(3), int(1)]),
            span: Span::new(0, 0),
        };
        // int(3) -> width 2, int(1) -> width 1, sum = 3
        assert_eq!(
            infer_kind(&e, &decls),
            Kind {
                width: 3,
                signed: false
            }
        );
    }

    #[test]
    fn lossless_add_grows_by_one_bit() {
        let mut decls = HashMap::new();
        decls.insert(
            "p0".to_string(),
            Kind {
                width: 8,
                signed: false,
            },
        );
        decls.insert(
            "p1".to_string(),
            Kind {
                width: 8,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::Binary {
                op: BinOp::Add,
                lhs: Box::new(ident("p0")),
                rhs: Box::new(ident("p1")),
            },
            span: Span::new(0, 0),
        };
        assert_eq!(
            infer_kind(&e, &decls),
            Kind {
                width: 9,
                signed: false
            }
        );
    }

    #[test]
    fn wrap_add_with_a_narrower_bare_literal_adapts_to_the_sized_operand() {
        // Regression: `cnt +% 1` (`cnt: bits[26]`) is an ordinary,
        // checker-valid program (`checker::widths::ops::matched_ty` lets a
        // bare `Ty::CtInt` literal adapt to its sized sibling's width, no
        // equality check) — but `infer_binary`'s `AddWrap` arm used to
        // call `infer_kind` on `1` independently, getting its own 1-bit
        // natural width, then PANICKED calling `matched_result(Kind{26},
        // Kind{1})` on a mismatch the checker never considered one. Found
        // by `fuzz_targets/pretty_roundtrip.rs` fuzzing a doubled
        // `cnt +% 1 +% 1` (the inner `cnt +% 1` becomes a hoist-candidate
        // child, which is the call path that reaches `infer_kind`; a
        // top-level `cnt <- cnt +% 1` never does).
        let mut decls = HashMap::new();
        decls.insert(
            "cnt".to_string(),
            Kind {
                width: 26,
                signed: false,
            },
        );
        let lit_rhs = Expr {
            kind: ExprKind::Binary {
                op: BinOp::AddWrap,
                lhs: Box::new(ident("cnt")),
                rhs: Box::new(int(1)),
            },
            span: Span::new(0, 0),
        };
        let lit_lhs = Expr {
            kind: ExprKind::Binary {
                op: BinOp::AddWrap,
                lhs: Box::new(int(1)),
                rhs: Box::new(ident("cnt")),
            },
            span: Span::new(0, 0),
        };
        let expected = Kind {
            width: 26,
            signed: false,
        };
        assert_eq!(infer_kind(&lit_rhs, &decls), expected);
        assert_eq!(infer_kind(&lit_lhs, &decls), expected);
    }
}
