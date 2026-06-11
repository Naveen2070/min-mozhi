//! Pass 4 — width and type checking (E0401–E0410).
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
//! Decisions (dev log 2026-06-11): `bit` ≡ `bits[1]`; lossless `+`/`-`
//! accept unequal widths (result `max+1`); `extend`/`trunc` allow the
//! no-op width (parametric code needs it at boundary bindings); `trunc`
//! keeps the LOW N bits; shift amounts are unsigned; `match` on `signed`
//! is rejected; slicing `signed` yields `bits`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{
    BinOp, Builtin, Conn, Dir, EnumDecl, Expr, ExprKind, Inst, LValue, Module, ModuleItem, Pattern,
    SeqStmt, TopItem, Type, UnOp,
};
use crate::span::Span;

use super::Checker;
use super::consteval::{self, Env};
use super::names::{Bind, Scope};

/// Widths above this are rejected (E0410) — keeps `2^n` arithmetic
/// trivially safe, and no real design comes close.
const MAX_WIDTH: i128 = 1_000_000;

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

/// One instantiation, resolved: which module, in which file, with the
/// child-side environment (file consts + parameter binding) ready for
/// evaluating the child's port types.
struct ChildBinding<'a> {
    file: usize,
    module: &'a Module,
    env: Env,
    binding: Vec<(String, i128)>,
}

