//! Shared value model + expression evaluator for the simulator.
//!
//! A [`Val`] is a 2-state bit-vector (≤128 bits) carrying a width and a signed
//! flag, honoring the spec's width semantics (lossless `+ - *` grow, the
//! `+% -% *%` family wraps, slices/concat/`extend`/`trunc` resize). [`eval`]
//! interprets an [`Expr`] against a [`Resolver`] — both the combinational
//! evaluator ([`super::comb`]) and the event-driven kernel ([`super::kernel`])
//! implement `Resolver`, so the expression semantics live in exactly one place.

use std::collections::{BTreeMap, HashMap};

use mimz_core::REPEAT_BUDGET;
use mimz_core::ast::{
    self, BinOp, Builtin, Expr, ExprKind, FnParam, FnStmt, FuncDecl, Pattern, Type, UnOp,
};

pub use mimz_core::bits::{Bits, mask};
pub use mimz_core::wide;

pub use mimz_core::bits::bits_to_decimal_string;

use binary::{binary_ctx, unary};
use fn_eval::{call, eval_fn_call};

use crate::sim::Diag;

/// A bit-vector value: the low `width` bits of `bits` are meaningful.
/// `pub` (re-exported at `mimz_sim::sim::Val`) since
/// `EmulationHost::on_change`/`on_tick` hand this to the shell crate's
/// peripheral implementations. NO LONGER `Copy` (Task 2,
/// `docs/superpowers/specs/2026-07-22-sim-wide-values-design.local.md`
/// §3) — `Bits::Wide`'s `Vec<u64>` can't be a bitwise copy. Every caller
/// that relied on implicit-copy semantics gets a compiler error at the
/// exact site needing an explicit `.clone()` (Task 7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Val {
    /// The value's bit pattern; only the low `width` bits are meaningful.
    /// MEANINGLESS when `unknown` is `true`.
    pub bits: Bits,
    /// Bit width, `1..=1_000_000` (`mimz_core::width_rules::MAX_WIDTH`).
    pub width: u32,
    /// Whether `bits` is interpreted as two's-complement `signed`.
    pub signed: bool,
    /// Coarse whole-value taint — see the pre-existing doc comment this
    /// field always had; unchanged by this task.
    pub unknown: bool,
}

