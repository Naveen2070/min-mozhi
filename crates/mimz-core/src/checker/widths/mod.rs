//! Pass 7 — width and type checking (E0401–E0410) + match
//! exhaustiveness (E0601/E0602).
//!
//! Enforces the language's core safety promise (spec/02 section 6):
//! exact widths everywhere, lossless `+`/`-`/`*` grow the result, the
//! wrapping family keeps it, `signed` and `bits` never mix, and an
//! unsized literal adapts to its context only if it fits.
//!
//! **Parametric widths**: there is no symbolic algebra. Every module is
//! checked under a CONCRETE parameter binding — its defaults, plus one
//! extra check per distinct binding it is instantiated with (memoized).
//! A module whose params lack defaults is checked only as instantiated;
//! never instantiated means its internals are skipped (passes 1–6 still
//! ran). Connection widths are checked at every instantiation by
//! evaluating the child's port types under the instance's arguments —
//! the checker-side mirror of the emitter's `width_subst`.
//!
//! Decisions (dev log 2026-06-11/12): `bit` ≡ `bits[1]`; lossless `+`/`-`
//! accept unequal widths (result `max+1`); `extend`/`trunc` allow the
//! no-op width (parametric code needs it at boundary bindings); `trunc`
//! keeps the LOW bits; shift amounts are unsigned; `match` on `signed`
//! is rejected; slicing `signed` yields `bits`; full enum/value coverage
//! is exhaustive without `_`.
//!
//! File layout (folder-of-files pattern, as in `parser/`): `mod.rs` owns
//! the [`Ty`] model, [`Wcx`], and the config worklist; `stmts.rs` walks a
//! module body and `on`/sequential statement lists; `expr/` is the
//! bidirectional typing engine; `ops/` types operators, concat, and
//! builtins; `insts.rs` resolves instantiation bindings and connection
//! widths; `patterns.rs` checks `match` patterns and exhaustiveness;
//! `bundles.rs` resolves bundle fields and structural shape matching;
//! `funcs.rs` checks `fn` bodies and injects match-arm payload bindings;
//! `sigs.rs` resolves declared signal/type widths (`collect_sigs`,
//! `resolve_ty`, `eval_width`/`eval_depth`/`eval_array_len`).

mod bundles;
mod expr;
mod funcs;
mod insts;
mod ops;
mod patterns;
mod sigs;
mod stmts;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{
    BinOp, EnumDecl, Expr, ExprKind, FieldInit, FnStmt, ForEachSource, FuncDecl, Module,
    ModuleItem, NamedArg, Pattern, SeqStmt, TopItem, Type,
};
use crate::span::Span;
use crate::width_rules::MAX_WIDTH;

use super::Checker;
use super::consteval::{self, Env};
use super::names::Scope;

/// Memory depth ceiling (number of cells). Like [`MAX_WIDTH`], a sanity bound
/// far above any real design — keeps `initial`-seed emission and the kernel's
/// address space trivially safe.
const MAX_DEPTH: i128 = 1_000_000;

/// Distinct (module, parameter binding) configurations checked before the
/// worklist stops enqueuing. Terminates pathological recursive
/// instantiation (`A(W)` containing `A(W+1)`); a real error for that
/// shape belongs to the elaboration slice.
const MAX_CONFIGS: usize = 1000;

/// `repeat` bodies are width-checked per iteration value (that is how
/// `data[i]` going out of range at the LAST iteration is caught), but a
/// huge range would make checking O(range) — past this many iterations
/// only the first two and the last are checked.
const MAX_REPEAT_CHECKS: i128 = 256;

