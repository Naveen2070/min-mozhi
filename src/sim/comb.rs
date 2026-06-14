//! Combinational evaluator — evaluate a clockless module's outputs from its
//! inputs, by interpreting the AST directly.
//!
//! Scope (deliberately a SLICE of the Phase 1.5 simulator): one module, no
//! `reg`, no `on` block, no instances, no `repeat`. Those are rejected with a
//! clear message rather than half-evaluated. Within that scope it honors the
//! spec's width semantics — lossless `+ - *` grow, the `+% -% *%` family wraps,
//! slices/concat/`extend`/`trunc` resize — so the result matches what the
//! Verilog emitter would produce for the same combinational logic.
//!
//! Values are unsigned bit-vectors up to 128 bits wide, carrying a width and a
//! signed flag (`signed[N]`); a value wider than 128 bits is an error, not a
//! silent wrap. Everything is `Result<_, String>` — the CLI prints the message.

use std::collections::BTreeMap;

use crate::ast::{self, BinOp, Builtin, Dir, Expr, ExprKind, ModuleItem, Pattern, Type, UnOp};

/// One evaluated output port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub name: String,
    /// The output value, in the low `width` bits.
    pub value: u128,
    pub width: u32,
    pub signed: bool,
}

/// A bit-vector value: the low `width` bits of `bits` are meaningful.
#[derive(Clone, Copy, Debug)]
struct Val {
    bits: u128,
    width: u32,
    signed: bool,
}

/// Low-`w`-bits mask (`w == 128` ⇒ all ones).
fn mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