impl Val {
    /// Builds a `Small`-path `Val`, masking `bits` to `width` (`width`
    /// floors at 1). UNCHANGED behavior from before this task — every
    /// existing caller with `width <= 128` gets the exact same `Val` it
    /// always did.
    pub fn new(bits: u128, width: u32, signed: bool) -> Val {
        Val {
            bits: Bits::Small(bits & mask(width)),
            width: width.max(1),
            signed,
            unknown: false,
        }
    }
    /// Builds a `Val` from a limb vector, masking to `width` and
    /// auto-narrowing to `Bits::Small` when `width <= 128` — so
    /// `width <= 128` implies `Small` is an invariant every OTHER
    /// constructor/consumer can rely on without re-checking. `limbs`
    /// must have exactly `wide::limb_count(width)` elements.
    pub(super) fn new_wide(mut limbs: Vec<u64>, width: u32, signed: bool) -> Val {
        wide::mask_to_width(&mut limbs, width);
        if width <= 128 {
            let lo = limbs.first().copied().unwrap_or(0) as u128;
            let hi = limbs.get(1).copied().unwrap_or(0) as u128;
            return Val::new(lo | (hi << 64), width, signed);
        }
        Val {
            bits: Bits::Wide(limbs),
            width,
            signed,
            unknown: false,
        }
    }
    /// An unconstrained value of the given width/signedness.
    pub fn unknown(width: u32, signed: bool) -> Val {
        Val {
            bits: Bits::Small(0),
            width: width.max(1),
            signed,
            unknown: true,
        }
    }
    /// A compile-time integer used as a value: minimal width that holds
    /// it. For an `i128`-ranged value (loop/repeat counters — always
    /// small in practice, bounded by their own sanity limits) — always
    /// `Small`. A source LITERAL (which may be arbitrary-width, BUG-13
    /// layer 2) goes through `from_literal` instead.
    pub fn from_int(v: i128) -> Val {
        if v >= 0 {
            let w = (128 - (v as u128).leading_zeros()).max(1);
            Val::new(v as u128, w, false)
        } else {
            let w = (129 - v.leading_ones()).max(1);
            Val::new(v as u128, w, true)
        }
    }
    /// A source integer literal (`ExprKind::Int`'s `Bits`) used as a
    /// runtime value: minimal width that holds it, same convention as
    /// `from_int`'s non-negative branch (a literal is always non-negative
    /// — the lexer never produces a negative `Bits`) but not capped at
    /// 128 bits (BUG-13 layer 2) — `new_wide` auto-narrows to `Small`
    /// when the natural width still fits.
    pub fn from_literal(value: &mimz_core::bits::Bits) -> Val {
        let width = mimz_core::bits::natural_width(value).max(1);
        let limbs = mimz_core::bits::to_limbs(value, width);
        Val::new_wide(limbs, width, false)
    }
    /// `true` if this value is on the wide (>128-bit) slow path.
    pub fn is_wide(&self) -> bool {
        matches!(self.bits, Bits::Wide(_))
    }
    /// This value's limbs, promoting a `Small` value to a
    /// `wide::limb_count(self.width)`-length vector on the fly. Used by
    /// every wide-path operator (Task 6) to treat both operands
    /// uniformly regardless of which one is actually wide.
    pub(super) fn to_limbs(&self) -> Vec<u64> {
        match &self.bits {
            Bits::Wide(v) => v.clone(),
            Bits::Small(b) => {
                let mut out = wide::zeros(self.width);
                out[0] = *b as u64;
                if out.len() > 1 {
                    out[1] = (*b >> 64) as u64;
                }
                out
            }
        }
    }
    /// Sign-aware value, sign-extended to i128 for signed comparisons.
    /// PANICS if called on a `Wide` value wider than 128 meaningful
    /// signed bits — every caller of this function operates on values
    /// already known to be `Small` (the narrow fast path only; Task 6's
    /// wide dispatch never calls this).
    pub fn as_i128(&self) -> i128 {
        let Bits::Small(bits) = &self.bits else {
            unreachable!("as_i128 called on a Wide value — narrow-path-only helper")
        };
        let m = mask(self.width);
        let b = bits & m;
        if self.signed && self.width >= 1 && (b >> (self.width - 1)) & 1 == 1 {
            (b | !m) as i128
        } else {
            b as i128
        }
    }
    /// The meaningful bits (masked to `width`) as a `u128` — PANICS on a
    /// `Wide` value (same "narrow-path-only" contract as `as_i128`;
    /// display code goes through `wide::to_decimal_string`/
    /// `to_binary_string` instead, see Task 11).
    pub fn masked(&self) -> u128 {
        let Bits::Small(bits) = &self.bits else {
            unreachable!("masked() called on a Wide value — narrow-path-only helper")
        };
        bits & mask(self.width)
    }
    /// This value's bits, masked to `width`, as a `Bits` — the
    /// `Bits`-returning counterpart to `masked()`/`as_i128()` for
    /// callers (like `Sim::peek`/`Sim::snapshot`) that must handle BOTH
    /// `Small` and `Wide` values, not just the narrow fast path.
    pub fn bits_masked(&self) -> Bits {
        match &self.bits {
            Bits::Small(b) => Bits::Small(b & mask(self.width)),
            Bits::Wide(limbs) => {
                let mut out = limbs.clone();
                wide::mask_to_width(&mut out, self.width);
                Bits::Wide(out)
            }
        }
    }
    /// The value's least significant bit — works for both `Small` and
    /// `Wide` without the caller needing to branch.
    pub(super) fn lsb(&self) -> u128 {
        match &self.bits {
            Bits::Small(b) => b & 1,
            Bits::Wide(limbs) => wide::bit_at(limbs, 0) as u128,
        }
    }
    /// This value's low 128 bits as a `u128`, for contexts (like a shift
    /// AMOUNT) that only ever care about small magnitudes regardless of
    /// the operand's declared width. A `Wide` value too large to matter
    /// here (shifting by more than 2^128) saturates to `u128::MAX`, which
    /// every caller already treats as "shift the whole value away."
    pub(super) fn bits_small_or_zero(&self) -> u128 {
        match &self.bits {
            Bits::Small(b) => *b,
            Bits::Wide(limbs) => {
                if wide::cmp_unsigned(limbs, &wide::from_u128(u128::MAX, self.width))
                    == std::cmp::Ordering::Greater
                {
                    u128::MAX
                } else {
                    (limbs.first().copied().unwrap_or(0) as u128)
                        | ((limbs.get(1).copied().unwrap_or(0) as u128) << 64)
                }
            }
        }
    }
}