/// The width-pass type of an expression. Lives only inside this pass —
/// the AST stays untyped. Not `Copy`: `CtInt` carries a `ConstVal`, which
/// may hold an arbitrary-width `Bits::Wide(Vec<u64>)` (BUG-13 layer 2) —
/// every former implicit-copy call site becomes an explicit `.clone()`
/// (cheap in every case but a genuinely wide compile-time constant, which
/// is rare).
#[derive(Clone)]
enum Ty<'a> {
    /// `bit` — identical to `bits[1]` everywhere ([`bits`] normalizes).
    Bit,
    /// `bits[N]`, N >= 2 after normalization.
    Bits(u128),
    /// `signed[N]` (two's complement; `signed[1]` is just the sign bit).
    Signed(u128),
    /// An enum value; compared by enum NAME (project-unique per E0002).
    Enum(&'a EnumDecl),
    /// `mem ...[DEPTH]` — an addressable memory. Stores the resolved element
    /// width/signedness inline (not a nested `Ty`, so `Ty` stays `Copy`) plus
    /// the depth. Indexing it (`m[addr]`) yields the element type.
    Memory {
        width: u128,
        signed: bool,
        depth: u128,
    },
    /// `<elem>[N]` — a fixed-size array value. Stores the resolved
    /// element width/signedness inline (not a nested `Ty`, so `Ty` stays
    /// `Copy`), plus the length. Indexing it (`arr[idx]`) yields the
    /// element type — mirrors `Memory`'s own shape exactly (an array is
    /// conceptually memory-shaped: one addressable value with N elements
    /// of one scalar type).
    Array {
        elem_width: u128,
        elem_signed: bool,
        len: u128,
    },
    /// A bundle-typed value — nominal identity only (bundles are matched
    /// by name, never structurally; see `same()`). Field widths are
    /// resolved on demand via `resolve_bundle_fields` rather than stored
    /// inline (a bundle's field list is dynamically sized, so storing it
    /// directly would break `Ty`'s `Copy` bound the way `Array`/`Memory`
    /// avoid by storing only scalar element info).
    Bundle {
        name: &'a str,
        /// `QualIdent::resolved_file` — disambiguates same-named bundles
        /// declared in different files. Threaded straight into
        /// `resolve_bundle_fields`'s `bfile_hint` param.
        bfile_hint: Option<usize>,
        args: &'a [NamedArg],
    },
    /// A compile-time integer: literal, const, parameter, or `repeat`
    /// variable. Polymorphic — adapts to any sized context it fits
    /// (spec/02 section 1.8). Carries the value (already at its minimal
    /// width/signedness, per `ConstVal`'s own invariant) for the fit check.
    CtInt(consteval::ConstVal),
    Clock,
    Reset,
    /// Something already reported (here or by an earlier pass). Absorbs
    /// every operation and never produces a second diagnostic.
    Unknown,
}

/// Normalizing constructor: `bits[1]` IS `bit` (decision 2026-06-11).
fn bits(n: u128) -> Ty<'static> {
    if n == 1 { Ty::Bit } else { Ty::Bits(n) }
}

/// Structural equality (after [`bits`] normalization); enums by name.
fn same(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Bit, Ty::Bit) | (Ty::Clock, Ty::Clock) | (Ty::Reset, Ty::Reset) => true,
        (Ty::Bits(x), Ty::Bits(y)) | (Ty::Signed(x), Ty::Signed(y)) => x == y,
        (
            Ty::Array {
                elem_width: aw,
                elem_signed: asig,
                len: al,
            },
            Ty::Array {
                elem_width: bw,
                elem_signed: bsig,
                len: bl,
            },
        ) => aw == bw && asig == bsig && al == bl,
        (Ty::Enum(x), Ty::Enum(y)) => x.name.name == y.name.name,
        (Ty::Bundle { name: a, .. }, Ty::Bundle { name: b, .. }) => a == b,
        _ => false,
    }
}