impl<'a> Checker<'a> {
    /// Pass 4 entry: check every module under its default binding, then
    /// every distinct binding discovered at instantiation sites.
    pub(super) fn check_widths(&mut self) {
        let files = self.files;
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
                ModuleItem::Clock(n) => {
                    cx.sigs.insert(n.name.clone(), Ty::Clock);
                }
                ModuleItem::Reset(n) => {
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
                | ModuleItem::Reset(_)
                | ModuleItem::Const(_)
                | ModuleItem::Enum(_) => {}
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
            }
        }
    }

    /// Type of an assignment target (`name`, `name[i]`, `name[hi:lo]`).
    fn lvalue_ty(&mut self, cx: &mut Wcx<'a>, lv: &'a LValue) -> Ty<'a> {
        let base = match cx.sigs.get(&lv.base.name) {
            Some(t) => *t,
            None => return Ty::Unknown, // E0101/E0108 already reported
        };
        let Some((first, second)) = &lv.index else {
            return base;
        };
        let n = match base {
            Ty::Bit => 1,
            Ty::Bits(n) | Ty::Signed(n) => n,
            Ty::Unknown => return Ty::Unknown,
            other => {
                self.err(
                    cx.file,
                    lv.span,
                    "E0406",
                    format!("{} cannot be indexed", show(&other)),
                    "only `bits[N]` / `signed[N]` values have addressable bits",
                );
                return Ty::Unknown;
            }
        };
        match second {
            None => {
                self.index_in_range(cx, first, n);
                Ty::Bit
            }
            Some(lo) => self.slice_ty(cx, first, lo, n).unwrap_or(Ty::Unknown),
        }
    }

    /// If the index is a compile-time value, range-check it against a
    /// width of `n`. Dynamic (signal) indices pass unchecked.
    fn index_in_range(&mut self, cx: &mut Wcx<'a>, idx: &'a Expr, n: u128) {
        let t = self.infer_ty(cx, idx);
        match t {
            Ty::CtInt(v) => {
                if v < 0 || !fits_in_count(v, n) {
                    self.err(
                        cx.file,
                        idx.span,
                        "E0406",
                        format!("index `{v}` is out of range"),
                        format!("the value has {n} bits, so indices run 0..={}", n - 1),
                    );
                }
            }
            Ty::Bit | Ty::Bits(_) | Ty::Unknown => {}
            Ty::Signed(_) => self.err(
                cx.file,
                idx.span,
                "E0403",
                "a `signed` value cannot be an index",
                "indices are non-negative — cast with `unsigned(...)` first",
            ),
            other => self.err(
                cx.file,
                idx.span,
                "E0406",
                format!("{} cannot be used as an index", show(&other)),
                "an index is a compile-time value or an unsigned signal",
            ),
        }
    }

    /// `[hi:lo]` bounds: both const, `lo <= hi < n`. Returns the slice type.
    fn slice_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        hi: &'a Expr,
        lo: &'a Expr,
        n: u128,
    ) -> Option<Ty<'a>> {
        let h = self.const_bound(cx, hi)?;
        let l = self.const_bound(cx, lo)?;
        if l > h {
            self.err(
                cx.file,
                hi.span.join(lo.span),
                "E0406",
                format!("slice bounds are reversed: `[{h}:{l}]`"),
                "slices are written `[hi:lo]`, most significant bit first \
                 (spec/02 section 1.8)",
            );
            return None;
        }
        if !fits_in_count(h, n) {
            self.err(
                cx.file,
                hi.span,
                "E0406",
                format!("slice bound `{h}` is out of range"),
                format!("the value has {n} bits, so bit positions run 0..={}", n - 1),
            );
            return None;
        }
        Some(bits((h - l) as u128 + 1))
    }

    /// A slice bound: must const-evaluate and be non-negative.
    fn const_bound(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<i128> {
        match consteval::eval(e, &cx.env) {
            Ok(v) if v >= 0 => Some(v),
            Ok(v) => {
                self.err(
                    cx.file,
                    e.span,
                    "E0406",
                    format!("slice bound `{v}` is negative"),
                    "bit positions count up from 0",
                );
                None
            }
            Err(d) => {
                self.diags.push(d.with_file(cx.file));
                None
            }
        }
    }

    /// Check `e` against a context-provided type. The expected type is
    /// pushed INTO `if`/`match` arms (so each arm is checked, not the
    /// unified whole) and into compile-time integers (the fit check).
    fn check_expr(&mut self, cx: &mut Wcx<'a>, e: &'a Expr, expected: Ty<'a>) {
        if matches!(expected, Ty::Unknown) {
            let _ = self.infer_ty(cx, e); // still surface inner errors
            return;
        }
        match &e.kind {
            ExprKind::IfExpr { cond, then, els } => {
                self.check_cond(cx, cond);
                self.check_expr(cx, then, expected);
                self.check_expr(cx, els, expected);
            }
            ExprKind::Match { scrutinee, arms } => {
                let st = self.infer_ty(cx, scrutinee);
                self.check_patterns(cx, scrutinee.span, st, arms);
                for arm in arms {
                    self.check_expr(cx, &arm.value, expected);
                }
            }
            _ => {
                let got = self.infer_ty(cx, e);
                self.expect_ty(cx, e, got, expected);
            }
        }
    }

    /// `got` must match `expected` (compile-time ints get the fit check).
    fn expect_ty(&mut self, cx: &mut Wcx<'a>, e: &'a Expr, got: Ty<'a>, expected: Ty<'a>) {
        match (got, expected) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => {}
            (Ty::CtInt(v), t) => self.fit(cx, e.span, v, t),
            (g, t) if same(&g, &t) => {}
            (g, t) => {
                // The classic dropped-carry moment: `value + 1` into a
                // same-width target. Teach `+%` (spec/02 section 1.2).
                let grew_by_one = matches!(
                    (&g, &t),
                    (Ty::Bits(gw), Ty::Bits(tw)) if *gw == tw + 1
                ) || matches!((&g, &t), (Ty::Bits(2), Ty::Bit));
                let is_add_sub = matches!(
                    &e.kind,
                    ExprKind::Binary {
                        op: BinOp::Add | BinOp::Sub,
                        ..
                    }
                );
                let help = if is_add_sub && grew_by_one {
                    "`+`/`-` are lossless — the result grows one bit so the \
                     carry is never dropped. For same-width wrap-around use \
                     `+%`/`-%`; to keep the carry, widen the target by one bit \
                     (spec/02 section 1.2)"
                        .to_string()
                } else {
                    format!(
                        "widths must match exactly — nothing resizes implicitly. \
                         `extend(x, N)` widens, `trunc(x, N)` or a slice narrows \
                         (spec/02 section 1.8); the target here is {}",
                        show(&t)
                    )
                };
                self.err(
                    cx.file,
                    e.span,
                    "E0401",
                    format!("expected {}, found {}", show(&t), show(&g)),
                    help,
                );
            }
        }
    }

    /// A compile-time integer meeting a sized context: does it fit?
    fn fit(&mut self, cx: &mut Wcx<'a>, span: Span, v: i128, t: Ty<'a>) {
        match t {
            Ty::Bit | Ty::Bits(_) => {
                let n = if let Ty::Bits(n) = t { n } else { 1 };
                if v < 0 {
                    self.err(
                        cx.file,
                        span,
                        "E0405",
                        format!("`{v}` is negative, but the context is unsigned"),
                        "negative values need a `signed[N]` context \
                         (spec/02 section 1.7)",
                    );
                } else if !fits_bits(v, n) {
                    self.err(
                        cx.file,
                        span,
                        "E0405",
                        format!("`{v}` does not fit in {} bits", n),
                        format!(
                            "{} bits hold 0..={} — widen the target or shrink the \
                             value (a literal never wraps silently)",
                            n,
                            max_unsigned(n)
                        ),
                    );
                }
            }
            Ty::Signed(n) => {
                if !fits_signed(v, n) {
                    self.err(
                        cx.file,
                        span,
                        "E0405",
                        format!("`{v}` does not fit in `signed[{n}]`"),
                        format!(
                            "`signed[{n}]` holds {}..={}",
                            min_signed(n),
                            max_signed_v(n)
                        ),
                    );
                }
            }
            Ty::Enum(en) => self.err(
                cx.file,
                span,
                "E0403",
                format!("a number is not a value of enum `{}`", en.name.name),
                format!(
                    "write a variant instead: `{}.{}`",
                    en.name.name,
                    en.variants
                        .first()
                        .map(|v| v.name.as_str())
                        .unwrap_or("...")
                ),
            ),
            Ty::Clock | Ty::Reset => self.err(
                cx.file,
                span,
                "E0403",
                format!("a number cannot drive {}", show(&t)),
                "clocks and resets come from the parent module, never from data",
            ),
            Ty::CtInt(_) | Ty::Unknown => {}
        }
    }

    /// `if`/`match`/`&&` conditions must be a single bit.
    fn check_cond(&mut self, cx: &mut Wcx<'a>, e: &'a Expr) {
        let t = self.infer_ty(cx, e);
        match t {
            Ty::Bit | Ty::Unknown => {}
            Ty::CtInt(v) if v == 0 || v == 1 => {}
            other => self.err(
                cx.file,
                e.span,
                "E0404",
                format!("a condition must be a single `bit`, found {}", show(&other)),
                "compare to make a bit (`x != 0`, `x == y`) or reduce a vector \
                 (`|x` is 1 when any bit is set)",
            ),
        }
    }

    /// Synthesize an expression's type bottom-up.
    fn infer_ty(&mut self, cx: &mut Wcx<'a>, e: &'a Expr) -> Ty<'a> {
        match &e.kind {
            ExprKind::Int { value, .. } => match i128::try_from(*value) {
                Ok(v) => Ty::CtInt(v),
                Err(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0405",
                        "literal is too large",
                        "values up to 2^127 - 1 are supported",
                    );
                    Ty::Unknown
                }
            },
            ExprKind::Bool(_) => Ty::Bit,
            ExprKind::Ident(name) => self.ident_ty(cx, e.span, name),
            ExprKind::Field { base, field } => self.field_ty(cx, base, field),
            ExprKind::Unary { op, expr } => self.unary_ty(cx, e, *op, expr),
            ExprKind::Binary { op, lhs, rhs } => self.binary_ty(cx, e, *op, lhs, rhs),
            ExprKind::IfExpr { cond, then, els } => {
                self.check_cond(cx, cond);
                let tt = self.infer_ty(cx, then);
                let et = self.infer_ty(cx, els);
                self.unify_arms(cx, e.span, &[(then.span, tt), (els.span, et)])
            }
            ExprKind::Match { scrutinee, arms } => {
                let st = self.infer_ty(cx, scrutinee);
                self.check_patterns(cx, scrutinee.span, st, arms);
                let arm_tys: Vec<(Span, Ty<'a>)> = arms
                    .iter()
                    .map(|a| (a.value.span, self.infer_ty(cx, &a.value)))
                    .collect();
                self.unify_arms(cx, e.span, &arm_tys)
            }
            ExprKind::Concat(parts) => self.concat_ty(cx, parts),
            ExprKind::Index { base, index } => {
                let bt = self.infer_ty(cx, base);
                let n = match bt {
                    Ty::Bit => 1,
                    Ty::Bits(n) | Ty::Signed(n) => n,
                    Ty::Unknown => return Ty::Unknown,
                    other => {
                        self.err(
                            cx.file,
                            base.span,
                            "E0406",
                            format!("{} cannot be indexed", show(&other)),
                            "only `bits[N]` / `signed[N]` values have addressable bits",
                        );
                        return Ty::Unknown;
                    }
                };
                self.index_in_range(cx, index, n);
                Ty::Bit
            }
            ExprKind::Slice { base, hi, lo } => {
                let bt = self.infer_ty(cx, base);
                let n = match bt {
                    Ty::Bit => 1,
                    Ty::Bits(n) | Ty::Signed(n) => n,
                    Ty::Unknown => return Ty::Unknown,
                    other => {
                        self.err(
                            cx.file,
                            base.span,
                            "E0406",
                            format!("{} cannot be sliced", show(&other)),
                            "only `bits[N]` / `signed[N]` values have addressable bits",
                        );
                        return Ty::Unknown;
                    }
                };
                // Slicing yields raw bits even from `signed` (decision).
                self.slice_ty(cx, hi, lo, n).unwrap_or(Ty::Unknown)
            }
            ExprKind::Call { func, args } => self.call_ty(cx, e, *func, args),
        }
    }

    /// What a bare name means as a VALUE in this module.
    fn ident_ty(&mut self, cx: &mut Wcx<'a>, span: Span, name: &str) -> Ty<'a> {
        if let Some(t) = cx.sigs.get(name) {
            return *t;
        }
        if let Some(v) = cx.env.get(name) {
            return Ty::CtInt(*v);
        }
        match cx.sc.names.get(name) {
            Some(Bind::Inst(_)) => {
                self.err(
                    cx.file,
                    span,
                    "E0403",
                    format!("`{name}` is an instance, not a value"),
                    format!("read one of its outputs instead: `{name}.port`"),
                );
                Ty::Unknown
            }
            Some(Bind::Enum(en)) => {
                self.err(
                    cx.file,
                    span,
                    "E0403",
                    format!("`{name}` is an enum TYPE, not a value"),
                    format!(
                        "pick a variant: `{name}.{}`",
                        en.variants
                            .first()
                            .map(|v| v.name.as_str())
                            .unwrap_or("...")
                    ),
                );
                Ty::Unknown
            }
            // Param/Const whose value failed to evaluate (reported), or a
            // name pass 3 already flagged as unknown (E0101).
            _ => Ty::Unknown,
        }
    }

    /// `base.field` — enum variant or instance output (mirrors pass 3's
    /// resolution; here we only need the TYPE).
    fn field_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        base: &'a Expr,
        field: &'a crate::ast::Ident,
    ) -> Ty<'a> {
        let core = match &base.kind {
            ExprKind::Index { base: b, .. } if matches!(b.kind, ExprKind::Ident(_)) => b,
            _ => base,
        };
        let ExprKind::Ident(name) = &core.kind else {
            return Ty::Unknown; // E0105 already reported
        };
        match cx.sc.names.get(name) {
            Some(Bind::Inst(inst)) => self.inst_output_ty(cx, inst, field),
            _ => match self.lookup_enum(&cx.sc, name) {
                Some(en) if en.variants.iter().any(|v| v.name == field.name) => Ty::Enum(en),
                _ => Ty::Unknown, // E0103 already reported
            },
        }
    }

    /// The width of `inst.output` in the parent: the child's port type,
    /// evaluated under this instantiation's parameter binding. Resolution
    /// is silent — the child's own config check owns its errors.
    fn inst_output_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        inst: &'a Inst,
        field: &'a crate::ast::Ident,
    ) -> Ty<'a> {
        let Some(child) = self.child_binding(cx, inst, false) else {
            return Ty::Unknown;
        };
        let Some(csc) = self.scopes.get(&child.module.name.name).cloned() else {
            return Ty::Unknown;
        };
        for item in &child.module.items {
            if let ModuleItem::Port {
                dir: Dir::Out,
                name,
                ty,
            } = item
                && name.name == field.name
            {
                let mut ccx = Wcx {
                    file: child.file,
                    sc: csc,
                    env: child.env,
                    sigs: HashMap::new(),
                };
                return self.resolve_ty_silent(&mut ccx, ty);
            }
        }
        Ty::Unknown // E0104 already reported
    }

    /// Bind the child's parameters for one instantiation: explicit args
    /// evaluate in the PARENT's env; omitted ones take their defaults
    /// (child env, left to right). Returns the child's file, module, env
    /// (file consts + binding), and the binding itself.
    fn child_binding(
        &mut self,
        cx: &Wcx<'a>,
        inst: &'a Inst,
        report: bool,
    ) -> Option<ChildBinding<'a>> {
        let &(cfile, cm) = self.modules.get(&inst.module.name)?;
        let mut cenv = self.file_consts[cfile].clone();
        let mut binding = Vec::new();
        for p in &cm.params {
            let arg = inst.args.iter().find(|a| a.name.name == p.name.name);
            let mut v = None;
            if let Some(arg) = arg {
                match consteval::eval(&arg.value, &cx.env) {
                    Ok(x) => v = Some(x),
                    Err(d) => {
                        if report {
                            self.diags.push(d.with_file(cx.file));
                        }
                    }
                }
            }
            if v.is_none()
                && let Some(d) = &p.default
            {
                v = consteval::eval(d, &cenv).ok();
            }
            let v = v?;
            cenv.insert(p.name.name.clone(), v);
            binding.push((p.name.name.clone(), v));
        }
        Some(ChildBinding {
            file: cfile,
            module: cm,
            env: cenv,
            binding,
        })
    }

    /// Width-check one instantiation: every connection against the
    /// child's port type under THIS binding, then enqueue the child
    /// config so its internals are checked under it too.
    fn check_inst_widths(&mut self, cx: &mut Wcx<'a>, inst: &'a Inst, found: &mut Vec<Config>) {
        let Some(child) = self.child_binding(cx, inst, true) else {
            return;
        };
        let cm = child.module;
        let csc = self.scopes.get(&cm.name.name).cloned();
        for Conn { port, signal } in &inst.conns {
            let mut expected = Ty::Unknown; // unknown/output ports: E0107 owns it
            for item in &cm.items {
                match item {
                    ModuleItem::Port {
                        dir: Dir::In,
                        name,
                        ty,
                    } if name.name == port.name => {
                        if let Some(csc) = &csc {
                            let mut ccx = Wcx {
                                file: child.file,
                                sc: csc.clone(),
                                env: child.env.clone(),
                                sigs: HashMap::new(),
                            };
                            expected = self.resolve_ty_silent(&mut ccx, ty);
                        }
                    }
                    ModuleItem::Clock(n) if n.name == port.name => expected = Ty::Clock,
                    ModuleItem::Reset(n) if n.name == port.name => expected = Ty::Reset,
                    _ => {}
                }
            }
            match expected {
                Ty::Unknown => {
                    let _ = self.infer_ty(cx, signal);
                }
                t => {
                    let got = self.infer_ty(cx, signal);
                    if let (Ty::Unknown, _) | (_, Ty::Unknown) = (got, t) {
                        continue;
                    }
                    if let Ty::CtInt(v) = got {
                        self.fit(cx, signal.span, v, t);
                        continue;
                    }
                    if !same(&got, &t) {
                        self.err(
                            cx.file,
                            signal.span,
                            "E0401",
                            format!(
                                "`{}`'s port `{}` is {}, but this connection is {}",
                                cm.name.name,
                                port.name,
                                show(&t),
                                show(&got)
                            ),
                            "widths must match exactly at module boundaries — \
                             `extend`/`trunc`/slice the signal, or change the \
                             parameter this width comes from",
                        );
                    }
                }
            }
        }
        found.push((cm.name.name.clone(), child.binding));
    }

    fn unary_ty(&mut self, cx: &mut Wcx<'a>, e: &'a Expr, op: UnOp, inner: &'a Expr) -> Ty<'a> {
        let t = self.infer_ty(cx, inner);
        if matches!(t, Ty::Unknown) {
            return Ty::Unknown;
        }
        if matches!(t, Ty::Clock | Ty::Reset) {
            return self.not_data(cx, inner.span, &t);
        }
        if let Ty::CtInt(_) = t {
            // Pure compile-time: fold (consteval explains what it rejects,
            // e.g. `~` has no width on an unsized value).
            return match consteval::eval(e, &cx.env) {
                Ok(v) => Ty::CtInt(v),
                Err(d) => {
                    self.diags.push(d.with_file(cx.file));
                    Ty::Unknown
                }
            };
        }
        match op {
            UnOp::Neg => match t {
                Ty::Signed(n) => Ty::Signed(n + 1), // lossless: gains the carry bit
                other => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`-` needs a `signed` value, found {}", show(&other)),
                        "negation is signed-only (spec/02 section 1.7) — use \
                         `0 -% x` for two's-complement wrap, or cast with \
                         `signed(x)` first",
                    );
                    Ty::Unknown
                }
            },
            UnOp::BitNot => match t {
                Ty::Bit | Ty::Bits(_) | Ty::Signed(_) => t,
                other => self.not_numeric(cx, e.span, &other, "`~`"),
            },
            UnOp::LogicNot => match t {
                Ty::Bit => Ty::Bit,
                other => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0404",
                        format!("`!` works on a single `bit`, found {}", show(&other)),
                        "make a bit first: compare (`x == 0`) or reduce (`|x`)",
                    );
                    Ty::Unknown
                }
            },
            UnOp::RedAnd | UnOp::RedOr | UnOp::RedXor => match t {
                Ty::Bit | Ty::Bits(_) => Ty::Bit,
                Ty::Signed(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0403",
                        "reductions work on `bits`, not `signed`",
                        "cast first: `|unsigned(x)` (spec/02 section 3)",
                    );
                    Ty::Unknown
                }
                other => self.not_numeric(cx, e.span, &other, "a reduction"),
            },
        }
    }

    fn binary_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        e: &'a Expr,
        op: BinOp,
        lhs: &'a Expr,
        rhs: &'a Expr,
    ) -> Ty<'a> {
        let lt = self.infer_ty(cx, lhs);
        let rt = self.infer_ty(cx, rhs);
        if matches!(lt, Ty::Unknown) || matches!(rt, Ty::Unknown) {
            return Ty::Unknown;
        }
        for (t, side) in [(&lt, lhs), (&rt, rhs)] {
            if matches!(t, Ty::Clock | Ty::Reset) {
                return self.not_data(cx, side.span, t);
            }
        }
        if let (Ty::CtInt(_), Ty::CtInt(_)) = (lt, rt) {
            // Pure compile-time: fold the whole node (consteval rejects
            // what genuinely has no compile-time meaning, e.g. `+%`).
            return match consteval::eval(e, &cx.env) {
                Ok(v) => Ty::CtInt(v),
                Err(d) => {
                    self.diags.push(d.with_file(cx.file));
                    Ty::Unknown
                }
            };
        }
        use BinOp::*;
        match op {
            Add | Sub | Mul => self.lossless_ty(cx, e, op, (lhs, lt), (rhs, rt)),
            AddWrap | SubWrap | MulWrap => self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt)),
            BitAnd | BitOr | BitXor => self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt)),
            Shl | Shr => self.shift_ty(cx, (lhs, lt), (rhs, rt)),
            Eq | Ne => {
                if let (Ty::Enum(a), Ty::Enum(b)) = (&lt, &rt) {
                    if a.name.name != b.name.name {
                        self.err(
                            cx.file,
                            e.span,
                            "E0403",
                            format!(
                                "cannot compare enum `{}` with enum `{}`",
                                a.name.name, b.name.name
                            ),
                            "only values of the SAME enum compare",
                        );
                    }
                    return Ty::Bit;
                }
                let _ = self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt));
                Ty::Bit
            }
            Lt | Le | Gt | Ge => {
                if matches!(lt, Ty::Enum(_)) || matches!(rt, Ty::Enum(_)) {
                    self.err(
                        cx.file,
                        e.span,
                        "E0403",
                        "enums have no order",
                        "an enum's binary encoding is a compiler detail — compare \
                         with `==`/`!=`, or model an ordered quantity as `bits[N]`",
                    );
                    return Ty::Bit;
                }
                let _ = self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt));
                Ty::Bit
            }
            LogicAnd | LogicOr => {
                for (t, side) in [(&lt, lhs), (&rt, rhs)] {
                    match t {
                        Ty::Bit => {}
                        Ty::CtInt(v) if *v == 0 || *v == 1 => {}
                        other => self.err(
                            cx.file,
                            side.span,
                            "E0404",
                            format!(
                                "`{}` works on single bits, found {}",
                                op_text(op),
                                show(other)
                            ),
                            "logical operators have no C-style truthiness — compare \
                             (`x != 0`) or reduce (`|x`) to make a bit first",
                        ),
                    }
                }
                Ty::Bit
            }
        }
    }

    /// `+`/`-`/`*` — lossless growth. Operand widths may differ (the
    /// result can never drop information); signedness must match. A
    /// compile-time operand takes the other side's width if it fits,
    /// otherwise its own minimal width (growing is always safe here).
    fn lossless_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        e: &'a Expr,
        op: BinOp,
        (lhs, lt): (&'a Expr, Ty<'a>),
        (rhs, rt): (&'a Expr, Ty<'a>),
    ) -> Ty<'a> {
        let _ = e;
        let (a, b) = match (lt, rt) {
            (Ty::CtInt(v), t) => {
                let Some(adapted) = self.adapt_lossless(cx, lhs.span, v, &t) else {
                    return Ty::Unknown;
                };
                (adapted, t)
            }
            (t, Ty::CtInt(v)) => {
                let Some(adapted) = self.adapt_lossless(cx, rhs.span, v, &t) else {
                    return Ty::Unknown;
                };
                (t, adapted)
            }
            (a, b) => (a, b),
        };
        let widths = match (&a, &b) {
            (Ty::Bit, Ty::Bit) => Some((1, 1, false)),
            (Ty::Bit, Ty::Bits(n)) | (Ty::Bits(n), Ty::Bit) => Some((1, *n, false)),
            (Ty::Bits(x), Ty::Bits(y)) => Some((*x, *y, false)),
            (Ty::Signed(x), Ty::Signed(y)) => Some((*x, *y, true)),
            _ => None,
        };
        let Some((x, y, signed)) = widths else {
            self.err(
                cx.file,
                lhs.span.join(rhs.span),
                "E0403",
                format!("`{}` cannot mix {} and {}", op_text(op), show(&a), show(&b)),
                "`signed` and `bits` never mix in an operator — convert \
                 visibly with `signed(x)` / `unsigned(x)` (spec/02 section 1.7)",
            );
            return Ty::Unknown;
        };
        let w = match op {
            BinOp::Mul => x + y,
            _ => x.max(y) + 1,
        };
        if signed { Ty::Signed(w) } else { bits(w) }
    }

    /// A compile-time operand of a lossless op: prefer the other side's
    /// width; if the value doesn't fit there, take its own minimal width
    /// (lossless growth makes that safe). Negative values need `signed`.
    fn adapt_lossless(
        &mut self,
        cx: &mut Wcx<'a>,
        span: Span,
        v: i128,
        other: &Ty<'a>,
    ) -> Option<Ty<'a>> {
        match other {
            Ty::Bit | Ty::Bits(_) => {
                if v < 0 {
                    self.fit(cx, span, v, *other); // reports the negative case
                    return None;
                }
                let n = if let Ty::Bits(n) = other { *n } else { 1 };
                Some(bits(n.max(min_bits(v))))
            }
            Ty::Signed(n) => Some(Ty::Signed((*n).max(min_signed_bits(v)))),
            _ => {
                self.fit(cx, span, v, *other);
                None
            }
        }
    }

    /// The width-matching operators (`+%` family, bitwise, comparisons):
    /// both sides the same kind and width; a compile-time operand adapts
    /// to the sized side (and must fit). Returns the common type.
    fn matched_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        op: &str,
        (lhs, lt): (&'a Expr, Ty<'a>),
        (rhs, rt): (&'a Expr, Ty<'a>),
    ) -> Ty<'a> {
        let (a, b) = match (lt, rt) {
            (Ty::CtInt(v), t) => {
                self.fit(cx, lhs.span, v, t);
                return t;
            }
            (t, Ty::CtInt(v)) => {
                self.fit(cx, rhs.span, v, t);
                return t;
            }
            (a, b) => (a, b),
        };
        if same(&a, &b) {
            if matches!(a, Ty::Enum(_)) {
                self.err(
                    cx.file,
                    lhs.span.join(rhs.span),
                    "E0403",
                    format!("`{op}` does not work on enum values"),
                    "enums only compare with `==`/`!=` and drive `match`",
                );
                return Ty::Unknown;
            }
            return a;
        }
        let kinds_differ = matches!(
            (&a, &b),
            (Ty::Signed(_), Ty::Bit | Ty::Bits(_)) | (Ty::Bit | Ty::Bits(_), Ty::Signed(_))
        ) || matches!((&a, &b), (Ty::Enum(_), _) | (_, Ty::Enum(_)));
        if kinds_differ {
            self.err(
                cx.file,
                lhs.span.join(rhs.span),
                "E0403",
                format!("`{op}` cannot mix {} and {}", show(&a), show(&b)),
                "`signed` and `bits` never mix in an operator — convert \
                 visibly with `signed(x)` / `unsigned(x)` (spec/02 section 1.7)",
            );
        } else {
            self.err(
                cx.file,
                lhs.span.join(rhs.span),
                "E0402",
                format!(
                    "`{op}` needs equal widths, found {} and {}",
                    show(&a),
                    show(&b)
                ),
                "this operator works bit-for-bit, so both sides must be the \
                 same width — `extend(x, N)` the narrow side, or slice the \
                 wide one (spec/02 section 3)",
            );
        }
        Ty::Unknown
    }

    /// `<<`/`>>`: the result keeps the LEFT operand's type; the amount is
    /// a compile-time value or an unsigned signal.
    fn shift_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        (lhs, lt): (&'a Expr, Ty<'a>),
        (rhs, rt): (&'a Expr, Ty<'a>),
    ) -> Ty<'a> {
        match rt {
            Ty::CtInt(v) if v < 0 => {
                self.err(
                    cx.file,
                    rhs.span,
                    "E0405",
                    format!("shift amount `{v}` is negative"),
                    "shift amounts count bits, so they are 0 or more",
                );
                return Ty::Unknown;
            }
            Ty::CtInt(_) | Ty::Bit | Ty::Bits(_) => {}
            Ty::Signed(_) => {
                self.err(
                    cx.file,
                    rhs.span,
                    "E0403",
                    "a shift amount cannot be `signed`",
                    "shift amounts are non-negative — cast with `unsigned(x)`",
                );
                return Ty::Unknown;
            }
            other => {
                self.err(
                    cx.file,
                    rhs.span,
                    "E0403",
                    format!("{} cannot be a shift amount", show(&other)),
                    "shift by a number or an unsigned signal",
                );
                return Ty::Unknown;
            }
        }
        match lt {
            Ty::Bit | Ty::Bits(_) | Ty::Signed(_) => lt, // width preserved (spec/02 section 3)
            Ty::CtInt(_) => {
                self.err(
                    cx.file,
                    lhs.span,
                    "E0405",
                    "shifting a bare literal has no width to preserve",
                    "give it one first: `extend(1, N) << k`, or shift a sized \
                     signal",
                );
                Ty::Unknown
            }
            other => self.not_numeric(cx, lhs.span, &other, "a shift"),
        }
    }

    /// `{a, b, c}` — every part sized `bits` (or `bit`); result is the sum.
    fn concat_ty(&mut self, cx: &mut Wcx<'a>, parts: &'a [Expr]) -> Ty<'a> {
        let mut sum: u128 = 0;
        let mut ok = true;
        for p in parts {
            let t = self.infer_ty(cx, p);
            match t {
                Ty::Bit => sum += 1,
                Ty::Bits(n) => sum += n,
                Ty::Unknown => ok = false,
                Ty::Signed(_) => {
                    self.err(
                        cx.file,
                        p.span,
                        "E0403",
                        "`signed` values do not concatenate directly",
                        "concatenation is bit-jugglery — make the intent visible \
                         with `unsigned(x)` first",
                    );
                    ok = false;
                }
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        p.span,
                        "E0405",
                        "a bare literal has no width inside `{...}`",
                        "every concat part needs a known width — `extend(1, N)` \
                         gives a literal one",
                    );
                    ok = false;
                }
                other => {
                    let _ = self.not_data(cx, p.span, &other);
                    ok = false;
                }
            }
        }
        if ok { bits(sum) } else { Ty::Unknown }
    }

    /// The four builtins (spec/02 sections 1.7–1.8).
    fn call_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        e: &'a Expr,
        func: Builtin,
        args: &'a [Expr],
    ) -> Ty<'a> {
        let Some(x) = args.first() else {
            return Ty::Unknown; // parser enforces arity
        };
        let xt = self.infer_ty(cx, x);
        if matches!(xt, Ty::Unknown) {
            return Ty::Unknown;
        }
        match func {
            Builtin::Extend | Builtin::Trunc => {
                let Some(narg) = args.get(1) else {
                    return Ty::Unknown;
                };
                let Some(n) = self.eval_width(cx, narg) else {
                    return Ty::Unknown;
                };
                let name = if func == Builtin::Extend {
                    "extend"
                } else {
                    "trunc"
                };
                let m = match xt {
                    Ty::Bit => 1,
                    Ty::Bits(w) | Ty::Signed(w) => w,
                    Ty::CtInt(v) => {
                        // `extend(1, N)` is the idiom for giving a literal an
                        // explicit width; trunc of a literal is confusion.
                        if func == Builtin::Extend {
                            self.fit(cx, x.span, v, bits(n));
                            return bits(n);
                        }
                        self.err(
                            cx.file,
                            e.span,
                            "E0407",
                            "`trunc` of a bare literal does nothing useful",
                            "literals adapt to their context automatically — just \
                             write the smaller value",
                        );
                        return Ty::Unknown;
                    }
                    other => return self.not_numeric(cx, x.span, &other, name),
                };
                if func == Builtin::Extend && n < m {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`extend` to {n} bits would NARROW a {m}-bit value"),
                        "`extend(x, N)` only widens (N >= the current width) — \
                         to drop bits, say so with `trunc(x, N)` or a slice",
                    );
                    return Ty::Unknown;
                }
                if func == Builtin::Trunc && n > m {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`trunc` to {n} bits would WIDEN a {m}-bit value"),
                        "`trunc(x, N)` only narrows (it keeps the low N bits) — \
                         to add bits, say so with `extend(x, N)`",
                    );
                    return Ty::Unknown;
                }
                match xt {
                    Ty::Signed(_) => Ty::Signed(n),
                    _ => bits(n),
                }
            }
            Builtin::SignedCast => match xt {
                Ty::Bit => Ty::Signed(1),
                Ty::Bits(n) => Ty::Signed(n),
                Ty::Signed(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "this value is already `signed`",
                        "`signed(x)` reinterprets `bits` as `signed` — applying \
                         it twice means one of them is a mistake",
                    );
                    Ty::Unknown
                }
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "literals do not need a `signed(...)` cast",
                        "a literal already adapts to signed contexts — write it \
                         where the `signed[N]` is",
                    );
                    Ty::Unknown
                }
                other => self.not_numeric(cx, x.span, &other, "`signed`"),
            },
            Builtin::UnsignedCast => match xt {
                Ty::Signed(n) => bits(n),
                Ty::Bit | Ty::Bits(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "this value is already unsigned",
                        "`unsigned(x)` reinterprets `signed` as `bits` — this one \
                         was never signed",
                    );
                    Ty::Unknown
                }
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "literals do not need an `unsigned(...)` cast",
                        "a literal already adapts to its context",
                    );
                    Ty::Unknown
                }
                other => self.not_numeric(cx, x.span, &other, "`unsigned`"),
            },
        }
    }

    /// `if`/`match` arms must agree. A compile-time arm adapts to a sized
    /// sibling; ALL-compile-time arms have no width to adopt in a
    /// width-free position.
    fn unify_arms(&mut self, cx: &mut Wcx<'a>, whole: Span, arms: &[(Span, Ty<'a>)]) -> Ty<'a> {
        let mut acc: Option<Ty<'a>> = None;
        for (_, t) in arms {
            if matches!(t, Ty::Unknown) {
                return Ty::Unknown;
            }
            if !matches!(t, Ty::CtInt(_)) {
                match &acc {
                    None => acc = Some(*t),
                    Some(prev) if same(prev, t) => {}
                    Some(prev) => {
                        self.err(
                            cx.file,
                            whole,
                            "E0408",
                            format!("the arms disagree: {} vs {}", show(prev), show(t)),
                            "every arm becomes the same wire, so all arms must \
                             have one type and width — `extend`/`trunc` the odd \
                             one out",
                        );
                        return Ty::Unknown;
                    }
                }
            }
        }
        let Some(result) = acc else {
            self.err(
                cx.file,
                whole,
                "E0405",
                "every arm is a bare literal, so this has no width",
                "use it where a width is known (an assignment or connection), \
                 or give one arm a width with `extend(value, N)`",
            );
            return Ty::Unknown;
        };
        // Now fit every compile-time arm against the agreed type.
        for (span, t) in arms {
            if let Ty::CtInt(v) = t {
                self.fit(cx, *span, *v, result);
            }
        }
        result
    }

    /// `match` patterns against the scrutinee's type (E0409).
    fn check_patterns(
        &mut self,
        cx: &mut Wcx<'a>,
        scrutinee: Span,
        st: Ty<'a>,
        arms: &'a [crate::ast::Arm],
    ) {
        match st {
            Ty::Unknown => {}
            Ty::Signed(_) => self.err(
                cx.file,
                scrutinee,
                "E0409",
                "cannot `match` on a `signed` value",
                "patterns cannot express negative numbers yet — match on \
                 `unsigned(x)` and compare signs separately",
            ),
            Ty::CtInt(_) => self.err(
                cx.file,
                scrutinee,
                "E0405",
                "`match` needs a sized value to scrutinize",
                "a bare compile-time value has no width — match on a signal, \
                 or decide with `if`/`else` at compile time",
            ),
            Ty::Clock | Ty::Reset => {
                let _ = self.not_data(cx, scrutinee, &st);
            }
            Ty::Bit | Ty::Bits(_) => {
                let n = if let Ty::Bits(n) = st { n } else { 1 };
                for arm in arms {
                    for p in &arm.patterns {
                        match p {
                            Pattern::Int { value, raw } => {
                                let v = i128::try_from(*value).unwrap_or(i128::MAX);
                                if !fits_bits(v, n) {
                                    self.err(
                                        cx.file,
                                        arm.value.span,
                                        "E0409",
                                        format!("pattern `{raw}` does not fit in {n} bits"),
                                        format!(
                                            "the matched value is {} — it can never \
                                             equal `{raw}`, so this arm is dead",
                                            show(&st)
                                        ),
                                    );
                                }
                            }
                            Pattern::Bool(_) => {
                                if n != 1 {
                                    self.err(
                                        cx.file,
                                        arm.value.span,
                                        "E0409",
                                        format!(
                                            "`true`/`false` patterns need a `bit`, not {}",
                                            show(&st)
                                        ),
                                        "match multi-bit values against numbers",
                                    );
                                }
                            }
                            Pattern::Variant { enum_name, .. } => self.err(
                                cx.file,
                                enum_name.span,
                                "E0409",
                                format!("variant pattern on {}", show(&st)),
                                "enum patterns match enum values — this scrutinee \
                                 is a plain vector, match numbers instead",
                            ),
                            Pattern::Wildcard => {}
                        }
                    }
                }
            }
            Ty::Enum(en) => {
                for arm in arms {
                    for p in &arm.patterns {
                        match p {
                            Pattern::Variant { enum_name, .. } => {
                                if enum_name.name != en.name.name {
                                    self.err(
                                        cx.file,
                                        enum_name.span,
                                        "E0409",
                                        format!(
                                            "pattern is from enum `{}`, but the value is \
                                             enum `{}`",
                                            enum_name.name, en.name.name
                                        ),
                                        format!("use `{}.<variant>` patterns here", en.name.name),
                                    );
                                }
                            }
                            Pattern::Int { raw, .. } => self.err(
                                cx.file,
                                arm.value.span,
                                "E0409",
                                format!("number pattern `{raw}` on enum `{}`", en.name.name),
                                "an enum's encoding is a compiler detail — match \
                                 its variants by name",
                            ),
                            Pattern::Bool(_) => self.err(
                                cx.file,
                                arm.value.span,
                                "E0409",
                                format!("`true`/`false` pattern on enum `{}`", en.name.name),
                                "match the enum's variants by name",
                            ),
                            Pattern::Wildcard => {}
                        }
                    }
                }
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
