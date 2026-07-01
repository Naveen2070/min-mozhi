//! Shared value model + expression evaluator for the simulator.
//!
//! A [`Val`] is a 2-state bit-vector (≤128 bits) carrying a width and a signed
//! flag, honoring the spec's width semantics (lossless `+ - *` grow, the
//! `+% -% *%` family wraps, slices/concat/`extend`/`trunc` resize). [`eval`]
//! interprets an [`Expr`] against a [`Resolver`] — both the combinational
//! evaluator ([`super::comb`]) and the event-driven kernel ([`super::kernel`])
//! implement `Resolver`, so the expression semantics live in exactly one place.

use std::collections::{BTreeMap, HashMap};

use crate::ast::{self, BinOp, Builtin, Expr, ExprKind, FuncDecl, Pattern, Type, UnOp};

/// Low-`w`-bits mask (`w >= 128` ⇒ all ones).
pub(super) fn mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

/// A bit-vector value: the low `width` bits of `bits` are meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Val {
    pub(super) bits: u128,
    pub(super) width: u32,
    pub(super) signed: bool,
}

impl Val {
    pub(super) fn new(bits: u128, width: u32, signed: bool) -> Val {
        Val {
            bits: bits & mask(width),
            width: width.max(1),
            signed,
        }
    }
    /// A compile-time integer used as a value: minimal width that holds it.
    pub(super) fn from_int(v: i128) -> Val {
        if v >= 0 {
            let w = (128 - (v as u128).leading_zeros()).max(1);
            Val::new(v as u128, w, false)
        } else {
            // Two's complement in just enough bits.
            let w = (129 - v.leading_ones()).max(1);
            Val::new(v as u128, w, true)
        }
    }
    /// Sign-aware value, sign-extended to i128 for signed comparisons.
    pub(super) fn as_i128(&self) -> i128 {
        let m = mask(self.width);
        let b = self.bits & m;
        if self.signed && self.width >= 1 && (b >> (self.width - 1)) & 1 == 1 {
            (b | !m) as i128
        } else {
            b as i128
        }
    }
    /// The meaningful bits (masked to `width`) — what a consumer stores/prints.
    pub(super) fn masked(&self) -> u128 {
        self.bits & mask(self.width)
    }
}

/// Resolves names while an expression is evaluated: a signal/reg/wire to its
/// current value, plus the compile-time integer environment for index and
/// slice bounds. The two evaluators differ only in `signal`.
pub(super) trait Resolver {
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
}

