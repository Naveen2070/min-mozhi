//! Pass 4 — width and type checking (E0401–E0410) + match
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
//! never instantiated means its internals are skipped (passes 1–3 still
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
//! File layout (split 2026-06-12, house module pattern as in parser/):
//! `mod.rs` owns the [`Ty`] model, [`Wcx`], the config worklist, and the
//! module-body walk; `expr.rs` is the bidirectional typing engine;
//! `ops.rs` types operators, concat, and builtins; `insts.rs` resolves
//! instantiation bindings and connection widths; `patterns.rs` checks
//! `match` patterns and exhaustiveness.

mod expr;
mod insts;
mod ops;
mod patterns;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{BinOp, EnumDecl, Expr, FuncDecl, Module, ModuleItem, SeqStmt, TopItem, Type};
use crate::span::Span;

use super::Checker;
use super::consteval::{self, Env};
use super::names::Scope;

/// Widths above this are rejected (E0410) — keeps `2^n` arithmetic
/// trivially safe, and no real design comes close.
const MAX_WIDTH: i128 = 1_000_000;

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
/// the AST stays untyped.
#[derive(Clone, Copy)]
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
    /// A compile-time integer: literal, const, parameter, or `repeat`
    /// variable. Polymorphic — adapts to any sized context it fits
    /// (spec/02 section 1.8). Carries the value for the fit check.
    CtInt(i128),
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
        (Ty::Enum(x), Ty::Enum(y)) => x.name.name == y.name.name,
        _ => false,
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
        Ty::CtInt(v) => format!("the compile-time value `{v}`"),
        Ty::Clock => "a clock".into(),
        Ty::Reset => "a reset".into(),
        Ty::Unknown => "an unknown type".into(),
    }
}

/// Does the compile-time value `v` fit in `n` unsigned bits?
fn fits_bits(v: i128, n: u128) -> bool {
    v >= 0 && (n >= 127 || v < (1i128 << n))
}

