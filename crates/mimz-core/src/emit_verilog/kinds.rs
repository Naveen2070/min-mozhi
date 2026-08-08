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
//!
//! BUG-41 (docs/audit/bugs.md): this used to panic on anything but a
//! fixed set of `ExprKind`s, so its ONLY caller (`expr::kind_is_inferrable`,
//! now retired) had to keep a matching, hand-maintained wildcard —
//! silently disabling every hoist an expression wrapping one of those
//! "unhandled" shapes (a `fn` call, an instance port, an `if`, a `match`,
//! a memory read) needed, reopening BUG-28/29's exact defect through a
//! different AST shape. `infer_kind` now returns `None` for anything it
//! cannot resolve — no separate gate to keep in sync, and no panic
//! surface (closes SEC-10's adjacent concern too): a caller either gets
//! a real `Kind` or gets nothing, and reacts safely to the second case
//! (never hoists — see each `expr.rs` call site). `FnCall`/`Field`
//! (instance-port read)/`IfExpr`/`Match`/`Index` (bit-select or memory
//! read) are now classified: `build_decls` (`module/ports.rs`)
//! precomputes every fact these need (an instance's output-port `Kind`,
//! a memory's element `Kind`, a `fn`'s declared return `Kind`) into this
//! same `decls` map under a few reserved keys (see `mem_elem_decl_key`/
//! `fn_ret_decl_key` below and instance-port's plain `{inst}_{port}`
//! key, which already matches `expr.rs`'s own `Field` rendering) — so
//! this function's own signature never needed to grow a second
//! parameter.
use std::collections::HashMap;

use crate::ast::{BinOp, Builtin, Expr, ExprKind};
use crate::width_rules::Kind;

/// Reserved `decls` key for a memory `name`'s ELEMENT `Kind` — inserted by
/// `build_decls`, read by this file's own `Index` arm. A memory's bare
/// name is never a valid scalar `Ident` reference on its own (the checker
/// rejects it, mirroring `Bind::Inst`'s "not a value" diagnostic), so
/// reusing `decls`' plain namespace for the memory's OWN name would be
/// safe too, but keying it separately keeps `Index`'s two cases (a bit-
/// select on an ordinary vector vs. a memory read) unambiguous without
/// having to ask "is this name a memory" through some other channel.
pub(crate) fn mem_elem_decl_key(name: &str) -> String {
    format!("__mimz_mem_elem__{name}")
}

/// Reserved `decls` key for a `fn` named `name`'s declared RETURN `Kind` —
/// inserted by `build_decls` once per module (independent of any
/// particular call's arguments — a `fn`'s return type never depends on
/// them), read by this file's own `FnCall` arm.
pub(crate) fn fn_ret_decl_key(name: &str) -> String {
    format!("__mimz_fnret__{name}")
}