/// Evaluate `e` against `r`. The single source of Min-Mozhi's expression
/// semantics for both the combinational evaluator and the kernel.
pub(super) fn eval<R: Resolver>(r: &mut R, e: &Expr) -> Result<Val, String> {
    match &e.kind {
        ExprKind::Int { value, .. } => Ok(Val::from_int(*value as i128)),
        ExprKind::Bool(b) => Ok(Val::new(*b as u128, 1, false)),
        ExprKind::Ident(n) => r.signal(n),
        ExprKind::Unary { op, expr } => Ok(unary(*op, eval(r, expr)?)),
        ExprKind::Binary { op, lhs, rhs } => {
            let l = eval(r, lhs)?;
            let rr = eval(r, rhs)?;
            binary(*op, l, rr)
        }
        ExprKind::IfExpr { cond, then, els } => {
            if eval(r, cond)?.bits & 1 == 1 {
                eval(r, then)
            } else {
                eval(r, els)
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            let s = eval(r, scrutinee)?;
            for arm in arms {
                for p in &arm.patterns {
                    if pattern_matches(p, &s)? {
                        return eval(r, &arm.value);
                    }
                }
            }
            Err("no `match` arm matched the value (enum patterns are not evaluated yet)".into())
        }
        ExprKind::Concat(parts) => {
            let vals: Vec<Val> = parts.iter().map(|p| eval(r, p)).collect::<Result<_, _>>()?;
            // Sum in u64 so many parts cannot wrap a u32 below the guard.
            let total64: u64 = vals.iter().map(|v| v.width as u64).sum();
            if total64 > 128 {
                return Err("concatenation exceeds 128 bits (evaluator limit)".into());
            }
            let total = total64 as u32;
            let mut bits = 0u128;
            let mut shift = total;
            for v in &vals {
                shift -= v.width;
                bits |= (v.bits & mask(v.width)) << shift;
            }
            Ok(Val::new(bits, total, false))
        }
        ExprKind::Replicate { count, parts } => {
            let n = const_eval(count, r.ints())?;
            if n < 1 {
                return Err("replication count must be at least 1".into());
            }
            let vals: Vec<Val> = parts.iter().map(|p| eval(r, p)).collect::<Result<_, _>>()?;
            // Inner group width, then the replicated total — both in u64 so the
            // product cannot wrap a u32 below the 128-bit guard.
            let inner64: u64 = vals.iter().map(|v| v.width as u64).sum();
            let total64 = inner64
                .checked_mul(n as u64)
                .filter(|t| *t <= 128)
                .ok_or("replication exceeds 128 bits (evaluator limit)")?;
            let inner = inner64 as u32;
            // Assemble the inner group once (widest part first), then repeat it.
            let mut chunk = 0u128;
            let mut shift = inner;
            for v in &vals {
                shift -= v.width;
                chunk |= (v.bits & mask(v.width)) << shift;
            }
            let mut bits = 0u128;
            for _ in 0..n {
                bits = (bits << inner) | chunk;
            }
            Ok(Val::new(bits, total64 as u32, false))
        }
        ExprKind::Index { base, index } => {
            // A memory read `m[addr]` resolves the address at RUNTIME and
            // returns the whole element; a bit-vector `s[i]` selects one bit
            // at a compile-time index.
            if let ExprKind::Ident(name) = &base.kind {
                if r.is_mem(name) {
                    let addr = eval(r, index)?;
                    return r.mem_read(name, addr.bits);
                }
            }
            let b = eval(r, base)?;
            let i = checked_index(const_eval(index, r.ints())?, b.width, "bit index")?;
            Ok(Val::new((b.bits >> i) & 1, 1, false))
        }
        ExprKind::Slice { base, hi, lo } => {
            let b = eval(r, base)?;
            let hi = checked_index(const_eval(hi, r.ints())?, b.width, "slice high bound")?;
            let lo = checked_index(const_eval(lo, r.ints())?, b.width, "slice low bound")?;
            if hi < lo {
                return Err("slice bounds reversed (write `[hi:lo]`, msb first)".into());
            }
            let w = hi - lo + 1;
            Ok(Val::new((b.bits >> lo) & mask(w), w, b.signed))
        }
        ExprKind::Field { .. } => {
            Err("enum-variant / instance-port access is not supported by the evaluator yet".into())
        }
        ExprKind::Call { func, args } => call(r, *func, args),
        ExprKind::FnCall { name, args } => eval_fn_call(r, name, args),
        ExprKind::BundleLit(_) => {
            Err("BundleLit reached value evaluator — should be pre-expanded by elaborate".into())
        }
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
    // Evaluate each argument in the CALLER's environment.
    let argv: Vec<Val> = args.iter().map(|a| eval(r, a)).collect::<Result<_, _>>()?;
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
    // Bind each param to its arg value, masked to the declared param type.
    let mut locals: BTreeMap<String, Val> = BTreeMap::new();
    for (param, val) in f.params.iter().zip(argv.iter()) {
        let (w, s) = type_width(&param.ty, consts)?;
        locals.insert(param.name.name.clone(), Val::new(val.bits, w, s));
    }
    let mut child = FnEnv {
        locals,
        consts,
        funcs,
    };
    // Evaluate each local `let` in order, binding its name for subsequent exprs.
    // Width parity: mask to inferred_width when the checker has set it, matching
    // the Verilog emitter's `reg [W-1:0]` declaration for each local.
    for local in &f.locals {
        let v = eval(&mut child, &local.value)?;
        let v = match local.inferred_width.get() {
            Some(w) => Val::new(v.bits, w, v.signed),
            None => v, // checker not run (e.g. bare sim test); trust the Val width
        };
        child.locals.insert(local.name.name.clone(), v);
    }
    eval(&mut child, &f.body)
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
}

impl Resolver for FnEnv<'_> {
    fn signal(&mut self, name: &str) -> Result<Val, String> {
        if let Some(v) = self.locals.get(name) {
            return Ok(*v);
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
}

fn call<R: Resolver>(r: &mut R, func: Builtin, args: &[Expr]) -> Result<Val, String> {
    match func {
        Builtin::Extend => {
            let v = eval(r, &args[0])?;
            let n = checked_width(const_eval(&args[1], r.ints())?)?;
            if n < v.width {
                return Err(format!(
                    "extend to {n} bits is narrower than the {}-bit value — use trunc",
                    v.width
                ));
            }
            let bits = if v.signed && v.width >= 1 && (v.bits >> (v.width - 1)) & 1 == 1 {
                v.bits | (mask(n) & !mask(v.width)) // sign-extend
            } else {
                v.bits & mask(v.width)
            };
            Ok(Val::new(bits, n, v.signed))
        }
        Builtin::Trunc => {
            let v = eval(r, &args[0])?;
            let n = checked_width(const_eval(&args[1], r.ints())?)?;
            Ok(Val::new(v.bits & mask(n), n, v.signed))
        }
        Builtin::SignedCast => {
            let v = eval(r, &args[0])?;
            Ok(Val::new(v.bits, v.width, true))
        }
        Builtin::UnsignedCast => {
            let v = eval(r, &args[0])?;
            Ok(Val::new(v.bits, v.width, false))
        }
        Builtin::Min => {
            let a = eval(r, &args[0])?;
            let b = eval(r, &args[1])?;
            Ok(if cmp_lt(a, b) { a } else { b })
        }
        Builtin::Max => {
            let a = eval(r, &args[0])?;
            let b = eval(r, &args[1])?;
            Ok(if cmp_lt(a, b) { b } else { a })
        }
        Builtin::Abs => {
            let v = eval(r, &args[0])?;
            // signed magnitude into width+1 (room for abs(MIN))
            let m = v.as_i128().unsigned_abs();
            Ok(Val::new(m & mask(v.width + 1), v.width + 1, true))
        }
        Builtin::Nand => {
            let v = eval(r, &args[0])?;
            let mk = mask(v.width);
            Ok(Val::new(((v.bits & mk) != mk) as u128, 1, false))
        }
        Builtin::Nor => {
            let v = eval(r, &args[0])?;
            Ok(Val::new(((v.bits & mask(v.width)) == 0) as u128, 1, false))
        }
        Builtin::Xnor => {
            let v = eval(r, &args[0])?;
            Ok(Val::new(
                (((v.bits & mask(v.width)).count_ones() & 1) == 0) as u128,
                1,
                false,
            ))
        }
        // `clog2` is compile-time only — the checker rejects it as a runtime
        // value (E0407) and folds it in widths, so a checked program never lands
        // here.
        Builtin::Clog2 => Err("clog2 is compile-time only".into()),
    }
}

fn unary(op: UnOp, v: Val) -> Val {
    match op {
        UnOp::Neg => {
            let bits = v.as_i128().wrapping_neg() as u128;
            Val::new(bits, v.width, true)
        }
        UnOp::BitNot => Val::new(!v.bits, v.width, v.signed),
        UnOp::LogicNot => Val::new((!(v.bits & 1)) & 1, 1, false),
        UnOp::RedAnd => Val::new(
            ((v.bits & mask(v.width)) == mask(v.width)) as u128,
            1,
            false,
        ),
        UnOp::RedOr => Val::new(((v.bits & mask(v.width)) != 0) as u128, 1, false),
        UnOp::RedXor => Val::new(
            ((v.bits & mask(v.width)).count_ones() & 1) as u128,
            1,
            false,
        ),
    }
}

fn binary(op: BinOp, l: Val, r: Val) -> Result<Val, String> {
    let wmax = l.width.max(r.width);
    let signed = l.signed || r.signed;
    let v = match op {
        // Lossless growth (spec/02 section 3). Operate on the SIGN-EXTENDED
        // values (`as_i128`) so a negative signed operand is widened correctly
        // before the result grows — matching Verilog's signed arithmetic. For
        // unsigned operands `as_i128` is the plain magnitude, so this is
        // identical to a raw-bit add/mul. (The wrapping family below keeps the
        // operand width, where the raw-bit op is already correct mod 2^width.)
        BinOp::Add => Val::new(
            l.as_i128().wrapping_add(r.as_i128()) as u128,
            wmax + 1,
            signed,
        ),
        BinOp::Sub => Val::new(
            l.as_i128().wrapping_sub(r.as_i128()) as u128,
            wmax + 1,
            true,
        ),
        BinOp::Mul => Val::new(
            l.as_i128().wrapping_mul(r.as_i128()) as u128,
            l.width + r.width,
            signed,
        ),
        // Wrapping family: keep operand width.
        BinOp::AddWrap => Val::new(l.bits.wrapping_add(r.bits), wmax, signed),
        BinOp::SubWrap => Val::new(l.bits.wrapping_sub(r.bits), wmax, signed),
        BinOp::MulWrap => Val::new(l.bits.wrapping_mul(r.bits), wmax, signed),
        BinOp::BitAnd => Val::new(l.bits & r.bits, wmax, signed),
        BinOp::BitOr => Val::new(l.bits | r.bits, wmax, signed),
        BinOp::BitXor => Val::new(l.bits ^ r.bits, wmax, signed),
        BinOp::Shl => {
            let shift = r.bits;
            let bits = if shift >= 128 {
                0
            } else {
                l.bits.checked_shl(shift as u32).unwrap_or(0)
            };
            let w = if shift >= 128 {
                128
            } else {
                (l.width + shift as u32).min(128)
            };
            Val::new(bits, w, l.signed)
        }
        BinOp::Shr => Val::new(
            if r.bits >= 128 {
                0
            } else {
                l.bits >> (r.bits as u32)
            },
            l.width,
            l.signed,
        ),
        BinOp::Eq => Val::new(
            ((l.bits & mask(wmax)) == (r.bits & mask(wmax))) as u128,
            1,
            false,
        ),
        BinOp::Ne => Val::new(
            ((l.bits & mask(wmax)) != (r.bits & mask(wmax))) as u128,
            1,
            false,
        ),
        BinOp::Lt => Val::new(cmp_lt(l, r) as u128, 1, false),
        BinOp::Le => Val::new((cmp_lt(l, r) || cmp_eq(l, r)) as u128, 1, false),
        BinOp::Gt => Val::new((!cmp_lt(l, r) && !cmp_eq(l, r)) as u128, 1, false),
        BinOp::Ge => Val::new((!cmp_lt(l, r)) as u128, 1, false),
        BinOp::LogicAnd => Val::new((l.bits & 1) & (r.bits & 1), 1, false),
        BinOp::LogicOr => Val::new((l.bits & 1) | (r.bits & 1), 1, false),
    };
    Ok(v)
}

fn cmp_lt(l: Val, r: Val) -> bool {
    if l.signed || r.signed {
        l.as_i128() < r.as_i128()
    } else {
        l.bits < r.bits
    }
}
fn cmp_eq(l: Val, r: Val) -> bool {
    if l.signed || r.signed {
        l.as_i128() == r.as_i128()
    } else {
        l.bits == r.bits
    }
}

pub(super) fn pattern_matches(p: &Pattern, s: &Val) -> Result<bool, String> {
    match p {
        Pattern::Wildcard => Ok(true),
        Pattern::Int { value, .. } => Ok((s.bits & mask(s.width)) == (*value & mask(s.width))),
        Pattern::IntMask { value, mask: m, .. } => Ok((s.bits & *m) == (*value & *m)),
        Pattern::Bool(b) => Ok((s.bits & 1) == (*b as u128)),
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
            n.name
        )),
        Type::Bundle { .. } => {
            Err("Type::Bundle reached type_width — should be pre-flattened by elaborate".into())
        }
    }
}

pub(super) fn checked_width(n: i128) -> Result<u32, String> {
    if n < 1 {
        Err(format!("width must be at least 1, got {n}"))
    } else if n > 128 {
        Err(format!("width {n} exceeds the simulator's 128-bit limit"))
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
    let env: crate::checker::consteval::Env = ints.iter().map(|(k, v)| (k.clone(), *v)).collect();
    crate::checker::consteval::eval(e, &env).map_err(|d| d.msg)
}

/// A bit index or slice bound must be a non-negative integer inside the value's
/// width. Rejects negative / out-of-range positions instead of truncating via
/// `as u32` or a later oversized shift (`>> n`, `n >= 128`, which panics).
fn checked_index(n: i128, width: u32, what: &str) -> Result<u32, String> {
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
    fn shl_does_not_truncate_to_left_operand_width() {
        // BUG-6: 1 << 2 was returning 0 because `1` has width 1, and the shift result
        // was truncated to width 1 (1 << 2 = 4; 4 & 1 = 0).
        // It should instead retain a width of at least width + shift.
        let l = Val::from_int(1); // width 1
        let r = Val::from_int(2);
        let res = binary(BinOp::Shl, l, r).unwrap();
        assert_eq!(res.masked(), 4);
        assert_eq!(res.width, 3); // 1 + 2

        let l2 = Val::from_int(8); // width 4 (1000)
        let r2 = Val::from_int(1);
        let res2 = binary(BinOp::Shl, l2, r2).unwrap();
        assert_eq!(res2.masked(), 16);
        assert_eq!(res2.width, 5); // 4 + 1
    }
}
