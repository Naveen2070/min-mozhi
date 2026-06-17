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
//! The value model and the expression evaluator live in `super::value`; this
//! module adds combinational driver resolution — a memoized walk with cycle
//! detection — on top, implementing that module's `Resolver` trait. `mimz eval`
//! is its CLI surface.

use std::collections::BTreeMap;

use crate::ast::{self, Dir, Expr, ModuleItem};

use super::value::{self, Resolver, Val};

/// One evaluated output port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub name: String,
    /// The output value, in the low `width` bits.
    pub value: u128,
    pub width: u32,
    pub signed: bool,
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
    let m = value::pick_module(file, module)?;

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
                Some(d) => value::const_eval(d, &ints)?,
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
            let v = value::const_eval(&c.value, &ints)?;
            ints.insert(c.name.name.clone(), v);
        }
    }
    for it in &m.items {
        if let ModuleItem::Const(c) = it {
            let v = value::const_eval(&c.value, &ints)?;
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
                let (w, s) = value::type_width(ty, &ints)?;
                sig_ty.insert(name.name.clone(), (w, s));
                if *dir == Dir::Out {
                    out_order.push((name.name.clone(), w, s));
                }
            }
            ModuleItem::Wire { name, ty, init } => {
                let (w, s) = value::type_width(ty, &ints)?;
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
            value: v.masked(),
            width: v.width,
            signed: v.signed,
        });
    }
    Ok(outputs)
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
        let v = value::eval(self, driver)?;
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
}

impl Resolver for Env<'_> {
    fn signal(&mut self, name: &str) -> Result<Val, String> {
        if self.sig_ty.contains_key(name) || self.drivers.contains_key(name) {
            self.resolve(name)
        } else if let Some(v) = self.ints.get(name) {
            Ok(Val::from_int(*v))
        } else {
            Err(format!("unknown name `{name}` in evaluation"))
        }
    }
    fn ints(&self) -> &BTreeMap<String, i128> {
        self.ints
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
    fn replication_repeats_the_group() {
        // `{2{a}}` = `{a, a}`, `{3{a}}` = `{a, a, a}`; a = 0b1010 (4 bits).
        let f = parse(
            "module R {\n  in a: bits[4]\n  out y: bits[8]\n  out z: bits[12]\n  y = {2{a}}\n  z = {3{a}}\n}\n",
        );
        let o = one(&f, &[("a", 0b1010)]);
        let m: BTreeMap<_, _> = o
            .iter()
            .map(|x| (x.name.as_str(), (x.value, x.width)))
            .collect();
        assert_eq!(m["y"], (0b1010_1010, 8));
        assert_eq!(m["z"], (0b1010_1010_1010, 12));
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
