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

/// Low-`w`-bits mask (`w >= 128` ⇒ all ones).
pub(super) fn mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

/// A bit-vector value: the low `width` bits of `bits` are meaningful. `pub`
/// (re-exported at `mimz_sim::sim::Val`) since `EmulationHost::on_change`/
/// `on_tick` hand this to the shell crate's peripheral implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Val {
    /// The value's bit pattern; only the low `width` bits are meaningful.
    /// MEANINGLESS when `unknown` is `true` — the bit pattern is NOT
    /// guaranteed to be any particular value (e.g. 0) in that case; callers
    /// must check `unknown` before trusting this field regardless.
    pub bits: u128,
    /// Bit width, `1..=128`.
    pub width: u32,
    /// Whether `bits` is interpreted as two's-complement `signed`.
    pub signed: bool,
    /// Coarse whole-value taint: `true` means this value is entirely
    /// unconstrained (an `extern module` instance output in `warn` sim
    /// mode — see `docs/superpowers/specs/2026-07-15-verilog-ffi-design.local.md`).
    /// NOT a per-bit X mask — the whole value is tainted or it isn't.
    /// Every operator propagates this: any operand tainted -> result
    /// tainted (see `unary`/`binary` and `eval`'s `Concat`/`Replicate`/
    /// `Index`/`Slice` arms).
    pub unknown: bool,
}

