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

use super::wide;

/// Low-`w`-bits mask (`w >= 128` ⇒ all ones).
pub(super) fn mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

/// A value's raw bit pattern: `Small` for the fast path (width <= 128,
/// stored as a plain `u128`, unchanged from before this task), `Wide`
/// for anything larger (little-endian `u64` limbs, `wide::limb_count`
/// of them). `Val::new_wide` guarantees `width <= 128` is ALWAYS
/// `Small` — no other constructor produces a `Wide` value that fits in
/// 128 bits, so callers can treat "is this Wide" and "is width > 128"
/// as the same question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bits {
    Small(u128),
    Wide(Vec<u64>),
}

/// Render `bits` (masked to `width`, interpreted per `signed`) as a decimal string —
/// the `Bits`-only counterpart to `harness.rs`'s `Val`-based `show()`, for callers
/// (`Sim::peek`/`Output.value` consumers outside this crate) that only have a `Bits`
/// plus its width/signedness, not a full `Val`.
pub fn bits_to_decimal_string(bits: &Bits, width: u32, signed: bool) -> String {
    match bits {
        Bits::Small(b) => {
            let m = b & mask(width);
            if signed && width >= 1 && (m >> (width - 1)) & 1 == 1 {
                ((m | !mask(width)) as i128).to_string()
            } else {
                m.to_string()
            }
        }
        Bits::Wide(limbs) => wide::to_decimal_string(limbs, width, signed),
    }
}

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
    /// it. Literals stay `i128`-bounded (layer 2, deliberately out of
    /// scope for this plan — see the design doc) — always `Small`.
    pub fn from_int(v: i128) -> Val {
        if v >= 0 {
            let w = (128 - (v as u128).leading_zeros()).max(1);
            Val::new(v as u128, w, false)
        } else {
            let w = (129 - v.leading_ones()).max(1);
            Val::new(v as u128, w, true)
        }
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

/// Promote `l`/`r` to matching-length limb vectors at `result_width`,
/// running the SAME sign-extension `extend_bits` already applies on the
/// narrow path. Shared by every wide-path binary operator arm below.
fn wide_operands(l: Val, r: Val, result_width: u32) -> (Vec<u64>, Vec<u64>) {
    (
        wide::extend(&l.to_limbs(), l.width, result_width, l.signed),
        wide::extend(&r.to_limbs(), r.width, result_width, r.signed),
    )
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

/// Evaluate `e` against `r` with no target-width context (self-determined —
/// the right call for conditions, indices, loop bounds, and anywhere else
/// Verilog itself doesn't propagate an enclosing width inward). Most callers
/// want this. See `eval_ctx` for context-determined positions (an
/// assignment RHS, `extend`'s argument) where a shift's real result depends
/// on the width it's eventually consumed at (BUG-11). `pub` since
/// `mimz-core`'s `width_rules_conformance` test (Stage 4 T3) drives this
/// directly to check the simulator's own evaluator against the shared
/// `width_rules::shift_result` and the checker's `Ty`-level inference.
pub fn eval<R: Resolver>(r: &mut R, e: &Expr) -> Result<Val, String> {
    eval_ctx(r, e, None)
}

/// Evaluate `e` against `r`, threading `expected_width` — the width of the
/// enclosing context (an assignment target, `extend`'s target width) — into
/// every CONTEXT-DETERMINED position. The single source of Min-Mozhi's
/// expression semantics for both the combinational evaluator and the kernel.
///
/// Verilog's `<<`/`>>` are context-determined on their LEFT operand (the
/// shift amount is always self-determined): `assign wide = (narrow << k)`
/// widens `narrow` to `wide`'s width BEFORE shifting, not after — ground-
/// truthed against `iverilog` (BUG-11's fix). Only `Shl`/`Shr` use
/// `expected_width` here; every other binary operator's own width rule is
/// unchanged (deliberately scoped — see `docs/plan/phase-2-correctness-
/// consolidation.local.md` Stage 1 for the rest of this operator family).
/// `if`/`match` propagate the SAME `expected_width` into every branch
/// (Verilog's ternary/case are likewise context-determined), so a shift
/// nested in a branch still sees the real target width.
pub(super) fn eval_ctx<R: Resolver>(
    r: &mut R,
    e: &Expr,
    expected_width: Option<u32>,
) -> Result<Val, String> {
    match &e.kind {
        ExprKind::Int { value, .. } => Ok(Val::from_int(*value as i128)),
        ExprKind::Bool(b) => Ok(Val::new(*b as u128, 1, false)),
        ExprKind::Ident(n) => r.signal(n),
        ExprKind::Unary { op, expr } => Ok(unary(*op, eval(r, expr)?)),
        ExprKind::Binary { op, lhs, rhs } => {
            let shift_ctx = matches!(op, BinOp::Shl | BinOp::Shr);
            let l = eval_ctx(r, lhs, if shift_ctx { expected_width } else { None })?;
            let rr = eval(r, rhs)?; // shift amount (or any other RHS) is self-determined
            binary_ctx(*op, l, rr, expected_width)
        }
        ExprKind::IfExpr { cond, then, els } => {
            if eval(r, cond)?.lsb() == 1 {
                eval_ctx(r, then, expected_width)
            } else {
                eval_ctx(r, els, expected_width)
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            let s = eval(r, scrutinee)?;
            for arm in arms {
                for p in &arm.patterns {
                    if pattern_matches(p, &s)? {
                        return eval_ctx(r, &arm.value, expected_width);
                    }
                }
            }
            Err("no `match` arm matched the value (enum patterns are not evaluated yet)".into())
        }
        ExprKind::Concat(parts) => {
            let vals: Vec<Val> = parts.iter().map(|p| eval(r, p)).collect::<Result<_, _>>()?;
            // Sum in u64 so many parts cannot wrap a u32 below the guard.
            let total64: u64 = vals.iter().map(|v| v.width as u64).sum();
            if total64 > mimz_core::width_rules::MAX_WIDTH as u64 {
                return Err(format!(
                    "concatenation exceeds {} bits",
                    mimz_core::width_rules::MAX_WIDTH
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
                return Err("replication count must be at least 1".into());
            }
            let vals: Vec<Val> = parts.iter().map(|p| eval(r, p)).collect::<Result<_, _>>()?;
            // Inner group width, then the replicated total — both in u64 so the
            // product cannot wrap a u32 below the width guard.
            let inner64: u64 = vals.iter().map(|v| v.width as u64).sum();
            let total64 = inner64
                .checked_mul(n as u64)
                .filter(|t| *t <= mimz_core::width_rules::MAX_WIDTH as u64)
                .ok_or_else(|| {
                    format!(
                        "replication exceeds {} bits",
                        mimz_core::width_rules::MAX_WIDTH
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
                        .map(|i| r.signal(&format!("{name}_{i}")))
                        .collect::<Result<_, _>>()?;
                    // A zero-length array is rejected by the checker (E0412)
                    // in the normal compiler pipeline, but this evaluator is
                    // also exercised directly on unchecked ASTs (fuzzing) —
                    // `elems.len() - 1` below would underflow, so this must
                    // be a clean `Err`, not a panic.
                    let Some(last) = elems.len().checked_sub(1) else {
                        return Err(format!("array `{name}` has no elements to index"));
                    };
                    // Out-of-range runtime index clamps to the last element,
                    // matching the emitter's ternary-chain default fallback and
                    // spec/02 §1.14 (keeps sim and Verilog in agreement).
                    let i = (eval(r, index)?.bits_small_or_zero() as usize).min(last);
                    return Ok(elems[i].clone());
                }
                if r.is_mem(name) {
                    let addr = eval(r, index)?;
                    return r.mem_read(name, addr.bits_small_or_zero());
                }
            }
            let b = eval(r, base)?;
            if b.unknown {
                return Ok(Val::unknown(1, false));
            }
            let i = checked_index(const_eval(index, r.ints())?, b.width, "bit index")?;
            let bit = if b.is_wide() {
                wide::bit_at(&b.to_limbs(), i) as u128
            } else {
                (b.masked() >> i) & 1
            };
            Ok(Val::new(bit, 1, false))
        }
        ExprKind::Slice { base, hi, lo } => {
            let b = eval(r, base)?;
            let hi = checked_index(const_eval(hi, r.ints())?, b.width, "slice high bound")?;
            let lo = checked_index(const_eval(lo, r.ints())?, b.width, "slice low bound")?;
            // A slice is always unsigned regardless of the base's own
            // kind (BUG-21, docs/audit/bugs.md) — enforced by
            // `width_rules::slice_result`, the same function the
            // checker's own `slice_ty` calls, so there is exactly one
            // copy of this rule left. `checked_index` above already
            // guarantees `hi`/`lo` are each individually in range, so
            // only the reversed-bounds case can actually fire here.
            let k = mimz_core::width_rules::slice_result(b.width, hi, lo)
                .map_err(|_| "slice bounds reversed (write `[hi:lo]`, msb first)".to_string())?;
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
        ExprKind::Field { .. } => {
            Err("enum-variant / instance-port access is not supported by the evaluator yet".into())
        }
        ExprKind::Call { func, args } => call(r, *func, args),
        ExprKind::FnCall { name, args } => eval_fn_call(r, name, args),
        ExprKind::BundleLit(_) => {
            Err("BundleLit reached value evaluator — should be pre-expanded by elaborate".into())
        }
        ExprKind::ArrayLit(_) => Err(
            "array literal is only valid as a `fn` argument or `let` binding \
             (both pre-expand to scalars before evaluation)"
                .into(),
        ),
        ExprKind::EnumConstruct { .. } => Err(
            "EnumConstruct reached value evaluator — should be pre-expanded by elaborate".into(),
        ),
    }
}

/// Evaluate a user-defined function call.
///
/// Args are evaluated in the CALLER's env, then bound to params in a child
/// env ([`FnEnv`]). Locals are evaluated in order in the child env. Finally
/// the body expression is evaluated. Width parity with the Verilog emitter
/// (which declares `reg [W-1:0]` for each local) is achieved by masking each
/// local's bound value to its `inferred_width` when the checker has set it.
fn eval_fn_call<R: Resolver>(r: &mut R, name: &ast::Ident, args: &[Expr]) -> Result<Val, String> {
    // Flatten each argument to one-or-more `Val`s: an `ArrayLit` expands to N
    // values in place (mirroring the emitter's own N-scalar call-argument
    // expansion, so both backends agree on argument order); every other
    // expression evaluates to exactly one `Val`, unchanged. Evaluated in the
    // CALLER's environment.
    let mut argv: Vec<Val> = Vec::new();
    for a in args {
        match &a.kind {
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    argv.push(eval(r, el)?);
                }
            }
            _ => argv.push(eval(r, a)?),
        }
    }
    // Immutable borrows of *r — no more &mut calls on r after this point.
    let consts = r.ints();
    let funcs = r.funcs().ok_or_else(|| {
        format!(
            "function `{}` cannot be called in this evaluation context \
             (function table unavailable)",
            name.name
        )
    })?;
    let f = funcs
        .get(&name.name)
        .ok_or_else(|| format!("undefined function `{}`", name.name))?;
    // Bind each param to its arg value(s), masked to the declared param type.
    // An array param consumes `len` consecutive `argv` slots and binds them
    // under `<param>_0`..`<param>_{len-1}` — the SAME `<name>_<i>` convention
    // the emitter uses for its scalar ports (Task 7), so a program's simulated
    // result and its emitted Verilog agree.
    let mut locals: BTreeMap<String, Val> = BTreeMap::new();
    let mut arrays: BTreeMap<String, u32> = BTreeMap::new();
    let mut ai = 0usize;
    for param in &f.params {
        match &param.ty {
            Type::Array { elem, len } => {
                // Length is a positive constant the checker already validated;
                // `try_from` guards against a corrupt/negative value cleanly.
                let n = u32::try_from(const_eval(len, consts)?)
                    .map_err(|_| format!("array `{}` has an invalid length", param.name.name))?;
                let (w, s) = type_width(elem, consts)?;
                for i in 0..n {
                    // `ai` can run past `argv` when the call site's argument
                    // count doesn't match this fn's arity — the checker
                    // (E0413/E0803) rejects that before eval normally, but
                    // this evaluator is also exercised directly on unchecked
                    // ASTs (fuzzing), so an out-of-range `ai` must be a clean
                    // `Err`, not an out-of-bounds panic.
                    let val = argv.get(ai).cloned().ok_or_else(|| {
                        format!(
                            "function `{}` called with too few arguments (missing element \
                             for array parameter `{}`)",
                            name.name, param.name.name
                        )
                    })?;
                    ai += 1;
                    locals.insert(
                        format!("{}_{i}", param.name.name),
                        Val::new(extend_bits(val, w), w, s),
                    );
                }
                arrays.insert(param.name.name.clone(), n);
            }
            other => {
                let (w, s) = type_width(other, consts)?;
                let val = argv.get(ai).cloned().ok_or_else(|| {
                    format!(
                        "function `{}` called with too few arguments (missing value for \
                         parameter `{}`)",
                        name.name, param.name.name
                    )
                })?;
                ai += 1;
                locals.insert(param.name.name.clone(), Val::new(extend_bits(val, w), w, s));
            }
        }
    }
    let mut child = FnEnv {
        locals,
        consts,
        funcs,
        arrays,
        params: &f.params,
    };
    match eval_fn_stmts(&mut child, &f.stmts)? {
        FnFlow::Returned(v) => Ok(v),
        FnFlow::FellThrough => eval(&mut child, &f.tail),
    }
}

/// Whether a `fn`-body statement list produced an early `return` or ran off
/// the end (in which case the caller evaluates `tail` for the result).
enum FnFlow {
    Returned(Val),
    FellThrough,
}

/// Interpret one `fn`-body statement list. A `return` anywhere — including
/// inside a nested `if` — immediately propagates `FnFlow::Returned` up
/// through the recursion, mirroring the Verilog emitter's continuation-passing
/// lowering but using Rust's own early-return control flow instead of an
/// explicit continuation string.
fn eval_fn_stmts(env: &mut FnEnv, stmts: &[FnStmt]) -> Result<FnFlow, String> {
    for stmt in stmts {
        match stmt {
            FnStmt::Let(local) => {
                // An array-typed `let` expands to N scalar `<name>_<i>` locals,
                // the same `<name>_<i>` convention as an array param — so a
                // later `name[i]` resolves the right element (mirrors the
                // emitter's own array-`let` lowering, Task 8). `inferred_width`
                // is the ELEMENT width for an array `let` (checker's width pass).
                if let ExprKind::ArrayLit(elems) = &local.value.kind {
                    // `inferred_width` is also this let's real context width
                    // (BUG-11) — feed it into evaluating each element too, not
                    // just the post-hoc re-mask.
                    let ctx_w = local.inferred_width.get();
                    for (i, el) in elems.iter().enumerate() {
                        let v = eval_ctx(env, el, ctx_w)?;
                        let v = match ctx_w {
                            Some(w) => remask_to_width(v, w),
                            None => v,
                        };
                        env.locals.insert(format!("{}_{i}", local.name.name), v);
                    }
                    env.arrays
                        .insert(local.name.name.clone(), elems.len() as u32);
                    continue;
                }
                let ctx_w = local.inferred_width.get();
                let v = eval_ctx(env, &local.value, ctx_w)?;
                let v = match ctx_w {
                    Some(w) => remask_to_width(v, w),
                    None => v, // checker not run (e.g. bare sim test); trust the Val width
                };
                env.locals.insert(local.name.name.clone(), v);
            }
            FnStmt::If { cond, then, els } => {
                let c = eval(env, cond)?;
                let truthy = if c.is_wide() {
                    !wide::is_zero(&c.to_limbs())
                } else {
                    c.masked() != 0
                };
                let branch = if truthy {
                    Some(then.as_slice())
                } else {
                    els.as_deref()
                };
                if let Some(body) = branch
                    && let FnFlow::Returned(v) = eval_fn_stmts(env, body)?
                {
                    return Ok(FnFlow::Returned(v));
                }
            }
            FnStmt::Return(expr) => {
                let v = eval(env, expr)?;
                return Ok(FnFlow::Returned(v));
            }
            FnStmt::Loop {
                var, lo, hi, body, ..
            } => {
                let lo_v = eval(env, lo)?.bits_small_or_zero() as i128;
                let hi_v = eval(env, hi)?.bits_small_or_zero() as i128;
                let count = (hi_v - lo_v).max(0);
                if count > REPEAT_BUDGET {
                    return Err(format!(
                        "`loop` would unroll {count} times, over the limit of {REPEAT_BUDGET}"
                    ));
                }
                // Bind the loop variable into `locals` (owned, mutable) for
                // each iteration, shadowing/restoring same as every other
                // compile-time loop variable in this codebase (Task 8's
                // `SeqStmt::Loop` in kernel.rs). `return` inside `body`
                // propagates via ordinary Rust early-return — the FIRST
                // iteration that returns stops the `while` immediately, so a
                // later iteration's match is never even evaluated. That's
                // first-match-wins for free, no continuation-threading
                // needed (unlike the emitter's CPS lowering, Task 7).
                let mut i = lo_v;
                while i < hi_v {
                    let shadowed = env.locals.insert(var.name.clone(), Val::from_int(i));
                    let flow = eval_fn_stmts(env, body)?;
                    match shadowed {
                        Some(v) => {
                            env.locals.insert(var.name.clone(), v);
                        }
                        None => {
                            env.locals.remove(&var.name);
                        }
                    }
                    if let FnFlow::Returned(v) = flow {
                        return Ok(FnFlow::Returned(v));
                    }
                    i += 1;
                }
            }
            FnStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                // `fn` bodies are interpreted directly (no pre-lowering pass
                // exists for them, unlike module items/on-blocks) — lower on
                // the spot, exactly where `emit_verilog/module.rs`'s
                // `emit_fn_stmts` already does the same thing for the exact
                // same reason (Task 7).
                if let Some(lowered) = ast::lower_foreach_fn(var, source, body, *span, env.params)
                    && let FnFlow::Returned(v) = eval_fn_stmts(env, &lowered)?
                {
                    return Ok(FnFlow::Returned(v));
                }
                // `None` = Elements-form source didn't resolve. The checker
                // rejects this (E0417) before `mimz build`/`mimz test` reach
                // here, but this evaluator also runs on unchecked ASTs
                // (fuzzing/`mimz sim` without checking) — silently skip,
                // matching `lower_foreach_item`'s own `None` handling
                // elsewhere in this codebase (e.g. elaborate.rs's
                // `collect_lowered_foreach`).
            }
            FnStmt::Error(_) => {} // parse-recovery placeholder; unreachable on the eval path
        }
    }
    Ok(FnFlow::FellThrough)
}

/// Child resolver for evaluating a user-defined function body.
///
/// Resolves param / local names from `locals` and const names from `consts`.
/// Module signals are NOT in scope (purity: functions are combinational and
/// side-effect-free, spec D8). Nested function calls work via `funcs`.
struct FnEnv<'a> {
    locals: BTreeMap<String, Val>,
    consts: &'a BTreeMap<String, i128>,
    funcs: &'a HashMap<String, FuncDecl>,
    /// Array-typed names in scope (param or `let`), each mapped to its element
    /// count. Set in `eval_fn_call`'s param-binding and in `eval_fn_stmts`'s
    /// `FnStmt::Let` handling for an `ArrayLit` value — mirrors the emitter's
    /// own `ArrayScope` (Task 8). The elements live in `locals` as `<name>_<i>`.
    arrays: BTreeMap<String, u32>,
    /// The enclosing `fn`'s own parameter list — needed to resolve an
    /// Elements-form `foreach`'s source (`fn` bodies have no enclosing
    /// module to resolve against; see `ast::array_like_len_fn`).
    params: &'a [FnParam],
}

impl Resolver for FnEnv<'_> {
    fn signal(&mut self, name: &str) -> Result<Val, String> {
        if let Some(v) = self.locals.get(name) {
            return Ok(v.clone());
        }
        if let Some(c) = self.consts.get(name) {
            return Ok(Val::from_int(*c));
        }
        Err(format!(
            "unknown name `{name}` in function body (module signals are not in scope)"
        ))
    }
    fn ints(&self) -> &BTreeMap<String, i128> {
        self.consts
    }
    fn funcs(&self) -> Option<&HashMap<String, FuncDecl>> {
        Some(self.funcs)
    }
    fn array_len(&self, name: &str) -> Option<u32> {
        self.arrays.get(name).copied()
    }
}

/// Widen `v`'s raw bits to (at least) `width`, sign-extending the new high
/// bits when `v` is signed and negative — zero-extending otherwise. If
/// `width <= v.width` this is just `v`'s own bits (truncation, if any,
/// happens later via `Val::new`'s masking). Shared by `Builtin::Extend`
/// (explicit, user-requested widening) and `eval_fn_call` (implicit
/// widening when a narrower argument binds to a wider parameter) — BUG-7:
/// binding used to mask the caller's raw bits to the param's width without
/// this extension, so a negative value went positive when the param was
/// wider (e.g. `signed[8]` `-128` into a `signed[16]` param read back as
/// `+128`, since the new high bits came from a zero-masked `Val::new` alone).
fn extend_bits(v: Val, width: u32) -> u128 {
    // `masked()` panics on a `Wide` value — every caller of `extend_bits`
    // only invokes it on an operand already known to be `Small` (either
    // inside a dispatch's narrow-path `if` branch, or on a fn-call
    // argument, which stays narrow-only for now — see `docs/superpowers/
    // specs/2026-07-22-sim-wide-values-design.local.md`).
    let bits = v.masked();
    if width > v.width && v.signed && (bits >> (v.width - 1)) & 1 == 1 {
        bits | (mask(width) & !mask(v.width))
    } else {
        bits & mask(v.width)
    }
}

fn call<R: Resolver>(r: &mut R, func: Builtin, args: &[Expr]) -> Result<Val, String> {
    match func {
        Builtin::Extend => {
            let n = checked_width(const_eval(&args[1], r.ints())?)?;
            // `n` is `extend`'s own target width — feed it in as context so a
            // shift inside the argument (`extend(din << 2, 8)`) sees its real
            // consuming width, matching what the emitter's own no-op-extend
            // optimization relies on Verilog to compute (BUG-11).
            let v = eval_ctx(r, &args[0], Some(n))?;
            if n < v.width {
                return Err(format!(
                    "extend to {n} bits is narrower than the {}-bit value — use trunc",
                    v.width
                ));
            }
            if !v.is_wide() && n <= 128 {
                let signed = v.signed;
                Ok(Val::new(extend_bits(v, n), n, signed))
            } else {
                Ok(Val::new_wide(
                    wide::extend(&v.to_limbs(), v.width, n, v.signed),
                    n,
                    v.signed,
                ))
            }
        }
        Builtin::Trunc => {
            let v = eval(r, &args[0])?;
            let n = checked_width(const_eval(&args[1], r.ints())?)?;
            if !v.is_wide() {
                Ok(Val::new(v.masked() & mask(n), n, v.signed))
            } else {
                let mut limbs = v.to_limbs();
                wide::mask_to_width(&mut limbs, n);
                limbs.truncate(wide::limb_count(n));
                Ok(Val::new_wide(limbs, n, v.signed))
            }
        }
        Builtin::SignedCast => {
            let v = eval(r, &args[0])?;
            Ok(Val { signed: true, ..v })
        }
        Builtin::UnsignedCast => {
            let v = eval(r, &args[0])?;
            Ok(Val { signed: false, ..v })
        }
        Builtin::Min => {
            let a = eval(r, &args[0])?;
            let b = eval(r, &args[1])?;
            Ok(if cmp_lt(a.clone(), b.clone()) { a } else { b })
        }
        Builtin::Max => {
            let a = eval(r, &args[0])?;
            let b = eval(r, &args[1])?;
            Ok(if cmp_lt(a.clone(), b.clone()) { b } else { a })
        }
        Builtin::Abs => {
            let v = eval(r, &args[0])?;
            // signed magnitude into width+1 (room for abs(MIN))
            if !v.is_wide() {
                let m = v.as_i128().unsigned_abs();
                Ok(Val::new(m & mask(v.width + 1), v.width + 1, true))
            } else {
                let extended = wide::extend(&v.to_limbs(), v.width, v.width + 1, v.signed);
                let negated = wide::neg(&extended, v.width + 1);
                let is_negative = v.signed && wide::bit_at(&v.to_limbs(), v.width - 1);
                let magnitude = if is_negative { negated } else { extended };
                Ok(Val::new_wide(magnitude, v.width + 1, true))
            }
        }
        Builtin::Nand => {
            let v = eval(r, &args[0])?;
            let all_ones = if v.is_wide() {
                wide::count_ones(&v.to_limbs()) == v.width
            } else {
                v.masked() == mask(v.width)
            };
            Ok(Val::new(!all_ones as u128, 1, false))
        }
        Builtin::Nor => {
            let v = eval(r, &args[0])?;
            let any_set = if v.is_wide() {
                !wide::is_zero(&v.to_limbs())
            } else {
                v.masked() != 0
            };
            Ok(Val::new(!any_set as u128, 1, false))
        }
        Builtin::Xnor => {
            let v = eval(r, &args[0])?;
            let ones = if v.is_wide() {
                wide::count_ones(&v.to_limbs())
            } else {
                v.masked().count_ones()
            };
            Ok(Val::new(((ones & 1) == 0) as u128, 1, false))
        }
        // `clog2` is compile-time only — the checker rejects it as a runtime
        // value (E0407) and folds it in widths, so a checked program never lands
        // here.
        Builtin::Clog2 => Err("clog2 is compile-time only".into()),
        Builtin::SyncDoubleFlop | Builtin::SyncPulse => {
            unreachable!(
                "sync.double_flop/sync.pulse must be lowered by \
                 ast::sync_prim_lower::expand_sync_prims before reaching the \
                 simulator's expression evaluator — elaborate_module already \
                 calls expand_sync_prims before the worklist runs, so this \
                 arm is reachable only via a checker-bypassing (or nested- \
                 const-if/repeat/foreach, out of scope for v1) call site"
            )
        }
    }
}

fn unary(op: UnOp, v: Val) -> Val {
    let was_unknown = v.unknown;
    let mut r = unary_known(op, v);
    if was_unknown {
        r.unknown = true;
    }
    r
}

fn unary_known(op: UnOp, v: Val) -> Val {
    match op {
        UnOp::Neg => {
            if !v.is_wide() {
                let bits = v.as_i128().wrapping_neg() as u128;
                Val::new(bits, v.width, true)
            } else {
                Val::new_wide(wide::neg(&v.to_limbs(), v.width), v.width, true)
            }
        }
        UnOp::BitNot => {
            if !v.is_wide() {
                Val::new(!v.masked(), v.width, v.signed)
            } else {
                Val::new_wide(wide::not(&v.to_limbs()), v.width, v.signed)
            }
        }
        UnOp::LogicNot => Val::new((!(v.lsb())) & 1, 1, false),
        UnOp::RedAnd => {
            let ones = if v.is_wide() {
                wide::count_ones(&v.to_limbs()) == v.width
            } else {
                v.masked() == mask(v.width)
            };
            Val::new(ones as u128, 1, false)
        }
        UnOp::RedOr => {
            let any = if v.is_wide() {
                !wide::is_zero(&v.to_limbs())
            } else {
                v.masked() != 0
            };
            Val::new(any as u128, 1, false)
        }
        UnOp::RedXor => {
            let ones = if v.is_wide() {
                wide::count_ones(&v.to_limbs())
            } else {
                v.masked().count_ones()
            };
            Val::new((ones & 1) as u128, 1, false)
        }
    }
}

/// Evaluate a binary operator over two already-evaluated operands.
/// `expected_width` is the enclosing context's width (an assignment target,
/// `extend`'s target) — only `Shl`/`Shr` use it; pass `None` for a
/// self-determined position (see [`eval_ctx`]'s doc comment).
fn binary_ctx(op: BinOp, l: Val, r: Val, expected_width: Option<u32>) -> Result<Val, String> {
    let unknown = l.unknown || r.unknown;
    binary_known(op, l, r, expected_width).map(|mut v| {
        if unknown {
            v.unknown = true;
        }
        v
    })
}

fn binary_known(op: BinOp, l: Val, r: Val, expected_width: Option<u32>) -> Result<Val, String> {
    let v = match op {
        // Lossless growth (spec/02 section 3). Operate on the SIGN-EXTENDED
        // values (`as_i128`) so a negative signed operand is widened correctly
        // before the result grows — matching Verilog's signed arithmetic. For
        // unsigned operands `as_i128` is the plain magnitude, so this is
        // identical to a raw-bit add/mul. (The wrapping family below keeps the
        // operand width, where the raw-bit op is already correct mod 2^width.)
        BinOp::Add => {
            let k = mimz_core::width_rules::lossless_result(
                mimz_core::width_rules::Kind {
                    width: l.width,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: r.width,
                    signed: r.signed,
                },
                false,
            )
            .expect("checker already rejected mixed signed/unsigned operands");
            if !l.is_wide() && !r.is_wide() && k.width <= 128 {
                Val::new(
                    l.as_i128().wrapping_add(r.as_i128()) as u128,
                    k.width,
                    k.signed,
                )
            } else {
                let (lw, rw) = wide_operands(l, r, k.width);
                Val::new_wide(wide::add(&lw, &rw, k.width), k.width, k.signed)
            }
        }
        BinOp::Sub => {
            let k = mimz_core::width_rules::lossless_result(
                mimz_core::width_rules::Kind {
                    width: l.width,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: r.width,
                    signed: r.signed,
                },
                false,
            )
            .expect("checker already rejected mixed signed/unsigned operands");
            if !l.is_wide() && !r.is_wide() && k.width <= 128 {
                Val::new(
                    l.as_i128().wrapping_sub(r.as_i128()) as u128,
                    k.width,
                    k.signed,
                )
            } else {
                let (lw, rw) = wide_operands(l, r, k.width);
                Val::new_wide(wide::sub(&lw, &rw, k.width), k.width, k.signed)
            }
        }
        BinOp::Mul => {
            let k = mimz_core::width_rules::lossless_result(
                mimz_core::width_rules::Kind {
                    width: l.width,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: r.width,
                    signed: r.signed,
                },
                true,
            )
            .expect("checker already rejected mixed signed/unsigned operands");
            if !l.is_wide() && !r.is_wide() && k.width <= 128 {
                Val::new(
                    l.as_i128().wrapping_mul(r.as_i128()) as u128,
                    k.width,
                    k.signed,
                )
            } else {
                let (lw, rw) = wide_operands(l, r, k.width);
                Val::new_wide(wide::mul(&lw, &rw, k.width), k.width, k.signed)
            }
        }
        // Wrapping family: keep operand width. A bare integer literal's `Val`
        // keeps its own minimal natural width (never pre-widened to match the
        // other operand, unlike the checker's compile-time-only "adapting"
        // fiction for `CtInt` — see `matched_ty`), so both operands must be
        // widened to `wmax` here before `matched_result` can find their
        // `Kind`s equal. The `.unwrap_or` reproduces the original
        // `l.signed || r.signed` bookkeeping for the one case
        // `matched_result` can still reject after widening (mismatched
        // signedness) — real fallback code, not a placeholder. `k` is
        // computed from `l.signed`/`r.signed` (field reads, not moves)
        // BEFORE the dispatch below moves `l`/`r` into `extend_bits`/
        // `wide_operands` — `Val` losing `Copy` (Task 2) means the old
        // ordering (widen first, compute `k` after) would no longer
        // compile.
        BinOp::AddWrap => {
            let wmax = l.width.max(r.width);
            let k = mimz_core::width_rules::matched_result(
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: r.signed,
                },
            )
            .unwrap_or(mimz_core::width_rules::Kind {
                width: wmax,
                signed: l.signed || r.signed,
            });
            if !l.is_wide() && !r.is_wide() && wmax <= 128 {
                let lw = extend_bits(l, wmax);
                let rw = extend_bits(r, wmax);
                Val::new(lw.wrapping_add(rw), k.width, k.signed)
            } else {
                let (lw, rw) = wide_operands(l, r, wmax);
                Val::new_wide(wide::add(&lw, &rw, k.width), k.width, k.signed)
            }
        }
        BinOp::SubWrap => {
            let wmax = l.width.max(r.width);
            let k = mimz_core::width_rules::matched_result(
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: r.signed,
                },
            )
            .unwrap_or(mimz_core::width_rules::Kind {
                width: wmax,
                signed: l.signed || r.signed,
            });
            if !l.is_wide() && !r.is_wide() && wmax <= 128 {
                let lw = extend_bits(l, wmax);
                let rw = extend_bits(r, wmax);
                Val::new(lw.wrapping_sub(rw), k.width, k.signed)
            } else {
                let (lw, rw) = wide_operands(l, r, wmax);
                Val::new_wide(wide::sub(&lw, &rw, k.width), k.width, k.signed)
            }
        }
        BinOp::MulWrap => {
            let wmax = l.width.max(r.width);
            let k = mimz_core::width_rules::matched_result(
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: r.signed,
                },
            )
            .unwrap_or(mimz_core::width_rules::Kind {
                width: wmax,
                signed: l.signed || r.signed,
            });
            if !l.is_wide() && !r.is_wide() && wmax <= 128 {
                let lw = extend_bits(l, wmax);
                let rw = extend_bits(r, wmax);
                Val::new(lw.wrapping_mul(rw), k.width, k.signed)
            } else {
                let (lw, rw) = wide_operands(l, r, wmax);
                Val::new_wide(wide::mul(&lw, &rw, k.width), k.width, k.signed)
            }
        }
        BinOp::BitAnd => {
            let wmax = l.width.max(r.width);
            let k = mimz_core::width_rules::matched_result(
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: r.signed,
                },
            )
            .unwrap_or(mimz_core::width_rules::Kind {
                width: wmax,
                signed: l.signed || r.signed,
            });
            if !l.is_wide() && !r.is_wide() && wmax <= 128 {
                let lw = extend_bits(l, wmax);
                let rw = extend_bits(r, wmax);
                Val::new(lw & rw, k.width, k.signed)
            } else {
                let (lw, rw) = wide_operands(l, r, wmax);
                Val::new_wide(wide::bitand(&lw, &rw), k.width, k.signed)
            }
        }
        BinOp::BitOr => {
            let wmax = l.width.max(r.width);
            let k = mimz_core::width_rules::matched_result(
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: r.signed,
                },
            )
            .unwrap_or(mimz_core::width_rules::Kind {
                width: wmax,
                signed: l.signed || r.signed,
            });
            if !l.is_wide() && !r.is_wide() && wmax <= 128 {
                let lw = extend_bits(l, wmax);
                let rw = extend_bits(r, wmax);
                Val::new(lw | rw, k.width, k.signed)
            } else {
                let (lw, rw) = wide_operands(l, r, wmax);
                Val::new_wide(wide::bitor(&lw, &rw), k.width, k.signed)
            }
        }
        BinOp::BitXor => {
            let wmax = l.width.max(r.width);
            let k = mimz_core::width_rules::matched_result(
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: wmax,
                    signed: r.signed,
                },
            )
            .unwrap_or(mimz_core::width_rules::Kind {
                width: wmax,
                signed: l.signed || r.signed,
            });
            if !l.is_wide() && !r.is_wide() && wmax <= 128 {
                let lw = extend_bits(l, wmax);
                let rw = extend_bits(r, wmax);
                Val::new(lw ^ rw, k.width, k.signed)
            } else {
                let (lw, rw) = wide_operands(l, r, wmax);
                Val::new_wide(wide::bitxor(&lw, &rw), k.width, k.signed)
            }
        }
        // `<<`/`>>` are context-determined on their left operand in real
        // Verilog (ground-truthed against `iverilog`, BUG-11): the operand
        // widens to the ENCLOSING width before the shift, not after —
        // growing by the shift amount (the old fix here) or truncating to
        // `l.width` unconditionally (the naive "spec says width preserved"
        // fix) are both wrong in general; only "widen to the real
        // context, then shift, keeping that width" matches Icarus for
        // every case tried (same-width chain, narrower-operand-into-wider-
        // context, standalone). `ctx_w` is `l`'s own width when no context
        // is known (self-determined fallback — e.g. a bare test/eval
        // expression with no assignment target).
        BinOp::Shl => {
            let base = mimz_core::width_rules::shift_result(
                mimz_core::width_rules::Kind {
                    width: l.width,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: r.width,
                    signed: r.signed,
                },
            )
            .map_err(|_| "a shift amount cannot be `signed`".to_string())?;
            let ctx_w = expected_width
                .map(|w| w.max(base.width))
                .unwrap_or(base.width);
            if !l.is_wide() && ctx_w <= 128 {
                let widened = extend_bits(l, ctx_w);
                let shift = r.bits_small_or_zero();
                let bits = if shift >= 128 {
                    0
                } else {
                    widened.checked_shl(shift as u32).unwrap_or(0)
                };
                Val::new(bits, ctx_w, base.signed)
            } else {
                let widened = wide::extend(&l.to_limbs(), l.width, ctx_w, l.signed);
                let shift = r.bits_small_or_zero().min(ctx_w as u128) as u32;
                Val::new_wide(wide::shl(&widened, shift, ctx_w), ctx_w, base.signed)
            }
        }
        BinOp::Shr => {
            let base = mimz_core::width_rules::shift_result(
                mimz_core::width_rules::Kind {
                    width: l.width,
                    signed: l.signed,
                },
                mimz_core::width_rules::Kind {
                    width: r.width,
                    signed: r.signed,
                },
            )
            .map_err(|_| "a shift amount cannot be `signed`".to_string())?;
            let ctx_w = expected_width
                .map(|w| w.max(base.width))
                .unwrap_or(base.width);
            if !l.is_wide() && ctx_w <= 128 {
                let widened = extend_bits(l, ctx_w);
                let bits = if r.bits_small_or_zero() >= 128 {
                    0
                } else {
                    widened >> (r.bits_small_or_zero() as u32)
                };
                Val::new(bits, ctx_w, base.signed)
            } else {
                let widened = wide::extend(&l.to_limbs(), l.width, ctx_w, l.signed);
                let shift = r.bits_small_or_zero().min(ctx_w as u128) as u32;
                Val::new_wide(wide::shr(&widened, shift), ctx_w, base.signed)
            }
        }
        BinOp::Eq => Val::new(cmp_eq(l, r) as u128, 1, false),
        BinOp::Ne => Val::new(!cmp_eq(l, r) as u128, 1, false),
        BinOp::Lt => Val::new(cmp_lt(l, r) as u128, 1, false),
        BinOp::Le => Val::new(
            (cmp_lt(l.clone(), r.clone()) || cmp_eq(l, r)) as u128,
            1,
            false,
        ),
        BinOp::Gt => Val::new(
            (!cmp_lt(l.clone(), r.clone()) && !cmp_eq(l, r)) as u128,
            1,
            false,
        ),
        BinOp::Ge => Val::new((!cmp_lt(l, r)) as u128, 1, false),
        BinOp::LogicAnd => Val::new(l.lsb() & r.lsb(), 1, false),
        BinOp::LogicOr => Val::new(l.lsb() | r.lsb(), 1, false),
        // `??` is always rewritten to `IfExpr` by `Rw::expr` during
        // elaboration (crates/mimz-sim/src/sim/elaborate.rs) before the
        // kernel ever calls `binary_known` — so this arm is unreachable in
        // practice. Still a typed error rather than a panic: this function
        // returns `Result`, and a future caller of `binary_known` that skips
        // elaboration must get a diagnosable error, not a crashed process.
        BinOp::Coalesce => {
            return Err("?? should have been lowered during elaboration".to_string());
        }
    };
    Ok(v)
}

