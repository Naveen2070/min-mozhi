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