/// mimz's own width/signedness for `expr`, computed directly from the
/// AST, or `None` if it cannot be resolved from `decls` alone (a `fn`
/// body's own params/`let`s, a module `parameter`, a testbench signal,
/// or a symbolic parametric width — none of these are keys in `decls`,
/// by `build_decls`'s own design). `decls` maps every signal name
/// declared in the CURRENT module (port/wire/register) to its
/// already-resolved `Kind`, PLUS the reserved instance-port/memory-
/// element/fn-return facts described above — built once per module by
/// `build_decls`, reused across every expression in that module's body.
///
/// Every caller must treat `None` as "cannot safely reason about this
/// expression's width here" and skip whatever hoist/mismatch check it
/// was about to run — never as license to guess a width, and never by
/// re-adding a `kind_is_inferrable`-style shape gate that has to be kept
/// in sync with this function by hand (that hand-sync is exactly what
/// caused BUG-41).
pub(crate) fn infer_kind(expr: &Expr, decls: &HashMap<String, Kind>) -> Option<Kind> {
    match &expr.kind {
        ExprKind::Int { value, .. } => Some(Kind {
            width: min_width_for(value),
            signed: false,
        }),
        ExprKind::Bool(_) => Some(Kind {
            width: 1,
            signed: false,
        }),
        ExprKind::Ident(name) => decls.get(name).copied(),
        ExprKind::Unary { op: _, expr: inner } => infer_kind(inner, decls),
        ExprKind::Binary { op, lhs, rhs } => infer_binary(*op, lhs, rhs, decls),
        ExprKind::Concat(parts) => {
            let width = parts
                .iter()
                .try_fold(0u32, |acc, p| Some(acc + infer_kind(p, decls)?.width))?;
            Some(Kind {
                width,
                signed: false,
            })
        }
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
            let width = parts
                .iter()
                .try_fold(0u32, |acc, p| Some(acc + infer_kind(p, decls)?.width))?;
            Some(Kind {
                width,
                signed: false,
            })
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
            let hi_v = const_fold(hi)?;
            let lo_v = const_fold(lo)?;
            crate::width_rules::slice_result(u32::MAX, hi_v, lo_v).ok()
        }
        ExprKind::Call { func, args } => infer_call(*func, args, decls),
        // BUG-41 (docs/audit/bugs.md): the five shapes below used to fall
        // through the retired `kind_is_inferrable`'s own wildcard.
        ExprKind::Index { base, .. } => {
            // A memory read (`m[addr]`) yields the element `Kind`
            // `build_decls` precomputed under `mem_elem_decl_key`; anything
            // else reaching `Index` is a bit-select on a plain vector —
            // always exactly 1 bit, regardless of the vector's own width
            // (a module-level array is rejected by the checker outside a
            // `fn` parameter, and a `fn` parameter's own array is never in
            // `decls` either way, so that shape never reaches here).
            let ExprKind::Ident(name) = &base.kind else {
                return None;
            };
            if let Some(k) = decls.get(&mem_elem_decl_key(name)) {
                Some(*k)
            } else if decls.contains_key(name) {
                Some(Kind {
                    width: 1,
                    signed: false,
                })
            } else {
                None
            }
        }
        ExprKind::FnCall { name, .. } => decls.get(&fn_ret_decl_key(&name.name)).copied(),
        ExprKind::Field { base, field } => {
            // Instance-port read (`inst.port`) — `build_decls` precomputes
            // every non-array instance's output-port `Kind` under the exact
            // name `expr.rs`'s own `Field` rendering already produces
            // (`{inst}_{port}`), so this is a plain `decls` lookup, same as
            // an `Ident`. An enum-variant reference, a bundle field, and an
            // array-instance's `inst[i].port` are not handled here (`None`
            // — same, pre-existing, safe fallback as everything else this
            // function cannot resolve).
            let ExprKind::Ident(base_name) = &base.kind else {
                return None;
            };
            decls.get(&format!("{base_name}_{}", field.name)).copied()
        }
        ExprKind::IfExpr { then, els, .. } => {
            // Either branch — the checker guarantees both arms the same
            // `Ty`, so whichever one resolves is `expr`'s own Kind too.
            infer_kind(then, decls).or_else(|| infer_kind(els, decls))
        }
        ExprKind::Match { arms, .. } => {
            // Same reasoning as `IfExpr`: the checker guarantees every arm
            // the same `Ty`, so the first arm that resolves is enough.
            arms.iter().find_map(|a| infer_kind(&a.value, decls))
        }
        // Bundles/enums/`??`/array literals never appear inside a
        // concat/replicate/comparison/cast/slice-base position (mimz's own
        // type rules forbid it), so `None` here is never a missed case —
        // it is the same "cannot appear here" fact the retired
        // `kind_is_inferrable` encoded as `false`.
        _ => None,
    }
}

/// The minimal bit width needed to represent `value` as an unsigned
/// literal (at least 1 bit) — mirrors the checker's own `min_bits`
/// (`checker/widths/mod.rs`), reimplemented here since that function is
/// `pub(super)`-scoped to the checker module tree, not reachable from
/// `emit_verilog`; this is a one-line arithmetic helper, not a rule with
/// any drift risk (there is nothing to "get wrong" differently between
/// two copies of `128 - value.leading_zeros()`).
fn min_width_for(value: &crate::bits::Bits) -> u32 {
    crate::bits::natural_width(value)
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

/// Same idea as `is_bare_int`, widened to also recognize a NEGATED
/// literal (`-128`, `ExprKind::Unary { op: Neg, expr: Int }`) — needed
/// specifically for `min`/`max` (BUG-35, `docs/audit/bugs.md`): unlike
/// `AddWrap`/`BitAnd`'s narrower caller, `min(-128, x)` is real, shipped
/// code (`showcase/pid_controller.mimz`'s `max(-128, min(total, 127))`),
/// not a hypothetical — the checker's own `infer_ty` keeps a negated
/// literal as the untyped `Ty::CtInt` too (its `Unary::Neg` arm negates a
/// `CtInt` in place, never promoting it to a sized type), so `matched_ty`
/// lets it adapt to `total`'s `signed[16]` the same way a bare `127`
/// does. `infer_kind`'s own `Unary` arm ignores `op` entirely and just
/// forwards `inner`'s `Kind` (an existing approximation, unrelated to
/// this fix) — which is exactly why a negated literal's `Kind` here is
/// its UNSIGNED inner literal's width, not a sign-aware one; this
/// function only needs to recognize the SHAPE, not compute its value.
fn is_ct_int_like(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::Int { .. } => true,
        ExprKind::Unary {
            op: crate::ast::UnOp::Neg,
            expr,
        } => matches!(expr.kind, ExprKind::Int { .. }),
        _ => false,
    }
}

