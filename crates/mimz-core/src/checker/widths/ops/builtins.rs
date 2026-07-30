use super::*;

impl<'a> Checker<'a> {
    /// The four builtins (spec/02 sections 1.7–1.8).
    pub(in crate::checker::widths) fn call_ty(
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
                let m = match &xt {
                    Ty::Bit => 1,
                    Ty::Bits(w) | Ty::Signed(w) => *w,
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
                    other => return self.not_numeric(cx, x.span, other, name),
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
            // `clog2` is compile-time only — reaching the value-typer means it
            // was used where a runtime value is expected. In a width/const/param
            // position it folds through `consteval` and never lands here.
            Builtin::Clog2 => {
                self.err(
                    cx.file,
                    e.span,
                    "E0407",
                    "`clog2` is a compile-time built-in and has no value here",
                    "use it where a constant is expected — a width `bits[clog2(N)]`, \
                     a `const`, or a parameter default",
                );
                Ty::Unknown
            }
            Builtin::SyncDoubleFlop | Builtin::SyncPulse => {
                let name = if func == Builtin::SyncDoubleFlop {
                    "sync.double_flop"
                } else {
                    "sync.pulse"
                };
                let Some(src_arg) = args.get(1) else {
                    return Ty::Unknown;
                };
                let Some(dst_arg) = args.get(2) else {
                    return Ty::Unknown;
                };
                let src_ty = self.infer_ty(cx, src_arg);
                let dst_ty = self.infer_ty(cx, dst_arg);
                if !matches!(src_ty, Ty::Clock) {
                    self.err(
                        cx.file,
                        src_arg.span,
                        "E0702",
                        format!("`{name}`'s second argument must be a declared `clock` name"),
                        "pass the clock the SIGNAL is already synchronous to — a \
                         `clock` declaration's own name, not a data expression",
                    );
                    return Ty::Unknown;
                }
                if !matches!(dst_ty, Ty::Clock) {
                    self.err(
                        cx.file,
                        dst_arg.span,
                        "E0702",
                        format!("`{name}`'s third argument must be a declared `clock` name"),
                        "pass the clock the RESULT should be synchronous to",
                    );
                    return Ty::Unknown;
                }
                let (crate::ast::ExprKind::Ident(src_name), crate::ast::ExprKind::Ident(dst_name)) =
                    (&src_arg.kind, &dst_arg.kind)
                else {
                    self.err(
                        cx.file,
                        e.span,
                        "E0702",
                        format!("`{name}`'s clock arguments must be bare clock names"),
                        "write the clock's own name directly, e.g. `clk_uart` — \
                         not a computed expression",
                    );
                    return Ty::Unknown;
                };
                if src_name == dst_name {
                    self.err(
                        cx.file,
                        e.span,
                        "E0702",
                        format!(
                            "`{name}`'s source and destination clocks are the same (`{src_name}`)"
                        ),
                        "synchronizing a signal to the clock it already belongs to \
                         is a no-op — check for a typo in one of the two clock names",
                    );
                    return Ty::Unknown;
                }
                let width_ok = matches!(xt, Ty::Bit) || matches!(xt, Ty::Bits(1));
                if !width_ok {
                    self.err(
                        cx.file,
                        x.span,
                        "E0703",
                        format!("`{name}`'s signal argument must be exactly 1 bit"),
                        "a 2-flop/toggle synchronizer is only sound for a single \
                         control bit — bit-independently synchronizing a wider bus \
                         is a real hardware hazard (bits can resolve on different \
                         destination cycles); a multi-bit-safe crossing (handshake \
                         or gray-coded FIFO) is not yet provided by this compiler",
                    );
                    return Ty::Unknown;
                }
                Ty::Bit
            }
            Builtin::Min | Builtin::Max => {
                let name = if func == Builtin::Min { "min" } else { "max" };
                let Some(b) = args.get(1) else {
                    return Ty::Unknown;
                };
                let bt = self.infer_ty(cx, b);
                if matches!(bt, Ty::Unknown) {
                    return Ty::Unknown;
                }
                if let (Ty::CtInt(_), Ty::CtInt(_)) = (&xt, &bt) {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`{name}` of two literals has no width"),
                        "give at least one operand a fixed width — a signal, or \
                         `extend(x, N)`",
                    );
                    return Ty::Unknown;
                }
                // Same operand rule as a comparison: equal kind + width (a
                // literal adapts to the sized side). The result is that type.
                self.matched_ty(cx, name, (x, xt), (b, bt))
            }
            Builtin::Abs => match xt {
                // Lossless like unary `-`: `abs(MIN)` needs the extra bit.
                Ty::Signed(n) => Ty::Signed(n + 1),
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "`abs` of a bare literal does nothing",
                        "write the non-negative value directly",
                    );
                    Ty::Unknown
                }
                other => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`abs` needs a `signed` value, found {}", show(&other)),
                        "absolute value is signed-only — `bits` are already \
                         non-negative; cast with `signed(x)` if needed",
                    );
                    Ty::Unknown
                }
            },
            Builtin::Nand | Builtin::Nor | Builtin::Xnor => {
                let name = match func {
                    Builtin::Nand => "`nand`",
                    Builtin::Nor => "`nor`",
                    _ => "`xnor`",
                };
                match xt {
                    // Negated reductions: a vector (or bit) collapses to one bit.
                    Ty::Bit | Ty::Bits(_) => Ty::Bit,
                    Ty::Signed(_) => {
                        self.err(
                            cx.file,
                            e.span,
                            "E0403",
                            "reductions work on `bits`, not `signed`",
                            "cast first: `nand(unsigned(x))` (spec/02 section 3)",
                        );
                        Ty::Unknown
                    }
                    other => self.not_numeric(cx, x.span, &other, name),
                }
            }
        }
    }
}