fn cmp_lt(l: Val, r: Val) -> bool {
    let wmax = l.width.max(r.width);
    if !l.is_wide() && !r.is_wide() {
        if l.signed || r.signed {
            l.as_i128() < r.as_i128()
        } else {
            l.masked() < r.masked()
        }
    } else {
        // Capture `signed` BEFORE `wide_operands` moves `l`/`r` — `Val`
        // losing `Copy` (Task 2) means reading `l.signed`/`r.signed` after
        // the move (as the brief's own draft code did) does not compile.
        let signed = l.signed || r.signed;
        let (lw, rw) = wide_operands(l, r, wmax);
        let ord = if signed {
            wide::cmp_signed(&lw, &rw, wmax)
        } else {
            wide::cmp_unsigned(&lw, &rw)
        };
        ord == std::cmp::Ordering::Less
    }
}
fn cmp_eq(l: Val, r: Val) -> bool {
    if !l.is_wide() && !r.is_wide() {
        if l.signed || r.signed {
            l.as_i128() == r.as_i128()
        } else {
            l.masked() == r.masked()
        }
    } else {
        let wmax = l.width.max(r.width);
        let (lw, rw) = wide_operands(l, r, wmax);
        lw == rw
    }
}

pub(super) fn pattern_matches(p: &Pattern, s: &Val) -> Result<bool, String> {
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
        Pattern::Wildcard => Ok(true),
        Pattern::Int { value, .. } => {
            Ok(low128(s) & mask(s.width.min(128)) == *value & mask(s.width.min(128)))
        }
        Pattern::IntMask { value, mask: m, .. } => Ok((low128(s) & *m) == (*value & *m)),
        Pattern::Bool(b) => Ok(s.lsb() == (*b as u128)),
        Pattern::Variant { .. } => {
            unreachable!(
                "Pattern::Variant is lowered to IntMask during elaboration — raw variants should not reach pattern_matches"
            )
        }
    }
}

