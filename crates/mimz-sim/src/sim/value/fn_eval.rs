use super::binary::{cmp_lt, extend_bits};
use super::remask_to_width;
use super::*;

use crate::sim::Diag;

/// Evaluate a user-defined function call.
///
/// Args are evaluated in the CALLER's env, then bound to params in a child
/// env ([`FnEnv`]). Locals are evaluated in order in the child env. Finally
/// the body expression is evaluated. Width parity with the Verilog emitter
/// (which declares `reg [W-1:0]` for each local) is achieved by masking each
/// local's bound value to its `inferred_width` when the checker has set it.
pub(super) fn eval_fn_call<R: Resolver>(
    r: &mut R,
    name: &ast::Ident,
    args: &[Expr],
) -> Result<Val, Box<Diag>> {
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
        Box::new(
            Diag::new(
                name.span,
                format!(
                    "function `{}` cannot be called in this evaluation context \
                     (function table unavailable)",
                    name.name
                ),
            )
            .with_code("S0223"),
        )
    })?;
    let f = funcs.get(&name.name).ok_or_else(|| {
        Box::new(
            Diag::new(name.span, format!("undefined function `{}`", name.name)).with_code("S0224"),
        )
    })?;
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
                let n = u32::try_from(const_eval(len, consts)?).map_err(|_| {
                    Box::new(
                        Diag::new(
                            len.span,
                            format!("array `{}` has an invalid length", param.name.name),
                        )
                        .with_code("S0225"),
                    )
                })?;
                let (w, s) = type_width(elem, consts, param.name.span)?;
                for i in 0..n {
                    // `ai` can run past `argv` when the call site's argument
                    // count doesn't match this fn's arity — the checker
                    // (E0413/E0803) rejects that before eval normally, but
                    // this evaluator is also exercised directly on unchecked
                    // ASTs (fuzzing), so an out-of-range `ai` must be a clean
                    // `Err`, not an out-of-bounds panic.
                    let val = argv.get(ai).cloned().ok_or_else(|| {
                        Box::new(
                            Diag::new(
                                name.span,
                                format!(
                                    "function `{}` called with too few arguments (missing \
                                     element for array parameter `{}`)",
                                    name.name, param.name.name
                                ),
                            )
                            .with_code("S0226"),
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
                let (w, s) = type_width(other, consts, param.name.span)?;
                let val = argv.get(ai).cloned().ok_or_else(|| {
                    Box::new(
                        Diag::new(
                            name.span,
                            format!(
                                "function `{}` called with too few arguments (missing value \
                                 for parameter `{}`)",
                                name.name, param.name.name
                            ),
                        )
                        .with_code("S0226"),
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
fn eval_fn_stmts(env: &mut FnEnv, stmts: &[FnStmt]) -> Result<FnFlow, Box<Diag>> {
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
                var,
                lo,
                hi,
                body,
                span,
            } => {
                let lo_v = eval(env, lo)?.bits_small_or_zero() as i128;
                let hi_v = eval(env, hi)?.bits_small_or_zero() as i128;
                let count = (hi_v - lo_v).max(0);
                if count > REPEAT_BUDGET {
                    return Err(Box::new(
                        Diag::new(
                            *span,
                            format!(
                                "`loop` would unroll {count} times, over the limit of {REPEAT_BUDGET}"
                            ),
                        )
                        .with_code("S0227"),
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
        // `Resolver::signal` itself stays `String` (trait signature is out of
        // scope per the design's "leave the boundary alone" decision) — the
        // caller (`eval_ctx`'s `Ident`/`Index` arms) wraps this with the
        // enclosing `Expr`'s span into a real `Diag` (S0201).
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

pub(super) fn call<R: Resolver>(r: &mut R, func: Builtin, args: &[Expr]) -> Result<Val, Box<Diag>> {
    match func {
        Builtin::Extend => {
            let n = checked_width(const_eval(&args[1], r.ints())?, args[1].span)?;
            // `n` is `extend`'s own target width — feed it in as context so a
            // shift inside the argument (`extend(din << 2, 8)`) sees its real
            // consuming width, matching what the emitter's own no-op-extend
            // optimization relies on Verilog to compute (BUG-11).
            let v = eval_ctx(r, &args[0], Some(n))?;
            if n < v.width {
                return Err(Box::new(
                    Diag::new(
                        args[0].span,
                        format!(
                            "extend to {n} bits is narrower than the {}-bit value — use trunc",
                            v.width
                        ),
                    )
                    .with_code("S0228"),
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
            let n = checked_width(const_eval(&args[1], r.ints())?, args[1].span)?;
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
        Builtin::Clog2 => Err(Box::new(
            Diag::new(
                args.first().map(|a| a.span).unwrap_or_default(),
                "clog2 is compile-time only",
            )
            .with_code("S0229"),
        )),
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