/// True iff `kind` should ADAPT to its sibling operand's `Kind` rather than
/// contributing an independent one of its own — a bare integer literal
/// (`is_bare_int`) or a plain `Ident` naming a module `int` parameter.
/// `kinds.rs` only ever runs on MODULE-BODY value expressions (never a
/// `fn` body, a testbench signal, or a `repeat` variable — this file's own
/// module doc), so within that domain a bare `Ident` absent from `decls`
/// is, by construction, a module parameter: the checker's own `ident_ty`
/// (`checker/widths/expr/mod.rs`) resolves every other legal name
/// (port/wire/reg/mem/instance) through the same signal table
/// `build_decls` mirrors, and would already have rejected an unknown name
/// before this code ever runs. A module `int` parameter is exactly
/// `Ty::CtInt` at the checker too (`ident_ty`'s `cx.env.get(name)`
/// branch) — untyped, adapting to its sized sibling via `adapt_lossless`/
/// `matched_ty` rather than carrying its own width — so treating it
/// identically to a bare literal here mirrors the checker's own rule.
///
/// BUG-46 (docs/audit/bugs.md): `extend(dur, 32) * TICK` (`TICK` a module
/// `int` parameter) used to be entirely unresolvable — `TICK` has no
/// `Kind` of its own and nothing let it defer to `dur`'s — so the whole
/// `Mul` was undecidable, the `Trunc` wrapping it never hoisted its base,
/// and Icarus rejected the resulting `(...)[(32)-1:0]` part-select on a
/// composite expression.
fn adapts_to_sibling(kind: &ExprKind, decls: &HashMap<String, Kind>) -> bool {
    is_bare_int(kind) || matches!(kind, ExprKind::Ident(name) if !decls.contains_key(name))
}

/// `lhs`/`rhs`'s `Kind`s for a LOSSLESS operator (`+`/`-`/`*`), with a
/// sibling-adapting operand (`adapts_to_sibling`) resolved to the OTHER
/// side's own `Kind` rather than attempted independently — mirroring the
/// checker's own `adapt_lossless` (`checker/widths/ops/mod.rs`), which
/// sizes a `Ty::CtInt` operand to `other.width.max(v's natural width)`;
/// this simplifies that to just `other.width` (dropping the `v`-exceeds-
/// `other` case), the same simplifying assumption the width-matching
/// family's existing bare-literal handling already makes. Sound whenever
/// the adapting side's actual value fits in `other`'s width — true for
/// every real module parameter used as a multiplier/addend against an
/// already-sized signal, which is the only shape this closes (BUG-46).
fn adapted_lossless_operands(
    lhs: &Expr,
    rhs: &Expr,
    decls: &HashMap<String, Kind>,
) -> Option<(Kind, Kind)> {
    match (
        adapts_to_sibling(&lhs.kind, decls),
        adapts_to_sibling(&rhs.kind, decls),
    ) {
        (true, false) => {
            let r = infer_kind(rhs, decls)?;
            Some((r, r))
        }
        (false, true) => {
            let l = infer_kind(lhs, decls)?;
            Some((l, l))
        }
        _ => Some((infer_kind(lhs, decls)?, infer_kind(rhs, decls)?)),
    }
}

/// Fold a compile-time-constant expression (a slice bound, a replication
/// count) to its `u32` value. Slice bounds/replication counts are
/// guaranteed compile-time constant by the checker (`slice_ty`/
/// `replicate_ty`) before this code ever runs, so only the literal case
/// needs handling — anything else is a structurally-impossible input at
/// this call site.
fn const_fold(expr: &Expr) -> Option<u32> {
    match &expr.kind {
        ExprKind::Int { value, .. } => Some(match value {
            crate::bits::Bits::Small(v) => *v as u32,
            crate::bits::Bits::Wide(limbs) => limbs.first().copied().unwrap_or(0) as u32,
        }),
        _ => None,
    }
}