/// Thin re-export of `wide::from_u128` — `kernel.rs` is a sibling module
/// and goes through `value`'s own surface rather than reaching into
/// `wide` directly, mirroring this codebase's existing `pub(super)`
/// visibility convention between sibling `sim::*` modules.
pub(super) fn wide_limbs_from_u128(v: u128, width: u32) -> Vec<u64> {
    wide::from_u128(v, width)
}

/// Build a `Val` from a raw `u128` bit pattern at `width`, zero-extending
/// into `Bits::Wide` when `width > 128` instead of defaulting to `Small`
/// like `Val::new` does. `Val::new` is deliberately narrow-only (its own
/// doc comment: "every caller with `width <= 128`") — it is UNSAFE to call
/// it with a `width` that isn't already known to be `<= 128`, because it
/// unconditionally builds `Bits::Small`, silently violating the
/// `width <= 128 ⟹ Small` invariant every dispatch in this file (and every
/// consumer of `Val::is_wide()`) relies on. This is the width-aware
/// counterpart for any construction site whose `width` is a SIGNAL's own
/// declared width (which may be runtime-supplied and arbitrarily large),
/// as opposed to a small literal constant (`1`, `8`, ...) a caller already
/// knows is narrow. Mirrors `Sim::set`'s existing `Small`-vs-`Wide` match.
pub(super) fn from_u128_at_width(v: u128, width: u32, signed: bool) -> Val {
    if width <= 128 {
        Val::new(v, width, signed)
    } else {
        Val::new_wide(wide::from_u128(v, width), width, signed)
    }
}

/// Build a `Val` from a compile-time-folded constant (`ConstVal`), sign/
/// zero-extending its own natural width to the signal's declared `width`.
/// The width-aware counterpart of `from_u128_at_width` for constants that
/// may already be wider than `u128` (BUG-13 layer 2) — `Reg.reset`/
/// `Mem.init` are `ConstVal` now, not `i128`.
pub(super) fn from_const_at_width(
    cv: &mimz_core::checker::consteval::ConstVal,
    width: u32,
    signed: bool,
) -> Val {
    let limbs = wide::extend(
        &mimz_core::bits::to_limbs(&cv.bits, cv.width),
        cv.width,
        width,
        cv.signed,
    );
    Val::new_wide(limbs, width, signed)
}

/// Reinterpret `v`'s raw bit pattern at a new width `w` — a pure re-mask
/// (truncating if `w < v.width`, zero-padding if `w > v.width`), NOT a
/// sign-extending resize (that's `extend_bits`/`wide::extend`). Used by
/// `eval_fn_stmts`'s `Let` handling to re-mask a local to its checker-
/// inferred width, mirroring the exact "reinterpret the same raw bits"
/// semantics the pre-`Bits`-enum code had via `Val::new(v.bits, w, ...)`.
fn remask_to_width(v: Val, w: u32) -> Val {
    let mut limbs = v.to_limbs();
    limbs.resize(wide::limb_count(w), 0);
    Val::new_wide(limbs, w, v.signed)
}

