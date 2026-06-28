//! The bidirectional typing engine: `check_expr` pushes an expected type
//! down (so `if`/`match` arms are checked individually and literals get
//! the fit check), `infer_ty` synthesizes bottom-up. Lvalue/index/slice
//! typing lives here too — the same range rules serve both sides of an
//! assignment.

use crate::ast::{BinOp, Expr, ExprKind, LValue};
use crate::span::Span;

use super::super::Checker;
use super::super::consteval;
use super::super::names::Bind;
use super::{
    Ty, Wcx, bits, fits_bits, fits_in_count, fits_signed, max_signed_v, max_unsigned, min_signed,
    same, show,
};

impl<'a> Checker<'a> {
    /// Type of an assignment target (`name`, `name[i]`, `name[hi:lo]`).
    pub(super) fn lvalue_ty(&mut self, cx: &mut Wcx<'a>, lv: &'a LValue) -> Ty<'a> {
        let base = match cx.sigs.get(&lv.base.name) {
            Some(t) => *t,
            None => return Ty::Unknown, // E0101/E0108 already reported
        };
        let Some((first, second)) = &lv.index else {
            return base;
        };
        // A memory write `m[addr] <- v` targets one cell (the element type);
        // a memory cannot be sliced.
        if let Ty::Memory {
            width,
            signed,
            depth,
        } = base
        {
            return match second {
                None => {
                    self.mem_addr_in_range(cx, first, depth);
                    if signed {
                        Ty::Signed(width)
                    } else {
                        bits(width)
                    }
                }
                Some(_) => {
                    self.err(
                        cx.file,
                        lv.span,
                        "E0406",
                        "a memory is addressed one cell at a time",
                        "write `m[addr] <- value` — a memory cannot be sliced `[hi:lo]`",
                    );
                    Ty::Unknown
                }
            };
        }
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

    /// Range-check a memory address against a depth of `depth` cells. Mirrors
    /// [`Self::index_in_range`] but the bound is a cell count, not a bit width,
    /// so the diagnostic speaks of addresses and cells. A compile-time address
    /// out of range is E0406; a runtime (signal) address passes unchecked.
    fn mem_addr_in_range(&mut self, cx: &mut Wcx<'a>, addr: &'a Expr, depth: u128) {
        let t = self.infer_ty(cx, addr);
        match t {
            Ty::CtInt(v) => {
                if v < 0 || !fits_in_count(v, depth) {
                    self.err(
                        cx.file,
                        addr.span,
                        "E0406",
                        format!("address `{v}` is out of range"),
                        format!(
                            "the memory has {depth} cells, so addresses run 0..={}",
                            depth - 1
                        ),
                    );
                }
            }
            Ty::Bit | Ty::Bits(_) | Ty::Unknown => {}
            Ty::Signed(_) => self.err(
                cx.file,
                addr.span,
                "E0403",
                "a `signed` value cannot be a memory address",
                "addresses are non-negative — cast with `unsigned(...)` first",
            ),
            other => self.err(
                cx.file,
                addr.span,
                "E0406",
                format!("{} cannot be used as a memory address", show(&other)),
                "an address is a compile-time value or an unsigned signal",
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
    pub(super) fn check_expr(&mut self, cx: &mut Wcx<'a>, e: &'a Expr, expected: Ty<'a>) {
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
                let en_decl = if let Ty::Enum(en) = st { Some(en) } else { None };
                for arm in arms {
                    let injected = if let Some(en) = en_decl {
                        self.inject_arm_bindings(cx, en, &arm.patterns)
                    } else {
                        Vec::new()
                    };
                    self.check_expr(cx, &arm.value, expected);
                    for name in &injected {
                        cx.sigs.remove(name.as_str());
                    }
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
                self.err_args(
                    cx.file,
                    e.span,
                    "E0401",
                    format!("expected {}, found {}", show(&t), show(&g)),
                    help,
                    vec![("expected", show(&t)), ("found", show(&g))],
                );
            }
        }
    }

    /// A compile-time integer meeting a sized context: does it fit?
    pub(super) fn fit(&mut self, cx: &mut Wcx<'a>, span: Span, v: i128, t: Ty<'a>) {
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
                        .map(|v| v.name.name.as_str())
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
            Ty::Memory { .. } => self.err(
                cx.file,
                span,
                "E0403",
                format!("a number cannot stand for {}", show(&t)),
                "address one cell — `m[addr]` — to read or write a value",
            ),
            Ty::CtInt(_) | Ty::Unknown => {}
        }
    }

    /// `if`/`match`/`&&` conditions must be a single bit.
    pub(super) fn check_cond(&mut self, cx: &mut Wcx<'a>, e: &'a Expr) {
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
    pub(super) fn infer_ty(&mut self, cx: &mut Wcx<'a>, e: &'a Expr) -> Ty<'a> {
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
                let en_decl = if let Ty::Enum(en) = st { Some(en) } else { None };
                let mut arm_tys = Vec::with_capacity(arms.len());
                for arm in arms {
                    let injected = if let Some(en) = en_decl {
                        self.inject_arm_bindings(cx, en, &arm.patterns)
                    } else {
                        Vec::new()
                    };
                    arm_tys.push((arm.value.span, self.infer_ty(cx, &arm.value)));
                    for name in &injected {
                        cx.sigs.remove(name.as_str());
                    }
                }
                self.unify_arms(cx, e.span, &arm_tys)
            }
            ExprKind::Concat(parts) => self.concat_ty(cx, parts),
            ExprKind::Replicate { count, parts } => self.replicate_ty(cx, count, parts),
            ExprKind::Index { base, index } => {
                let bt = self.infer_ty(cx, base);
                // A memory read `m[addr]` yields the element type (the address
                // may be a runtime signal); a bit-vector index yields one bit.
                if let Ty::Memory {
                    width,
                    signed,
                    depth,
                } = bt
                {
                    self.mem_addr_in_range(cx, index, depth);
                    return if signed {
                        Ty::Signed(width)
                    } else {
                        bits(width)
                    };
                }
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
            ExprKind::FnCall { name, args } => {
                // Arity was checked in an earlier pass (E0803/E1110); unknown
                // callee here means a prior pass already reported the error.
                let (ffile, func) = match self.funcs.get(&name.name).copied() {
                    Some(x) => x,
                    None => {
                        for a in args {
                            let _ = self.infer_ty(cx, a);
                        }
                        return Ty::Unknown;
                    }
                };
                // Check each arg width against the corresponding param type.
                // Uses check_expr (→ expect_ty), mirroring the connection-width
                // check in check_inst_widths (widths/insts.rs) which uses
                // infer_ty + fit + same for the same "got vs expected" logic.
                for (arg, param) in args.iter().zip(func.params.iter()) {
                    let param_ty = self.fn_type_for_file(ffile, &param.ty);
                    self.check_expr(cx, arg, param_ty);
                }
                // The call's type is the function's declared return type.
                self.fn_type_for_file(ffile, &func.ret)
            }
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
                            .map(|v| v.name.name.as_str())
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
                Some(en) if en.variants.iter().any(|v| v.name.name == field.name) => Ty::Enum(en),
                _ => Ty::Unknown, // E0103 already reported
            },
        }
    }
}
