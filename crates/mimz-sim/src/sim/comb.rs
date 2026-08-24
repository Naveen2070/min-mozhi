//! Combinational evaluator — evaluate a clockless module's outputs from its
//! inputs, by interpreting the AST directly.
//!
//! Scope (deliberately a SLICE of the Phase 1.5 simulator): one module, no
//! `reg`, no `on` block, no instances, no `repeat`, no `sync loop`. Those are
//! rejected with a clear message rather than half-evaluated. Within that scope
//! it honors the
//! spec's width semantics — lossless `+ - *` grow, the `+% -% *%` family wraps,
//! slices/concat/`extend`/`trunc` resize — so the result matches what the
//! Verilog emitter would produce for the same combinational logic.
//!
//! The value model and the expression evaluator live in `super::value`; this
//! module adds combinational driver resolution — a memoized walk with cycle
//! detection — on top, implementing that module's `Resolver` trait. `mimz eval`
//! is its CLI surface.

use std::collections::{BTreeMap, HashMap};

use mimz_core::ast::{self, Dir, Expr, FuncDecl, ModuleItem};

// BUG-43 (docs/audit/bugs.md): this file used to carry its own
// byte-identical copy of a zero-padding `remask_to_width`. The one rule
// now lives at `value::resize_to_width`, which sign-extends a signed
// source instead of dropping its sign — three copies of one resize rule
// was the same drift surface GAP-1 describes.
use super::value::{self, Resolver, Val, resize_to_width};
use crate::sim::Diag;

/// Flatten `const if` nodes in `items`, evaluating conditions against `ints`.
/// Items from winning branches replace the ConstIf node; losing branches drop.
fn flatten_const_if<'a>(
    items: &'a [ModuleItem],
    ints: &BTreeMap<String, i128>,
) -> Vec<&'a ModuleItem> {
    let mut out = Vec::new();
    for item in items {
        match item {
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                let val = value::const_eval(cond, ints).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                out.extend(flatten_const_if(branch, ints));
            }
            _ => out.push(item),
        }
    }
    out
}

