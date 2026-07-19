//! Shared width/kind rules — Stage 4, Phase A1a
//! (`docs/superpowers/specs/2026-07-19-shared-width-semantics-design.local.md`).
//!
//! The checker (`checker/widths/`) and the simulator
//! (`mimz-sim`'s `sim/value.rs`) each computed expression width/
//! signedness rules independently, and independent copies of the same
//! rule can drift — BUG-21 (`docs/audit/bugs.md`) is the exact shape:
//! the simulator's slice evaluator inherited the sliced base's
//! signedness instead of always being unsigned, disagreeing with the
//! checker's own `slice_ty`. This module holds ONE implementation per
//! operator rule; both sides call into it instead of each carrying
//! their own copy.
//!
//! Deliberately narrow: only `shift_result`/`slice_result` so far (the
//! two operators Stage 4's T3 conformance table starts with). The
//! `same_width`/`lossless`/`concat` families remain in
//! `checker/widths/ops.rs`, unconverted, as explicit follow-up work —
//! not an oversight, a scoping decision made when this plan was written.

/// A value's width + signedness — the minimal shape every per-operator
/// width/kind rule needs, independent of whether the caller has a
/// static type (the checker's `Ty`) or a concrete runtime value (the
/// simulator's `Val`). Deliberately does NOT model anything beyond a
/// scalar bit-vector — bundle/enum/memory/array width rules are
/// structurally different problems (field layout, variant encoding,
/// cell width) that don't fit a `(width, signed)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind {
    pub width: u32,
    pub signed: bool,
}

/// Why a rule function rejected its inputs. No message text — each
/// caller (the checker's diagnostics, the simulator's plain `String`
/// errors) renders its own wording from these structured fields, so
/// existing error codes/text are unaffected by this refactor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleError {
    /// A shift amount was `signed` (spec/02 section 3: shift amounts are
    /// non-negative, so `signed` never makes sense there).
    ShiftAmountSigned,
    /// `[hi:lo]` written with `hi < lo` — slices are `[hi:lo]`, most
    /// significant bit first.
    SliceReversed { hi: u32, lo: u32 },
    /// `hi` is not a valid bit position of a value that's `base_width`
    /// bits wide (`hi >= base_width`).
    SliceOutOfRange { hi: u32, base_width: u32 },
}

/// `<<`/`>>`: the result keeps the LEFT operand's kind (spec/02 section
/// 3 — a STATIC type-system invariant, not a claim about the runtime
/// value Verilog computes once that type flows into a wider context via
/// an explicit `extend()`; see `docs/audit/bugs.md`'s BUG-11 for why
/// those are deliberately different rules at different levels). The
/// simulator layers its own additional context-width growth on top of
/// this result — see `sim/value.rs`'s `Shl`/`Shr` arms — that part is
/// simulator-specific value-flow behavior, not a shared static rule.
pub fn shift_result(lhs: Kind, amount: Kind) -> Result<Kind, RuleError> {
    if amount.signed {
        return Err(RuleError::ShiftAmountSigned);
    }
    Ok(lhs)
}

/// `[hi:lo]`, both bounds inclusive, `0 <= lo <= hi < base_width`.
/// Always unsigned regardless of the sliced value's own kind — this one
/// function, called from both the checker's `slice_ty` and the
/// simulator's `ExprKind::Slice` evaluation, is what makes BUG-21
/// (`docs/audit/bugs.md`) structurally impossible to reintroduce: there
/// is no second copy of this rule left to drift.
pub fn slice_result(base_width: u32, hi: u32, lo: u32) -> Result<Kind, RuleError> {
    if hi < lo {
        return Err(RuleError::SliceReversed { hi, lo });
    }
    if hi >= base_width {
        return Err(RuleError::SliceOutOfRange { hi, base_width });
    }
    Ok(Kind {
        width: hi - lo + 1,
        signed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_result_preserves_lhs_kind() {
        let lhs = Kind {
            width: 8,
            signed: false,
        };
        let amount = Kind {
            width: 3,
            signed: false,
        };
        assert_eq!(shift_result(lhs, amount), Ok(lhs));
    }

    #[test]
    fn shift_result_preserves_signed_lhs() {
        let lhs = Kind {
            width: 16,
            signed: true,
        };
        let amount = Kind {
            width: 4,
            signed: false,
        };
        assert_eq!(shift_result(lhs, amount), Ok(lhs));
    }

    #[test]
    fn shift_result_rejects_signed_amount() {
        let lhs = Kind {
            width: 8,
            signed: false,
        };
        let amount = Kind {
            width: 3,
            signed: true,
        };
        assert_eq!(shift_result(lhs, amount), Err(RuleError::ShiftAmountSigned));
    }

    #[test]
    fn slice_result_computes_width_and_is_always_unsigned() {
        // A slice of ANYTHING is unsigned — this is the exact rule
        // BUG-21 (docs/audit/bugs.md) found the simulator getting wrong
        // by copying the sliced base's own signedness instead.
        assert_eq!(
            slice_result(9, 5, 3),
            Ok(Kind {
                width: 3,
                signed: false
            })
        );
    }

    #[test]
    fn slice_result_single_bit() {
        assert_eq!(
            slice_result(8, 4, 4),
            Ok(Kind {
                width: 1,
                signed: false
            })
        );
    }

    #[test]
    fn slice_result_rejects_reversed_bounds() {
        assert_eq!(
            slice_result(8, 2, 5),
            Err(RuleError::SliceReversed { hi: 2, lo: 5 })
        );
    }

    #[test]
    fn slice_result_rejects_out_of_range_hi() {
        assert_eq!(
            slice_result(8, 8, 0),
            Err(RuleError::SliceOutOfRange {
                hi: 8,
                base_width: 8
            })
        );
    }
}