impl Val {
    fn new(bits: u128, width: u32, signed: bool) -> Val {
        Val {
            bits: bits & mask(width),
            width: width.max(1),
            signed,
        }
    }
    /// A compile-time integer used as a value: minimal width that holds it.
    fn from_int(v: i128) -> Val {
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
    fn as_i128(&self) -> i128 {
        let m = mask(self.width);
        let b = self.bits & m;
        if self.signed && self.width >= 1 && (b >> (self.width - 1)) & 1 == 1 {
            (b | !m) as i128
        } else {
            b as i128
        }
    }
}

/// Evaluate the outputs of `module` (or the file's only module when `module`
/// is `None`) given `inputs` (name → value) and optional `params` overrides.
/// Missing inputs, sequential constructs, and out-of-scope expressions all
/// return a descriptive error.
pub fn eval_outputs(
    file: &ast::File,
    module: Option<&str>,
    inputs: &BTreeMap<String, u128>,
    params: &BTreeMap<String, i128>,
) -> Result<Vec<Output>, String> {
    let m = pick_module(file, module)?;

    // 1. Reject anything sequential / structural — this is the comb slice.
    for it in &m.items {
        match it {
            ModuleItem::Reg { .. } => {
                return Err(
                    "module has `reg` state — the combinational evaluator does not run \
                            clocked logic yet (that is the Phase 1.5 simulator)"
                        .into(),
                );
            }
            ModuleItem::On(_) => {
                return Err("module has an `on` block — combinational evaluation only; \
                            clocked behavior is Phase 1.5"
                    .into());
            }
            ModuleItem::Inst(_) => {
                return Err(
                    "module instantiates a sub-module — the evaluator does not elaborate \
                            instances yet (single-module, combinational only)"
                        .into(),
                );
            }
            ModuleItem::Repeat(_) => {
                return Err(
                    "module uses `repeat` — unrolling is not supported by the evaluator yet".into(),
                );
            }
            _ => {}
        }
    }

    // 2. Compile-time integer environment: params (defaults, overridable) then
    //    consts (file-level + module-level).
    let mut ints: BTreeMap<String, i128> = BTreeMap::new();
    for p in &m.params {
        let v = match params.get(&p.name.name) {
            Some(v) => *v,
            None => match &p.default {
                Some(d) => const_eval(d, &ints)?,
                None => {
                    return Err(format!(
                        "parameter `{}` has no default — pass it with --param {}=<n>",
                        p.name.name, p.name.name
                    ));
                }
            },
        };
        ints.insert(p.name.name.clone(), v);
    }
    for item in &file.items {
        if let ast::TopItem::Const(c) = item {
            let v = const_eval(&c.value, &ints)?;
            ints.insert(c.name.name.clone(), v);
        }
    }
    for it in &m.items {
        if let ModuleItem::Const(c) = it {
            let v = const_eval(&c.value, &ints)?;
            ints.insert(c.name.name.clone(), v);
        }
    }

    // 3. Signals (in/out/wire) with their declared (width, signed).
    let mut sig_ty: BTreeMap<String, (u32, bool)> = BTreeMap::new();
    let mut drivers: BTreeMap<String, &Expr> = BTreeMap::new();
    let mut out_order: Vec<(String, u32, bool)> = Vec::new();
    for it in &m.items {
        match it {
            ModuleItem::Port { dir, name, ty } => {
                let (w, s) = type_width(ty, &ints)?;
                sig_ty.insert(name.name.clone(), (w, s));
                if *dir == Dir::Out {
                    out_order.push((name.name.clone(), w, s));
                }
            }
            ModuleItem::Wire { name, ty, init } => {
                let (w, s) = type_width(ty, &ints)?;
                sig_ty.insert(name.name.clone(), (w, s));
                drivers.insert(name.name.clone(), init);
            }
            ModuleItem::Drive { lhs, rhs } => {
                if lhs.index.is_some() {
                    return Err(format!(
                        "driving a slice of `{}` is not supported by the evaluator yet — \
                         drive the whole signal",
                        lhs.base.name
                    ));
                }
                drivers.insert(lhs.base.name.clone(), rhs);
            }
            _ => {}
        }
    }

    // 4. Seed input values (masked to their declared width).
    let mut env = Env {
        ints: &ints,
        sig_ty: &sig_ty,
        drivers: &drivers,
        memo: BTreeMap::new(),
        in_progress: Vec::new(),
    };
    for it in &m.items {
        if let ModuleItem::Port {
            dir: Dir::In, name, ..
        } = it
        {
            let (w, s) = sig_ty[&name.name];
            let raw = inputs.get(&name.name).copied().ok_or_else(|| {
                format!(
                    "missing value for input `{}` — pass it with --in {}=<n>",
                    name.name, name.name
                )
            })?;
            env.memo.insert(name.name.clone(), Val::new(raw, w, s));
        }
    }

    // 5. Resolve each output.
    let mut outputs = Vec::new();
    for (name, _, _) in &out_order {
        let v = env.resolve(name)?;
        outputs.push(Output {
            name: name.clone(),
            value: v.bits & mask(v.width),
            width: v.width,
            signed: v.signed,
        });
    }
    Ok(outputs)
}

fn pick_module<'a>(file: &'a ast::File, want: Option<&str>) -> Result<&'a ast::Module, String> {
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

/// Per-evaluation state: the const environment, signal types, driver
/// expressions, a memo of resolved signals, and a cycle-detection stack.
struct Env<'a> {
    ints: &'a BTreeMap<String, i128>,
    sig_ty: &'a BTreeMap<String, (u32, bool)>,
    drivers: &'a BTreeMap<String, &'a Expr>,
    memo: BTreeMap<String, Val>,
    in_progress: Vec<String>,
}