/// Does `v` fit in `n` two's-complement bits?
fn fits_signed(v: i128, n: u128) -> bool {
    if n >= 128 {
        return true;
    }
    let half = 1i128 << (n - 1);
    (-half..half).contains(&v)
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

/// A (module name, parameter binding) pair waiting to be checked.
type Config = (String, Vec<(String, i128)>);

impl<'a> Checker<'a> {
    /// Pass 4 entry: check every module under its default binding, then
    /// every distinct binding discovered at instantiation sites.
    pub(super) fn check_widths(&mut self) {
        let files = self.files;

        // Function bodies are monomorphic: check each canonical fn once.
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                if let TopItem::Func(func) = item {
                    let canonical = self
                        .funcs
                        .get(&func.name.name)
                        .is_some_and(|&(_, c)| std::ptr::eq(c, func));
                    if canonical {
                        self.check_func_body_widths(file, func);
                    }
                }
            }
        }

        let mut work: Vec<Config> = Vec::new();
        // Seed in file order (deterministic diagnostics), canonical
        // modules only (E0001 losers are skipped).
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                let TopItem::Module(m) = item else { continue };
                let canonical = self
                    .modules
                    .get(&m.name.name)
                    .is_some_and(|&(_, c)| std::ptr::eq(c, m));
                if !canonical {
                    continue;
                }
                if let Some(binding) = self.default_binding(file, m, true) {
                    work.push((m.name.name.clone(), binding));
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
            let Some(&(file, m)) = self.modules.get(&cfg.0) else {
                continue;
            };
            let found = self.check_module_widths(file, m, &cfg.1);
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
                    env.insert(p.name.name.clone(), v);
                    binding.push((p.name.name.clone(), v));
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
        let Some(sc) = self.scopes.get(&m.name.name).cloned() else {
            return Vec::new();
        };
        let mut env = self.file_consts[file].clone();
        for (name, v) in binding {
            env.insert(name.clone(), *v);
        }
        for item in &m.items {
            if let ModuleItem::Const(c) = item {
                // Eval failures were already reported by pass 3.
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

    /// Resolve every declared signal's type up front (declaration order
    /// in a module is free, so uses may precede declarations). This is
    /// where E0410 (bad width expression) fires.
    fn collect_sigs(&mut self, cx: &mut Wcx<'a>, items: &'a [ModuleItem]) {
        for item in items {
            match item {
                ModuleItem::Port { name, ty, .. }
                | ModuleItem::Wire { name, ty, .. }
                | ModuleItem::Reg { name, ty, .. } => {
                    let t = self.resolve_ty(cx, ty);
                    cx.sigs.insert(name.name.clone(), t);
                }
                ModuleItem::Mem {
                    name, ty, depth, ..
                } => {
                    let t = match self.resolve_ty(cx, ty) {
                        Ty::Bit => Some((1, false)),
                        Ty::Bits(n) => Some((n, false)),
                        Ty::Signed(n) => Some((n, true)),
                        Ty::Unknown => None, // width error already reported
                        other => {
                            self.err(
                                cx.file,
                                name.span,
                                "E0409",
                                format!("{} cannot be a memory element type", show(&other)),
                                "memory elements are `bit`, `bits[N]`, or `signed[N]` — \
                                 store an enum's encoding as `bits[N]` for now",
                            );
                            None
                        }
                    };
                    let resolved = match (t, self.eval_depth(cx, depth)) {
                        (Some((width, signed)), Some(d)) => Ty::Memory {
                            width,
                            signed,
                            depth: d,
                        },
                        _ => Ty::Unknown,
                    };
                    cx.sigs.insert(name.name.clone(), resolved);
                }
                ModuleItem::Clock(n) => {
                    cx.sigs.insert(n.name.clone(), Ty::Clock);
                }
                ModuleItem::Reset { name: n, .. } => {
                    cx.sigs.insert(n.name.clone(), Ty::Reset);
                }
                ModuleItem::Repeat(r) => {
                    // Types inside `repeat` resolve under a representative
                    // value (`lo`); per-iteration width EXPRESSIONS in
                    // declarations are an elaboration-slice concern.
                    let lo = consteval::eval(&r.lo, &cx.env).unwrap_or(0);
                    let shadowed = cx.env.insert(r.var.name.clone(), lo);
                    self.collect_sigs(cx, &r.items);
                    self.unshadow(cx, &r.var.name, shadowed);
                }
                _ => {}
            }
        }
    }

    fn unshadow(&mut self, cx: &mut Wcx<'a>, name: &str, shadowed: Option<i128>) {
        match shadowed {
            Some(v) => cx.env.insert(name.to_string(), v),
            None => cx.env.remove(name),
        };
    }

    /// AST type -> pass type, under the current binding.
    fn resolve_ty(&mut self, cx: &mut Wcx<'a>, ty: &'a Type) -> Ty<'a> {
        match ty {
            Type::Bit => Ty::Bit,
            Type::Bits(w) => match self.eval_width(cx, w) {
                Some(n) => bits(n),
                None => Ty::Unknown,
            },
            Type::Signed(w) => match self.eval_width(cx, w) {
                Some(n) => Ty::Signed(n),
                None => Ty::Unknown,
            },
            Type::Named(n) => match self.lookup_enum(&cx.sc, &n.name) {
                Some(e) => Ty::Enum(e),
                None => Ty::Unknown, // E0103 already reported
            },
        }
    }

    /// Like [`Self::resolve_ty`] but never reports — used when resolving
    /// a CHILD module's port type at a use site (the child's own
    /// definition check is where its declaration errors belong).
    fn resolve_ty_silent(&mut self, cx: &mut Wcx<'a>, ty: &'a Type) -> Ty<'a> {
        let before = self.diags.len();
        let t = self.resolve_ty(cx, ty);
        self.diags.truncate(before);
        t
    }

    /// Evaluate a width expression and validate the value (E0410).
    fn eval_width(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<u128> {
        match consteval::eval(e, &cx.env) {
            Ok(v) if (1..=MAX_WIDTH).contains(&v) => Some(v as u128),
            Ok(v) => {
                self.err(
                    cx.file,
                    e.span,
                    "E0410",
                    format!("`{v}` is not a valid width"),
                    format!(
                        "hardware needs at least one bit — a width must be between 1 \
                         and {MAX_WIDTH}"
                    ),
                );
                None
            }
            Err(d) => {
                self.diags.push(d.with_file(cx.file));
                None
            }
        }
    }

    /// Evaluate a memory depth expression and validate it (E0410). Like a
    /// width, a depth must be a positive compile-time constant within
    /// [`MAX_DEPTH`].
    fn eval_depth(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<u128> {
        match consteval::eval(e, &cx.env) {
            Ok(v) if (1..=MAX_DEPTH).contains(&v) => Some(v as u128),
            Ok(v) => {
                self.err(
                    cx.file,
                    e.span,
                    "E0410",
                    format!("`{v}` is not a valid memory depth"),
                    format!(
                        "a memory needs at least one cell — the depth must be between 1 \
                         and {MAX_DEPTH}"
                    ),
                );
                None
            }
            Err(d) => {
                self.diags.push(d.with_file(cx.file));
                None
            }
        }
    }

    /// Walk a module body, checking every width-bearing position.
    fn walk_width_items(
        &mut self,
        cx: &mut Wcx<'a>,
        items: &'a [ModuleItem],
        found: &mut Vec<Config>,
    ) {
        for item in items {
            match item {
                ModuleItem::Wire { name, init, .. } => {
                    let expected = cx.sigs.get(&name.name).copied().unwrap_or(Ty::Unknown);
                    self.check_expr(cx, init, expected);
                }
                ModuleItem::Reg { name, reset, .. } => {
                    let expected = cx.sigs.get(&name.name).copied().unwrap_or(Ty::Unknown);
                    self.check_expr(cx, reset, expected);
                }
                ModuleItem::Mem { name, init, .. } => {
                    // The init value seeds every cell, so it is checked against
                    // the element type.
                    let expected = match cx.sigs.get(&name.name) {
                        Some(Ty::Memory { width, signed, .. }) => {
                            if *signed {
                                Ty::Signed(*width)
                            } else {
                                bits(*width)
                            }
                        }
                        _ => Ty::Unknown,
                    };
                    self.check_expr(cx, init, expected);
                }
                ModuleItem::Drive { lhs, rhs } => {
                    let expected = self.lvalue_ty(cx, lhs);
                    self.check_expr(cx, rhs, expected);
                }
                ModuleItem::On(on) => self.seq_width_stmts(cx, &on.body),
                ModuleItem::Inst(inst) => self.check_inst_widths(cx, inst, found),
                ModuleItem::Repeat(r) => {
                    // Bounds that do not const-eval were reported by pass 3.
                    let (Ok(lo), Ok(hi)) = (
                        consteval::eval(&r.lo, &cx.env),
                        consteval::eval(&r.hi, &cx.env),
                    ) else {
                        continue;
                    };
                    let values: Vec<i128> = if hi - lo > MAX_REPEAT_CHECKS {
                        vec![lo, lo + 1, hi - 1]
                    } else {
                        (lo..hi).collect()
                    };
                    for v in values {
                        let shadowed = cx.env.insert(r.var.name.clone(), v);
                        let before = self.diags.len();
                        self.walk_width_items(cx, &r.items, found);
                        self.unshadow(cx, &r.var.name, shadowed);
                        if self.diags.len() > before {
                            break; // one iteration's worth of errors is enough
                        }
                    }
                }
                ModuleItem::Port { .. }
                | ModuleItem::Clock(_)
                | ModuleItem::Reset { .. }
                | ModuleItem::Const(_)
                | ModuleItem::Enum(_)
                | ModuleItem::Error(_) => {}
            }
        }
    }

    fn seq_width_stmts(&mut self, cx: &mut Wcx<'a>, stmts: &'a [SeqStmt]) {
        for s in stmts {
            match s {
                SeqStmt::Assign { lhs, rhs } => {
                    let expected = self.lvalue_ty(cx, lhs);
                    self.check_expr(cx, rhs, expected);
                }
                SeqStmt::If { cond, then, els } => {
                    self.check_cond(cx, cond);
                    self.seq_width_stmts(cx, then);
                    if let Some(els) = els {
                        self.seq_width_stmts(cx, els);
                    }
                }
                SeqStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// Shared "clocks/resets are not data" error. Returns `Unknown`.
    fn not_data(&mut self, cx: &mut Wcx<'a>, span: Span, t: &Ty<'a>) -> Ty<'a> {
        self.err(
            cx.file,
            span,
            "E0403",
            format!("{} is not data", show(t)),
            "clocks and resets only appear in `on rise(clk)` and module \
             connections — they never enter expressions (spec/02 section 1.2)",
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

    /// Width-check one function body (E0804). Functions are monomorphic —
    /// param types use file consts only, so each function is checked once.
    fn check_func_body_widths(&mut self, file: usize, func: &'a FuncDecl) {
        let env = self.file_consts[file].clone();
        let mut cx = Wcx {
            file,
            sc: Rc::new(super::names::Scope {
                names: HashMap::new(),
            }),
            env,
            sigs: HashMap::new(),
        };
        // Seed the signal environment with concrete param types.
        for param in &func.params {
            let ty = self.resolve_ty(&mut cx, &param.ty);
            cx.sigs.insert(param.name.name.clone(), ty);
        }
        // Fold each local let: infer width and add to sigs so subsequent
        // locals and the body can reference the name.
        for local in &func.locals {
            let ty = self.infer_ty(&mut cx, &local.value);
            cx.sigs.insert(local.name.name.clone(), ty);
        }
        // Resolve the declared return type and check the body against it.
        let ret_ty = self.resolve_ty(&mut cx, &func.ret);
        let body_ty = self.infer_ty(&mut cx, &func.body);
        match (body_ty, ret_ty) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => {}
            (Ty::CtInt(v), t) => self.fit(&mut cx, func.body.span, v, t),
            (g, t) if same(&g, &t) => {}
            (g, t) => {
                self.err(
                    cx.file,
                    func.body.span,
                    "E0804",
                    format!(
                        "function `{}` body is {}, but the declared return type is {}",
                        func.name.name,
                        show(&g),
                        show(&t)
                    ),
                    format!(
                        "the return expression must match the declared return type exactly — \
                         use `extend`, `trunc`, or a slice to resize (spec/02 section 5); \
                         the target here is {}",
                        show(&t)
                    ),
                );
            }
        }
    }

    /// Resolve a function parameter or return type under the function's
    /// file const env. Silent — the function's own body check owns any
    /// type-resolution errors. Called by the [`ExprKind::FnCall`] width
    /// handler in `widths/expr.rs` (mirrors the port-type resolution in
    /// [`Self::check_inst_widths`] / `widths/insts.rs`).
    fn fn_type_for_file(&mut self, ffile: usize, ty: &'a Type) -> Ty<'a> {
        let fenv = self.file_consts[ffile].clone();
        let mut fcx = Wcx {
            file: ffile,
            sc: Rc::new(super::names::Scope {
                names: HashMap::new(),
            }),
            env: fenv,
            sigs: HashMap::new(),
        };
        self.resolve_ty_silent(&mut fcx, ty)
    }
}

/// `v` is a valid bit position for a width of `n` (0 <= v < n).
fn fits_in_count(v: i128, n: u128) -> bool {
    v >= 0 && (v as u128) < n
}

/// Minimal unsigned width that holds `v` (>= 0). `0` and `1` need 1 bit.
fn min_bits(v: i128) -> u128 {
    (128 - v.leading_zeros()).max(1) as u128
}

/// Minimal two's-complement width that holds `v`.
fn min_signed_bits(v: i128) -> u128 {
    if v >= 0 {
        min_bits(v) + 1 // room for the sign bit
    } else {
        (129 - (!v).leading_zeros()).max(1) as u128
    }
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
    }
}