/// Resolves names while an expression is evaluated: a signal/reg/wire to its
/// current value, plus the compile-time integer environment for index and
/// slice bounds. The two evaluators differ only in `signal`. `pub` (not just
/// `pub(super)`) since `mimz-core`'s `width_rules_conformance` integration
/// test (Stage 4 T3) implements this trait to drive `eval` from outside
/// `mimz-sim` entirely.
pub trait Resolver {
    /// Resolve `name` to a value — a signal (evaluating its driver if
    /// combinational) or a compile-time constant. Errors if `name` is neither.
    fn signal(&mut self, name: &str) -> Result<Val, String>;
    /// The compile-time integer environment (params + consts).
    fn ints(&self) -> &BTreeMap<String, i128>;
    /// Is `name` a memory? Distinguishes `m[addr]` (a runtime-addressed memory
    /// read returning the element) from `s[i]` (a constant-indexed bit select).
    /// Resolvers without memory state (the combinational-only evaluator) say no.
    fn is_mem(&self, _name: &str) -> bool {
        false
    }
    /// Read cell `addr` of memory `name`. Returns the cell's current value (or
    /// the memory's init value for a never-written / out-of-range cell).
    fn mem_read(&mut self, name: &str, _addr: u128) -> Result<Val, String> {
        Err(format!("memory `{name}` is not available in this context"))
    }
    /// The user-defined function table — `None` in contexts that have no access
    /// to the parsed function declarations (e.g. a bare test without elaboration).
    fn funcs(&self) -> Option<&HashMap<String, FuncDecl>> {
        None
    }
    /// If `name` is an array in scope (a `fn` param or `let` binding), its
    /// element count — so `name[i]` resolves against the synthesized `name_i`
    /// scalars. Resolvers with no array scope (module signals) say `None`.
    fn array_len(&self, _name: &str) -> Option<u32> {
        None
    }
}