/// Outcome of comparing a required bundle's field shape against a
/// provided bundle's field shape (feature 2.9: structural interface
/// matching, `docs/plan/phase-2-ir-synthesis.md`). `Compatible` when every
/// field the `required` side declares also exists in `provided` with an
/// identical type — extra fields on `provided` are allowed and ignored; no
/// field ever widens/narrows implicitly (the language's no-silent-
/// truncation rule holds here too).
pub(super) enum BundleShapeMatch {
    Compatible,
    MissingField(String),
    FieldTypeMismatch {
        field: String,
        expected: String,
        got: String,
    },
}

/// The concrete bit-width a `fn`-body `let` binding of this type would
/// declare as a Verilog `reg` (mirrors the `FnStmt::Let` width-inference
/// match in `check_fn_stmt_widths`, reused there both for a NEW binding and
/// to look up an EXISTING one's width when checking for a shadowing
/// conflict — BUG-9). `None` for a type with no single scalar reg width
/// (memory, bundle, enum, clock/reset, `Unknown`).
fn scalar_width(t: &Ty) -> Option<u32> {
    match t {
        Ty::Bit => Some(1),
        Ty::Bits(n) | Ty::Signed(n) => Some(*n as u32),
        // `ConstVal` is already stored at its minimal width (every
        // constructor runs it through `bits::shrink`) — no separate
        // min_bits/min_signed_bits computation needed anymore.
        Ty::CtInt(v) => Some(v.width),
        Ty::Array { elem_width, .. } => Some(*elem_width as u32),
        _ => None,
    }
}

/// Human name for error messages.
fn show(t: &Ty) -> String {
    match t {
        Ty::Bit => "`bit`".into(),
        Ty::Bits(n) => format!("`bits[{n}]`"),
        Ty::Signed(n) => format!("`signed[{n}]`"),
        Ty::Enum(e) => format!("enum `{}`", e.name.name),
        Ty::Memory {
            width,
            signed,
            depth,
        } => {
            let elem = if *signed {
                format!("signed[{width}]")
            } else {
                format!("bits[{width}]")
            };
            format!("memory `{elem}[{depth}]`")
        }
        Ty::Array {
            elem_width,
            elem_signed,
            len,
        } => {
            let elem = if *elem_signed {
                format!("signed[{elem_width}]")
            } else if *elem_width == 1 {
                "bit".to_string()
            } else {
                format!("bits[{elem_width}]")
            };
            format!("{elem}[{len}]")
        }
        // `__Valid`/`__ValidSigned` are compiler-synthesized to back `T?`
        // (Task 2/3) — show them back as the surface syntax the user
        // actually wrote, never the internal name.
        Ty::Bundle { name, args, .. } => match *name {
            "__Valid" | "__ValidSigned" => {
                let n = args
                    .iter()
                    .find(|a| a.name.name == "N")
                    .map(|a| crate::pretty::expr_str(&a.value))
                    .unwrap_or_else(|| "N".to_string());
                if *name == "__Valid" {
                    if n == "1" {
                        "`bit?`".to_string()
                    } else {
                        format!("`bits[{n}]?`")
                    }
                } else {
                    format!("`signed[{n}]?`")
                }
            }
            _ => format!("bundle `{name}`"),
        },
        Ty::CtInt(v) => format!("the compile-time value `{v}`"),
        Ty::Clock => "a clock".into(),
        Ty::Reset => "a reset".into(),
        Ty::Unknown => "an unknown type".into(),
    }
}

/// One module being checked under one concrete parameter binding.
struct Wcx<'a> {
    file: usize,
    sc: Rc<Scope<'a>>,
    /// file consts + parameter binding + module consts + `repeat` vars.
    env: Env,
    /// signal name -> resolved type (ports, wires, regs, clocks, resets).
    sigs: HashMap<String, Ty<'a>>,
}

/// A (file, module name, parameter binding) triple waiting to be checked.
/// The file index disambiguates same-named modules from different files
/// (spec/02 section 1.5b) — a bare-name key would conflate two distinct
/// modules that happen to share a name and a binding.
type Config = (usize, String, Vec<(String, i128)>);