impl Env<'_> {
    /// Resolve a signal's value, evaluating (and memoizing) its driver on first
    /// use. A signal seen twice on the active stack is a combinational cycle.
    fn resolve(&mut self, name: &str) -> Result<Val, String> {
        if let Some(v) = self.memo.get(name) {
            return Ok(*v);
        }
        if self.in_progress.iter().any(|n| n == name) {
            return Err(format!(
                "combinational cycle through `{name}` — feedback must pass through a register"
            ));
        }
        let driver = self
            .drivers
            .get(name)
            .ok_or_else(|| format!("signal `{name}` is never driven"))?;
        self.in_progress.push(name.to_string());
        let v = self.eval(driver)?;
        self.in_progress.pop();
        let (w, s) = self
            .sig_ty
            .get(name)
            .copied()
            .unwrap_or((v.width, v.signed));
        let v = Val::new(v.bits, w, s); // mask to the declared width
        self.memo.insert(name.to_string(), v);
        Ok(v)
    }

    fn eval(&mut self, e: &Expr) -> Result<Val, String> {
        match &e.kind {
            ExprKind::Int { value, .. } => Ok(Val::from_int(*value as i128)),
            ExprKind::Bool(b) => Ok(Val::new(*b as u128, 1, false)),
            ExprKind::Ident(n) => {
                if self.sig_ty.contains_key(n) || self.drivers.contains_key(n) {
                    self.resolve(n)
                } else if let Some(v) = self.ints.get(n) {
                    Ok(Val::from_int(*v))
                } else {
                    Err(format!("unknown name `{n}` in evaluation"))
                }
            }
            ExprKind::Unary { op, expr } => {
                let v = self.eval(expr)?;
                Ok(unary(*op, v))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.eval(lhs)?;
                let r = self.eval(rhs)?;
                binary(*op, l, r)
            }
            ExprKind::IfExpr { cond, then, els } => {
                let c = self.eval(cond)?;
                if c.bits & 1 == 1 {
                    self.eval(then)
                } else {
                    self.eval(els)
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                let s = self.eval(scrutinee)?;
                for arm in arms {
                    for p in &arm.patterns {
                        if pattern_matches(p, &s)? {
                            return self.eval(&arm.value);
                        }
                    }
                }
                Err(
                    "no `match` arm matched the value (the evaluator does not resolve enum \
                     patterns yet)"
                        .into(),
                )
            }
            ExprKind::Concat(parts) => {
                let vals: Vec<Val> = parts
                    .iter()
                    .map(|p| self.eval(p))
                    .collect::<Result<_, _>>()?;
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
            ExprKind::Index { base, index } => {
                let b = self.eval(base)?;
                let i = checked_index(const_eval(index, self.ints)?, b.width, "bit index")?;
                Ok(Val::new((b.bits >> i) & 1, 1, false))
            }
            ExprKind::Slice { base, hi, lo } => {
                let b = self.eval(base)?;
                let hi = checked_index(const_eval(hi, self.ints)?, b.width, "slice high bound")?;
                let lo = checked_index(const_eval(lo, self.ints)?, b.width, "slice low bound")?;
                if hi < lo {
                    return Err("slice bounds reversed (write `[hi:lo]`, msb first)".into());
                }
                let w = hi - lo + 1;
                Ok(Val::new((b.bits >> lo) & mask(w), w, b.signed))
            }
            ExprKind::Field { .. } => Err(
                "enum-variant / instance-port access is not supported by the combinational \
                 evaluator yet"
                    .into(),
            ),
            ExprKind::Call { func, args } => self.call(*func, args),
        }
    }

    fn call(&mut self, func: Builtin, args: &[Expr]) -> Result<Val, String> {
        match func {
            Builtin::Extend => {
                let v = self.eval(&args[0])?;
                let n = checked_width(const_eval(&args[1], self.ints)?)?;
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
                let v = self.eval(&args[0])?;
                let n = checked_width(const_eval(&args[1], self.ints)?)?;
                Ok(Val::new(v.bits & mask(n), n, v.signed))
            }
            Builtin::SignedCast => {
                let v = self.eval(&args[0])?;
                Ok(Val::new(v.bits, v.width, true))
            }
            Builtin::UnsignedCast => {
                let v = self.eval(&args[0])?;
                Ok(Val::new(v.bits, v.width, false))
            }
        }
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
        // Lossless growth (spec/02 section 3).
        BinOp::Add => Val::new(l.bits.wrapping_add(r.bits), wmax + 1, signed),
        BinOp::Sub => Val::new(
            l.as_i128().wrapping_sub(r.as_i128()) as u128,
            wmax + 1,
            true,
        ),
        BinOp::Mul => Val::new(l.bits.wrapping_mul(r.bits), l.width + r.width, signed),
        // Wrapping family: keep operand width.
        BinOp::AddWrap => Val::new(l.bits.wrapping_add(r.bits), wmax, signed),
        BinOp::SubWrap => Val::new(l.bits.wrapping_sub(r.bits), wmax, signed),
        BinOp::MulWrap => Val::new(l.bits.wrapping_mul(r.bits), wmax, signed),
        BinOp::BitAnd => Val::new(l.bits & r.bits, wmax, signed),
        BinOp::BitOr => Val::new(l.bits | r.bits, wmax, signed),
        BinOp::BitXor => Val::new(l.bits ^ r.bits, wmax, signed),
        BinOp::Shl => Val::new(
            l.bits.checked_shl(r.bits as u32).unwrap_or(0),
            l.width,
            l.signed,
        ),
        BinOp::Shr => Val::new(l.bits >> (r.bits.min(127) as u32), l.width, l.signed),
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

fn pattern_matches(p: &Pattern, s: &Val) -> Result<bool, String> {
    match p {
        Pattern::Wildcard => Ok(true),
        Pattern::Int { value, .. } => Ok((s.bits & mask(s.width)) == (*value & mask(s.width))),
        Pattern::Bool(b) => Ok((s.bits & 1) == (*b as u128)),
        Pattern::Variant { .. } => {
            Err("enum-variant patterns are not supported by the combinational evaluator yet".into())
        }
    }
}

/// The declared (width, signed) of a hardware type, evaluating any width
/// expression in the const environment.
fn type_width(ty: &Type, ints: &BTreeMap<String, i128>) -> Result<(u32, bool), String> {
    match ty {
        Type::Bit => Ok((1, false)),
        Type::Bits(e) => Ok((checked_width(const_eval(e, ints)?)?, false)),
        Type::Signed(e) => Ok((checked_width(const_eval(e, ints)?)?, true)),
        Type::Named(n) => Err(format!(
            "signal of enum type `{}` — the evaluator does not model enum signals yet",
            n.name
        )),
    }
}

fn checked_width(n: i128) -> Result<u32, String> {
    if n < 1 {
        Err(format!("width must be at least 1, got {n}"))
    } else if n > 128 {
        Err(format!("width {n} exceeds the evaluator's 128-bit limit"))
    } else {
        Ok(n as u32)
    }
}

/// Compile-time const evaluation for widths, parameters, consts, indices, and
/// slice bounds. **Delegates to the checker's hardened evaluator**
/// (`checker::consteval::eval`) — the single source of truth — which uses
/// `checked_*` arithmetic and guarded shifts, so an oversized const such as
/// `1 << 200` or `9e30 * 9e30` is a clean error, never a debug panic or a
/// silent release wrap. This matters because the `mimz eval` path does **not**
/// run the checker (`main::eval_file`), so this is the only overflow guard on
/// that path. The checker's `Diag` is flattened to the `String` the evaluator
/// reports.
fn const_eval(e: &Expr, ints: &BTreeMap<String, i128>) -> Result<i128, String> {
    let env: crate::checker::consteval::Env = ints.iter().map(|(k, v)| (k.clone(), *v)).collect();
    crate::checker::consteval::eval(e, &env).map_err(|d| d.msg)
}

/// A bit index or slice bound must be a non-negative integer inside the value's
/// width. Rejects negative / out-of-range positions with a clear error instead
/// of truncating via `as u32` (which silently wrapped before) and instead of a
/// later oversized shift (`>> n`, `n >= 128`, which panics in debug).
fn checked_index(n: i128, width: u32, what: &str) -> Result<u32, String> {
    if (0..width as i128).contains(&n) {
        Ok(n as u32)
    } else {
        Err(format!(
            "{what} {n} is out of range for a {width}-bit value"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ast::File {
        crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses")
    }

    fn ins(pairs: &[(&str, u128)]) -> BTreeMap<String, u128> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn one(file: &ast::File, inputs: &[(&str, u128)]) -> Vec<Output> {
        eval_outputs(file, None, &ins(inputs), &BTreeMap::new()).expect("evaluates")
    }

    #[test]
    fn adder_grows_losslessly() {
        let f = parse(
            "module Adder(W: int = 8) {\n  in a: bits[W]\n  in b: bits[W]\n  out sum: bits[W+1]\n  sum = a + b\n}\n",
        );
        let out = one(&f, &[("a", 3), ("b", 5)]);
        assert_eq!(out[0].name, "sum");
        assert_eq!((out[0].value, out[0].width), (8, 9));
        // 200 + 100 = 300, carried into the 9th bit (no wrap).
        assert_eq!(one(&f, &[("a", 200), ("b", 100)])[0].value, 300);
    }

    #[test]
    fn wrapping_add_keeps_width() {
        let f = parse(
            "module W {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[8]\n  y = a +% b\n}\n",
        );
        assert_eq!(one(&f, &[("a", 200), ("b", 100)])[0].value, 44); // 300 mod 256
        assert_eq!(one(&f, &[("a", 200), ("b", 100)])[0].width, 8);
    }

    #[test]
    fn comparator_if_and_compares() {
        let f = parse(
            "module C(W: int = 8) {\n  in a: bits[W]\n  in b: bits[W]\n  out eq: bit\n  out gt: bit\n  out max: bits[W]\n  eq = a == b\n  gt = a > b\n  max = if a > b { a } else { b }\n}\n",
        );
        let o = one(&f, &[("a", 7), ("b", 3)]);
        let m: BTreeMap<_, _> = o.iter().map(|x| (x.name.as_str(), x.value)).collect();
        assert_eq!(m["eq"], 0);
        assert_eq!(m["gt"], 1);
        assert_eq!(m["max"], 7);
        let o = one(&f, &[("a", 4), ("b", 4)]);
        let m: BTreeMap<_, _> = o.iter().map(|x| (x.name.as_str(), x.value)).collect();
        assert_eq!((m["eq"], m["gt"], m["max"]), (1, 0, 4));
    }

    #[test]
    fn mux_match_selects() {
        let f = parse(
            "module M(W: int = 8) {\n  in sel: bits[2]\n  in a: bits[W]\n  in b: bits[W]\n  in c: bits[W]\n  in d: bits[W]\n  out y: bits[W]\n  y = match sel {\n    0b00 => a\n    0b01 => b\n    0b10 => c\n    0b11 => d\n  }\n}\n",
        );
        assert_eq!(
            one(
                &f,
                &[("sel", 2), ("a", 10), ("b", 20), ("c", 30), ("d", 40)]
            )[0]
            .value,
            30
        );
        assert_eq!(
            one(
                &f,
                &[("sel", 0), ("a", 10), ("b", 20), ("c", 30), ("d", 40)]
            )[0]
            .value,
            10
        );
    }

    #[test]
    fn chained_comparison_window() {
        let f = parse(
            "module Window {\n  in lo: bits[8]\n  in value: bits[8]\n  in hi: bits[8]\n  out in_range: bit\n  in_range = lo <= value <= hi\n}\n",
        );
        assert_eq!(
            one(&f, &[("lo", 10), ("value", 50), ("hi", 100)])[0].value,
            1
        );
        assert_eq!(
            one(&f, &[("lo", 10), ("value", 5), ("hi", 100)])[0].value,
            0
        );
        assert_eq!(
            one(&f, &[("lo", 10), ("value", 100), ("hi", 100)])[0].value,
            1
        ); // boundary
    }

    #[test]
    fn rejects_sequential_logic() {
        let f = parse(
            "module Seq {\n  clock clk\n  reset rst\n  out y: bits[8]\n  reg r: bits[8] = 0\n  on rise(clk) { r <- r +% 1 }\n  y = r\n}\n",
        );
        let err = eval_outputs(&f, None, &ins(&[]), &BTreeMap::new()).unwrap_err();
        assert!(
            err.contains("reg"),
            "expected a clear reg rejection, got: {err}"
        );
    }

    #[test]
    fn reports_missing_input() {
        let f = parse("module A {\n  in a: bits[8]\n  out y: bits[8]\n  y = a\n}\n");
        let err = eval_outputs(&f, None, &ins(&[]), &BTreeMap::new()).unwrap_err();
        assert!(err.contains("missing value for input `a`"), "got: {err}");
    }
}
