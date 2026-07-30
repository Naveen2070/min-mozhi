use super::*;

impl<'a> Checker<'a> {
    /// Type of an assignment target (`name`, `name[i]`, `name[hi:lo]`).
    pub(in crate::checker::widths) fn lvalue_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        lv: &'a LValue,
    ) -> Ty<'a> {
        let base = match cx.sigs.get(&lv.base.name) {
            Some(t) => t.clone(),
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
    pub(super) fn index_in_range(&mut self, cx: &mut Wcx<'a>, idx: &'a Expr, n: u128) {
        let t = self.infer_ty(cx, idx);
        match t {
            Ty::CtInt(v) => {
                if !fits_in_count(&v, n) {
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
    pub(super) fn mem_addr_in_range(&mut self, cx: &mut Wcx<'a>, addr: &'a Expr, depth: u128) {
        let t = self.infer_ty(cx, addr);
        match t {
            Ty::CtInt(v) => {
                if !fits_in_count(&v, depth) {
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

    /// `[hi:lo]` bounds: both const, `lo <= hi < n`. Returns the slice
    /// type. The actual reversed/range/signedness rule is shared with
    /// the simulator via `width_rules::slice_result` (Stage 4, A1a) —
    /// this is the exact function whose signedness rule BUG-21 found
    /// duplicated (and diverged) in the simulator's own evaluator.
    pub(super) fn slice_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        hi: &'a Expr,
        lo: &'a Expr,
        n: u128,
    ) -> Option<Ty<'a>> {
        let h = self.const_bound(cx, hi)?;
        let l = self.const_bound(cx, lo)?;
        // `h`/`l` are non-negative here (`const_bound` already rejected a
        // negative bound), but unlike `n` (checker-bounded by `MAX_WIDTH`,
        // see `ops.rs`'s `width_u32` doc comment) they come straight from
        // an arbitrary user constant expression and have no upper bound —
        // a raw `as u32` would wrap modulo 2^32 (e.g. `2^32` -> `0`) and
        // silently accept a bound that should be rejected as out of range.
        // Saturate instead: any `h`/`l` over `u32::MAX` becomes `u32::MAX`,
        // which is always `>= n` (itself `<= u32::MAX`) and so still trips
        // `slice_result`'s out-of-range check below — the diagnostic text
        // prints the original `i128` `h`/`l`, not this narrowed value.
        let h32 = super::super::ops::width_u32(h as u128);
        let l32 = super::super::ops::width_u32(l as u128);
        match crate::width_rules::slice_result(n as u32, h32, l32) {
            Ok(k) => Some(bits(k.width as u128)),
            Err(crate::width_rules::RuleError::SliceReversed { .. }) => {
                self.err(
                    cx.file,
                    hi.span.join(lo.span),
                    "E0406",
                    format!("slice bounds are reversed: `[{h}:{l}]`"),
                    "slices are written `[hi:lo]`, most significant bit first \
                     (spec/02 section 1.8)",
                );
                None
            }
            Err(crate::width_rules::RuleError::SliceOutOfRange { .. }) => {
                self.err(
                    cx.file,
                    hi.span,
                    "E0406",
                    format!("slice bound `{h}` is out of range"),
                    format!("the value has {n} bits, so bit positions run 0..={}", n - 1),
                );
                None
            }
            Err(_) => unreachable!("slice_result only returns SliceReversed/SliceOutOfRange"),
        }
    }

    /// A slice bound: must const-evaluate and be non-negative. Saturates
    /// to `i128` (a bound this far over `MAX_WIDTH` is already invalid —
    /// `slice_ty`'s caller saturates it further to `u32`, see its comment).
    fn const_bound(&mut self, cx: &Wcx<'a>, e: &'a Expr) -> Option<i128> {
        match consteval::eval(e, &cx.env) {
            Ok(v) if !v.is_negative() => Some(v.to_i128_saturating()),
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
}