impl<'a> Checker<'a> {
    /// Pass 7 entry: check every module under its default binding, then
    /// every distinct binding discovered at instantiation sites.
    pub(super) fn check_widths(&mut self) {
        let files = self.files;

        // Function bodies are monomorphic: check each canonical fn once.
        // Also check top-level enum payload field types (E0807).
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                match item {
                    TopItem::Func(func) => {
                        let canonical = self
                            .funcs
                            .get(&func.name.name)
                            .is_some_and(|&(_, c)| std::ptr::eq(c, func));
                        if canonical {
                            self.check_func_body_widths(file, func);
                        }
                    }
                    TopItem::Enum(e) => {
                        let env = self.file_consts[file].clone();
                        let mut cx = Wcx {
                            file,
                            sc: Rc::new(Scope {
                                names: HashMap::new(),
                            }),
                            env,
                            sigs: HashMap::new(),
                        };
                        let (tag_w, max_payload_w) = self.enum_tag_and_payload_widths(&mut cx, e);
                        let total_w = if max_payload_w == 0 {
                            tag_w
                        } else {
                            tag_w + max_payload_w
                        };
                        e.inferred_total_width.set(Some(total_w as u32));
                    }
                    _ => {}
                }
            }
        }

        let mut work: Vec<Config> = Vec::new();
        // Seed in file order (deterministic diagnostics). Same-named
        // modules from different files are legal (spec/02 section 1.5b)
        // and each gets its own independent check — no "canonical" skip,
        // which would silently leave every module but the first-declared
        // one unchecked (the same bug class fixed in drivers.rs).
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                let TopItem::Module(m) = item else { continue };
                if let Some(binding) = self.default_binding(file, m, true) {
                    work.push((file, m.name.name.clone(), binding));
                }
            }
        }

        let mut done: HashSet<Config> = HashSet::new();
        let mut next = 0;
        while next < work.len() {
            let cfg = work[next].clone();
            next += 1;
            if !done.insert(cfg.clone()) {
                continue;
            }
            let Some(&(_, m)) = self
                .modules
                .get(&cfg.1)
                .and_then(|v| v.iter().find(|&&(f, _)| f == cfg.0))
            else {
                continue;
            };
            let found = self.check_module_widths(cfg.0, m, &cfg.2);
            if done.len() < MAX_CONFIGS {
                work.extend(found);
            }
        }
    }

    /// Bind every parameter of `m` to its default, left to right (a
    /// default may use earlier params). `None` if any param has no
    /// default or its default does not evaluate; `report` controls
    /// whether that eval failure becomes a diagnostic (true at the seed,
    /// false when re-derived at use sites).
    pub(super) fn default_binding(
        &mut self,
        file: usize,
        m: &'a Module,
        report: bool,
    ) -> Option<Vec<(String, i128)>> {
        let mut env = self.file_consts[file].clone();
        let mut binding = Vec::new();
        for p in &m.params {
            let d = p.default.as_ref()?;
            match consteval::eval(d, &env) {
                Ok(v) => {
                    binding.push((p.name.name.clone(), v.to_i128_saturating()));
                    env.insert(p.name.name.clone(), v);
                }
                Err(diag) => {
                    if report {
                        self.diags.push(diag.with_file(file));
                    }
                    return None;
                }
            }
        }
        Some(binding)
    }

    /// Check one module under one concrete binding. Returns the child
    /// configurations discovered at its instantiation sites.
    fn check_module_widths(
        &mut self,
        file: usize,
        m: &'a Module,
        binding: &[(String, i128)],
    ) -> Vec<Config> {
        let Some(sc) = self.scopes.get(&(file, m.name.name.clone())).cloned() else {
            return Vec::new();
        };
        let mut env = self.file_consts[file].clone();
        for (name, v) in binding {
            env.insert(name.clone(), consteval::ConstVal::from_i128(*v));
        }
        for item in &m.items {
            if let ModuleItem::Const(c) = item {
                // Eval failures were already reported by pass 6.
                if let Ok(v) = consteval::eval(&c.value, &env) {
                    env.insert(c.name.name.clone(), v);
                }
            }
        }
        let mut cx = Wcx {
            file,
            sc,
            env,
            sigs: HashMap::new(),
        };
        self.collect_sigs(&mut cx, &m.items);
        let mut found = Vec::new();
        self.walk_width_items(&mut cx, &m.items, &mut found);
        found
    }

    fn unshadow(&mut self, cx: &mut Wcx<'a>, name: &str, shadowed: Option<consteval::ConstVal>) {
        match shadowed {
            Some(v) => cx.env.insert(name.to_string(), v),
            None => cx.env.remove(name),
        };
    }

    /// Shared "this is not a plain data value" error (BUG-31: the help is
    /// branched per `Ty` variant — a learner who wrote an enum in a concat
    /// must not be told about clocks and resets). Returns `Unknown`.
    fn not_data(&mut self, cx: &mut Wcx<'a>, span: Span, t: &Ty<'a>) -> Ty<'a> {
        let help = match t {
            Ty::Enum(_) => {
                "an enum is a symbolic state, not a number — match on it, or add an \
                 explicit encoding if you need its bits"
            }
            Ty::Memory { .. } | Ty::Array { .. } => {
                "this is a whole memory/array, not a single value — index it \
                 (`m[addr]`) to get one element"
            }
            Ty::Bundle { .. } => {
                "this is a whole bundle, not a single value — access one field \
                 (`bus.field`) to get data"
            }
            _ => {
                "clocks and resets only appear in `on rise(clk)` and module \
                 connections — they never enter expressions (spec/02 section 1.2)"
            }
        };
        self.err(
            cx.file,
            span,
            "E0403",
            format!("{} is not data", show(t)),
            help,
        );
        Ty::Unknown
    }

    /// Shared "this thing has no bits" error. Returns `Unknown`.
    fn not_numeric(&mut self, cx: &mut Wcx<'a>, span: Span, t: &Ty<'a>, what: &str) -> Ty<'a> {
        self.err(
            cx.file,
            span,
            "E0407",
            format!("{what} needs a sized value, found {}", show(t)),
            "this operation works on `bit`/`bits[N]`/`signed[N]` values",
        );
        Ty::Unknown
    }
}

