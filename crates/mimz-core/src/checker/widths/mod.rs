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

use crate::ast::{
    BinOp, EnumDecl, Expr, ExprKind, FieldInit, FnStmt, FuncDecl, Module, ModuleItem, NamedArg,
    Pattern, SeqStmt, TopItem, Type,
};
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
    /// signal name -> original AST type, for bundle-typed signals only.
    /// Used to recover bundle name/args after `resolve_ty` returns Unknown.
    bundle_sigs: HashMap<String, &'a Type>,
}

/// A (file, module name, parameter binding) triple waiting to be checked.
/// The file index disambiguates same-named modules from different files
/// (spec/02 section 1.5b) — a bare-name key would conflate two distinct
/// modules that happen to share a name and a binding.
type Config = (usize, String, Vec<(String, i128)>);

impl<'a> Checker<'a> {
    /// Pass 4 entry: check every module under its default binding, then
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
                            bundle_sigs: HashMap::new(),
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
        let Some(sc) = self.scopes.get(&(file, m.name.name.clone())).cloned() else {
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
            bundle_sigs: HashMap::new(),
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
                    if matches!(t, Ty::Unknown) && self.is_bundle_ty(ty) {
                        cx.bundle_sigs.insert(name.name.clone(), ty);
                    }
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
                ModuleItem::SyncLoop(sl) => {
                    let result_t = self.resolve_ty(cx, &sl.result_ty);
                    cx.sigs.insert(format!("{}_start", sl.name.name), Ty::Bit);
                    cx.sigs.insert(format!("{}_done", sl.name.name), Ty::Bit);
                    cx.sigs.insert(format!("{}_result", sl.name.name), result_t);
                    cx.sigs.insert(format!("{}_running", sl.name.name), Ty::Bit);
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
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &cx.env).unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.collect_sigs(cx, branch);
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
            // ponytail: bundle type → Unknown until T6 wires full bundle width model
            Type::Bundle { .. } => Ty::Unknown,
            Type::Named(n) => match self.lookup_enum(&cx.sc, &n.name.name) {
                Some(e) => Ty::Enum(e),
                // E0103/E0906 already reported, or bundle name (T6 will handle)
                None => Ty::Unknown,
            },
            Type::Array { elem, len } => {
                // A bundle-named element resolves to `Ty::Unknown` SILENTLY
                // (see `Type::Named`/`Type::Bundle` arms above — bundle width
                // resolution is deferred to Task 6, so no diagnostic fires
                // there). Left unchecked, that would make this whole array
                // type silently `Unknown` with no E0411 ever reported. Catch
                // it here, before resolving, using the same `is_bundle_ty`
                // check `collect_sigs` already uses to detect bundle-typed
                // signals.
                if self.is_bundle_ty(elem) {
                    let bname = ast_bundle_name(elem).unwrap_or("?");
                    self.err(
                        cx.file,
                        len.span,
                        "E0411",
                        format!("bundle `{bname}` cannot be an array element type"),
                        "array elements are `bit`, `bits[N]`, or `signed[N]` — \
                         nested arrays and enum/bundle elements are not supported in v1",
                    );
                    return Ty::Unknown;
                }
                let elem_ty = self.resolve_ty(cx, elem);
                let (elem_width, elem_signed) = match elem_ty {
                    Ty::Bit => (1, false),
                    Ty::Bits(n) => (n, false),
                    Ty::Signed(n) => (n, true),
                    Ty::Unknown => return Ty::Unknown, // element error already reported
                    other => {
                        // `Type` (the AST node) has no span of its own — `elem`
                        // is a bare `Box<Type>` (see ast/mod.rs's `Type::Array`
                        // doc comment). `len` is the only span-bearing part of
                        // this `Type::Array` node available at this call site,
                        // so it is the best available anchor for the diagnostic
                        // (mirrors E0409's approach of pointing at whatever
                        // span is actually in scope, there the memory name's).
                        self.err(
                            cx.file,
                            len.span,
                            "E0411",
                            format!("{} cannot be an array element type", show(&other)),
                            "array elements are `bit`, `bits[N]`, or `signed[N]` — \
                             nested arrays and enum/bundle elements are not supported in v1",
                        );
                        return Ty::Unknown;
                    }
                };
                match self.eval_array_len(cx, len) {
                    Some(n) => Ty::Array {
                        elem_width,
                        elem_signed,
                        len: n,
                    },
                    None => Ty::Unknown,
                }
            }
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

    /// Evaluate an array's length expression and validate it (E0412).
    /// Like a memory depth, a length must be a positive compile-time
    /// constant. Mirrors `eval_depth` exactly (same shape, different
    /// error code/wording since an array isn't addressable the way a
    /// memory is — no MAX_DEPTH-style upper bound is enforced here since
    /// an array is fully unrolled into scalar hardware at compile time;
    /// an unreasonably large N will simply produce a lot of Verilog, the
    /// same honesty story `repeat` already tells for large bounds).
    fn eval_array_len(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<u128> {
        match consteval::eval(e, &cx.env) {
            Ok(v) if v >= 1 => Some(v as u128),
            Ok(v) => {
                self.err(
                    cx.file,
                    e.span,
                    "E0412",
                    format!("`{v}` is not a valid array length"),
                    "an array needs at least one element — the length must be a positive compile-time constant",
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
                ModuleItem::SyncLoop(sl) => {
                    let result_t = cx
                        .sigs
                        .get(&format!("{}_result", sl.name.name))
                        .copied()
                        .unwrap_or(Ty::Unknown);
                    self.check_expr(cx, &sl.result_init, result_t);
                    // Bounds that do not const-eval were already reported by
                    // pass 3 (names.rs) — nothing more to check here. `lo`
                    // isn't used in the width formula below (see the comment
                    // there), but must still const-eval — same skip-if-either-
                    // fails behavior as before Finding 2's fix.
                    let (Ok(_lo), Ok(hi)) = (
                        consteval::eval(&sl.lo, &cx.env),
                        consteval::eval(&sl.hi, &cx.env),
                    ) else {
                        continue;
                    };
                    // Counter width: `clog2(hi)`, NOT `clog2(hi - lo)` — the
                    // physical `_cnt` register (`ast::lower_sync_loop`) holds
                    // the LIVE INDEX VALUE (`lo..hi-1`), not the iteration
                    // count, so it must be wide enough for `hi - 1`, the
                    // largest value it ever holds, regardless of `lo`. Using
                    // `hi - lo` here under-sizes the body's view of the loop
                    // variable whenever `lo != 0` (final whole-branch review
                    // Finding 2 — mirrors the lowering fix already applied in
                    // `ast::sync_loop_lower`, see
                    // `counter_width_is_clog2_hi_not_clog2_range_when_lo_nonzero`).
                    // This one is a real runtime signal (unlike an ordinary
                    // compile-time `repeat`/`loop` var) — shadow it (and the
                    // accumulator name) in `cx.sigs` for the body walk,
                    // unrolled exactly once (the body is emitted/simulated
                    // once, never per-iteration, unlike `Repeat`/`Loop`).
                    let var_t = bits(consteval::clog2_bits(hi.max(1) as u128) as u128);
                    let shadowed_var = cx.sigs.insert(sl.var.name.clone(), var_t);
                    let shadowed_result = cx.sigs.insert(sl.result_name.name.clone(), result_t);
                    self.seq_width_stmts(cx, &sl.body);
                    match shadowed_var {
                        Some(t) => {
                            cx.sigs.insert(sl.var.name.clone(), t);
                        }
                        None => {
                            cx.sigs.remove(&sl.var.name);
                        }
                    }
                    match shadowed_result {
                        Some(t) => {
                            cx.sigs.insert(sl.result_name.name.clone(), t);
                        }
                        None => {
                            cx.sigs.remove(&sl.result_name.name);
                        }
                    }
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
                    // ponytail: lhs.index on a bundle port is invalid (caught elsewhere); bundle_sigs lookup is still safe here.
                    let lhs_bundle = cx.bundle_sigs.get(&lhs.base.name).copied();
                    if let Some(lhs_ty) = lhs_bundle {
                        // LHS is bundle-typed: dispatch by RHS shape.
                        match &rhs.kind {
                            ExprKind::BundleLit(inits) => {
                                let bname = ast_bundle_name(lhs_ty);
                                let bargs = ast_bundle_args(lhs_ty);
                                if let Some(bname) = bname {
                                    let bfile_hint = ast_bundle_file(lhs_ty);
                                    self.check_bundle_lit(
                                        cx, bname, bfile_hint, bargs, inits, rhs.span,
                                    );
                                }
                            }
                            ExprKind::Ident(rhs_sig) => {
                                // Nominal type check (E0907): both sides must name the same bundle.
                                // note: nominal-only today; structural subtyping adds one
                                // field-list comparison (2.9); first-class IR bundle
                                // (post-Phase 2) promotes BundleType to a Type variant in IR
                                let rhs_bundle = cx.bundle_sigs.get(rhs_sig.as_str()).copied();
                                if let Some(rhs_ty) = rhs_bundle
                                    && let (Some(l), Some(r)) =
                                        (ast_bundle_name(lhs_ty), ast_bundle_name(rhs_ty))
                                    && l != r
                                {
                                    self.err(
                                        cx.file,
                                        rhs.span,
                                        "E0907",
                                        format!(
                                            "bundle type mismatch: cannot assign \
                                                     `{r}` where `{l}` is expected"
                                        ),
                                        "bundle types are matched by name — they \
                                                 must be the same bundle declaration",
                                    );
                                }
                            }
                            _ => {
                                // Non-literal, non-ident RHS assigned to a bundle port.
                                // Recurse for inner errors; no scalar type to check against.
                                let _ = self.infer_ty(cx, rhs);
                            }
                        }
                    } else {
                        let expected = self.lvalue_ty(cx, lhs);
                        self.check_expr(cx, rhs, expected);
                    }
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
                ModuleItem::Enum(e) => {
                    let (tag_w, max_payload_w) = self.enum_tag_and_payload_widths(cx, e);
                    let total_w = if max_payload_w == 0 {
                        tag_w
                    } else {
                        tag_w + max_payload_w
                    };
                    e.inferred_total_width.set(Some(total_w as u32));
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &cx.env).unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.walk_width_items(cx, branch, found);
                }
                ModuleItem::Port { .. }
                | ModuleItem::Clock(_)
                | ModuleItem::Reset { .. }
                | ModuleItem::Const(_)
                | ModuleItem::Error(_) => {}
                ModuleItem::BundleDestructure {
                    bindings,
                    expr,
                    span,
                } => {
                    // E0903: duplicate binding names in the destructure pattern.
                    let mut seen: HashMap<&str, Span> = HashMap::new();
                    for b in bindings {
                        if seen.insert(b.name.as_str(), b.span).is_some() {
                            self.err(
                                cx.file,
                                b.span,
                                "E0903",
                                format!("duplicate binding `{}` in bundle destructure", b.name),
                                "each field can only be bound once in a destructure",
                            );
                        }
                    }
                    // E0907: verify expr is actually bundle-typed (Ty::Unknown for non-bundles
                    // produces no further diagnostic; pass 3 already reported unknown names).
                    let _ = self.infer_ty(cx, expr);
                    let _ = span; // span available for future E0907-on-destructure diagnostics
                }
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
                SeqStmt::Default { name, val, .. } => {
                    let expected = cx.sigs.get(&name.name).copied().unwrap_or(Ty::Unknown);
                    self.check_expr(cx, val, expected);
                }
                SeqStmt::Loop {
                    var, lo, hi, body, ..
                } => {
                    // Bounds that do not const-eval were reported by pass 3.
                    let (Ok(lo_v), Ok(hi_v)) =
                        (consteval::eval(lo, &cx.env), consteval::eval(hi, &cx.env))
                    else {
                        continue;
                    };
                    let values: Vec<i128> = if hi_v - lo_v > MAX_REPEAT_CHECKS {
                        vec![lo_v, lo_v + 1, hi_v - 1]
                    } else {
                        (lo_v..hi_v).collect()
                    };
                    for v in values {
                        let shadowed = cx.env.insert(var.name.clone(), v);
                        let before = self.diags.len();
                        self.seq_width_stmts(cx, body);
                        self.unshadow(cx, &var.name, shadowed);
                        if self.diags.len() > before {
                            break;
                        }
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
    /// Compute `(tag_width, max_payload_width)` for an enum decl, emitting
    /// E0807 for any payload field whose type is not a concrete bit-vector.
    /// D4: tag_w = clog2(variant_count).max(1); D6: tag-only variants contribute 0 payload.
    fn enum_tag_and_payload_widths(
        &mut self,
        cx: &mut Wcx<'a>,
        decl: &'a EnumDecl,
    ) -> (u128, u128) {
        let tag_w = consteval::clog2_bits(decl.variants.len() as u128).max(1) as u128;
        let max_payload = decl
            .variants
            .iter()
            .map(|v| {
                v.fields
                    .iter()
                    .map(|f| match self.resolve_ty(cx, &f.ty) {
                        Ty::Bit => 1u128,
                        Ty::Bits(n) | Ty::Signed(n) => n,
                        Ty::Enum(_) | Ty::Memory { .. } => {
                            self.err(
                                cx.file,
                                f.span,
                                "E0807",
                                format!(
                                    "payload field `{}` must be a bit-vector type \
                                     (`bit`, `bits[N]`, `signed[N]`)",
                                    f.name.name
                                ),
                                "enum and memory types cannot be payload fields — \
                                 encode the value as `bits[N]` manually",
                            );
                            0
                        }
                        Ty::Array { .. } => {
                            self.err(
                                cx.file,
                                f.span,
                                "E0807",
                                format!(
                                    "payload field `{}` must be a bit-vector type \
                                     (`bit`, `bits[N]`, `signed[N]`)",
                                    f.name.name
                                ),
                                "arrays cannot be payload fields either — \
                                 encode the value as `bits[N]` manually",
                            );
                            0
                        }
                        _ => 0, // Unknown: E0103 already reported
                    })
                    .sum::<u128>()
            })
            .max()
            .unwrap_or(0);
        (tag_w, max_payload)
    }

    /// Inject payload binding types into `cx.sigs` for one match arm.
    /// Returns `(name, prev)` pairs so the caller can restore the prior state
    /// after checking the arm body. Silent — E0807 was already emitted at the
    /// enum's declaration site.
    fn inject_arm_bindings(
        &mut self,
        cx: &mut Wcx<'a>,
        en: &'a EnumDecl,
        patterns: &[Pattern],
    ) -> Vec<(String, Option<Ty<'a>>)> {
        let mut injected = Vec::new();
        for p in patterns {
            if let Pattern::Variant {
                variant, bindings, ..
            } = p
                && let Some(ev) = en.variants.iter().find(|v| v.name.name == variant.name)
            {
                for (binding, field) in bindings.iter().zip(ev.fields.iter()) {
                    let ty = self.resolve_ty_silent(cx, &field.ty);
                    match ty {
                        Ty::Bit | Ty::Bits(_) | Ty::Signed(_) => {
                            let prev = cx.sigs.insert(binding.name.clone(), ty);
                            injected.push((binding.name.clone(), prev));
                        }
                        _ => {} // Enum/Memory/Unknown — leave as Unknown (E0807 already reported)
                    }
                }
            }
        }
        injected
    }

    /// True if `ty` names a registered bundle (either `Type::Named` or parametric `Type::Bundle`).
    fn is_bundle_ty(&self, ty: &Type) -> bool {
        match ty {
            Type::Named(id) => self.bundles.contains_key(&id.name.name),
            Type::Bundle { name, .. } => self.bundles.contains_key(&name.name.name),
            _ => false,
        }
    }

    /// Resolve a bundle's fields to `(name, Ty)` pairs under the given args.
    /// `bfile_hint` is the bundle type's own `QualIdent::resolved_file`
    /// (set by names.rs pass 3) — the exact candidate when it names one,
    /// else the sole candidate (bare-and-unambiguous; a `None` hint here
    /// only ever means an already-reported ambiguous/unknown reference).
    /// Returns `None` and emits E0906 if a required param has no value.
    fn resolve_bundle_fields(
        &mut self,
        cx: &Wcx<'a>,
        bname: &str,
        bfile_hint: Option<usize>,
        bargs: &[NamedArg],
        span: Span,
    ) -> Option<Vec<(String, Ty<'a>)>> {
        let candidates = self.bundles.get(bname)?;
        let &(bfile, bdecl) = candidates
            .iter()
            .find(|&&(f, _)| Some(f) == bfile_hint)
            .or_else(|| candidates.first())?;
        let mut benv = self.file_consts[bfile].clone();
        for param in &bdecl.params {
            let arg = bargs.iter().find(|a| a.name.name == param.name.name);
            if let Some(a) = arg {
                if let Ok(v) = consteval::eval(&a.value, &cx.env) {
                    benv.insert(param.name.name.clone(), v);
                }
            } else if let Some(def) = &param.default {
                match consteval::eval(def, &benv) {
                    Ok(v) => {
                        benv.insert(param.name.name.clone(), v);
                    }
                    Err(d) => {
                        self.diags.push(d.with_file(bfile));
                        return None;
                    }
                }
            } else {
                self.err(
                    bfile,
                    span,
                    "E0906",
                    format!("bundle `{bname}` param `{}` has no value", param.name.name),
                    "provide the value: `BundleName(PARAM: value)`",
                );
                return None;
            }
        }
        // ponytail: field types must be self-contained; outer scope excluded by design.
        let mut tmp = Wcx {
            file: bfile,
            sc: Rc::new(super::names::Scope {
                names: HashMap::new(),
            }),
            env: benv,
            sigs: HashMap::new(),
            bundle_sigs: HashMap::new(),
        };
        let fields = bdecl
            .fields
            .iter()
            .map(|f| (f.name.name.clone(), self.resolve_ty(&mut tmp, &f.ty)))
            .collect();
        Some(fields)
    }

    /// Field-by-field check of a bundle literal against its declared type.
    /// Emits E0901 (missing field) and E0902 (unknown field), then checks
    /// each supplied field value's width against the declared field type.
    fn check_bundle_lit(
        &mut self,
        cx: &mut Wcx<'a>,
        bname: &str,
        bfile_hint: Option<usize>,
        bargs: &[NamedArg],
        inits: &'a [FieldInit],
        span: Span,
    ) {
        let fields = match self.resolve_bundle_fields(cx, bname, bfile_hint, bargs, span) {
            Some(f) => f,
            None => {
                // Bundle lookup failed; recurse anyway to surface inner errors.
                for init in inits {
                    let _ = self.infer_ty(cx, &init.value);
                }
                return;
            }
        };
        // E0902: literal provides a field that the bundle doesn't declare.
        for init in inits {
            if !fields.iter().any(|(n, _)| *n == init.name.name) {
                self.err(
                    cx.file,
                    init.name.span,
                    "E0902",
                    format!("bundle `{bname}` has no field `{}`", init.name.name),
                    "check the bundle declaration for the correct field names",
                );
            }
        }
        // E0901: bundle declares a field the literal omits; type-check present fields.
        for (fname, fty) in &fields {
            if let Some(init) = inits.iter().find(|i| i.name.name == *fname) {
                self.check_expr(cx, &init.value, *fty);
            } else {
                self.err(
                    cx.file,
                    span,
                    "E0901",
                    format!("bundle literal missing field `{fname}`"),
                    format!("add `{fname}: <expr>` to the literal"),
                );
            }
        }
    }

    fn check_func_body_widths(&mut self, file: usize, func: &'a FuncDecl) {
        let env = self.file_consts[file].clone();
        let mut cx = Wcx {
            file,
            sc: Rc::new(super::names::Scope {
                names: HashMap::new(),
            }),
            env,
            sigs: HashMap::new(),
            bundle_sigs: HashMap::new(),
        };
        // Seed the signal environment with concrete param types.
        for param in &func.params {
            let ty = self.resolve_ty(&mut cx, &param.ty);
            cx.sigs.insert(param.name.name.clone(), ty);
        }
        let ret_ty = self.resolve_ty(&mut cx, &func.ret);
        self.check_fn_stmt_widths(&mut cx, &func.stmts, ret_ty, &func.name.name);
        // The tail is the guaranteed fallthrough — always checked, exactly
        // like every `return` expression.
        let tail_ty = self.infer_ty(&mut cx, &func.tail);
        self.check_return_ty(&mut cx, func.tail.span, tail_ty, ret_ty, &func.name.name);
    }

    /// Width-check one `fn`-body statement list. Folds `let` bindings into
    /// `cx.sigs` sequentially — a `let` bound BEFORE an `if` stays visible
    /// inside both branches and after, but a `let` bound INSIDE a branch is
    /// scoped to that branch only: `cx.sigs` is snapshotted before checking
    /// `then`, restored before checking `els` (so `then`'s bindings don't
    /// leak into `els`), and restored again after the `if` so nothing
    /// leaks into later statements or the tail. (An earlier version of this
    /// comment claimed a "flat scope model" shared with `on`-block
    /// `SeqStmt::If` as a deliberate simplification — see
    /// `check_fn_stmt_names`'s doc comment for why that claim was wrong and
    /// this was a real soundness gap, not a design choice.)
    fn check_fn_stmt_widths(
        &mut self,
        cx: &mut Wcx<'a>,
        stmts: &'a [FnStmt],
        ret_ty: Ty<'a>,
        func_name: &str,
    ) {
        for stmt in stmts {
            match stmt {
                FnStmt::Let(local) => {
                    let ty = self.infer_ty(cx, &local.value);
                    let w: Option<u32> = match ty {
                        Ty::Bit => Some(1),
                        Ty::Bits(n) | Ty::Signed(n) => Some(n as u32),
                        Ty::CtInt(v) => Some(if v >= 0 {
                            min_bits(v)
                        } else {
                            min_signed_bits(v)
                        } as u32),
                        // An array-typed `let` has no single register width —
                        // it lowers to N scalar `reg`s of the ELEMENT width.
                        // Record the element width so the emitter can size each
                        // `<name>_<i>` reg (emit_verilog `render_fn_decl`).
                        Ty::Array { elem_width, .. } => Some(elem_width as u32),
                        _ => None,
                    };
                    if let Some(w) = w {
                        local.inferred_width.set(Some(w));
                    }
                    cx.sigs.insert(local.name.name.clone(), ty);
                }
                FnStmt::If { cond, then, els } => {
                    self.check_cond(cx, cond);
                    let sigs_before = cx.sigs.clone();
                    self.check_fn_stmt_widths(cx, then, ret_ty, func_name);
                    if let Some(els) = els {
                        cx.sigs = sigs_before.clone();
                        self.check_fn_stmt_widths(cx, els, ret_ty, func_name);
                    }
                    cx.sigs = sigs_before;
                }
                FnStmt::Return(expr) => {
                    let ty = self.infer_ty(cx, expr);
                    self.check_return_ty(cx, expr.span, ty, ret_ty, func_name);
                }
                FnStmt::Loop {
                    var, lo, hi, body, ..
                } => {
                    // Bounds that do not const-eval were reported by pass 3.
                    let (Ok(lo_v), Ok(hi_v)) =
                        (consteval::eval(lo, &cx.env), consteval::eval(hi, &cx.env))
                    else {
                        continue;
                    };
                    let values: Vec<i128> = if hi_v - lo_v > MAX_REPEAT_CHECKS {
                        vec![lo_v, lo_v + 1, hi_v - 1]
                    } else {
                        (lo_v..hi_v).collect()
                    };
                    let sigs_before = cx.sigs.clone();
                    for v in values {
                        let shadowed = cx.env.insert(var.name.clone(), v);
                        cx.sigs = sigs_before.clone();
                        let before = self.diags.len();
                        self.check_fn_stmt_widths(cx, body, ret_ty, func_name);
                        self.unshadow(cx, &var.name, shadowed);
                        if self.diags.len() > before {
                            break; // one iteration's worth of errors is enough
                        }
                    }
                    cx.sigs = sigs_before;
                }
                FnStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// Shared E0804 check: does `ty` (a `return` expression's or the tail's
    /// inferred type) match the function's declared return type? Extracted
    /// so both `return` sites and the tail use identical logic.
    fn check_return_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        span: Span,
        ty: Ty<'a>,
        ret_ty: Ty<'a>,
        func_name: &str,
    ) {
        match (ty, ret_ty) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => {}
            (Ty::CtInt(v), t) => self.fit(cx, span, v, t),
            (g, t) if same(&g, &t) => {}
            (g, t) => {
                self.err(
                    cx.file,
                    span,
                    "E0804",
                    format!(
                        "function `{func_name}` body is {}, but the declared return type is {}",
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
            bundle_sigs: HashMap::new(),
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

/// Extract the bundle name from an AST type (Named or parametric Bundle).
fn ast_bundle_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Named(id) => Some(&id.name.name),
        Type::Bundle { name, .. } => Some(&name.name.name),
        _ => None,
    }
}

/// The bundle name's `resolved_file` (set by names.rs pass 3), for
/// disambiguating which same-named bundle's fields to resolve.
fn ast_bundle_file(ty: &Type) -> Option<usize> {
    match ty {
        Type::Named(id) => id.resolved_file.get(),
        Type::Bundle { name, .. } => name.resolved_file.get(),
        _ => None,
    }
}

/// Extract the parameter args slice from a parametric bundle type, or `&[]`.
fn ast_bundle_args(ty: &Type) -> &[NamedArg] {
    match ty {
        Type::Bundle { args, .. } => args.as_slice(),
        _ => &[],
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
}