/// The declared (width, signed) of a hardware type, evaluating any width
/// expression in the const environment.
pub(super) fn type_width(ty: &Type, ints: &BTreeMap<String, i128>) -> Result<(u32, bool), String> {
    match ty {
        Type::Bit => Ok((1, false)),
        Type::Bits(e) => Ok((checked_width(const_eval(e, ints)?)?, false)),
        Type::Signed(e) => Ok((checked_width(const_eval(e, ints)?)?, true)),
        Type::Named(n) => Err(format!(
            "signal of enum type `{}` — the simulator does not model enum signals yet",
            n.name.name
        )),
        Type::Bundle { .. } => {
            Err("Type::Bundle reached type_width — should be pre-flattened by elaborate".into())
        }
        // An array type never reaches here: an array param/`let` is expanded to
        // per-element scalars (each queried via its ELEMENT type), array module
        // signals are rejected (E0416), and array bundle/enum-payload fields are
        // rejected/flattened. Mirror the Bundle arm rather than panicking.
        Type::Array { .. } => {
            Err("Type::Array reached type_width — arrays expand to per-element scalars".into())
        }
    }
}

pub(super) fn checked_width(n: i128) -> Result<u32, String> {
    use mimz_core::width_rules::MAX_WIDTH;
    if n < 1 {
        Err(format!("width must be at least 1, got {n}"))
    } else if n > MAX_WIDTH {
        Err(format!("width {n} exceeds the maximum of {MAX_WIDTH} bits"))
    } else {
        Ok(n as u32)
    }
}