/// `v` is a valid bit position for a width of `n` (0 <= v < n). `n` is
/// always a checker-bounded count (a signal width or memory depth, both
/// <= `MAX_WIDTH`/`MAX_DEPTH`), so `to_i128_saturating` never actually
/// saturates here except for a `v` that's already far too large to fit —
/// exactly the "not in range" answer this function should give it.
fn fits_in_count(v: &consteval::ConstVal, n: u128) -> bool {
    !v.is_negative() && (v.to_i128_saturating() as u128) < n
}

fn max_unsigned(n: u128) -> String {
    if n >= 127 {
        format!("2^{n} - 1")
    } else {
        ((1i128 << n) - 1).to_string()
    }
}

fn min_signed(n: u128) -> String {
    if n >= 128 {
        format!("-2^{}", n - 1)
    } else {
        (-(1i128 << (n - 1))).to_string()
    }
}

fn max_signed_v(n: u128) -> String {
    if n >= 128 {
        format!("2^{} - 1", n - 1)
    } else {
        ((1i128 << (n - 1)) - 1).to_string()
    }
}

/// Extract the bundle name from an AST type (Named or parametric Bundle).
fn ast_bundle_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Named(id) => Some(&id.name.name),
        Type::Bundle { name, .. } => Some(&name.name.name),
        _ => None,
    }
}

/// Source spelling of a binary operator (for error messages).
fn op_text(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        AddWrap => "+%",
        SubWrap => "-%",
        MulWrap => "*%",
        Shl => "<<",
        Shr => ">>",
        BitAnd => "&",
        BitOr => "|",
        BitXor => "^",
        Eq => "==",
        Ne => "!=",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        LogicAnd => "&&",
        LogicOr => "||",
        Coalesce => "??",
    }
}