/// `<<`'s growth (`width_rules::shift_result`) needs to know whether the
/// shift amount is a compile-time constant (grow by exactly that value)
/// or a genuine runtime signal (grow by its worst case). A bare literal
/// is the only shape this function recognizes as constant — a module
/// `int` parameter used as a shift amount is NOT treated as one (unlike
/// `adapts_to_sibling`'s lossless/wrap-family handling): its own value is
/// not generally knowable here without threading `self.env` through, so
/// `infer_kind(Ident(param))` still returns `None` for it, and the shift
/// safely falls back to unhoisted rendering — real Verilog's own context
/// growth is harmless there (BUG-30, `docs/audit/bugs.md`, "over-growth
/// is always safe once the base growth is already lossless").
fn shift_const_amount(amount: &Expr) -> Option<u128> {
    match &amount.kind {
        ExprKind::Int {
            value: crate::bits::Bits::Small(v),
            ..
        } => Some(*v),
        _ => None,
    }
}

/// Unlike the retired `kind_is_inferrable`, which eagerly required BOTH
/// operands resolvable for EVERY `op` (including comparisons, which
/// never actually consult either operand's `Kind` — see the doc on its
/// own `Binary` arm, "conservative... never wrong"), this only demands
/// what each arm actually uses: a comparison/logic result is always
/// exactly 1 bit by mimz's own rule regardless of either operand, so it
/// resolves unconditionally — strictly more permissive than before
/// (never wrong, since `verilog_self_determined_kind`'s own comparison
/// arm agrees no Verilog-specific rule ever differs here either).
fn infer_binary(op: BinOp, lhs: &Expr, rhs: &Expr, decls: &HashMap<String, Kind>) -> Option<Kind> {
    match op {
        BinOp::Shl | BinOp::Shr => {
            let l = infer_kind(lhs, decls)?;
            let r = infer_kind(rhs, decls)?;
            crate::width_rules::shift_result(l, r, shift_const_amount(rhs), op == BinOp::Shl).ok()
        }
        BinOp::Add | BinOp::Sub => {
            let (l, r) = adapted_lossless_operands(lhs, rhs, decls)?;
            crate::width_rules::lossless_result(l, r, false).ok()
        }
        BinOp::Mul => {
            let (l, r) = adapted_lossless_operands(lhs, rhs, decls)?;
            crate::width_rules::lossless_result(l, r, true).ok()
        }
        BinOp::AddWrap
        | BinOp::SubWrap
        | BinOp::MulWrap
        | BinOp::BitAnd
        | BinOp::BitOr
        | BinOp::BitXor => {
            // A bare integer literal OR a module `int` parameter reference
            // is the checker's `Ty::CtInt` — untyped, adapting to a sized
            // sibling operand's width/signedness rather than carrying its
            // own minimal natural width (`checker::widths::ops::
            // matched_ty`'s `(Ty::CtInt(v), t) | (t, Ty::CtInt(v))` arms,
            // which return the SIZED side's type unconditionally, no
            // equality check). `infer_kind`'s own `Int` arm has no such
            // context, so without this, `cnt +% 1` (`cnt: bits[26]`) — a
            // perfectly ordinary, checker-valid program — infers the
            // literal `1` at its own 1-bit width and mismatches the
            // `matched_result` fallback below on a mismatch the checker
            // never considered one.
            match (
                adapts_to_sibling(&lhs.kind, decls),
                adapts_to_sibling(&rhs.kind, decls),
            ) {
                (true, false) => infer_kind(rhs, decls),
                (false, true) => infer_kind(lhs, decls),
                _ => {
                    let l = infer_kind(lhs, decls)?;
                    let r = infer_kind(rhs, decls)?;
                    crate::width_rules::matched_result(l, r).ok()
                }
            }
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Some(Kind {
            width: 1,
            signed: false,
        }),
        BinOp::LogicAnd | BinOp::LogicOr => Some(Kind {
            width: 1,
            signed: false,
        }),
        // `??` cannot appear in a self-determined position (mimz's own
        // type rules forbid it there) — same "cannot appear here" `None`
        // as everything else this function does not classify.
        BinOp::Coalesce => None,
    }
}

/// Exhaustive over `Builtin`, no wildcard arm — matches
/// `self_determined.rs`'s own `verilog_self_determined_kind` convention
/// (BUG-28/29's doc there): a 15th variant fails the build here too,
/// instead of silently inheriting `None`.
fn infer_call(func: Builtin, args: &[Expr], decls: &HashMap<String, Kind>) -> Option<Kind> {
    match func {
        Builtin::Extend | Builtin::Trunc => {
            let n = const_fold(&args[1])?;
            let base_signed = infer_kind(&args[0], decls)?.signed;
            Some(Kind {
                width: n,
                signed: base_signed,
            })
        }
        Builtin::SignedCast => Some(Kind {
            width: infer_kind(&args[0], decls)?.width,
            signed: true,
        }),
        Builtin::UnsignedCast => Some(Kind {
            width: infer_kind(&args[0], decls)?.width,
            signed: false,
        }),
        Builtin::Encoding => Some(Kind {
            width: infer_kind(&args[0], decls)?.width,
            signed: false,
        }),
        // `abs(x)` for `x: signed[N]` is `signed[N+1]` — lossless like
        // unary `-`, the extra bit covers `abs(MIN)`. Matches the
        // checker's own rule (`checker/widths/ops/builtins.rs`,
        // `Builtin::Abs => Ty::Signed(n + 1)`).
        Builtin::Abs => Some(Kind {
            width: infer_kind(&args[0], decls)?.width + 1,
            signed: true,
        }),
        // Reductions collapse to one bit, unsigned — matches the checker's
        // own rule (`checker/widths/ops/builtins.rs`, `Ty::Bit | Ty::Bits(_)
        // => Ty::Bit`). BUG-35 (docs/audit/bugs.md): missing here entirely
        // before this fix, which — via `kind_is_inferrable`'s matching
        // `_ => false` gap in `expr.rs` — made any expression wrapping one
        // of these untouchable by the hoist machinery, not just
        // unclassified for its own sake.
        Builtin::Nand | Builtin::Nor | Builtin::Xnor => Some(Kind {
            width: 1,
            signed: false,
        }),
        // "A literal (or negated literal) adapts to its sized sibling"
        // rule — matches the checker's own `matched_ty` call for
        // `min`/`max` (`checker/widths/ops/builtins.rs`). Uses
        // `is_ct_int_like`, not `infer_binary`'s narrower `is_bare_int`
        // above — see that function's own doc comment for why.
        Builtin::Min | Builtin::Max => {
            match (is_ct_int_like(&args[0].kind), is_ct_int_like(&args[1].kind)) {
                (true, false) => infer_kind(&args[1], decls),
                (false, true) => infer_kind(&args[0], decls),
                _ => {
                    let l = infer_kind(&args[0], decls)?;
                    let r = infer_kind(&args[1], decls)?;
                    crate::width_rules::matched_result(l, r).ok()
                }
            }
        }
        // Const-folded before emit (`Clog2`) or lowered to registers/always
        // blocks before emit (`SyncDoubleFlop`/`SyncPulse`) — never reaches
        // a rendered self-determined position as a runtime expression,
        // same reasoning as `self_determined.rs`'s own matching arm.
        Builtin::Clog2 | Builtin::SyncDoubleFlop | Builtin::SyncPulse => None,
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
                value: value.into(),
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
            Some(Kind {
                width: 8,
                signed: false
            })
        );
    }

    #[test]
    fn ident_not_in_decls_is_none() {
        let decls = HashMap::new();
        assert_eq!(infer_kind(&ident("nope"), &decls), None);
    }

    #[test]
    fn literal_gets_its_minimal_width() {
        let decls = HashMap::new();
        assert_eq!(
            infer_kind(&int(3), &decls),
            Some(Kind {
                width: 2,
                signed: false
            })
        );
        assert_eq!(
            infer_kind(&int(0), &decls),
            Some(Kind {
                width: 1,
                signed: false
            })
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
            Some(Kind {
                width: 3,
                signed: false
            })
        );
    }

    #[test]
    fn concat_with_an_unresolvable_part_is_none() {
        let decls = HashMap::new();
        let e = Expr {
            kind: ExprKind::Concat(vec![int(3), ident("unknown")]),
            span: Span::new(0, 0),
        };
        assert_eq!(infer_kind(&e, &decls), None);
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
            Some(Kind {
                width: 9,
                signed: false
            })
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
        let expected = Some(Kind {
            width: 26,
            signed: false,
        });
        assert_eq!(infer_kind(&lit_rhs, &decls), expected);
        assert_eq!(infer_kind(&lit_lhs, &decls), expected);
    }

    #[test]
    fn lossless_mul_with_a_module_parameter_adapts_to_the_sized_operand() {
        // BUG-46 (docs/audit/bugs.md): `dur * TICK` (`TICK` a module `int`
        // parameter, absent from `decls` by design) used to make the whole
        // `Mul` unresolvable, so `Trunc`'s base-hoist never fired and
        // Icarus rejected the resulting part-select on a composite
        // expression. `TICK` must adapt to `dur`'s `Kind` instead.
        let mut decls = HashMap::new();
        decls.insert(
            "dur".to_string(),
            Kind {
                width: 32,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::Binary {
                op: BinOp::Mul,
                lhs: Box::new(ident("dur")),
                rhs: Box::new(ident("TICK")),
            },
            span: Span::new(0, 0),
        };
        assert_eq!(
            infer_kind(&e, &decls),
            Some(Kind {
                width: 64,
                signed: false
            })
        );
    }

    #[test]
    fn encoding_kind_matches_its_argument_width_unsigned() {
        let mut decls = HashMap::new();
        decls.insert(
            "state".to_string(),
            Kind {
                width: 2,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::Call {
                func: Builtin::Encoding,
                args: vec![ident("state")],
            },
            span: Span::new(0, 0),
        };
        assert_eq!(
            infer_kind(&e, &decls),
            Some(Kind {
                width: 2,
                signed: false
            })
        );
    }

    fn index(base: &str, idx: u128) -> Expr {
        Expr {
            kind: ExprKind::Index {
                base: Box::new(ident(base)),
                index: Box::new(int(idx)),
            },
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn index_on_a_plain_vector_is_one_bit() {
        let mut decls = HashMap::new();
        decls.insert(
            "p0".to_string(),
            Kind {
                width: 8,
                signed: false,
            },
        );
        assert_eq!(
            infer_kind(&index("p0", 3), &decls),
            Some(Kind {
                width: 1,
                signed: false
            })
        );
    }

    #[test]
    fn index_on_a_memory_yields_the_element_kind() {
        // BUG-41 (docs/audit/bugs.md): a memory read must NOT collapse to
        // 1 bit like an ordinary bit-select.
        let mut decls = HashMap::new();
        decls.insert(
            mem_elem_decl_key("m"),
            Kind {
                width: 8,
                signed: false,
            },
        );
        assert_eq!(
            infer_kind(&index("m", 0), &decls),
            Some(Kind {
                width: 8,
                signed: false
            })
        );
    }

    #[test]
    fn index_on_an_unknown_name_is_none() {
        let decls = HashMap::new();
        assert_eq!(infer_kind(&index("nope", 0), &decls), None);
    }

    #[test]
    fn fn_call_resolves_from_the_reserved_return_kind_key() {
        let mut decls = HashMap::new();
        decls.insert(
            fn_ret_decl_key("ident4"),
            Kind {
                width: 4,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::FnCall {
                name: crate::ast::Ident {
                    name: "ident4".to_string(),
                    span: Span::new(0, 0),
                },
                args: vec![ident("a")],
            },
            span: Span::new(0, 0),
        };
        assert_eq!(
            infer_kind(&e, &decls),
            Some(Kind {
                width: 4,
                signed: false
            })
        );
    }

    #[test]
    fn field_on_an_instance_resolves_from_the_mangled_port_key() {
        let mut decls = HashMap::new();
        decls.insert(
            "s_q".to_string(),
            Kind {
                width: 4,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::Field {
                base: Box::new(ident("s")),
                field: crate::ast::Ident {
                    name: "q".to_string(),
                    span: Span::new(0, 0),
                },
            },
            span: Span::new(0, 0),
        };
        assert_eq!(
            infer_kind(&e, &decls),
            Some(Kind {
                width: 4,
                signed: false
            })
        );
    }

    #[test]
    fn if_expr_resolves_from_either_branch() {
        let mut decls = HashMap::new();
        decls.insert(
            "a".to_string(),
            Kind {
                width: 4,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(ident("cond")),
                then: Box::new(ident("a")),
                els: Box::new(ident("a")),
            },
            span: Span::new(0, 0),
        };
        assert_eq!(
            infer_kind(&e, &decls),
            Some(Kind {
                width: 4,
                signed: false
            })
        );
    }

    #[test]
    fn if_expr_is_none_when_neither_branch_resolves() {
        let decls = HashMap::new();
        let e = Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(ident("cond")),
                then: Box::new(ident("unknown_a")),
                els: Box::new(ident("unknown_b")),
            },
            span: Span::new(0, 0),
        };
        assert_eq!(infer_kind(&e, &decls), None);
    }
}
