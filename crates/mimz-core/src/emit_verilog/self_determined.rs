//! Verilog's own self-determined-width rule — Stage 4, Phase A1b
//! (`docs/superpowers/specs/2026-07-19-emitter-self-determined-position-design.local.md`).
//!
//! What real Verilog computes as an expression's width when it lands in
//! a self-determined position (concat member, replication's repeated
//! part/count, comparison operand, `$signed`/`$unsigned` argument) —
//! NOT mimz's own semantics (that's `kinds::infer_kind`). Confirmed
//! empirically against real `iverilog`, matching this codebase's
//! existing convention for BUG-18/19/20/21's own investigations.

use std::collections::HashMap;

use crate::ast::{BinOp, Builtin, Expr, ExprKind};
use crate::width_rules::Kind;

use super::kinds::infer_kind;

/// What Verilog would compute as `expr`'s width in a self-determined
/// position. `None` means "no Verilog-specific rule differs from
/// mimz's own here" (a plain identifier, an explicitly-sized literal) —
/// nothing for the caller to compare against.
pub(crate) fn verilog_self_determined_kind(
    expr: &Expr,
    decls: &HashMap<String, Kind>,
) -> Option<Kind> {
    match &expr.kind {
        ExprKind::Ident(_) | ExprKind::Int { .. } | ExprKind::Bool(_) => None,
        ExprKind::Binary { op, lhs, rhs } => match op {
            // Comparisons are always 1-bit self-determined regardless of
            // operand kind — same as mimz's own rule, so no mismatch is
            // possible; `None` (nothing to check).
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => None,
            // Every other binary operator: Verilog self-determines each
            // operand at its OWN width (no growth, no context), then
            // takes the max — the exact "matched" rule, applied
            // uniformly, not just to the width-matching family.
            _ => {
                let l = self_determined_operand_width(lhs, decls);
                let r = self_determined_operand_width(rhs, decls);
                Some(Kind {
                    width: l.max(r),
                    signed: infer_kind(expr, decls).signed,
                })
            }
        },
        ExprKind::Call {
            func: Builtin::SignedCast | Builtin::UnsignedCast,
            args,
        } => {
            // `$signed`/`$unsigned`'s argument is self-determined at its
            // own width (confirmed empirically during BUG-18/19/20/21's
            // investigations) — same width mimz's own model gives, UNLESS
            // the argument is itself a mismatched sub-expression, which
            // is caught by recursing into it, not by this call site.
            verilog_self_determined_kind(&args[0], decls)
        }
        _ => None,
    }
}

/// A single operand's OWN self-determined width, ignoring any
/// surrounding context — recurses through the same binary-operator rule
/// so a NESTED mismatch is visible to the caller's `l.max(r)` too.
fn self_determined_operand_width(expr: &Expr, decls: &HashMap<String, Kind>) -> u32 {
    verilog_self_determined_kind(expr, decls)
        .unwrap_or_else(|| infer_kind(expr, decls))
        .width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ExprKind;
    use crate::span::Span;

    fn ident(name: &str) -> Expr {
        Expr {
            kind: ExprKind::Ident(name.to_string()),
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn plain_identifier_has_no_verilog_specific_rule() {
        let decls = HashMap::new();
        assert_eq!(verilog_self_determined_kind(&ident("p0"), &decls), None);
    }

    #[test]
    fn lossless_sub_self_determines_to_max_operand_width_not_growth() {
        let mut decls = HashMap::new();
        decls.insert(
            "p0".to_string(),
            Kind {
                width: 15,
                signed: false,
            },
        );
        decls.insert(
            "p1".to_string(),
            Kind {
                width: 15,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::Binary {
                op: BinOp::Sub,
                lhs: Box::new(ident("p0")),
                rhs: Box::new(ident("p1")),
            },
            span: Span::new(0, 0),
        };
        // Verilog: max(15,15) = 15, no growth (unlike mimz's own
        // lossless_result, which would say 16) — this is BUG-19's exact
        // mismatch, now representable and detectable.
        assert_eq!(
            verilog_self_determined_kind(&e, &decls),
            Some(Kind {
                width: 15,
                signed: false
            })
        );
    }

    #[test]
    fn comparison_has_no_verilog_specific_rule() {
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
                op: BinOp::Eq,
                lhs: Box::new(ident("p0")),
                rhs: Box::new(ident("p1")),
            },
            span: Span::new(0, 0),
        };
        assert_eq!(verilog_self_determined_kind(&e, &decls), None);
    }
}