#[cfg(test)]
mod tests {
    use crate::{checker::check, diag::Diag, lexer, parser};

    /// Parse + run the full checker; panics if it doesn't parse (this file's
    /// other checker tests live in `checker::tests`, which does the same via
    /// its own private `parse`/`errs` helpers — this test lives here instead,
    /// self-contained, so this commit touches only `widths/mod.rs`).
    fn diags_for(src: &str) -> Vec<Diag> {
        let toks = lexer::lex(src).expect("lexes");
        let file = parser::parse(toks).expect("parses");
        check(&[file]).expect_err("expected checker errors")
    }

    #[test]
    fn sync_loop_result_init_width_checked() {
        // Body re-assigns `result` to itself (same width, no body-induced
        // error) so the ONLY possible diagnostic is the init-width check.
        let src = "module M {\n  clock clk\n  sync loop s on rise(clk) (i: 0..4) -> result: bits[4] = 999 {\n    result <- result\n  }\n}\n";
        let diags = diags_for(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code.is_some_and(|c| c.starts_with("E04"))),
            "expected an E04xx width diagnostic, got: {diags:?}"
        );
    }

    /// Final whole-branch review, Finding 2: with `lo != 0`, the loop
    /// variable's checker-recorded width must be `clog2(hi)` (the value-range
    /// formula the lowering already uses for the physical `_cnt` register —
    /// see `ast::sync_loop_lower`'s `counter_width_is_clog2_hi_not_clog2_range_when_lo_nonzero`),
    /// NOT `clog2(hi - lo)` (the iteration-count formula this file used to
    /// use). `lo=4, hi=12`: `clog2(hi)=4` bits, `clog2(hi-lo)=clog2(8)=3`
    /// bits — the two formulas disagree, so this case pins the bug. The body
    /// assigns the loop var `i` straight into the 4-bit accumulator: under
    /// the old (buggy) 3-bit typing this is a real width mismatch and the
    /// checker would reject it; under the fixed 4-bit typing it's an exact
    /// match, so the checker must accept the module with zero diagnostics.
    #[test]
    fn sync_loop_var_width_is_clog2_hi_not_clog2_range_when_lo_nonzero() {
        let src = "module M {\n  clock clk\n  sync loop s on rise(clk) (i: 4..12) -> result: bits[4] = 0 {\n    result <- i\n  }\n}\n";
        let toks = lexer::lex(src).expect("lexes");
        let file = parser::parse(toks).expect("parses");
        let res = check(&[file]);
        assert!(
            res.is_ok(),
            "expected no diagnostics (loop var must be typed bits[4] = clog2(12)), got: {:?}",
            res.err()
        );
    }

    #[test]
    fn bundle_typed_fn_param_supports_field_access() {
        // Today this falsely fails with E0105 ("h has no fields") because
        // `check_func_body_widths` never populates `bundle_sigs` for `fn`
        // params — the bug this task fixes by deleting `bundle_sigs`
        // entirely and reading real bundle info from `cx.sigs` instead.
        let src = "bundle Handshake(W: int = 8) {\n  valid: bit\n  data: bits[W]\n}\n\
               fn get_valid(h: Handshake(W: 8)) -> bit {\n  h.valid\n}\n\
               module M {\n  in a: bit\n  out y: bit\n  y = get_valid({ valid: a, data: 0 })\n}\n";
        let toks = lexer::lex(src).expect("lexes");
        let file = parser::parse(toks).expect("parses");
        let res = check(&[file]);
        assert!(
            res.is_ok(),
            "bundle-typed fn param field access should be legal, got: {:?}",
            res.err()
        );
    }

    #[test]
    fn module_param_field_access_is_rejected() {
        // `W` is a module-level `Bind::Param` (`m.params`), never a fn
        // param — it can never be bundle-typed (`ParamTy` is `Int`/`Bool`
        // only). Regression test: `field_ty`'s bundle-lookup fallback used
        // to key ONLY on `cx.sigs` presence, which never held module
        // params at all, so this silently type-checked with no diagnostic.
        let src = "module M(W: int = 8) {\n  in a: bit\n  out y: bit\n  y = a & W.foo\n}\n";
        let diags = diags_for(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code == Some("E0105") && d.msg.contains("is a parameter")),
            "expected E0105 'is a parameter' for module-param field access, got: {diags:?}"
        );
    }

    #[test]
    fn mem_field_access_reports_exactly_one_diagnostic() {
        // `my_mem` is `Bind::Mem`, present in both `cx.sc.names` (pass 6
        // diagnoses it there, correctly worded "is a memory") AND
        // `cx.sigs` (populated by `collect_sigs`). Regression test:
        // `field_ty`'s bundle-lookup fallback used to key on `cx.sigs`
        // presence alone, so it ALSO fired for mem/clock/reset — which
        // don't match its `In`/`Out`/`Wire`/`Reg` arms, so it produced a
        // second, wrongly-worded ("is a parameter") E0105 alongside pass
        // 3's correct one.
        let src = "module M {\n  in a: bit\n  out y: bit\n  mem my_mem: bits[8][16] = 0\n  \
                   y = a & my_mem.foo\n}\n";
        let diags = diags_for(src);
        let e0105s: Vec<&Diag> = diags.iter().filter(|d| d.code == Some("E0105")).collect();
        assert_eq!(
            e0105s.len(),
            1,
            "expected exactly one E0105 for mem field access, got: {diags:?}"
        );
        assert!(
            e0105s[0].msg.contains("is a memory"),
            "expected the correctly-worded 'is a memory' diagnostic, got: {:?}",
            e0105s[0].msg
        );
    }

    #[test]
    fn enum_variant_from_wrong_enum_is_rejected() {
        // Regression test: `field_ty`'s `Bind::Enum` case used to fall into
        // the wildcard arm and return `Ty::Unknown` for every `State.Red`
        // expression, which made assigning a DIFFERENT enum's variant into a
        // `State`-typed reg silently type-check (both `expect_ty` and the
        // enum-vs-enum equality check no-op on `Ty::Unknown`). `Other.X`
        // assigned into a `State`-typed reg must be a real type mismatch.
        let src = "module M {\n  clock clk\n  enum State { Red, Green }\n  \
                   enum Other { X, Y }\n  reg state: State = State.Red\n  \
                   on rise(clk) {\n    state <- Other.X\n  }\n}\n";
        let diags = diags_for(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code.is_some_and(|c| c.starts_with("E0"))),
            "expected a type-mismatch diagnostic for assigning `Other.X` into a \
             `State`-typed reg, got: {diags:?}"
        );
    }

    #[test]
    fn bundle_literal_tail_return_is_shape_checked() {
        // Pins check_func_body_widths's specific use of check_return_expr —
        // a bundle-literal TAIL (not a `return` statement) missing a
        // declared field must be rejected. Complements
        // E0901_bundle_return_missing_field.mimz (tests/fixtures/errors/),
        // which exercises the same check_return_expr function but through
        // the FnStmt::Return call site instead.
        let src = "bundle Handshake(W: int = 8) {\n  valid: bit\n  data: bits[W]\n}\n\
                   fn make_handshake(v: bit) -> Handshake(W: 8) {\n  { valid: v }\n}\n\
                   module M {\n  in a: bit\n  wire req: Handshake(W: 8) = make_handshake(a)\n  \
                   out y: bit\n  y = req.valid\n}\n";
        let diags = diags_for(src);
        assert!(
            diags.iter().any(|d| d.code == Some("E0901")),
            "expected E0901 (bundle literal missing field `data`), got: {diags:?}"
        );
    }
}