/// Evaluate `e` against `r`. Every position is self-determined now — `<<`
/// GROWS instead of threading an enclosing target width in (BUG-30,
/// `docs/audit/bugs.md`, superseding BUG-11's context-threading fix: the
/// declared type already bounds the true value, so no ambient width needs
/// to reach a shift from an assignment target, `extend`'s argument, or a
/// branch). `pub` since `mimz-core`'s `width_rules_conformance` test
/// (Stage 4 T3) drives this directly to check the simulator's own
/// evaluator against the shared `width_rules::shift_result` and the
/// checker's `Ty`-level inference.
pub fn eval<R: Resolver>(r: &mut R, e: &Expr) -> Result<Val, Box<Diag>> {
    match &e.kind {
        ExprKind::Int { value, .. } => Ok(Val::from_literal(value)),
        ExprKind::Bool(b) => Ok(Val::new(*b as u128, 1, false)),
        ExprKind::Ident(n) => r
            .signal(n)
            .map_err(|msg| crate::sim::diag::diag_from_bridged(e.span, msg, "S0201")),
        ExprKind::Unary { op, expr } => Ok(unary(*op, eval(r, expr)?)),
        // BUG-34 (docs/audit/bugs.md): a fused `Shl`/`Shr` chain must be
        // evaluated as one unit (`binary::eval_shift_chain`), not per-node
        // — see that function's doc comment for why. This also covers a
        // LONE shift (chain-of-one), so `shl`/`shr` are never reached from
        // here directly; they stay as directly-callable primitives for
        // callers (and tests) that already have both operands as `Val`s.
        ExprKind::Binary {
            op: BinOp::Shl | BinOp::Shr,
            ..
        } => binary::eval_shift_chain(r, e),
        ExprKind::Binary { op, lhs, rhs } => {
            let l = eval(r, lhs)?;
            let rr = eval(r, rhs)?;
            binary_ctx(*op, l, rr, None, e.span)
        }
        ExprKind::IfExpr { cond, then, els } => {
            if eval(r, cond)?.lsb() == 1 {
                eval(r, then)
            } else {
                eval(r, els)
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            let s = eval(r, scrutinee)?;
            for arm in arms {
                for p in &arm.patterns {
                    if pattern_matches(p, &s) {
                        return eval(r, &arm.value);
                    }
                }
            }
            Err(Box::new(
                Diag::new(
                    e.span,
                    "no `match` arm matched the value (enum patterns are not evaluated yet)",
                )
                .with_code("S0202"),
            ))
        }
        ExprKind::Concat(parts) => {
            let vals: Vec<Val> = parts.iter().map(|p| eval(r, p)).collect::<Result<_, _>>()?;
            // Sum in u64 so many parts cannot wrap a u32 below the guard.
            let total64: u64 = vals.iter().map(|v| v.width as u64).sum();
            if total64 > mimz_core::width_rules::MAX_WIDTH as u64 {
                return Err(Box::new(
                    Diag::new(
                        e.span,
                        format!(
                            "concatenation exceeds {} bits",
                            mimz_core::width_rules::MAX_WIDTH
                        ),
                    )
                    .with_code("S0203"),
                ));
            }
            let total = total64 as u32;
            if vals.iter().any(|v| v.unknown) {
                return Ok(Val::unknown(total, false));
            }
            let mut limbs = wide::zeros(total);
            let mut shift = total;
            for v in &vals {
                shift -= v.width;
                let placed = wide::shl(
                    &wide::extend(&v.to_limbs(), v.width, total, false),
                    shift,
                    total,
                );
                limbs = wide::bitor(&limbs, &placed);
            }
            Ok(Val::new_wide(limbs, total, false))
        }
        ExprKind::Replicate { count, parts } => {
            let n = const_eval(count, r.ints())?;
            if n < 1 {
                return Err(Box::new(
                    Diag::new(count.span, "replication count must be at least 1")
                        .with_code("S0204"),
                ));
            }
            let vals: Vec<Val> = parts.iter().map(|p| eval(r, p)).collect::<Result<_, _>>()?;
            // Inner group width, then the replicated total — both in u64 so the
            // product cannot wrap a u32 below the width guard.
            let inner64: u64 = vals.iter().map(|v| v.width as u64).sum();
            let total64 = inner64
                .checked_mul(n as u64)
                .filter(|t| *t <= mimz_core::width_rules::MAX_WIDTH as u64)
                .ok_or_else(|| {
                    Box::new(
                        Diag::new(
                            e.span,
                            format!(
                                "replication exceeds {} bits",
                                mimz_core::width_rules::MAX_WIDTH
                            ),
                        )
                        .with_code("S0203"),
                    )
                })?;
            if vals.iter().any(|v| v.unknown) {
                return Ok(Val::unknown(total64 as u32, false));
            }
            let total = total64 as u32;
            let inner = inner64 as u32;
            // Assemble the inner group once (widest part first), then repeat it.
            let mut chunk = wide::zeros(inner);
            let mut shift = inner;
            for v in &vals {
                shift -= v.width;
                let placed = wide::shl(
                    &wide::extend(&v.to_limbs(), v.width, inner, false),
                    shift,
                    inner,
                );
                chunk = wide::bitor(&chunk, &placed);
            }
            let mut limbs = wide::zeros(total);
            for i in 0..n {
                let shift = inner * (n - 1 - i) as u32;
                let placed = wide::shl(&wide::extend(&chunk, inner, total, false), shift, total);
                limbs = wide::bitor(&limbs, &placed);
            }
            Ok(Val::new_wide(limbs, total, false))
        }
        ExprKind::Index { base, index } => {
            // An array element `vals[i]` (array-typed param or `let`) resolves
            // to the synthesized scalar `vals_i` — a constant index folds to
            // the right name, a runtime index picks it out of the element Vec
            // (plain Rust indexing; no mux needed, unlike the Verilog emitter).
            // A memory read `m[addr]` resolves the address at RUNTIME and
            // returns the whole element; a bit-vector `s[i]` selects one bit
            // at a compile-time index.
            if let ExprKind::Ident(name) = &base.kind {
                if let Some(len) = r.array_len(name) {
                    let elems: Vec<Val> = (0..len)
                        .map(|i| {
                            r.signal(&format!("{name}_{i}")).map_err(|msg| {
                                crate::sim::diag::diag_from_bridged(e.span, msg, "S0201")
                            })
                        })
                        .collect::<Result<_, _>>()?;
                    // A zero-length array is rejected by the checker (E0412)
                    // in the normal compiler pipeline, but this evaluator is
                    // also exercised directly on unchecked ASTs (fuzzing) —
                    // `elems.len() - 1` below would underflow, so this must
                    // be a clean `Err`, not a panic.
                    let Some(last) = elems.len().checked_sub(1) else {
                        return Err(Box::new(
                            Diag::new(e.span, format!("array `{name}` has no elements to index"))
                                .with_code("S0205"),
                        ));
                    };
                    // Out-of-range runtime index clamps to the last element,
                    // matching the emitter's ternary-chain default fallback and
                    // spec/02 §1.14 (keeps sim and Verilog in agreement).
                    let i = (eval(r, index)?.bits_small_or_zero() as usize).min(last);
                    return Ok(elems[i].clone());
                }
                if r.is_mem(name) {
                    let addr = eval(r, index)?;
                    return r
                        .mem_read(name, addr.bits_small_or_zero())
                        .map_err(|msg| crate::sim::diag::diag_from_bridged(e.span, msg, "S0206"));
                }
            }
            let b = eval(r, base)?;
            if b.unknown {
                return Ok(Val::unknown(1, false));
            }
            let i = checked_index(
                const_eval(index, r.ints())?,
                b.width,
                "bit index",
                index.span,
            )?;
            let bit = if b.is_wide() {
                wide::bit_at(&b.to_limbs(), i) as u128
            } else {
                (b.masked() >> i) & 1
            };
            Ok(Val::new(bit, 1, false))
        }
        ExprKind::Slice { base, hi, lo } => {
            let b = eval(r, base)?;
            let hi_v = checked_index(
                const_eval(hi, r.ints())?,
                b.width,
                "slice high bound",
                hi.span,
            )?;
            let lo_v = checked_index(
                const_eval(lo, r.ints())?,
                b.width,
                "slice low bound",
                lo.span,
            )?;
            // A slice is always unsigned regardless of the base's own
            // kind (BUG-21, docs/audit/bugs.md) — enforced by
            // `width_rules::slice_result`, the same function the
            // checker's own `slice_ty` calls, so there is exactly one
            // copy of this rule left. `checked_index` above already
            // guarantees `hi`/`lo` are each individually in range, so
            // only the reversed-bounds case can actually fire here.
            let k = mimz_core::width_rules::slice_result(b.width, hi_v, lo_v).map_err(|_| {
                Box::new(
                    Diag::new(
                        hi.span.join(lo.span),
                        "slice bounds reversed (write `[hi:lo]`, msb first)",
                    )
                    .with_code("S0208"),
                )
            })?;
            let lo = lo_v;
            if b.unknown {
                return Ok(Val::unknown(k.width, false));
            }
            if !b.is_wide() {
                Ok(Val::new((b.masked() >> lo) & mask(k.width), k.width, false))
            } else {
                let mut shifted = wide::shr(&b.to_limbs(), lo);
                shifted.resize(wide::limb_count(k.width), 0);
                Ok(Val::new_wide(shifted, k.width, false))
            }
        }
        ExprKind::Field { .. } => Err(Box::new(
            Diag::new(
                e.span,
                "enum-variant / instance-port access is not supported by the evaluator yet",
            )
            .with_code("S0209"),
        )),
        ExprKind::Call { func, args } => call(r, *func, args),
        ExprKind::FnCall { name, args } => eval_fn_call(r, name, args),
        ExprKind::BundleLit(_) => Err(Box::new(
            Diag::new(
                e.span,
                "BundleLit reached value evaluator — should be pre-expanded by elaborate",
            )
            .with_code("S0210"),
        )),
        ExprKind::ArrayLit(_) => Err(Box::new(
            Diag::new(
                e.span,
                "array literal is only valid as a `fn` argument or `let` binding \
                 (both pre-expand to scalars before evaluation)",
            )
            .with_code("S0211"),
        )),
        ExprKind::EnumConstruct { .. } => Err(Box::new(
            Diag::new(
                e.span,
                "EnumConstruct reached value evaluator — should be pre-expanded by elaborate",
            )
            .with_code("S0212"),
        )),
    }
}

pub(super) fn pattern_matches(p: &Pattern, s: &Val) -> bool {
    // Helper: extract the low 128 bits of s without the saturation that
    // bits_small_or_zero() applies to values > u128::MAX.
    let low128 = |s: &Val| -> u128 {
        match &s.bits {
            Bits::Small(b) => *b,
            Bits::Wide(limbs) => {
                (limbs.first().copied().unwrap_or(0) as u128)
                    | ((limbs.get(1).copied().unwrap_or(0) as u128) << 64)
            }
        }
    };
    match p {
        Pattern::Wildcard => true,
        Pattern::Int { value, .. } => {
            // Same low-128-bits extraction as `low128(s)` above, for the
            // pattern literal's own (possibly wide, BUG-13 layer 2) `Bits`.
            let vlow = match value {
                Bits::Small(b) => *b,
                Bits::Wide(limbs) => {
                    (limbs.first().copied().unwrap_or(0) as u128)
                        | ((limbs.get(1).copied().unwrap_or(0) as u128) << 64)
                }
            };
            low128(s) & mask(s.width.min(128)) == vlow & mask(s.width.min(128))
        }
        Pattern::IntMask { value, mask: m, .. } => (low128(s) & *m) == (*value & *m),
        Pattern::Bool(b) => s.lsb() == (*b as u128),
        // BUG-40 (docs/audit/bugs.md): true ONLY on the clocked path
        // (`elaborate/rewrite.rs`, which always lowers a `Pattern::Variant`
        // to `IntMask` first, after validating the enum name is real).
        // `comb::eval_outputs` — the standalone combinational-only
        // evaluator the fuzz harness and `differential()`
        // (tests/self_determined_regression.rs) call directly, deliberately
        // bypassing the checker — has no such lowering pass, so a
        // syntactically-legal `EnumName.variant` pattern referencing a name
        // that isn't a real enum (the parser never checks that; only the
        // checker does) can reach here unlowered. Never match rather than
        // panic — the sibling S0202 "no match arm matched" error already
        // anticipates exactly this ("enum patterns are not evaluated yet").
        Pattern::Variant { .. } => false,
    }
}

/// The declared (width, signed) of a hardware type, evaluating any width
/// expression in the const environment. `span` is the declaring signal/
/// field/param's own span — `ast::Type` itself carries none.
pub(super) fn type_width(
    ty: &Type,
    ints: &BTreeMap<String, i128>,
    span: mimz_core::span::Span,
) -> Result<(u32, bool), Box<Diag>> {
    match ty {
        Type::Bit => Ok((1, false)),
        Type::Bits(e) => Ok((checked_width(const_eval(e, ints)?, e.span)?, false)),
        Type::Signed(e) => Ok((checked_width(const_eval(e, ints)?, e.span)?, true)),
        Type::Named(n) => Err(Box::new(
            Diag::new(
                span,
                format!(
                    "signal of enum type `{}` — the simulator does not model enum signals yet",
                    n.name.name
                ),
            )
            .with_code("S0213"),
        )),
        Type::Bundle { .. } => Err(Box::new(
            Diag::new(
                span,
                "Type::Bundle reached type_width — should be pre-flattened by elaborate",
            )
            .with_code("S0214"),
        )),
        // An array type never reaches here: an array param/`let` is expanded to
        // per-element scalars (each queried via its ELEMENT type), array module
        // signals are rejected (E0416), and array bundle/enum-payload fields are
        // rejected/flattened. Mirror the Bundle arm rather than panicking.
        Type::Array { .. } => Err(Box::new(
            Diag::new(
                span,
                "Type::Array reached type_width — arrays expand to per-element scalars",
            )
            .with_code("S0215"),
        )),
    }
}

pub(super) fn checked_width(n: i128, span: mimz_core::span::Span) -> Result<u32, Box<Diag>> {
    use mimz_core::width_rules::MAX_WIDTH;
    if n < 1 {
        Err(Box::new(
            Diag::new(span, format!("width must be at least 1, got {n}")).with_code("S0216"),
        ))
    } else if n > MAX_WIDTH {
        Err(Box::new(
            Diag::new(
                span,
                format!("width {n} exceeds the maximum of {MAX_WIDTH} bits"),
            )
            .with_code("S0217"),
        ))
    } else {
        Ok(n as u32)
    }
}

/// Build the checker's `Env` (name -> `ConstVal`) from this file's own
/// folded-`i128` `consts`/`params` map, via `ConstVal::from_i128`. Shared by
/// `const_eval` and `const_eval_wide` below.
fn build_env(ints: &BTreeMap<String, i128>) -> mimz_core::checker::consteval::Env {
    ints.iter()
        .map(|(k, v)| {
            (
                k.clone(),
                mimz_core::checker::consteval::ConstVal::from_i128(*v),
            )
        })
        .collect()
}

/// Compile-time const evaluation for widths, parameters, consts, indices, and
/// slice bounds. **Delegates to the checker's hardened evaluator**
/// (`checker::consteval::eval`) — the single source of truth — which uses
/// `checked_*` arithmetic and guarded shifts, so an oversized const such as
/// `1 << 200` is a clean error, never a debug panic or a silent release wrap.
///
/// Narrows the checker's arbitrary-width `ConstVal` result back to `i128`
/// via `to_i128_saturating` — every caller here wants a STRUCTURAL size
/// (a width, depth, index, or repeat bound), never a `Reg.reset`/`Mem.init`
/// DATA value, and those are already capped far below `i128::MAX` by their
/// own sanity limits (`MAX_WIDTH`, `MAX_DEPTH`), so this narrowing is exact
/// in every legal design (BUG-13 layer 2's arbitrary-width representation
/// only actually matters for `const_eval_wide`, below).
pub(super) fn const_eval(e: &Expr, ints: &BTreeMap<String, i128>) -> Result<i128, Box<Diag>> {
    mimz_core::checker::consteval::eval(e, &build_env(ints))
        .map(|v| v.to_i128_saturating())
        .map_err(Box::new)
}

/// Compile-time const evaluation for a `Reg.reset`/`Mem.init` expression —
/// the one place a compile-time value's own MAGNITUDE (not just a
/// structural size) matters, so this returns the checker's own
/// arbitrary-width `ConstVal` directly instead of narrowing it to `i128`
/// (BUG-13 layer 2).
pub(super) fn const_eval_wide(
    e: &Expr,
    ints: &BTreeMap<String, i128>,
) -> Result<mimz_core::checker::consteval::ConstVal, Box<Diag>> {
    mimz_core::checker::consteval::eval(e, &build_env(ints)).map_err(Box::new)
}

/// A bit index or slice bound must be a non-negative integer inside the value's
/// width. Rejects negative / out-of-range positions instead of truncating via
/// `as u32` or a later oversized shift (`>> n`, `n >= 128`, which panics).
pub(super) fn checked_index(
    n: i128,
    width: u32,
    what: &str,
    span: mimz_core::span::Span,
) -> Result<u32, Box<Diag>> {
    if (0..width as i128).contains(&n) {
        Ok(n as u32)
    } else {
        Err(Box::new(
            Diag::new(
                span,
                format!("{what} {n} is out of range for a {width}-bit value"),
            )
            .with_code("S0207"),
        ))
    }
}

/// Pick `module` from `file`, or the file's only module when `None`. No
/// span in scope precise enough to beat `Span::default()` — `want` is a
/// bare `&str` and an absent/ambiguous module has no single declaration to
/// point at (mirrors `elaborate_project_with_mode`'s own defensive
/// zero-span "no files" case).
pub(super) fn pick_module<'a>(
    file: &'a ast::File,
    want: Option<&str>,
) -> Result<&'a ast::Module, Box<Diag>> {
    let mods: Vec<&ast::Module> = file
        .items
        .iter()
        .filter_map(|i| match i {
            ast::TopItem::Module(m) => Some(m),
            _ => None,
        })
        .collect();
    match want {
        Some(n) => mods
            .iter()
            .copied()
            .find(|m| m.name.name == n)
            .ok_or_else(|| {
                Box::new(
                    Diag::new(
                        mimz_core::span::Span::default(),
                        format!("no module named `{n}` in this file"),
                    )
                    .with_code("S0218"),
                )
            }),
        None => match mods.as_slice() {
            [one] => Ok(one),
            [] => Err(Box::new(
                Diag::new(mimz_core::span::Span::default(), "file defines no module")
                    .with_code("S0219"),
            )),
            many => Err(Box::new(
                Diag::new(
                    mimz_core::span::Span::default(),
                    format!(
                        "file defines {} modules — choose one with --module <name>",
                        many.len()
                    ),
                )
                .with_code("S0220"),
            )),
        },
    }
}

mod binary;
mod fn_eval;

#[cfg(test)]
mod tests;