impl Val {
    /// Builds a `Val`, masking `bits` to `width` (`width` floors at 1 — no
    /// zero-width signal exists in Min-Mozhi).
    pub fn new(bits: u128, width: u32, signed: bool) -> Val {
        Val {
            bits: bits & mask(width),
            width: width.max(1),
            signed,
            unknown: false,
        }
    }
    /// An unconstrained value of the given width/signedness — see the
    /// `unknown` field's doc comment. `bits` is `0` but MUST NOT be relied
    /// upon by any caller; only `unknown` carries meaning here.
    pub fn unknown(width: u32, signed: bool) -> Val {
        Val {
            bits: 0,
            width: width.max(1),
            signed,
            unknown: true,
        }
    }
    /// A compile-time integer used as a value: minimal width that holds it.
    pub fn from_int(v: i128) -> Val {
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
    pub fn as_i128(&self) -> i128 {
        let m = mask(self.width);
        let b = self.bits & m;
        if self.signed && self.width >= 1 && (b >> (self.width - 1)) & 1 == 1 {
            (b | !m) as i128
        } else {
            b as i128
        }
    }
    /// The meaningful bits (masked to `width`) — what a consumer stores/prints.
    pub fn masked(&self) -> u128 {
        self.bits & mask(self.width)
    }
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
            if eval(r, cond)?.bits & 1 == 1 {
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
            if total64 > 128 {
                return Err("concatenation exceeds 128 bits (evaluator limit)".into());
            }
            let total = total64 as u32;
            if vals.iter().any(|v| v.unknown) {
                return Ok(Val::unknown(total, false));
            }
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
            if vals.iter().any(|v| v.unknown) {
                return Ok(Val::unknown(total64 as u32, false));
            }
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
                    let i = (eval(r, index)?.bits as usize).min(last);
                    return Ok(elems[i]);
                }
                if r.is_mem(name) {
                    let addr = eval(r, index)?;
                    return r.mem_read(name, addr.bits);
                }
            }
            let b = eval(r, base)?;
            if b.unknown {
                return Ok(Val::unknown(1, false));
            }
            let i = checked_index(const_eval(index, r.ints())?, b.width, "bit index")?;
            Ok(Val::new((b.bits >> i) & 1, 1, false))
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
            Ok(Val::new((b.bits >> lo) & mask(k.width), k.width, false))
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
                    let val = *argv.get(ai).ok_or_else(|| {
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
                let val = *argv.get(ai).ok_or_else(|| {
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
                            Some(w) => Val::new(v.bits, w, v.signed),
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
                    Some(w) => Val::new(v.bits, w, v.signed),
                    None => v, // checker not run (e.g. bare sim test); trust the Val width
                };
                env.locals.insert(local.name.name.clone(), v);
            }
            FnStmt::If { cond, then, els } => {
                let c = eval(env, cond)?;
                let branch = if c.bits != 0 {
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
                let lo_v = eval(env, lo)?.bits as i128;
                let hi_v = eval(env, hi)?.bits as i128;
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
    if width > v.width && v.signed && (v.bits >> (v.width - 1)) & 1 == 1 {
        v.bits | (mask(width) & !mask(v.width))
    } else {
        v.bits & mask(v.width)
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
            Ok(Val::new(extend_bits(v, n), n, v.signed))
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
    let mut r = unary_known(op, v);
    if v.unknown {
        r.unknown = true;
    }
    r
}

fn unary_known(op: UnOp, v: Val) -> Val {
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
    let wmax = l.width.max(r.width);
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
            Val::new(
                l.as_i128().wrapping_add(r.as_i128()) as u128,
                k.width,
                k.signed,
            )
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
            Val::new(
                l.as_i128().wrapping_sub(r.as_i128()) as u128,
                k.width,
                k.signed,
            )
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
            Val::new(
                l.as_i128().wrapping_mul(r.as_i128()) as u128,
                k.width,
                k.signed,
            )
        }
        // Wrapping family: keep operand width. A bare integer literal's `Val`
        // keeps its own minimal natural width (never pre-widened to match the
        // other operand, unlike the checker's compile-time-only "adapting"
        // fiction for `CtInt` — see `matched_ty`), so both operands must be
        // widened to `wmax` here before `matched_result` can find their
        // `Kind`s equal. The `.unwrap_or` reproduces the original
        // `l.signed || r.signed` bookkeeping for the one case
        // `matched_result` can still reject after widening (mismatched
        // signedness) — real fallback code, not a placeholder.
        BinOp::AddWrap => {
            let wmax = l.width.max(r.width);
            let lw = extend_bits(l, wmax);
            let rw = extend_bits(r, wmax);
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
            Val::new(lw.wrapping_add(rw), k.width, k.signed)
        }
        BinOp::SubWrap => {
            let wmax = l.width.max(r.width);
            let lw = extend_bits(l, wmax);
            let rw = extend_bits(r, wmax);
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
            Val::new(lw.wrapping_sub(rw), k.width, k.signed)
        }
        BinOp::MulWrap => {
            let wmax = l.width.max(r.width);
            let lw = extend_bits(l, wmax);
            let rw = extend_bits(r, wmax);
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
            Val::new(lw.wrapping_mul(rw), k.width, k.signed)
        }
        BinOp::BitAnd => {
            let wmax = l.width.max(r.width);
            let lw = extend_bits(l, wmax);
            let rw = extend_bits(r, wmax);
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
            Val::new(lw & rw, k.width, k.signed)
        }
        BinOp::BitOr => {
            let wmax = l.width.max(r.width);
            let lw = extend_bits(l, wmax);
            let rw = extend_bits(r, wmax);
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
            Val::new(lw | rw, k.width, k.signed)
        }
        BinOp::BitXor => {
            let wmax = l.width.max(r.width);
            let lw = extend_bits(l, wmax);
            let rw = extend_bits(r, wmax);
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
            Val::new(lw ^ rw, k.width, k.signed)
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
            let widened = extend_bits(l, ctx_w);
            let shift = r.bits;
            let bits = if shift >= 128 {
                0
            } else {
                widened.checked_shl(shift as u32).unwrap_or(0)
            };
            Val::new(bits, ctx_w, base.signed)
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
            let widened = extend_bits(l, ctx_w);
            let bits = if r.bits >= 128 {
                0
            } else {
                widened >> (r.bits as u32)
            };
            Val::new(bits, ctx_w, base.signed)
        }
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
        assert_eq!(result.bits, 0b1010 & 1);
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
        let inputs: BTreeMap<String, u128> = [
            ("a".to_string(), 1u128),
            ("b".to_string(), 2u128),
            ("idx".to_string(), 0u128),
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
        Val::new(out.value, out.width, out.signed).as_i128()
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
        assert_eq!(
            Val::new(out.value, out.width, out.signed).as_i128(),
            -128,
            "got raw bits {:#x} at width {}",
            out.value,
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
        let inputs: BTreeMap<String, u128> = [("x".to_string(), 1u128)].into_iter().collect();
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
        assert!(!binary_ctx(BinOp::Add, a, b, None).unwrap().unknown);
        assert!(!unary(UnOp::BitNot, a).unknown);
    }
}