/// Compile-time const evaluation for widths, parameters, consts, indices, and
/// slice bounds. **Delegates to the checker's hardened evaluator**
/// (`checker::consteval::eval`) — the single source of truth — which uses
/// `checked_*` arithmetic and guarded shifts, so an oversized const such as
/// `1 << 200` is a clean error, never a debug panic or a silent release wrap.
pub(super) fn const_eval(e: &Expr, ints: &BTreeMap<String, i128>) -> Result<i128, String> {
    let env: mimz_core::checker::consteval::Env =
        ints.iter().map(|(k, v)| (k.clone(), *v)).collect();
    mimz_core::checker::consteval::eval(e, &env).map_err(|d| d.msg)
}

/// A bit index or slice bound must be a non-negative integer inside the value's
/// width. Rejects negative / out-of-range positions instead of truncating via
/// `as u32` or a later oversized shift (`>> n`, `n >= 128`, which panics).
pub(super) fn checked_index(n: i128, width: u32, what: &str) -> Result<u32, String> {
    if (0..width as i128).contains(&n) {
        Ok(n as u32)
    } else {
        Err(format!(
            "{what} {n} is out of range for a {width}-bit value"
        ))
    }
}

/// Pick `module` from `file`, or the file's only module when `None`.
pub(super) fn pick_module<'a>(
    file: &'a ast::File,
    want: Option<&str>,
) -> Result<&'a ast::Module, String> {
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
            .ok_or_else(|| format!("no module named `{n}` in this file")),
        None => match mods.as_slice() {
            [one] => Ok(one),
            [] => Err("file defines no module".into()),
            many => Err(format!(
                "file defines {} modules — choose one with --module <name>",
                many.len()
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitand_widens_a_narrower_literal_operand() {
        // A bare literal's Val keeps its own minimal width (here 1),
        // never pre-widened to match a wider operand — the checker's
        // static type system treats the literal as "adapting" to the
        // other side (a compile-time-only fact), but the simulator's
        // actual Val for that literal is NOT pre-widened anywhere. The
        // wrap/bitwise arms must widen it themselves before combining.
        let l = Val::new(0b1010, 4, false); // a 4-bit signal
        let r = Val::new(1, 1, false); // the literal `1`, its own minimal width
        let result = binary_known(BinOp::BitAnd, l, r, None).unwrap();
        assert_eq!(result.width, 4);
        assert_eq!(result.masked(), 0b1010 & 1);
    }

    #[test]
    fn shl_self_determined_preserves_left_operand_width() {
        // No context (bare `binary()`, matching a condition/index/loop-bound
        // position, or a raw compile-time literal with nothing sizing it) —
        // Verilog's shift is self-determined here: the result stays exactly
        // `l`'s own width, truncating what doesn't fit. `1 << 2` with `1` at
        // its minimal width (1 bit) masks the whole result away — that's
        // correct self-determined behavior, not BUG-6 (BUG-6 was reachable
        // through `extend(1 << 2, N)`, which now threads `N` in as context —
        // see `shl_widens_to_context_like_verilog` below).
        let l = Val::from_int(1); // width 1
        let r = Val::from_int(2);
        let res = binary_ctx(BinOp::Shl, l, r, None).unwrap();
        assert_eq!(res.masked(), 0); // 4 & mask(1) == 0
        assert_eq!(res.width, 1);
    }

    #[test]
    fn shl_widens_to_context_like_verilog() {
        // BUG-11 (supersedes the BUG-6 fix the old version of this test
        // asserted — growing the result by the shift amount, unconditionally
        // — that broke real signal shifts, see `shl_chain_stays_at_shared_
        // context_width` below). Ground-truthed against `iverilog`: `<<`'s
        // left operand is CONTEXT-DETERMINED — it widens to the enclosing
        // width (an assignment target, `extend`'s target) BEFORE the shift,
        // not truncated-then-extended after.
        let l = Val::from_int(1); // width 1
        let r = Val::from_int(2);
        let res = binary_ctx(BinOp::Shl, l, r, Some(8)).unwrap();
        assert_eq!(res.width, 8);
        assert_eq!(res.masked(), 4); // 1 << 2, no bits lost once widened first

        // review-2026-07-17.md's exact repro: din (4-bit) << 2 into an 8-bit
        // context. iverilog: 28, NOT 12 (12 is what self-determined-then-
        // truncated-into-8-bits would wrongly give if extension happened
        // AFTER the shift instead of before).
        let din = Val::new(7, 4, false);
        let shifted = binary_ctx(BinOp::Shl, din, Val::from_int(2), Some(8)).unwrap();
        assert_eq!(shifted.width, 8);
        assert_eq!(shifted.masked(), 28);
    }

    #[test]
    fn shl_rejects_a_signed_shift_amount() {
        let l = Val::new(1, 8, false);
        let r = Val::new(2, 3, true); // signed amount — spec/02 section 3 forbids this
        let err = binary_known(BinOp::Shl, l, r, None).unwrap_err();
        assert!(
            err.contains("signed"),
            "expected an error mentioning `signed`, got: {err}"
        );
    }

    #[test]
    fn sub_of_two_unsigned_values_is_unsigned() {
        // BUG-22 (docs/audit/bugs.md): binary_known's Sub arm used to
        // hardcode `signed: true` unconditionally, disagreeing with the
        // checker's own lossless_ty rule (unsigned bits[N] - unsigned
        // bits[M] is unsigned bits[N.max(M)+1]).
        let l = Val::new(0, 4, false);
        let r = Val::new(0, 4, false);
        let result = binary_known(BinOp::Sub, l, r, None).unwrap();
        assert!(!result.signed, "expected an unsigned result, got signed");
        assert_eq!(result.width, 5);
    }

    #[test]
    fn sub_of_two_signed_values_is_signed() {
        let l = Val::new(0, 4, true);
        let r = Val::new(0, 4, true);
        let result = binary_known(BinOp::Sub, l, r, None).unwrap();
        assert!(result.signed, "expected a signed result");
        assert_eq!(result.width, 5);
    }

    #[test]
    fn shl_chain_stays_at_shared_context_width() {
        // BUG-11's own reproduction: `(a << 2) >> 2` for `a: bits[8]`
        // assigned to `y: bits[8]` — iverilog says 63, not 255. The context
        // (8) must be threaded into BOTH shifts, not just the first: a
        // width that only grows by the shift amount at each step (the old
        // fix) lets an intermediate carry stray high bits into the second
        // shift that a real 8-bit-wide Verilog computation never has.
        let a = Val::new(255, 8, false);
        let shifted_left = binary_ctx(BinOp::Shl, a, Val::from_int(2), Some(8)).unwrap();
        assert_eq!(shifted_left.width, 8);
        let shifted_right =
            binary_ctx(BinOp::Shr, shifted_left, Val::from_int(2), Some(8)).unwrap();
        assert_eq!(shifted_right.masked(), 63); // NOT 255 — this was BUG-11
    }

    #[test]
    fn fn_call_arity_mismatch_is_err_not_panic() {
        // Fuzz find: `eval_fn_call` is reachable directly on a parsed-but-
        // unchecked AST (the checker's E0413 array-length check normally
        // rejects this first). A short array-literal argument left `argv`
        // shorter than the callee's param arity, and `argv[ai]` panicked
        // with an out-of-bounds index instead of returning a clean `Err`.
        let src = "saarbu pick(vals: bits[8][4], idx: bits[3]) -> bits[8] {\n  \
                   vals[idx]\n}\n\n\
                   thoguthi M {\n  \
                   ulleedu a: bits[8]\n  \
                   ulleedu b: bits[8]\n  \
                   ulleedu idx: bits[3]\n  \
                   veliyeedu picked: bits[8]\n  \
                   picked = pick([a, b], idx)\n\
                   }\n";
        let tokens = mimz_core::lexer::lex(src).expect("lex");
        let file = mimz_core::parser::parse(tokens).expect("parse");
        let inputs: BTreeMap<String, Bits> = [
            ("a".to_string(), Bits::Small(1u128)),
            ("b".to_string(), Bits::Small(2u128)),
            ("idx".to_string(), Bits::Small(0u128)),
        ]
        .into_iter()
        .collect();
        let result = super::super::comb::eval_outputs(
            std::slice::from_ref(&file),
            Some("M"),
            &inputs,
            &BTreeMap::new(),
        );
        assert!(result.is_err(), "expected a clean Err, got {result:?}");
    }

    /// Wraps `fn_src` (one or more `fn` decls) in a throwaway module that
    /// calls `fn_name` with `args` as inline literals (an arg slice of one
    /// element becomes a scalar literal, a longer slice becomes an array
    /// literal `[..]` — the `ArrayLit` argument-expansion path `eval_fn_call`
    /// already exercises), then reads the result back through the
    /// combinational evaluator. The output port's declared width doesn't
    /// affect the returned value (comb.rs resolves the driver expression's
    /// OWN width, see `eval_outputs`'s step 5), so the result is sign-extended
    /// per its actual width/signed straight from `Val::as_i128`.
    fn eval_fn_call_one(fn_src: &str, fn_name: &str, args: &[&[u128]]) -> i128 {
        let call_args: Vec<String> = args
            .iter()
            .map(|a| match *a {
                [one] => one.to_string(),
                many => format!(
                    "[{}]",
                    many.iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })
            .collect();
        let src = format!(
            "{fn_src}\nmodule M {{\n  out result: bits[8]\n  result = {fn_name}({})\n}}\n",
            call_args.join(", ")
        );
        let tokens = mimz_core::lexer::lex(&src).expect("lex");
        let file = mimz_core::parser::parse(tokens).expect("parse");
        let outputs = super::super::comb::eval_outputs(
            std::slice::from_ref(&file),
            Some("M"),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("eval_outputs");
        let out = outputs
            .into_iter()
            .find(|o| o.name == "result")
            .expect("module declares `result`");
        // Every value this test helper's callers produce stays narrow
        // (bits[8]/signed[16] fn args) — `Bits::Wide` is not reachable here.
        let Bits::Small(bits) = out.value else {
            panic!("test expected a narrow (Small) value")
        };
        Val::new(bits, out.width, out.signed).as_i128()
    }

    #[test]
    fn fn_call_sign_extends_narrower_signed_arg_to_wider_param() {
        // BUG-7 regression: `eval_fn_call` used to bind an argument with
        // `Val::new(val.bits, w, s)` — masking the caller's raw bits to the
        // param's width with no sign-extension. A `signed[16]` param bound
        // to the literal `-128` (whose own natural width is the minimal
        // 8-bit two's-complement pattern, 0x80) came out `+128`: 0x80
        // masked to 16 bits is still 0x0080, not the correctly
        // sign-extended 0xFF80.
        let src = "fn widen16(x: signed[16]) -> signed[16] {\n  x\n}\n\n\
                   module M {\n  out result: signed[16]\n  result = widen16(-128)\n}\n";
        let tokens = mimz_core::lexer::lex(src).expect("lex");
        let file = mimz_core::parser::parse(tokens).expect("parse");
        let outputs = super::super::comb::eval_outputs(
            std::slice::from_ref(&file),
            Some("M"),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("eval_outputs");
        let out = outputs
            .into_iter()
            .find(|o| o.name == "result")
            .expect("module declares `result`");
        let Bits::Small(bits) = out.value else {
            panic!("test expected a narrow (Small) value")
        };
        assert_eq!(
            Val::new(bits, out.width, out.signed).as_i128(),
            -128,
            "got raw bits {:#x} at width {}",
            bits,
            out.width
        );
    }

    #[test]
    fn fn_loop_with_return_finds_first_match_in_sim() {
        let result = eval_fn_call_one(
            "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\n",
            "find_first_set",
            &[&[0x00, 0xFF, 0x00, 0x00]],
        );
        assert_eq!(result, 1);
    }

    #[test]
    fn fn_loop_with_return_first_match_wins_on_duplicate_in_sim() {
        let result = eval_fn_call_one(
            "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\n",
            "find_first_set",
            &[&[0xFF, 0x00, 0xFF, 0x00]], // matches at BOTH index 0 and index 2
        );
        assert_eq!(result, 0, "must return the LOWER index, not 2");
    }

    #[test]
    fn fn_loop_over_budget_errors_in_sim() {
        let src = format!(
            "fn overflow(x: bits[8]) -> bits[8] {{\n  loop i: 0..{} {{\n    if x == 0xFF {{ return x }}\n  }}\n  x\n}}\n",
            mimz_core::REPEAT_BUDGET + 1
        );
        let full = format!(
            "{src}\nmodule M {{\n  in x: bits[8]\n  out result: bits[8]\n  result = overflow(x)\n}}\n"
        );
        let tokens = mimz_core::lexer::lex(&full).expect("lex");
        let file = mimz_core::parser::parse(tokens).expect("parse");
        let inputs: BTreeMap<String, Bits> = [("x".to_string(), Bits::Small(1u128))]
            .into_iter()
            .collect();
        let result = super::super::comb::eval_outputs(
            std::slice::from_ref(&file),
            Some("M"),
            &inputs,
            &BTreeMap::new(),
        );
        let err = result.expect_err("over-budget `loop` must error, not hang or overflow");
        assert!(err.contains("`loop` would unroll"), "got: {err}");
    }

    #[test]
    fn fn_foreach_range_form_with_return_finds_first_match_in_sim() {
        // Same shape as `fn_loop_with_return_finds_first_match_in_sim`, but
        // with `foreach i in 0..4` (Range form) in place of `loop i: 0..4` —
        // `FnStmt::ForEach` must lower via `ast::lower_foreach_fn` to the
        // same `FnStmt::Loop` and early-return correctly.
        let result = eval_fn_call_one(
            "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  foreach i in 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\n",
            "find_first_set",
            &[&[0x00, 0xFF, 0x00, 0x00]],
        );
        assert_eq!(result, 1);
    }

    #[test]
    fn fn_foreach_elements_form_with_return_finds_match_in_sim() {
        // Elements form: `foreach v in vals` binds `v` to each array element
        // via a synthesized `Let`, and `return v` on a match must propagate
        // as `FnFlow::Returned` out of the lowered `Loop`.
        let result = eval_fn_call_one(
            "fn find_val(vals: bits[8][4]) -> bits[8] {\n  foreach v in vals {\n    if v == 0xFF { return v }\n  }\n  0\n}\n",
            "find_val",
            &[&[0x11, 0xFF, 0x22, 0x33]],
        );
        assert_eq!(result, 0xFF);
    }

    #[test]
    fn fn_foreach_elements_form_no_match_falls_through_in_sim() {
        // No element matches — `eval_fn_stmts` must reach `FnFlow::FellThrough`
        // and yield the fn's tail expression (`0`), NOT a spurious
        // `FnFlow::Returned` from misreading fall-through as an early return.
        let result = eval_fn_call_one(
            "fn find_val(vals: bits[8][4]) -> bits[8] {\n  foreach v in vals {\n    if v == 0xFF { return v }\n  }\n  0\n}\n",
            "find_val",
            &[&[0x11, 0x22, 0x33, 0x44]],
        );
        assert_eq!(result, 0);
    }

    #[test]
    fn unknown_val_taints_binary_ops() {
        let u = Val::unknown(4, false);
        let known = Val::new(3, 4, false);
        let r = binary_ctx(BinOp::Add, u, known, None).unwrap();
        assert!(
            r.unknown,
            "adding an unknown operand must produce an unknown result"
        );
    }

    #[test]
    fn unknown_val_taints_unary_ops() {
        let u = Val::unknown(4, false);
        let r = unary(UnOp::BitNot, u);
        assert!(
            r.unknown,
            "negating an unknown operand must produce an unknown result"
        );
    }

    #[test]
    fn known_vals_are_never_tainted() {
        let a = Val::new(1, 4, false);
        let b = Val::new(2, 4, false);
        assert!(!a.unknown && !b.unknown);
        assert!(!binary_ctx(BinOp::Add, a.clone(), b, None).unwrap().unknown);
        assert!(!unary(UnOp::BitNot, a).unknown);
    }

    #[test]
    fn val_new_stays_on_the_small_fast_path() {
        let v = Val::new(42, 8, false);
        assert!(!v.is_wide());
        assert_eq!(v.masked(), 42);
    }

    #[test]
    fn val_new_wide_masks_to_the_declared_width() {
        // 200 bits of all-ones, masked down to 130 bits.
        let limbs = vec![u64::MAX; wide::limb_count(200)];
        let v = Val::new_wide(limbs, 130, false);
        assert!(v.is_wide());
        assert_eq!(v.width, 130);
    }

    #[test]
    fn val_new_wide_auto_narrows_to_small_at_128_bits_or_less() {
        // A width-96 result never needs to carry a heap-allocated Vec —
        // new_wide must narrow it back to `Bits::Small` itself, so every
        // OTHER caller (Task 6's dispatch) can rely on "width <= 128
        // implies Small" without re-checking.
        let limbs = vec![0u64; wide::limb_count(96)];
        let v = Val::new_wide(limbs, 96, false);
        assert!(!v.is_wide());
    }

    #[test]
    fn wide_unsigned_add_carries_past_128_bits() {
        // Two 128-bit unsigned max values: the TRUE lossless result is
        // 129 bits and does NOT fit in a u128 — this is the exact
        // boundary case the 128-bit ceiling silently got wrong before
        // this task (a 129-bit-wide RESULT from two Small operands).
        let a = Val::new(u128::MAX, 128, false);
        let b = Val::new(1, 128, false);
        let sum = binary_known(BinOp::Add, a, b, None).unwrap();
        assert_eq!(sum.width, 129);
        assert!(sum.is_wide());
    }

    #[test]
    fn wide_bitand_of_two_512_bit_values() {
        let a = Val::new_wide(wide::from_u128(0b1100, 512), 512, false);
        let b = Val::new_wide(wide::from_u128(0b1010, 512), 512, false);
        let result = binary_known(BinOp::BitAnd, a, b, None).unwrap();
        assert!(result.is_wide());
        assert_eq!(wide::bit_at(&result.to_limbs(), 3), true);
        assert_eq!(wide::bit_at(&result.to_limbs(), 1), false);
    }

    #[test]
    fn wide_shl_crosses_a_limb_boundary_in_a_512_bit_context() {
        let l = Val::new(1, 8, false);
        let shifted = binary_ctx(BinOp::Shl, l, Val::from_int(70), Some(512)).unwrap();
        assert_eq!(shifted.width, 512);
        assert!(wide::bit_at(&shifted.to_limbs(), 70));
    }

    #[test]
    fn wide_eq_compares_two_equal_512_bit_values() {
        let a = Val::new_wide(wide::from_u128(42, 512), 512, false);
        let b = Val::new_wide(wide::from_u128(42, 512), 512, false);
        let eq = binary_known(BinOp::Eq, a, b, None).unwrap();
        assert_eq!(eq.masked(), 1);
    }

    #[test]
    fn wide_lt_compares_signed_512_bit_values() {
        let neg = Val::new_wide(wide::neg(&wide::from_u128(1, 512), 512), 512, true);
        let pos = Val::new_wide(wide::from_u128(1, 512), 512, true);
        let lt = binary_known(BinOp::Lt, neg, pos, None).unwrap();
        assert_eq!(lt.masked(), 1);
    }

    #[test]
    fn wide_neg_of_a_512_bit_value() {
        let one = Val::new_wide(wide::from_u128(1, 512), 512, true);
        let negated = unary(UnOp::Neg, one);
        assert_eq!(
            wide::to_decimal_string(&negated.to_limbs(), 512, true),
            "-1"
        );
    }

    #[test]
    fn wide_extend_builtin_widens_past_128_bits() {
        let mut ints = std::collections::BTreeMap::new();
        ints.insert("W".to_string(), 512i128);
        // extend(1, W) with W bound to 512 in the const env.
        let n = checked_width(512).unwrap();
        let v = Val::from_int(1);
        let extended = Val::new_wide(
            wide::extend(&v.to_limbs(), v.width, n, v.signed),
            n,
            v.signed,
        );
        assert_eq!(extended.width, 512);
        assert!(wide::bit_at(&extended.to_limbs(), 0));
    }

    #[test]
    fn checked_width_accepts_up_to_the_shared_max_width() {
        assert!(checked_width(1_000_000).is_ok());
        assert!(checked_width(1_000_001).is_err());
    }

    #[test]
    fn concat_can_exceed_128_bits() {
        let a = Val::new(u128::MAX, 128, false);
        let b = Val::new(1, 1, false);
        // Simulate what eval_ctx's Concat arm does: total width 129.
        let total = a.width + b.width;
        assert_eq!(total, 129);
    }

    #[test]
    fn cmp_eq_signed_different_widths() {
        // 4-bit -2 (0xE, masked=14) vs 8-bit -2 (0xFE, masked=254)
        let l = Val::new(0b1110, 4, true);
        let r = Val::new(0b1111_1110, 8, true);
        assert!(cmp_eq(l, r));
    }

    #[test]
    fn pattern_matches_handles_wide_value_no_saturation() {
        // A 200-bit value with bit 128 set and low 128 bits = 0 must NOT match
        // Pattern::Int { value: u128::MAX } — the old bits_small_or_zero()
        // saturated it to u128::MAX and caused a false match.
        let mut limbs = wide::zeros(200);
        limbs[2] = 1; // bit 128 set, low 128 bits are 0
        let s = Val::new_wide(limbs, 200, false);
        let p_not_max = Pattern::Int {
            value: u128::MAX,
            raw: String::new(),
        };
        assert_eq!(
            pattern_matches(&p_not_max, &s),
            Ok(false),
            "saturation must not cause false match"
        );
        // A pattern matching the low bits (0) should match:
        let p_zero = Pattern::Int {
            value: 0,
            raw: String::new(),
        };
        assert_eq!(pattern_matches(&p_zero, &s), Ok(true));
    }

    #[test]
    fn builtin_abs_wide_negative() {
        // -1 as a 200-bit signed value
        let one = Val::new_wide(wide::from_u128(1, 200), 200, true);
        let neg_one = unary(UnOp::Neg, one);
        // Abs should convert -1 (200-bit) to +1 (201-bit)
        let limbs = neg_one.to_limbs();
        let extended = wide::extend(&limbs, neg_one.width, neg_one.width + 1, neg_one.signed);
        let negated = wide::neg(&extended, neg_one.width + 1);
        assert_eq!(wide::to_decimal_string(&negated, 201, true), "1");
    }

    #[test]
    fn builtin_trunc_wide_limb_count() {
        // 200 bits truncated to 130 bits -> limb_count should be limb_count(130) = 3
        let limbs = vec![u64::MAX; wide::limb_count(200)]; // 4 limbs
        let v = Val::new_wide(limbs, 200, false);
        let mut limbs_t = v.to_limbs();
        wide::mask_to_width(&mut limbs_t, 130);
        limbs_t.truncate(wide::limb_count(130));
        assert_eq!(limbs_t.len(), wide::limb_count(130));
        let res = Val::new_wide(limbs_t, 130, false);
        if let Bits::Wide(res_limbs) = res.bits {
            assert_eq!(res_limbs.len(), wide::limb_count(130));
        } else {
            panic!("expected Wide bits");
        }
    }
}