/// Collect module-level `const` declarations (including those inside winning
/// `const if` branches) into `ints`. Propagates errors from const evaluation.
fn collect_module_consts(
    items: &[ModuleItem],
    ints: &mut BTreeMap<String, i128>,
) -> Result<(), Box<Diag>> {
    for it in items {
        match it {
            ModuleItem::Const(c) => {
                let v = value::const_eval(&c.value, ints)?;
                ints.insert(c.name.name.clone(), v);
            }
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                let val = value::const_eval(cond, ints).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                collect_module_consts(branch, ints)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// One evaluated output port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// The output port's name.
    pub name: String,
    /// The output value, in the low `width` bits.
    pub value: value::Bits,
    /// Bit width of the output port.
    pub width: u32,
    /// Whether the output port is `signed`.
    pub signed: bool,
}

/// Evaluate the outputs of `module` (or the entry file's only module when
/// `module` is `None`) given `inputs` (name → value) and optional `params`
/// overrides. `files[0]` is the entry file; remaining files supply functions
/// (D3: functions are project-wide). Missing inputs, sequential constructs,
/// and out-of-scope expressions all return a descriptive error.
pub fn eval_outputs(
    files: &[ast::File],
    module: Option<&str>,
    inputs: &BTreeMap<String, value::Bits>,
    params: &BTreeMap<String, i128>,
) -> Result<Vec<Output>, Box<Diag>> {
    let file = files.first().ok_or_else(|| {
        Box::new(
            Diag::new(mimz_core::span::Span::default(), "eval_outputs: no files")
                .with_code("S0230"),
        )
    })?;
    let m = value::pick_module(file, module)?;

    // 1. Reject anything sequential / structural — this is the comb slice.
    for it in &m.items {
        match it {
            ModuleItem::Reg { name, .. } => {
                return Err(Box::new(
                    Diag::new(
                        name.span,
                        "module has `reg` state — the combinational evaluator does not run \
                         clocked logic yet (that is the Phase 1.5 simulator)",
                    )
                    .with_code("S0231"),
                ));
            }
            ModuleItem::On(on) => {
                return Err(Box::new(
                    Diag::new(
                        on.span,
                        "module has an `on` block — combinational evaluation only; \
                         clocked behavior is Phase 1.5",
                    )
                    .with_code("S0232"),
                ));
            }
            ModuleItem::Inst(inst) => {
                return Err(Box::new(
                    Diag::new(
                        inst.span,
                        "module instantiates a sub-module — the evaluator does not elaborate \
                         instances yet (single-module, combinational only)",
                    )
                    .with_code("S0233"),
                ));
            }
            ModuleItem::Repeat(r) => {
                return Err(Box::new(
                    Diag::new(
                        r.span,
                        "module uses `repeat` — unrolling is not supported by the evaluator yet",
                    )
                    .with_code("S0234"),
                ));
            }
            ModuleItem::SyncLoop(sl) => {
                return Err(Box::new(
                    Diag::new(
                        sl.name.span,
                        "module uses `sync loop` — clocked, multi-cycle evaluation is not \
                         supported by the combinational-only evaluator; use the real simulator \
                         (`mimz sim`/`mimz test`) instead",
                    )
                    .with_code("S0235"),
                ));
            }
            _ => {}
        }
    }

    // 2a. User-defined functions from ALL files (D3: functions are project-wide),
    //    available to `FnCall` expressions in this module's combinational logic.
    let funcs: HashMap<String, FuncDecl> = files
        .iter()
        .flat_map(|f| f.items.iter())
        .filter_map(|it| {
            if let ast::TopItem::Func(f) = it {
                Some((f.name.name.clone(), f.clone()))
            } else {
                None
            }
        })
        .collect();

    // 2b. Compile-time integer environment: params (defaults, overridable) then
    //    consts (file-level + module-level).
    let mut ints: BTreeMap<String, i128> = BTreeMap::new();
    for p in &m.params {
        let v = match params.get(&p.name.name) {
            Some(v) => *v,
            None => match &p.default {
                Some(d) => value::const_eval(d, &ints)?,
                None => {
                    return Err(Box::new(
                        Diag::new(
                            p.name.span,
                            format!(
                                "parameter `{}` has no default — pass it with --param {}=<n>",
                                p.name.name, p.name.name
                            ),
                        )
                        .with_code("S0121"),
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
    collect_module_consts(&m.items, &mut ints)?;
    let flat_items: Vec<&ModuleItem> = flatten_const_if(&m.items, &ints);

    // 3. Signals (in/out/wire) with their declared (width, signed).
    let mut sig_ty: BTreeMap<String, (u32, bool)> = BTreeMap::new();
    let mut drivers: BTreeMap<String, Expr> = BTreeMap::new();
    // A bit-indexed (`sig[i] = …`) or slice (`sig[hi:lo] = …`, BUG-17) drive
    // is collected per bit here and assembled into a `Concat` after the item
    // loop — same strategy `crates/mimz-sim/src/sim/elaborate/module.rs`'s
    // `record_drive` uses. A slice drive just expands to one bit-drive entry
    // per bit position, each reading the matching bit straight off the RHS.
    let mut bit_drives: BTreeMap<String, BTreeMap<u32, Expr>> = BTreeMap::new();
    let mut out_order: Vec<(String, u32, bool)> = Vec::new();
    for it in flat_items.iter().copied() {
        match it {
            ModuleItem::Port { dir, name, ty } => {
                let (w, s) = value::type_width(ty, &ints, name.span)?;
                sig_ty.insert(name.name.clone(), (w, s));
                if *dir == Dir::Out {
                    out_order.push((name.name.clone(), w, s));
                }
            }
            ModuleItem::Wire { name, ty, init } => {
                let (w, s) = value::type_width(ty, &ints, name.span)?;
                sig_ty.insert(name.name.clone(), (w, s));
                drivers.insert(name.name.clone(), init.clone());
            }
            ModuleItem::Drive { lhs, rhs } => match &lhs.index {
                None => {
                    drivers.insert(lhs.base.name.clone(), rhs.clone());
                }
                Some((idx, None)) => {
                    let bit = value::const_eval(idx, &ints)?;
                    if !(0..128).contains(&bit) {
                        return Err(Box::new(
                            Diag::new(
                                idx.span,
                                format!(
                                    "bit index {bit} driving `{}` is out of range (0..128)",
                                    lhs.base.name
                                ),
                            )
                            .with_code("S0134"),
                        ));
                    }
                    bit_drives
                        .entry(lhs.base.name.clone())
                        .or_default()
                        .insert(bit as u32, rhs.clone());
                }
                Some((hi_e, Some(lo_e))) => {
                    let hi = value::const_eval(hi_e, &ints)?;
                    let lo = value::const_eval(lo_e, &ints)?;
                    if !(0..128).contains(&hi) || !(0..128).contains(&lo) {
                        return Err(Box::new(
                            Diag::new(
                                hi_e.span.join(lo_e.span),
                                format!(
                                    "slice bound driving `{}` is out of range (0..128)",
                                    lhs.base.name
                                ),
                            )
                            .with_code("S0135"),
                        ));
                    }
                    if hi < lo {
                        return Err(Box::new(
                            Diag::new(
                                hi_e.span.join(lo_e.span),
                                format!(
                                    "slice bounds driving `{}` are reversed (write `[hi:lo]`, \
                                     most significant bit first)",
                                    lhs.base.name
                                ),
                            )
                            .with_code("S0136"),
                        ));
                    }
                    let span = rhs.span;
                    let entry = bit_drives.entry(lhs.base.name.clone()).or_default();
                    // A compile-time-constant RHS (e.g. `i * 2` after a
                    // `foreach` unroll substitutes `i` with a literal)
                    // adapts to the slice's width like any other CtInt
                    // assignment — pull each target bit from the folded
                    // value (arithmetic shift, so a negative constant
                    // sign-extends correctly) instead of indexing into the
                    // raw expression, which may have a narrower "natural"
                    // width of its own (see elaborate/module.rs's
                    // `record_drive`, the same fix mirrored here).
                    if let Ok(v) = value::const_eval(rhs, &ints) {
                        for b in lo..=hi {
                            let bit = ((v >> (b - lo)) & 1) as u128;
                            entry.insert(
                                b as u32,
                                Expr {
                                    kind: ast::ExprKind::Int {
                                        value: bit.into(),
                                        raw: bit.to_string(),
                                    },
                                    span,
                                },
                            );
                        }
                    } else {
                        for b in lo..=hi {
                            let rhs_bit = (b - lo) as u128;
                            let sel = Expr {
                                kind: ast::ExprKind::Index {
                                    base: Box::new(rhs.clone()),
                                    index: Box::new(Expr {
                                        kind: ast::ExprKind::Int {
                                            value: rhs_bit.into(),
                                            raw: rhs_bit.to_string(),
                                        },
                                        span,
                                    }),
                                },
                                span,
                            };
                            entry.insert(b as u32, sel);
                        }
                    }
                }
            },
            _ => {}
        }
    }
    for (sig, bits) in bit_drives {
        let width = sig_ty.get(&sig).map(|(w, _)| *w).ok_or_else(|| {
            Box::new(
                Diag::new(
                    m.span,
                    format!("bit-driven signal `{sig}` has no declaration"),
                )
                .with_code("S0129"),
            )
        })?;
        let mut parts = Vec::with_capacity(width as usize);
        for b in (0..width).rev() {
            let e = bits.get(&b).ok_or_else(|| {
                Box::new(
                    Diag::new(m.span, format!("signal `{sig}` bit {b} is not driven"))
                        .with_code("S0130"),
                )
            })?;
            parts.push(e.clone());
        }
        let span = parts.first().map(|e| e.span).unwrap_or(m.span);
        drivers.insert(
            sig,
            Expr {
                kind: ast::ExprKind::Concat(parts),
                span,
            },
        );
    }

    // 4. Seed input values (masked to their declared width).
    let mut env = Env {
        ints: &ints,
        sig_ty: &sig_ty,
        drivers: &drivers,
        memo: BTreeMap::new(),
        in_progress: Vec::new(),
        funcs: &funcs,
    };
    for it in flat_items.iter().copied() {
        if let ModuleItem::Port {
            dir: Dir::In, name, ..
        } = it
        {
            let (w, s) = sig_ty[&name.name];
            let raw = inputs.get(&name.name).cloned().ok_or_else(|| {
                Box::new(
                    Diag::new(
                        name.span,
                        format!(
                            "missing value for input `{}` — pass it with --in {}=<n>",
                            name.name, name.name
                        ),
                    )
                    .with_code("S0236"),
                )
            })?;
            let val = match raw {
                value::Bits::Small(b) if w <= 128 => Val::new(b, w, s),
                value::Bits::Small(b) => Val::new_wide(value::wide_limbs_from_u128(b, w), w, s),
                value::Bits::Wide(limbs) => Val::new_wide(limbs, w, s),
            };
            env.memo.insert(name.name.clone(), val);
        }
    }

    // 5. Resolve each output.
    let mut outputs = Vec::new();
    for (name, _, _) in &out_order {
        let v = env.resolve(name)?;
        outputs.push(Output {
            name: name.clone(),
            value: v.bits_masked(),
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
    drivers: &'a BTreeMap<String, Expr>,
    memo: BTreeMap<String, Val>,
    in_progress: Vec<String>,
    funcs: &'a HashMap<String, FuncDecl>,
}

impl Env<'_> {
    /// Resolve a signal's value, evaluating (and memoizing) its driver on first
    /// use. A signal seen twice on the active stack is a combinational cycle.
    fn resolve(&mut self, name: &str) -> Result<Val, Box<Diag>> {
        if let Some(v) = self.memo.get(name) {
            return Ok(v.clone());
        }
        if self.in_progress.iter().any(|n| n == name) {
            return Err(Box::new(
                Diag::new(
                    mimz_core::span::Span::default(),
                    format!(
                        "combinational cycle through `{name}` — feedback must pass through a register"
                    ),
                )
                .with_code("S0238"),
            ));
        }
        let driver = self.drivers.get(name).ok_or_else(|| {
            Box::new(
                Diag::new(
                    mimz_core::span::Span::default(),
                    format!("signal `{name}` is never driven"),
                )
                .with_code("S0237"),
            )
        })?;
        self.in_progress.push(name.to_string());
        // `<<` self-determines its own (grown) width now (BUG-30,
        // `docs/audit/bugs.md`) — no target width needs to reach `driver`,
        // the `resize_to_width` below still reconciles the result against
        // the signal's declared width either way.
        let v = value::eval(self, driver)?;
        self.in_progress.pop();
        let (w, s) = self
            .sig_ty
            .get(name)
            .copied()
            .unwrap_or((v.width, v.signed));
        let v = resize_to_width(v, w, s); // mask to the declared width
        self.memo.insert(name.to_string(), v.clone());
        Ok(v)
    }
}

impl Resolver for Env<'_> {
    fn signal(&mut self, name: &str) -> Result<Val, String> {
        if self.sig_ty.contains_key(name) || self.drivers.contains_key(name) {
            // BUG-27: preserve `resolve`'s own code (e.g. `S0238`,
            // combinational cycle) across the `Resolver::signal` boundary
            // instead of always discarding it down to a plain message —
            // `eval`'s `Ident` arm recovers it via `diag_from_bridged`.
            self.resolve(name).map_err(|e| match e.code {
                Some(code) => crate::sim::diag::bridge_code(code, &e.msg),
                None => e.msg,
            })
        } else if let Some(v) = self.ints.get(name) {
            Ok(Val::from_int(*v))
        } else {
            Err(format!("unknown name `{name}` in evaluation"))
        }
    }
    fn ints(&self) -> &BTreeMap<String, i128> {
        self.ints
    }
    fn funcs(&self) -> Option<&HashMap<String, FuncDecl>> {
        Some(self.funcs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ast::File {
        mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses")
    }

    fn ins(pairs: &[(&str, u128)]) -> BTreeMap<String, value::Bits> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), value::Bits::Small(*v)))
            .collect()
    }

    fn one(file: &ast::File, inputs: &[(&str, u128)]) -> Vec<Output> {
        eval_outputs(
            std::slice::from_ref(file),
            None,
            &ins(inputs),
            &BTreeMap::new(),
        )
        .expect("evaluates")
    }

    /// Unwrap a narrow-path `Output.value`/test result — every test in this
    /// module only ever drives/reads `bits[<=128]` signals, so `Bits::Wide`
    /// is never actually produced here; this is the test-only counterpart to
    /// `Val::masked`'s "narrow-path-only" contract.
    fn small(b: &value::Bits) -> u128 {
        match b {
            value::Bits::Small(v) => *v,
            value::Bits::Wide(_) => panic!("test expected a narrow (Small) value"),
        }
    }

    #[test]
    fn zero_length_array_param_index_is_a_clean_err_not_a_panic() {
        // Regression (fuzz: lex_parse_eval, crash-3de69b943336db288b4aaab6a2d210dc7d83555d):
        // `bits[8][0]` is rejected by the checker's E0412 in the normal
        // `mimz compile`/`mimz test` pipeline, but this evaluator is also
        // exercised directly on unchecked ASTs (fuzzing), where a
        // zero-length array param used to underflow `elems.len() - 1` in
        // the array-index eval (`src/sim/value.rs`).
        let f = parse(
            "fn first(vals: bits[8][0]) -> bits[8] {\n  vals[0]\n}\n\nmodule M {\n  in a: bits[8]\n  out y: bits[8]\n  y = first(a)\n}\n",
        );
        let err = eval_outputs(
            std::slice::from_ref(&f),
            None,
            &ins(&[("a", 1)]),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.msg.contains("no elements to index"), "got: {}", err.msg);
    }

    #[test]
    fn match_pattern_referencing_an_unknown_enum_is_a_clean_err_not_a_panic() {
        // BUG-40 (docs/audit/bugs.md): same "fuzz: lex_parse_eval" class as
        // the zero-length-array test above. The parser accepts
        // `EnumName.variant` pattern syntax without checking `EnumName` is
        // a real declared enum (that's the checker's job) — a raw fuzzed
        // `.mimz` file can put one in a match arm and reach this
        // checker-bypassing evaluator directly. Used to panic
        // (`pattern_matches`'s `unreachable!()`, which only actually holds
        // on the clocked elaboration path) on the FIRST pattern check, before
        // ever considering any later arm — deliberately no wildcard arm here
        // so a correct fix falls through to the ordinary "no arm matched"
        // error instead, rather than happening to succeed via a trailing `_`.
        let f = parse(
            "module M {\n  in req: bits[3]\n  out grant: bits[2]\n  \
             grant = match req {\n    Bogus.X => 0b01\n  }\n}\n",
        );
        let err = eval_outputs(
            std::slice::from_ref(&f),
            None,
            &ins(&[("req", 1)]),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(
            err.msg.contains("no `match` arm matched"),
            "got: {}",
            err.msg
        );
    }

    #[test]
    fn adder_grows_losslessly() {
        let f = parse(
            "module Adder(W: int = 8) {\n  in a: bits[W]\n  in b: bits[W]\n  out sum: bits[W+1]\n  sum = a + b\n}\n",
        );
        let out = one(&f, &[("a", 3), ("b", 5)]);
        assert_eq!(out[0].name, "sum");
        assert_eq!((small(&out[0].value), out[0].width), (8, 9));
        // 200 + 100 = 300, carried into the 9th bit (no wrap).
        assert_eq!(small(&one(&f, &[("a", 200), ("b", 100)])[0].value), 300);
    }

    #[test]
    fn wrapping_add_keeps_width() {
        let f = parse(
            "module W {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[8]\n  y = a +% b\n}\n",
        );
        assert_eq!(small(&one(&f, &[("a", 200), ("b", 100)])[0].value), 44); // 300 mod 256
        assert_eq!(one(&f, &[("a", 200), ("b", 100)])[0].width, 8);
    }

    #[test]
    fn comparator_if_and_compares() {
        let f = parse(
            "module C(W: int = 8) {\n  in a: bits[W]\n  in b: bits[W]\n  out eq: bit\n  out gt: bit\n  out max: bits[W]\n  eq = a == b\n  gt = a > b\n  max = if a > b { a } else { b }\n}\n",
        );
        let o = one(&f, &[("a", 7), ("b", 3)]);
        let m: BTreeMap<_, _> = o
            .iter()
            .map(|x| (x.name.as_str(), small(&x.value)))
            .collect();
        assert_eq!(m["eq"], 0);
        assert_eq!(m["gt"], 1);
        assert_eq!(m["max"], 7);
        let o = one(&f, &[("a", 4), ("b", 4)]);
        let m: BTreeMap<_, _> = o
            .iter()
            .map(|x| (x.name.as_str(), small(&x.value)))
            .collect();
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
            .map(|x| (x.name.as_str(), (small(&x.value), x.width)))
            .collect();
        assert_eq!(m["y"], (0b1010_1010, 8));
        assert_eq!(m["z"], (0b1010_1010_1010, 12));
    }

    #[test]
    fn dont_care_match_picks_the_masked_arm() {
        // `0b1?? => 3`, `0b01? => 2`, `_ => 0` on a bits[3] priority decoder.
        let f = parse(
            "module D {\n  in s: bits[3]\n  out y: bits[2]\n  y = match s {\n    0b1?? => 0b11\n    0b01? => 0b10\n    _ => 0b00\n  }\n}\n",
        );
        let pick = |v: u128| small(&one(&f, &[("s", v)])[0].value);
        assert_eq!(pick(0b100), 3); // matches 0b1??
        assert_eq!(pick(0b111), 3); // matches 0b1??
        assert_eq!(pick(0b010), 2); // matches 0b01?
        assert_eq!(pick(0b001), 0); // falls to `_`
    }

    #[test]
    fn mux_match_selects() {
        let f = parse(
            "module M(W: int = 8) {\n  in sel: bits[2]\n  in a: bits[W]\n  in b: bits[W]\n  in c: bits[W]\n  in d: bits[W]\n  out y: bits[W]\n  y = match sel {\n    0b00 => a\n    0b01 => b\n    0b10 => c\n    0b11 => d\n  }\n}\n",
        );
        assert_eq!(
            small(
                &one(
                    &f,
                    &[("sel", 2), ("a", 10), ("b", 20), ("c", 30), ("d", 40)]
                )[0]
                .value
            ),
            30
        );
        assert_eq!(
            small(
                &one(
                    &f,
                    &[("sel", 0), ("a", 10), ("b", 20), ("c", 30), ("d", 40)]
                )[0]
                .value
            ),
            10
        );
    }

    #[test]
    fn chained_comparison_window() {
        let f = parse(
            "module Window {\n  in lo: bits[8]\n  in value: bits[8]\n  in hi: bits[8]\n  out in_range: bit\n  in_range = lo <= value <= hi\n}\n",
        );
        assert_eq!(
            small(&one(&f, &[("lo", 10), ("value", 50), ("hi", 100)])[0].value),
            1
        );
        assert_eq!(
            small(&one(&f, &[("lo", 10), ("value", 5), ("hi", 100)])[0].value),
            0
        );
        assert_eq!(
            small(&one(&f, &[("lo", 10), ("value", 100), ("hi", 100)])[0].value),
            1
        ); // boundary
    }

    #[test]
    fn rejects_sequential_logic() {
        let f = parse(
            "module Seq {\n  clock clk\n  reset rst\n  out y: bits[8]\n  reg r: bits[8] = 0\n  on rise(clk) { r <- r +% 1 }\n  y = r\n}\n",
        );
        let err = eval_outputs(&[f], None, &ins(&[]), &BTreeMap::new()).unwrap_err();
        assert!(
            err.msg.contains("reg"),
            "expected a clear reg rejection, got: {}",
            err.msg
        );
    }

    #[test]
    fn reports_missing_input() {
        let f = parse("module A {\n  in a: bits[8]\n  out y: bits[8]\n  y = a\n}\n");
        let err = eval_outputs(&[f], None, &ins(&[]), &BTreeMap::new()).unwrap_err();
        assert!(
            err.msg.contains("missing value for input `a`"),
            "got: {}",
            err.msg
        );
    }

    #[test]
    fn shift_left_zero_amt() {
        let f = parse(
            "module S {\n  in a: bits[64]\n  in s: bits[8]\n  out y: bits[64]\n  y = a << s\n}\n",
        );
        assert_eq!(small(&one(&f, &[("a", 1), ("s", 0)])[0].value), 1);
    }

    #[test]
    fn shift_right_zero_amt() {
        let f = parse(
            "module S {\n  in a: bits[64]\n  in s: bits[8]\n  out y: bits[64]\n  y = a >> s\n}\n",
        );
        assert_eq!(small(&one(&f, &[("a", 2), ("s", 0)])[0].value), 2);
    }

    #[test]
    fn shift_left_max_width() {
        // BUG-30 (`docs/audit/bugs.md`): `<<` now grows by the amount's own
        // worst case when the amount is a genuine runtime signal — a
        // `bits[128]` amount could hold anything up to `2^128 - 1`, so
        // growing to guarantee no bits are lost is impossible under
        // `MAX_WIDTH`. This is a clean, honest rejection (`S0222`)
        // replacing what used to be a silently-computed value.
        let f = parse(
            "module S {\n  in a: bits[128]\n  in s: bits[128]\n  out y: bits[128]\n  y = a << s\n}\n",
        );
        let err =
            eval_outputs(&[f], None, &ins(&[("a", 1), ("s", 127)]), &BTreeMap::new()).unwrap_err();
        assert!(err.msg.contains("width limit"), "got: {}", err.msg);
    }

    #[test]
    fn shift_left_exceeding_width_is_zero() {
        // BUG-30: a `bits[128]`-wide dynamic shift amount is now rejected
        // outright (see `shift_left_max_width` above) rather than silently
        // computing anything — the old "yields 0 for an oversized shift"
        // behavior no longer applies once `<<` grows by default.
        let f = parse(
            "module S {\n  in a: bits[128]\n  in s: bits[128]\n  out y: bits[128]\n  y = a << s\n}\n",
        );
        for amt in [128u128, 200, u128::MAX] {
            let err = eval_outputs(
                std::slice::from_ref(&f),
                None,
                &ins(&[("a", 1), ("s", amt)]),
                &BTreeMap::new(),
            )
            .unwrap_err();
            assert!(err.msg.contains("width limit"), "got: {}", err.msg);
        }
    }

    #[test]
    fn shift_right_exceeding_width_is_zero() {
        let f = parse(
            "module S {\n  in a: bits[128]\n  in s: bits[128]\n  out y: bits[128]\n  y = a >> s\n}\n",
        );
        assert_eq!(small(&one(&f, &[("a", 2), ("s", 128)])[0].value), 0);
        assert_eq!(small(&one(&f, &[("a", 2), ("s", 200)])[0].value), 0);
        assert_eq!(small(&one(&f, &[("a", 2), ("s", u128::MAX)])[0].value), 0);
    }

    #[test]
    fn shift_left_bit_32_set_in_amt() {
        // Originally guarded a bug where bit ≥ 32 set in a shift amount hit
        // an `as u32` truncation in the raw arithmetic. BUG-30 (`docs/audit/
        // bugs.md`) makes that precondition unreachable through any valid
        // module: an amount value with bit 32 set needs a >=33-bit-wide
        // signal to hold it, and growing by that signal's own worst case
        // (`2^33 - 1`) already exceeds `MAX_WIDTH` — rejected before the
        // arithmetic ever runs (same `S0222` as `shift_left_max_width`).
        // `shift_right_bit_32_set_in_amt` below covers `>>`, which never
        // grows and so still reaches the raw arithmetic this guarded.
        let f = parse(
            "module S {\n  in a: bits[128]\n  in s: bits[128]\n  out y: bits[128]\n  y = a << s\n}\n",
        );
        let err = eval_outputs(
            &[f],
            None,
            &ins(&[("a", 1), ("s", 1u128 << 32)]),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.msg.contains("width limit"), "got: {}", err.msg);
    }

    #[test]
    fn shift_right_bit_32_set_in_amt() {
        let f = parse(
            "module S {\n  in a: bits[128]\n  in s: bits[128]\n  out y: bits[128]\n  y = a >> s\n}\n",
        );
        assert_eq!(
            small(&one(&f, &[("a", 1u128 << 63), ("s", 1u128 << 32)])[0].value),
            0
        );
    }

    // --- user function call tests (Task 10) ---

    /// `mac(a, b, c) = let p = a *% b; extend(p, 16) +% extend(c, 16)`
    /// params bits[8], ret bits[16].
    const MAC_SRC: &str = "\
fn mac(a: bits[8], b: bits[8], c: bits[8]) -> bits[16] {\n\
    let p = a *% b\n\
    extend(p, 16) +% extend(c, 16)\n\
}\n\
module M {\n\
    in a: bits[8]\n    in b: bits[8]\n    in c: bits[8]\n\
    out y: bits[16]\n\
    y = mac(a, b, c)\n\
}\n";

    #[test]
    fn sim_fn_call_mac_basic() {
        // mac(3, 4, 5) = 3*4 + 5 = 17 at bits[16]
        let f = parse(MAC_SRC);
        let out = eval_outputs(
            std::slice::from_ref(&f),
            None,
            &ins(&[("a", 3), ("b", 4), ("c", 5)]),
            &BTreeMap::new(),
        )
        .expect("mac(3,4,5) evaluates");
        assert_eq!(small(&out[0].value), 17, "mac(3,4,5) must equal 17");
        assert_eq!(out[0].width, 16);
    }

    #[test]
    fn sim_fn_call_mac_wrap_truncation() {
        // p = 200 *% 200 at bits[8] = 40000 mod 256 = 64 (NOT 40000)
        // result = extend(64, 16) +% extend(0, 16) = 64
        let f = parse(MAC_SRC);
        let out = eval_outputs(
            std::slice::from_ref(&f),
            None,
            &ins(&[("a", 200), ("b", 200), ("c", 0)]),
            &BTreeMap::new(),
        )
        .expect("mac(200,200,0) evaluates");
        assert_eq!(
            small(&out[0].value),
            64,
            "wrap-truncation: p must be 8-bit (64), not 40000"
        );
    }

    #[test]
    fn chained_signed_shift_context_extends_before_the_shift() {
        // BUG-34 (docs/audit/bugs.md): a `>>` immediately consumed by an
        // outer `<<`, with no `extend()`/named wire between them, must
        // sign-extend its SIGNED left operand to the FINAL enclosing width
        // BEFORE either shift runs — the same BUG-11 "shift operands are
        // context-determined" rule, which BUG-30 only re-applied to `<<`'s
        // own growth, not to a `>>` feeding one. Verified against real
        // `iverilog` on this exact source: `p2 = -9563` (raw 55973 as
        // signed[16]), `y = (p2 >> 4) << 7` computes -76544 (pattern
        // 8312064), not 447744 (`>>`'s self-determined-at-16-bits logical
        // shift result, re-extended too late by the outer `<<`).
        let f = parse(
            "module Fuzz {\n  in p2: signed[16]\n  out y: signed[23]\n  y = ((p2 >> 4) << 7)\n}\n",
        );
        let out = one(&f, &[("p2", 55973)]);
        assert_eq!(out[0].width, 23);
        assert_eq!(small(&out[0].value), 8312064); // -76544 as a 23-bit pattern
    }

    #[test]
    fn eval_outputs_handles_a_wide_input() {
        let src =
            "module M(WIDTH: int = 200) {\n  in a: bits[WIDTH]\n  out b: bits[WIDTH]\n  b = a\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let mut inputs = std::collections::BTreeMap::new();
        inputs.insert(
            "a".to_string(),
            super::value::Bits::Wide(crate::sim::wide::from_u128(123, 200)),
        );
        let outputs = eval_outputs(&[f], Some("M"), &inputs, &std::collections::BTreeMap::new())
            .expect("eval_outputs");
        let b = outputs
            .into_iter()
            .find(|o| o.name == "b")
            .expect("declares b");
        assert_eq!(
            b.value,
            super::value::Bits::Wide(crate::sim::wide::from_u128(123, 200))
        );
    }
}
