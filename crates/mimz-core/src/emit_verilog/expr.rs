//! Expression rendering: operators, literals (base-preserving), enum
//! constants, instance-port wires, if → ternary, match → ternary chains,
//! and builtin casts. `subst` replaces child-module parameter names with
//! instance arguments when rendering child port widths.

use super::*;

/// Array-typed names in scope while rendering ONE `fn` body: maps a param
/// or `let`-bound name to `(element_width_string, length)`, so an `Ident`,
/// `ArrayLit`, or `Index` base referring to it can be expanded/resolved to
/// its `<name>_<i>` scalar ports. Built once per `render_fn_decl` from
/// `decl.params` and `fn_all_locals(decl.stmts)`; empty for every
/// non-`fn`-body expression render.
pub(super) type ArrayScope = HashMap<String, (String, u128)>;

/// Build a synthetic `<base>.<field_name>` field-access expression — used to
/// desugar `raw ?? 0` into `raw.valid ? raw.data : 0` by reusing the
/// existing `ExprKind::Field` rendering instead of hand-formatting
/// `<name>_<field>` strings. No such helper existed before this task; kept
/// local (not a method) since it only builds a value, it doesn't render one.
fn field_expr(base: &Expr, field_name: &str) -> Expr {
    Expr {
        kind: ExprKind::Field {
            base: Box::new(base.clone()),
            field: Ident {
                name: field_name.into(),
                span: base.span,
            },
        },
        span: base.span,
    }
}

// BUG-41 (docs/audit/bugs.md): the hand-maintained `kind_is_inferrable`
// gate that used to live here is retired — `kinds::infer_kind` now
// returns `Option<Kind>` directly (`None` where this used to say
// `false`), so every call site below matches on it once instead of
// checking a separate, easy-to-drift function first. See `kinds.rs`'s
// own module doc for why keeping the gate and the classifier in sync by
// hand was the defect generator in the first place.

/// True iff `kind`'s own top-level operator is one whose spec-defined
/// result depends on the width it was originally evaluated at — lossless
/// growth (`+`/`-`/`*`) or wrap-modulus (`+%`/`-%`/`*%` — BUG-19's own
/// documented class, `docs/audit/bugs.md`). Every OTHER operator
/// (`<<`/`>>`/`&`/`|`/`^`/comparisons/…) gives the SAME value no matter
/// what width Verilog happens to (re)compute it at, so `Builtin::Extend`'s
/// argument-hoist (below) only needs to isolate THIS family, not every
/// non-identifier shape.
///
/// `Shl`/`Shr` are NOT in this family: unlike lossless/wrap arithmetic
/// (context-independent by mimz's own design — `binary_ctx`'s own doc
/// comment says only `Shl`/`Shr` ever consult `expected_width`), a
/// shift's correct value CAN depend on the ambient width the reference
/// simulator threads through certain AST positions (BUG-24; see
/// `is_shift_binop` and `hoist_width_effect_operand`'s `allow_shift`
/// parameter for the exact, narrower, position-scoped rule).
fn is_width_effect_binop(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Binary {
            op: BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::AddWrap
                | BinOp::SubWrap
                | BinOp::MulWrap,
            ..
        }
    )
}

/// True for `Shl`/`Shr` — BUG-24's family. Unlike `is_width_effect_binop`'s
/// lossless/wrap family (context-independent, safe to hoist unconditionally
/// everywhere), a shift's correct value CAN depend on the ambient width the
/// simulator threads through (`mimz-sim`'s `eval_ctx`, ground-truthed against
/// Icarus since BUG-11) — so callers must only treat this as hoistable at a
/// position where that ambient width is ALSO `None` (self-determined) in the
/// simulator's own model. See `hoist_width_effect_operand`'s `allow_shift`
/// parameter for the exact call-site scoping (regression found and fixed
/// after BUG-24's own fix was applied too broadly — see docs/audit/bugs.md).
fn is_shift_binop(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Binary {
            op: BinOp::Shl | BinOp::Shr,
            ..
        }
    )
}

/// True iff `text` is a bare Verilog identifier (letters, digits,
/// underscore, not starting with a digit) — the ONLY kind of text a
/// Verilog part-select (`x[hi:lo]`) or a self-determined position that
/// needs a definite-width signal reference can safely receive without
/// hoisting to a wire first. Deliberately conservative: a hoisted
/// wire's own name (`__mimz_sub_N`) always passes this check trivially.
pub(super) fn is_plain_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Emitter<'_> {
    /// Render an expression with no substitutions (the common case).
    pub(super) fn expr(&mut self, e: &Expr) -> String {
        self.expr_subst(e, &HashMap::new(), &ArrayScope::new())
    }

    /// Task 3 (BUG-62(b), GAP-16, `docs/plan/v0.2-class-closure-round6.local.md`):
    /// tried from a self-determined-position call site's own `None` arm
    /// (a concat/replicate member, a reduction/nand/nor/xnor operand, a
    /// `$signed`/`$unsigned` cast operand — never a comparison or a slice/
    /// bit-select/`trunc` BASE, see each call site's own comment) right
    /// before it falls through to `hoist_unresolved`'s diagnostic.
    ///
    /// `infer_kind` can never resolve `extend(x, W)`'s own `Kind` when `W`
    /// is a module `int` parameter rather than a literal — `infer_call`'s
    /// `Extend` arm needs a folded `u32`. Every self-determined-position
    /// caller used to render the UN-widened `x` in that case (BUG-62 ⑦⑧:
    /// `{ b, extend(a, W) }` emitted `{b, (a)}`, silently dropping the
    /// widening — the concat's own bit positions came out wrong). This
    /// renders the widening explicitly instead — `{{(W-N){fill}}, x}` is
    /// exactly W bits wherever it appears, in ANY position, no `Kind`
    /// (and so no hoisted wire sized by one) required.
    ///
    /// Returns `None` (caller falls back to `hoist_unresolved`) when
    /// `expr` isn't `extend(_, <symbolic>)`, or when `x`'s OWN `Kind` also
    /// doesn't resolve — nothing to widen FROM either, a residual case
    /// this doesn't attempt to close.
    ///
    /// Deliberately NOT folded into `Builtin::Extend`'s own render arm:
    /// that arm runs for EVERY position (including an ordinary context-
    /// determined operand, e.g. `sr <= old | extend(din, WIDTH)`), where
    /// Verilog's own automatic widening already renders `(din)` correctly
    /// — confirmed empirically (rendering the explicit form unconditionally
    /// there changed nothing about correctness but needlessly rewrote an
    /// already-correct shipped golden, `shift_register`'s).
    ///
    /// Ceiling (ponytail): if `W` folds to exactly `N` at some
    /// instantiation, the replication count `(W)-(N)` is 0, which
    /// Verilog-2005 (IEEE 1364-2005 §5.1.14) requires to be a POSITIVE
    /// constant — `{0{...}}` is technically illegal there, though every
    /// toolchain this project targets accepts it in practice. Not solved
    /// here; would need a `generate`/conditional selection to close for
    /// real.
    ///
    /// Round-8 plan Task 10 (BUG-72): `Builtin::Trunc` needed the
    /// identical `None`-arm recovery `Extend` already had — `infer_call`'s
    /// `Extend | Trunc` arm folds the width argument with `const_fold` for
    /// BOTH builtins, so a module `int` parameter (or an unresolvable base)
    /// makes `infer_kind` return `None` for a symbolic-width `trunc` the
    /// same way it does for `extend`, and every one of this function's 8
    /// call sites fell through to `hoist_unresolved`'s diagnostic — a
    /// checker-accepted, `extend`-symmetric construct the compiler flatly
    /// refused. `trunc` needs none of `Extend`'s own width-math rebuild
    /// though: its ORDINARY render arm below (`x[(n)-1:0]`) never
    /// const-folds `n` at all, substituting it as TEXT unconditionally — a
    /// Verilog part-select's own width is always exactly `n` bits by
    /// construction, in ANY position, unlike a plain `extend()` passthrough
    /// (which relies on ambient context Verilog doesn't provide at a
    /// self-determined position). So the ordinary arm's own rendering IS
    /// already the self-determined-position answer too — every caller here
    /// already computed it, as `text`, via `render_shift_ctx_operand`
    /// (which dispatches through that same ordinary arm) before ever
    /// calling this function. Reusing it directly, rather than re-rendering
    /// `args[0]` from scratch the way `Extend`'s own arm does, avoids a
    /// second, redundant hoist of the base that would otherwise leave the
    /// FIRST hoisted wire dead and unreferenced.
    fn try_widen_symbolic_extend(
        &mut self,
        expr: &Expr,
        text: &str,
        subst: &HashMap<&str, &Expr>,
        arrays: &ArrayScope,
    ) -> Option<String> {
        match &expr.kind {
            ExprKind::Call {
                func: Builtin::Extend,
                args,
            } => {
                if consteval::eval(&args[1], &self.env).is_ok() {
                    return None; // width folds — the ordinary path already handles it
                }
                let decls = Rc::clone(&self.cur_decls);
                let k = crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env)?;
                if k.width == 0 {
                    return None;
                }
                let base_text = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                let named =
                    self.hoist_slice_base_if_needed(base_text, k.width, k.signed, args[0].span);
                let w_text = self.expr_subst(&args[1], subst, arrays);
                let fill = if k.signed {
                    format!("{named}[{}]", k.width - 1)
                } else {
                    "1'b0".to_string()
                };
                let mut widened = String::from("{{(");
                widened.push_str(&w_text);
                widened.push_str(")-(");
                widened.push_str(&k.width.to_string());
                widened.push_str("){");
                widened.push_str(&fill);
                widened.push_str("}}, ");
                widened.push_str(&named);
                widened.push('}');
                Some(widened)
            }
            ExprKind::Call {
                func: Builtin::Trunc,
                ..
            } => Some(text.to_string()),
            _ => None,
        }
    }

    /// Try to resolve an expression to a constant integer value, seeing
    /// through `extend`/`trunc` that wrap a literal (BUG-18). `extend` widens
    /// without changing the value; `trunc` masks it to its low N bits. Returns
    /// `None` for anything with a runtime component (a signal or a computed
    /// expression) — those already carry a definite Verilog width from their
    /// own declaration, so the `Builtin::Extend` passthrough is safe for them.
    /// Values are always non-negative (source literals are unsized and
    /// non-negative; masking preserves that), so callers render `W'd{v}`.
    ///
    /// Returns `Bits` (not `i128`, BUG-13 layer 2) — a literal here may be
    /// wider than 128 bits.
    fn resolve_const_value(&self, e: &Expr) -> Option<crate::bits::Bits> {
        match &e.kind {
            ExprKind::Int { value, .. } => Some(value.clone()),
            ExprKind::Call {
                func: Builtin::Extend,
                args,
            } => self.resolve_const_value(&args[0]),
            ExprKind::Call {
                func: Builtin::Trunc,
                args,
            } => {
                let v = self.resolve_const_value(&args[0])?;
                let n_val = consteval::eval(&args[1], &self.env).ok()?;
                let n = crate::bits::to_limbs(&n_val.bits, n_val.width)
                    .first()
                    .copied()? as u32;
                let width = crate::bits::natural_width(&v);
                let limbs = crate::bits::to_limbs(&v, width.max(n));
                let mut truncated = limbs;
                crate::wide::mask_to_width(&mut truncated, n);
                Some(crate::bits::from_limbs(truncated, n))
            }
            _ => None,
        }
    }

    /// Unconditionally hoists `text` into a fresh explicit-width wire
    /// when `child` is a width-effect binary operator (lossless
    /// `+`/`-`/`*` or wrapping `+%`/`-%`/`*%` — `is_width_effect_binop`)
    /// — BUG-23 (`docs/audit/bugs.md`): real Verilog's arithmetic/
    /// bitwise operators are context-determined (a connected tree
    /// computes ONE width for the whole tree), so a wrap operator
    /// nested as a direct operand of ANY other operator has its own
    /// width truncation silently redone at that operator's wider
    /// context instead of at its own declared width — UNLESS it is
    /// first materialized as a named signal, whose declared width
    /// Verilog cannot widen out from under. Lossless operators are
    /// also matched by `is_width_effect_binop` even though they are
    /// SAFE to compute at any wider context (extra bits are harmless
    /// leading zero/sign extension) — hoisting them here too is
    /// unnecessary but not incorrect, and reusing this one existing,
    /// already-tested check (rather than a narrower wrap-only match)
    /// keeps this function's caller list a single, simple rule instead
    /// of two nearly-identical ones.
    ///
    /// Called after every recursive descent into an operand position
    /// within `expr_subst` — never at the true top-level statement-RHS
    /// render (the external entry points — `Drive`, `Wire` init, `Reg`
    /// next-state, etc. — call `expr_subst`/`expr` directly and never
    /// wrap their own result through this function), which is already
    /// correct on its own: the assignment target's declared width
    /// already pins a bare `y = a -% b`'s result correctly.
    ///
    /// `allow_shift` additionally admits a `Shl`/`Shr` child (BUG-24) —
    /// but ONLY at call sites where the reference simulator
    /// (`mimz-sim/src/sim/value.rs`) also evaluates that position with
    /// `expected_width: None` (self-determined), so this hoist's
    /// bottom-up `infer_kind` computation matches the simulator's own
    /// value exactly. Passing `true` at a position where the simulator
    /// instead threads a real ambient width in (`Builtin::Extend`'s
    /// argument, an `IfExpr`/`Match` branch, or a shift's LHS when the
    /// OUTER operator is itself a shift) would hoist to the WRONG width —
    /// see each call site for its justification.
    fn hoist_width_effect_operand(
        &mut self,
        child: &Expr,
        text: String,
        decls: &HashMap<String, crate::width_rules::Kind>,
        allow_shift: bool,
    ) -> String {
        let hoistable =
            is_width_effect_binop(&child.kind) || (allow_shift && is_shift_binop(&child.kind));
        // `None` (Kind unresolvable — a `fn`-body local, a module
        // `parameter`, a testbench signal, or a symbolic parametric width;
        // BUG-41's own remaining, pre-existing residue, `docs/audit/bugs.md`)
        // leaves `text` unchanged: a wire declaration needs a CONCRETE
        // width, which is exactly the one thing this branch doesn't have —
        // the same, already-correct fallback these shapes had before this
        // task (`kind_is_inferrable`'s own retired doc comment).
        if !hoistable {
            return text;
        }
        match crate::emit_verilog::kinds::infer_kind(child, decls, &self.env) {
            Some(kind) => {
                self.hoist_slice_base_if_needed(text, kind.width, kind.signed, child.span)
            }
            // NOT routed through `hoist_unresolved` (Task 1): unlike the
            // `hoist_if_needed`-family call sites in this file, `None`
            // here is not the "twelve call sites, one shared silent
            // branch" GAP-16 was filed against — this is BUG-30's own,
            // separately-documented safe case (`kinds.rs`'s
            // `shift_const_amount` and `adapts_to_sibling` doc comments,
            // cited there by bug number): a module `int` parameter inside
            // a width-effect/shift child has no `Kind` by construction
            // (mimz's own type system gives it none — it is `Ty::CtInt`,
            // not `bits[N]`), and real Verilog's own context growth is
            // harmless here once the base growth is already lossless —
            // proven by `examples/*/shift.mimz`'s `extend(3 << AMOUNT,
            // 8)`, a working, golden-verified shape this exact fallback
            // has always covered. Asserting on it would be noise, not
            // signal — Task 3 (BUG-62(b)) is the wider, separate parameter-
            // width story (an `extend`/`trunc` WIDTH argument that
            // silently drops the whole hoist), not this one.
            None => text,
        }
    }

    /// Render `child` at a position that is (`allow_shift: true`) or is
    /// not (`false`) self-determined for shift purposes — the same
    /// distinction `hoist_width_effect_operand`'s own `allow_shift` makes,
    /// but applied at RENDER time rather than only to the post-hoc hoist.
    ///
    /// BUG-55 (docs/audit/bugs.md): `eval_ctx`'s `IfExpr`/`Match` arms
    /// propagate the SAME `expected_width` they themselves received into
    /// EVERY branch/arm. So when `child` is itself an `if`/`match` sitting
    /// in a self-determined position (a concat member, `extend()`'s
    /// argument, a `min`/`max`/`abs`/cast operand, …), its branches are
    /// self-determined too — the default rendering `if_expr_subst`/
    /// `match_subst` give an `if`/`match` (`allow_shift: false`, correct
    /// for the ordinary context-determined case BUG-24 pinned) is wrong
    /// here, the exact same context-escape BUG-47 fixed for `extend()`'s
    /// own direct shift argument, one AST node deeper. Every call site
    /// that already passes `allow_shift: true` to `hoist_width_effect_operand`
    /// should render its child through this function instead of
    /// `expr_subst` directly, so a shift hiding inside an `if`/`match`
    /// branch there gets the same hoist a bare shift child already does.
    fn render_shift_ctx_operand(
        &mut self,
        child: &Expr,
        subst: &HashMap<&str, &Expr>,
        arrays: &ArrayScope,
        allow_shift: bool,
    ) -> String {
        let text = if allow_shift {
            match &child.kind {
                ExprKind::IfExpr { cond, then, els } => {
                    self.if_expr_subst(cond, then, els, subst, arrays, true)
                }
                ExprKind::Match { scrutinee, arms } => {
                    self.match_subst(scrutinee, arms, subst, arrays, true)
                }
                _ => self.expr_subst(child, subst, arrays),
            }
        } else {
            self.expr_subst(child, subst, arrays)
        };
        let decls = Rc::clone(&self.cur_decls);
        // BUG-59 (docs/audit/bugs.md): `!allow_shift` reaching THIS
        // function only ever happens at one call site — a `Shl`/`Shr`'s
        // own LHS (`allow_shift_lhs` below). If `child` is an `if`/`match`
        // there, its branches can hide a fused shift chain
        // (`eval_shift_chain`, BUG-34) that mimz-sim resolves BOTTOM-UP,
        // in isolation, with no ambient context. Real Verilog's ternary is
        // context-PROPAGATING though: rendered inline as this outer
        // shift's own un-hoisted LHS, its branches get re-derived at the
        // OUTER assignment's full (grown) width, and an inner `>>` inside
        // a branch truncates DIFFERENTLY at that wider width than the
        // kernel's isolated one does — a VALUE mismatch despite mimz's
        // own `Kind` for the whole `if`/`match` already agreeing with
        // Verilog's self-determined one (BUG-52/`hoist_if_needed`'s
        // mismatch check would never fire here; both sides already say
        // 11 bits, confirmed by hand against real Icarus). Deliberately
        // NOT folded into `hoist_width_effect_operand` itself — that
        // function is ALSO called from `if_expr_subst`/`match_subst`'s own
        // branch rendering with `allow_shift: false` for the ordinary,
        // already-correct nested-ternary case (BUG-24), where the same
        // check would spuriously over-hoist (confirmed: broke two
        // showcase goldens before this was scoped down to here).
        if !allow_shift
            && matches!(child.kind, ExprKind::IfExpr { .. } | ExprKind::Match { .. })
            && let Some(kind) = crate::emit_verilog::kinds::infer_kind(child, &decls, &self.env)
        {
            return self.hoist_slice_base_if_needed(text, kind.width, kind.signed, child.span);
        }
        self.hoist_width_effect_operand(child, text, &decls, allow_shift)
    }

    /// `ExprKind::IfExpr`'s own rendering, factored out so
    /// `render_shift_ctx_operand` (BUG-55) can re-enter it with
    /// `allow_shift: true` when the whole `if` sits in a self-determined
    /// position. `expr_subst`'s own dispatch calls this with `false` — the
    /// ordinary context-determined case (BUG-24).
    fn if_expr_subst(
        &mut self,
        cond: &Expr,
        then: &Expr,
        els: &Expr,
        subst: &HashMap<&str, &Expr>,
        arrays: &ArrayScope,
        allow_shift: bool,
    ) -> String {
        // A condition that folds at compile time (typically on a `repeat`
        // variable) collapses to the taken branch — this is what keeps
        // `if i == 0 { cin } else { fa[i-1].cout }` from emitting the dead
        // `fa[-1]` arm at i == 0.
        if let Ok(c) = consteval::eval(cond, &self.env) {
            return if !c.is_zero() {
                self.expr_subst(then, subst, arrays)
            } else {
                self.expr_subst(els, subst, arrays)
            };
        }
        let decls = Rc::clone(&self.cur_decls);
        let c = self.expr_subst(cond, subst, arrays);
        // `allow_shift` (BUG-24's fix, BUG-55's correction): `eval_ctx`'s
        // `IfExpr` arm propagates the SAME `expected_width` the `IfExpr`
        // itself received into BOTH `then`/`els` — a shift branch here is
        // self-determined exactly when the `if` itself is, so this must
        // mirror the caller's own position instead of hardcoding `false`.
        // See `tests/self_determined_regression.rs` for proving cases on
        // both sides of that distinction.
        let t = self.expr_subst(then, subst, arrays);
        let t = self.hoist_width_effect_operand(then, t, &decls, allow_shift);
        let f = self.expr_subst(els, subst, arrays);
        let f = self.hoist_width_effect_operand(els, f, &decls, allow_shift);
        format!("(({c}) ? ({t}) : ({f}))")
    }

    /// `ExprKind::Match`'s own rendering, factored out for the same reason
    /// as `if_expr_subst` immediately above — see its doc comment.
    fn match_subst(
        &mut self,
        scrutinee: &Expr,
        arms: &[crate::ast::Arm],
        subst: &HashMap<&str, &Expr>,
        arrays: &ArrayScope,
        allow_shift: bool,
    ) -> String {
        // Nested ternaries; the final arm becomes the default.
        let s = self.expr_subst(scrutinee, subst, arrays);
        let mut out = String::new();
        let mut closing = 0usize;
        for (arm_idx, arm) in arms.iter().enumerate() {
            // For tagged enum patterns with payload bindings, build a
            // substitution map: binding_name → scrutinee[hi:lo] slice expr.
            // These are merged into `subst` when rendering the arm value.
            let binding_exprs: Vec<(String, Expr)> = self.arm_binding_exprs(arm, scrutinee);
            let mut arm_subst: HashMap<&str, &Expr> = subst.clone();
            for (name, expr) in &binding_exprs {
                arm_subst.insert(name.as_str(), expr);
            }

            let v = self.expr_subst(&arm.value, &arm_subst, arrays);
            let decls = Rc::clone(&self.cur_decls);
            // `allow_shift` — same reason as `IfExpr` above: `eval_ctx`'s
            // `Match` arm propagates the SAME `expected_width` into
            // `arm.value`, so a shift arm here is self-determined exactly
            // when the `match` itself is (BUG-55).
            let v = self.hoist_width_effect_operand(&arm.value, v, &decls, allow_shift);
            let is_last = arm_idx == arms.len() - 1;
            let is_wild = arm.patterns.iter().any(|p| matches!(p, Pattern::Wildcard));
            if is_last || is_wild {
                out.push_str(&v);
                break;
            }
            let conds: Vec<String> = arm
                .patterns
                .iter()
                .map(|p| match p {
                    Pattern::Int { value, raw } => {
                        format!("({s} == {})", verilog_literal(value, raw))
                    }
                    Pattern::IntMask {
                        value, mask, width, ..
                    } => {
                        // `(s & 'bMASK) == 'bVALUE`, both sized to the
                        // pattern width (don't-care bits are 0 in both).
                        let w = *width as usize;
                        format!("(({s} & 'b{:0w$b}) == 'b{:0w$b})", mask, value, w = w)
                    }
                    Pattern::Bool(b) => {
                        format!("({s} == {})", if *b { "1'b1" } else { "1'b0" })
                    }
                    Pattern::Variant {
                        enum_name,
                        variant,
                        bindings: _,
                    } => self.variant_cond(&s, &enum_name.name, &variant.name),
                    Pattern::Wildcard => "1'b1".to_string(),
                })
                .collect();
            out.push_str(&format!("({}) ? ({v}) : (", conds.join(" || ")));
            closing += 1;
        }
        out.push_str(&")".repeat(closing));
        format!("({out})")
    }

    /// Render an index or slice bound. A non-literal that folds at compile
    /// time — a `repeat` variable or arithmetic over one, like `i + 1` —
    /// collapses to its decimal value (`sum[i] → sum[2]`); plain literals
    /// keep their written base, and anything symbolic (a parameter, a
    /// dynamic signal index) renders unchanged.
    pub(super) fn index_expr(
        &mut self,
        e: &Expr,
        subst: &HashMap<&str, &Expr>,
        arrays: &ArrayScope,
    ) -> String {
        if !matches!(e.kind, ExprKind::Int { .. })
            && let Ok(v) = consteval::eval(e, &self.env)
        {
            return v.to_string();
        }
        self.expr_subst(e, subst, arrays)
    }

    /// Render an expression to Verilog text. Compound results are wrapped
    /// in parentheses unconditionally — correctness over prettiness; a
    /// future emitter can use real precedence (architecture invariant #6).
    /// `subst` maps child-module parameter names to instance arguments.
    pub(super) fn expr_subst(
        &mut self,
        e: &Expr,
        subst: &HashMap<&str, &Expr>,
        arrays: &ArrayScope,
    ) -> String {
        match &e.kind {
            ExprKind::Int { value, raw } => verilog_literal(value, raw),
            ExprKind::Bool(b) => if *b { "1'b1" } else { "1'b0" }.to_string(),
            ExprKind::Ident(name) => {
                // Child-param substitution wins (we're rendering a child's
                // port width); then compile-time consts/`repeat` vars fold
                // to literals; otherwise it's a symbolic signal or param.
                if let Some(replacement) = subst.get(name.as_str()) {
                    let r = self.expr(replacement);
                    // Parens protect compound argument expressions; an
                    // atomic render (a literal or a bare name — e.g. a
                    // child const folded by `instance()`) needs none.
                    if r.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '\'')
                    {
                        r
                    } else {
                        format!("({r})")
                    }
                } else if let Some(v) = self.env.get(name.as_str()) {
                    v.to_string()
                } else {
                    name.clone()
                }
            }
            ExprKind::Field { base, field } => {
                // Enum.Variant → localparam; inst.port → auto wire.
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.project.enums.contains_key(base_name) {
                        return enum_const(base_name, &field.name);
                    }
                    return format!("{}_{}", base_name, field.name);
                }
                // Array instance output `fa[i].port` → wire `fa__<i>_port`
                // (the index folds against the current `repeat` env).
                if let ExprKind::Index { base: arr, index } = &base.kind
                    && let ExprKind::Ident(arr_name) = &arr.kind
                {
                    return match self.eval_const(index) {
                        Some(n) => format!("{arr_name}__{n}_{}", field.name),
                        None => "0".into(), // eval_const already reported
                    };
                }
                self.err(
                    e.span,
                    "field access on a complex expression is not supported",
                    "",
                );
                "0".into()
            }
            ExprKind::Unary { op, expr: inner } => {
                let x = self.render_shift_ctx_operand(inner, subst, arrays, true);
                // BUG-60 (docs/audit/bugs.md): a reduction's OPERAND is
                // itself a self-determined position — Verilog computes
                // `&x`/`|x`/`^x` over `x`'s own rendered width, never the
                // reduction's (always 1-bit) RESULT width, which is all
                // `verilog_self_determined_kind`'s `RedAnd|RedOr|RedXor`
                // arm answers. `extend(a, 8)` renders as the bare `(a)`
                // here exactly like everywhere else self-determined, so
                // without this hoist the reduction silently runs over
                // `a`'s narrower 4 bits instead of the zero/sign-extended
                // 8. Same hoist shape `SignedCast`/`UnsignedCast`/
                // `Encoding` already use for their own argument, just
                // gated to the reduction ops — every other unary op
                // (`-`/`~`/`!`) leaves its operand's rendered width
                // unchanged, so nothing to hoist there.
                let x = match op {
                    UnOp::RedAnd | UnOp::RedOr | UnOp::RedXor => {
                        let decls = Rc::clone(&self.cur_decls);
                        match crate::emit_verilog::kinds::infer_kind(inner, &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(inner, x, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(inner, &x, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(
                                        inner,
                                        "Unary reduction operand",
                                        x,
                                        false,
                                    )
                                }),
                        }
                    }
                    _ => x,
                };
                let sym = match op {
                    UnOp::Neg => "-",
                    UnOp::BitNot => "~",
                    UnOp::LogicNot => "!",
                    UnOp::RedAnd => "&",
                    UnOp::RedOr => "|",
                    UnOp::RedXor => "^",
                };
                format!("({sym}{x})")
            }
            // `??` unwrap form: reaches here only when the WHOLE expression
            // is used in a scalar (non-bundle) position — a `wire`/`Drive`
            // RHS like `raw ?? 0`. The OR-mux form (result itself
            // bundle-typed) never renders as a single scalar expression; it
            // is desugared at the wire-init/Drive/port-connection/fn-arg
            // level instead (Task 8), so it never reaches this arm.
            ExprKind::Binary {
                op: BinOp::Coalesce,
                lhs,
                rhs,
            } => {
                let valid = self.expr_subst(&field_expr(lhs, "valid"), subst, arrays);
                let data = self.expr_subst(&field_expr(lhs, "data"), subst, arrays);
                let fallback = self.expr_subst(rhs, subst, arrays);
                format!("({valid} ? {data} : {fallback})")
            }
            ExprKind::Binary { op, lhs, rhs } => {
                // Hoist BOTH operands, but ONLY for a comparison — those
                // are the one family whose operands the Verilog LRM
                // itself always treats as self-determined regardless of
                // where the comparison sits (relational/equality operator
                // operands, LRM 5.5.1), so `verilog_self_determined_kind`'s
                // answer is meaningful for `lhs`/`rhs` here. Every OTHER
                // operator's operands are normally CONTEXT-determined (they
                // inherit the surrounding assignment/expression's width,
                // not their own) — checking them against the self-
                // determined rule here would be comparing against the
                // wrong rule entirely, not just an unnecessary no-op:
                // confirmed empirically (`shift`'s golden test) that doing
                // this unconditionally for e.g. `Shl` spuriously hoists a
                // plain top-level `y = 1 << 3`, whose un-hoisted rendering
                // was already correct (real Verilog computes it via the
                // ASSIGNMENT's context there, not a self-determined rule).
                // A width-matching/wrap-family/lossless operator's own
                // MEMBERSHIP in a genuinely self-determined position (a
                // concat/replicate part, `extend()`'s argument) is instead
                // caught at those call sites directly (`ExprKind::Concat`/
                // `Replicate` above, `Builtin::Extend` below) — hoisting
                // the WHOLE sub-expression there, not by second-guessing
                // this shared arm's own `lhs`/`rhs`.
                let (l, r) = if matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                ) {
                    let decls = Rc::clone(&self.cur_decls);
                    let l = {
                        let text = self.expr_subst(lhs, subst, arrays);
                        match crate::emit_verilog::kinds::infer_kind(lhs, &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(lhs, text, k, &decls),
                            // NOT routed through `hoist_unresolved` (Task 1):
                            // a comparison's operands are self-determined
                            // in the Verilog LRM sense of "evaluated
                            // independently," but `==`/`<`/etc. auto-widen
                            // the NARROWER operand to match before
                            // comparing (LRM 5.5.1) — unlike a concat/
                            // reduction/cast member, which uses the
                            // operand's rendered bits AS-IS, a comparison
                            // only needs the VALUE right, and an
                            // un-hoisted parameter-driven operand (`x ==
                            // (DEPTH - 1)`, `examples/*/std/fifo.mimz`, a
                            // working golden shape) already gets that from
                            // Verilog's own widening — no exact self-
                            // determined width is required the way GAP-16's
                            // twelve call sites need one.
                            None => text,
                        }
                    };
                    let r = {
                        let text = self.expr_subst(rhs, subst, arrays);
                        match crate::emit_verilog::kinds::infer_kind(rhs, &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(rhs, text, k, &decls),
                            None => text, // see `lhs`'s arm immediately above
                        }
                    };
                    (l, r)
                } else {
                    // BUG-23 (docs/audit/bugs.md): every OTHER binary
                    // operator's operands are context-determined, but a
                    // width-effect operand (lossless or wrapping —
                    // `is_width_effect_binop`) sitting HERE, as a direct
                    // operand of this outer operator, still needs its
                    // own width pinned before Verilog's context
                    // propagation reaches it — hoisting is unconditional
                    // on shape (`hoist_width_effect_operand`), not a
                    // Kind-mismatch check, since a wrap operator's own
                    // `infer_kind`/`verilog_self_determined_kind` always
                    // AGREE (both compute `max(l, r)`, no growth) — the
                    // bug is context ESCAPE of the operand's internal
                    // arithmetic once it's textually embedded here, not
                    // a self-determined-position width disagreement, so
                    // the mismatch check alone would never catch it.
                    //
                    // `allow_shift` (BUG-24, narrowed after a regression —
                    // see `is_shift_binop`'s doc): the RHS of ANY binary
                    // operator is always self-determined for a shift child
                    // (`mimz-sim/src/sim/value.rs`'s `eval_ctx` always uses
                    // plain `eval` — `expected_width: None` — for `rhs`,
                    // regardless of `op`), so `true` unconditionally. The
                    // LHS is self-determined too UNLESS the outer `op` is
                    // ITSELF a shift, in which case the simulator threads
                    // its OWN `expected_width` into the LHS
                    // (`shift_ctx`-gated in `eval_ctx`) — a shift child
                    // sitting there must be left un-hoisted so real
                    // Verilog's ordinary context propagation reaches it,
                    // matching that threaded semantics instead of freezing
                    // it at the wrong (bottom-up, context-free) width.
                    let allow_shift_lhs = !matches!(op, BinOp::Shl | BinOp::Shr);
                    let l = self.render_shift_ctx_operand(lhs, subst, arrays, allow_shift_lhs);
                    let r = self.render_shift_ctx_operand(rhs, subst, arrays, true);
                    (l, r)
                };
                // BUG-56 (docs/audit/bugs.md): a bare literal operand of
                // one of the adapt-to-sibling operators renders as an
                // unsized token above (`verilog_literal`, via `expr_subst`)
                // — fine standing alone, but real Icarus refuses it once
                // THIS WHOLE expression ends up nested inside a concat/
                // replication member. Size it to the sibling's own
                // resolved width (the width the literal checker-legally
                // adapted to) whenever that's resolvable; unresolvable
                // (`None`) leaves it exactly as rendered above — the same
                // safe fallback every other GATE consumer in this file
                // uses, not a new risk.
                let (l, r) = if matches!(
                    op,
                    BinOp::BitAnd
                        | BinOp::BitOr
                        | BinOp::BitXor
                        | BinOp::AddWrap
                        | BinOp::SubWrap
                        | BinOp::MulWrap
                ) {
                    let decls = Rc::clone(&self.cur_decls);
                    let l = if let ExprKind::Int { value, raw } = &lhs.kind {
                        match crate::emit_verilog::kinds::infer_kind(rhs, &decls, &self.env) {
                            Some(k) => verilog_literal_sized(value, raw, k.width),
                            None => l,
                        }
                    } else {
                        l
                    };
                    let r = if let ExprKind::Int { value, raw } = &rhs.kind {
                        match crate::emit_verilog::kinds::infer_kind(lhs, &decls, &self.env) {
                            Some(k) => verilog_literal_sized(value, raw, k.width),
                            None => r,
                        }
                    } else {
                        r
                    };
                    (l, r)
                } else {
                    (l, r)
                };
                // Wrapping ops: hoisted above (BUG-23) whenever they are
                // a direct operand of another operator; a bare top-level
                // `y = a -% b` needs no hoist — the assignment target's
                // own declared width already pins it correctly.
                let sym = match op {
                    BinOp::Add | BinOp::AddWrap => "+",
                    BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*",
                    BinOp::Shl => "<<",
                    BinOp::Shr => ">>",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::LogicAnd => "&&",
                    BinOp::LogicOr => "||",
                    // Unreachable in practice: the arm above intercepts
                    // every `Coalesce` before the match reaches this one.
                    // Kept only because `op`'s static type still ranges
                    // over all of `BinOp` here, so the inner match must
                    // stay exhaustive.
                    BinOp::Coalesce => unreachable!("Coalesce is handled by the arm above"),
                };
                format!("({l} {sym} {r})")
            }
            // Factored into `if_expr_subst`/`match_subst` (BUG-55,
            // docs/audit/bugs.md) so `render_shift_ctx_operand` can
            // re-enter either with `allow_shift: true` when the whole
            // `if`/`match` sits in a self-determined position. `false`
            // here is the ordinary context-determined case (BUG-24).
            ExprKind::IfExpr { cond, then, els } => {
                self.if_expr_subst(cond, then, els, subst, arrays, false)
            }
            ExprKind::Match { scrutinee, arms } => {
                self.match_subst(scrutinee, arms, subst, arrays, false)
            }
            ExprKind::Concat(parts) => {
                let decls = Rc::clone(&self.cur_decls);
                let ps: Vec<String> = parts
                    .iter()
                    .map(|p| {
                        // `allow_shift: true` — `eval_ctx`'s `Concat`/
                        // `Replicate` arms always evaluate a part with
                        // plain `eval` (`expected_width: None`), so a
                        // shift part here is self-determined — and so is
                        // an `if`/`match` part's own branches (BUG-55),
                        // which `render_shift_ctx_operand` also handles.
                        let text = self.render_shift_ctx_operand(p, subst, arrays, true);
                        match crate::emit_verilog::kinds::infer_kind(p, &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(p, text, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(p, &text, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(p, "concat member", text, false)
                                }),
                        }
                    })
                    .collect();
                format!("{{{}}}", ps.join(", "))
            }
            ExprKind::Replicate { count, parts } => {
                let decls = Rc::clone(&self.cur_decls);
                let c = self.index_expr(count, subst, arrays);
                let ps: Vec<String> = parts
                    .iter()
                    .map(|p| {
                        // `allow_shift: true` — `eval_ctx`'s `Concat`/
                        // `Replicate` arms always evaluate a part with
                        // plain `eval` (`expected_width: None`), so a
                        // shift part here is self-determined — and so is
                        // an `if`/`match` part's own branches (BUG-55),
                        // which `render_shift_ctx_operand` also handles.
                        let text = self.render_shift_ctx_operand(p, subst, arrays, true);
                        match crate::emit_verilog::kinds::infer_kind(p, &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(p, text, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(p, &text, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(p, "replicate member", text, false)
                                }),
                        }
                    })
                    .collect();
                format!("{{{c}{{{}}}}}", ps.join(", "))
            }
            ExprKind::Index { base, index } => {
                // Indexing an array-typed param/`let` (elaborated to
                // `<name>_<i>` scalars, Task 7's convention) never indexes a
                // real Verilog array. A CONSTANT index resolves straight to
                // the matching scalar — zero cost. A runtime index generates
                // a ternary-chain mux over every element: the same shape a
                // user would hand-write with `if i==0 {...} else if i==1
                // {...}`, just emitter-synthesized.
                if let ExprKind::Ident(n) = &base.kind
                    && let Some((_, len)) = arrays.get(n)
                {
                    if let Ok(idx) = consteval::eval(index, &self.env) {
                        return format!("{n}_{idx}");
                    }
                    let idx = self.expr_subst(index, subst, arrays);
                    // A zero-length array is rejected by the checker (E0412)
                    // in the normal `mimz compile` pipeline, but this emitter
                    // is also exercised directly on unchecked ASTs (fuzzing)
                    // — `len - 1` below would underflow, so this must be a
                    // clean diagnostic, not a panic.
                    let Some(last) = len.checked_sub(1) else {
                        self.err(e.span, "array has no elements to index", "");
                        return "0".into();
                    };
                    let mut chain = format!("{n}_{last}"); // default: last element
                    for i in (0..last).rev() {
                        chain = format!("(({idx})=={i}) ? {n}_{i} : ({chain})");
                    }
                    return chain;
                }
                // `allow_shift: true` — `eval_ctx`'s `Index` arm evaluates
                // `base` with plain `eval` (self-determined).
                let b = self.render_shift_ctx_operand(base, subst, arrays, true);
                // BUG-61 (docs/audit/bugs.md): Verilog's bit-select
                // (`x[i]`) grammar only accepts a plain identifier as `x`,
                // the identical BUG-20 constraint `ExprKind::Slice` already
                // hoists for a few lines below — a composite base
                // (`extend(a,8)[7]`, `{a,b}[3]`) is a SYNTAX error in real
                // Verilog, not just a width mismatch, and nothing hoisted
                // it here before this fix. Same unconditional-on-shape
                // call `Slice`'s own arm uses; `false` for the same
                // reason — a bit-select's result is unsigned regardless of
                // the base's own declared signedness.
                let decls = Rc::clone(&self.cur_decls);
                let b = match crate::emit_verilog::kinds::infer_kind(base, &decls, &self.env) {
                    Some(k) => self.hoist_slice_base_if_needed(b, k.width, false, base.span),
                    None => self.hoist_unresolved(base, "bit-select base", b, true),
                };
                let i = self.index_expr(index, subst, arrays);
                format!("{b}[{i}]")
            }
            ExprKind::Slice { base, hi, lo } => {
                // BUG-20: Verilog's part-select (`x[hi:lo]`) grammar only
                // accepts a plain identifier as `x` — a composite base
                // expression (`(p0 & p1)[1:0]`) is a syntax error in real
                // Verilog, not just a width mismatch, so this hoists
                // unconditionally on shape (`is_plain_identifier`, inside
                // `hoist_slice_base_if_needed`) rather than on a Kind
                // mismatch. `infer_kind` returning `None` still guards the
                // width it needs to size the hoisted wire — a
                // `fn`-body/testbench/width slice base is left exactly as
                // before this task. Always `false` for `signed` — a
                // part-select's result is unsigned regardless of the
                // base's own declared signedness.
                let b = self.expr_subst(base, subst, arrays);
                let decls = Rc::clone(&self.cur_decls);
                let b = match crate::emit_verilog::kinds::infer_kind(base, &decls, &self.env) {
                    Some(k) => self.hoist_slice_base_if_needed(b, k.width, false, base.span),
                    None => self.hoist_unresolved(base, "slice base", b, true),
                };
                let h = self.index_expr(hi, subst, arrays);
                let l = self.index_expr(lo, subst, arrays);
                format!("{b}[{h}:{l}]")
            }
            ExprKind::FnCall { name, args } => {
                // Mark this function (and all transitive callees) for injection
                // at module-body top; then render as a Verilog function call.
                // An array-typed argument expands to the N scalar arguments the
                // callee's array param elaborated into (Task 7's `<name>_<i>`
                // port convention): an array LITERAL expands element-by-element,
                // and a bare array-typed name (param or `let`) expands to its
                // `<name>_<i>` scalars. A bundle-typed argument (BUG-10, see
                // `render_fn_decl`'s matching flatten) expands by the CALLEE's
                // declared param field NAMES, not the argument's own bundle
                // type — required so a structurally-matched but differently-
                // named/ordered argument (feature 2.9) still resolves to the
                // right `<arg>_<field>` wires, since flattened signal names
                // are always keyed by field name, never by declaration order.
                // Every other argument passes through 1:1.
                self.mark_fn_used(&name.name);
                let callee_params = self
                    .project
                    .funcs
                    .get(name.name.as_str())
                    .copied()
                    .map(|d| d.params.as_slice());
                let mut args_str: Vec<String> = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    let bundle_fields =
                        callee_params
                            .and_then(|params| params.get(i))
                            .and_then(|p| match &p.ty {
                                Type::Bundle {
                                    name: bname,
                                    args: bargs,
                                } => Some(self.resolve_bundle_fields(bname, bargs)),
                                Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                                    Some(self.resolve_bundle_fields(id, &[]))
                                }
                                _ => None,
                            });
                    match &a.kind {
                        ExprKind::ArrayLit(elems) => {
                            for el in elems {
                                args_str.push(self.expr_subst(el, subst, arrays));
                            }
                        }
                        ExprKind::Ident(n) if arrays.contains_key(n) => {
                            let (_, len) = &arrays[n];
                            for i in 0..*len {
                                args_str.push(format!("{n}_{i}"));
                            }
                        }
                        ExprKind::Ident(n) if bundle_fields.is_some() => {
                            for (fname, _) in bundle_fields.as_ref().unwrap() {
                                args_str.push(format!("{n}_{fname}"));
                            }
                        }
                        ExprKind::Binary {
                            op: BinOp::Coalesce,
                            lhs: clhs,
                            rhs: crhs,
                        } if bundle_fields.is_some() => {
                            let fnames: Vec<String> = bundle_fields
                                .as_ref()
                                .unwrap()
                                .iter()
                                .map(|(fname, _)| fname.clone())
                                .collect();
                            for fname in fnames {
                                let raw = self.coalesce_field_expr(clhs, crhs, &fname);
                                // `coalesce_field_expr` always wraps its result in
                                // exactly one outer paren pair; strip it here — a
                                // fn-call argument is already unambiguously
                                // delimited by `(`/`,`/`)`, and keeping the extra
                                // parens would make the first argument open with
                                // `((`, indistinguishable at a glance from the
                                // unexpanded single-argument bug this desugar
                                // replaces.
                                let trimmed = raw
                                    .strip_prefix('(')
                                    .and_then(|s| s.strip_suffix(')'))
                                    .unwrap_or(&raw)
                                    .to_string();
                                args_str.push(trimmed);
                            }
                        }
                        _ => {
                            // `allow_shift: true` — `eval_fn_call` evaluates
                            // every argument with plain `eval` FIRST, then
                            // separately extends it to the parameter's width
                            // AFTER (`extend_bits`); the extension is post-
                            // hoc, not threaded into evaluation, so a shift
                            // argument here is self-determined.
                            args_str.push(self.render_shift_ctx_operand(a, subst, arrays, true));
                        }
                    }
                }
                format!("{}({})", name.name, args_str.join(", "))
            }
            ExprKind::Call { func, args } => match func {
                Builtin::SignedCast => {
                    let decls = Rc::clone(&self.cur_decls);
                    // `allow_shift: true` — `Builtin::SignedCast`/
                    // `UnsignedCast` arguments are always evaluated by
                    // `eval_ctx`'s `Call` arm with plain `eval` (see
                    // `call`'s own match arms).
                    let text = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let hoisted =
                        match crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(&args[0], text, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(&args[0], &text, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(
                                        &args[0],
                                        "signed-cast operand",
                                        text,
                                        false,
                                    )
                                }),
                        };
                    format!("$signed({hoisted})")
                }
                Builtin::UnsignedCast => {
                    let decls = Rc::clone(&self.cur_decls);
                    // `allow_shift: true` — `Builtin::SignedCast`/
                    // `UnsignedCast` arguments are always evaluated by
                    // `eval_ctx`'s `Call` arm with plain `eval` (see
                    // `call`'s own match arms).
                    let text = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let hoisted =
                        match crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(&args[0], text, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(&args[0], &text, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(
                                        &args[0],
                                        "unsigned-cast operand",
                                        text,
                                        false,
                                    )
                                }),
                        };
                    format!("$unsigned({hoisted})")
                }
                Builtin::Encoding => {
                    let decls = Rc::clone(&self.cur_decls);
                    // `allow_shift: true` — mirrors `SignedCast`/
                    // `UnsignedCast` immediately above: this builtin's
                    // argument is always evaluated by plain `eval`, never
                    // `eval_ctx`, so a shift argument here is
                    // self-determined.
                    let text = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let hoisted =
                        match crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(&args[0], text, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(&args[0], &text, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(&args[0], "encoding operand", text, false)
                                }),
                        };
                    format!("$unsigned({hoisted})")
                }
                // Extension is context-automatic in Verilog assignments:
                // unsigned operands zero-extend; `signed`-declared ones
                // SIGN-extend (declarations carry `signed`, see
                // `width_subst`). The checker has already verified widths.
                //
                // BUG-18: that implicit widening only fires in
                // context-determined positions. A concatenation operand is
                // self-determined (Verilog LRM) — it must carry its own
                // width. A named signal already does (from its declaration),
                // but an unsized literal does NOT, so `extend(3, 12)` inside a
                // `{...}` is rejected by Icarus ("indefinite width"). When the
                // argument resolves to a constant (a literal, possibly through
                // nested extend/trunc), render it as an explicitly-sized
                // literal at this extend's target width; otherwise fall back
                // to the passthrough — which is syntactically safe (parens
                // are always valid Verilog), but NOT semantically safe for
                // every argument shape.
                //
                // BUG-19 (docs/audit/bugs.md, its second filed repro, T2 v2):
                // parens do not stop Verilog's context propagation — a plain
                // `((expr))` is still a context-determined position, so when
                // this whole `extend(...)` call is itself an operand of a
                // wider context-determined operator (e.g. `&` next to a
                // concat sibling), Verilog re-derives `args[0]`'s own
                // arithmetic AT THAT WIDER WIDTH instead of at its own
                // natural one — silently changing a wrap operator's modulus
                // (`+%`/`-%`) or a lossless operator's growth (BUG-19's own
                // doc: "any operator whose spec-defined result depends on
                // the width it was originally evaluated at") — and, per
                // BUG-24, a shift (`<<`/`>>`), whose LEFT operand is
                // likewise context-determined in real Verilog. Only THAT
                // family is unsound this way — `p0 & p1`, a comparison, a
                // bare signal, all give the SAME value no matter what width
                // Verilog happens to (re)compute them at, so hoisting them
                // here would be pure noise (confirmed empirically: doing
                // this unconditionally for any non-identifier `args[0]`
                // spuriously hoisted `extend(p0 & p1, N)`, changing golden
                // output with no correctness benefit). So this checks
                // `args[0]`'s own TOP-level operator directly, not a Kind
                // mismatch (`mimz_kind`/`verilog_self_determined_kind` agree
                // on `args[0]`'s own width here regardless — the bug is
                // context ESCAPE of ITS INTERNAL ARITHMETIC, not a self-
                // determined-position width disagreement, so the usual
                // mismatch check would never fire): when it IS one of the
                // width-effect operators, hoist it into its own wire at its
                // own natural width (same shape-based, unconditional-once-
                // triggered mechanism BUG-20's `Slice` fix below uses) so
                // its internal arithmetic evaluates in an assignment fixed
                // at exactly that width — then this extend's own (still-
                // unsized) passthrough zero/sign-extends the now-fixed wire
                // value correctly, matching mimz's intended semantics.
                Builtin::Extend => {
                    match (
                        self.resolve_const_value(&args[0]),
                        consteval::eval(&args[1], &self.env).ok(),
                    ) {
                        (Some(v), Some(w)) => {
                            let vw = crate::bits::natural_width(&v).max(1);
                            format!(
                                "{}'d{}",
                                w.to_i128_saturating() as u128,
                                crate::bits::bits_to_decimal_string(&v, vw, false)
                            )
                        }
                        // `_` covers BOTH "width doesn't fold" (a module
                        // `parameter`, Task 3/BUG-62(b)'s own case) and
                        // "value doesn't fold" — the same context-
                        // determined passthrough is correct for both here
                        // at this call's OWN position: whichever position
                        // actually NEEDS the width-symbolic widening made
                        // explicit (a concat/reduction/cast/nand member —
                        // a genuinely self-determined one) asks for it
                        // itself, via `try_widen_symbolic_extend`, from
                        // its OWN `None`-arm below, not from here — doing
                        // it unconditionally at every position (including
                        // an ordinary context-determined operand, e.g. `sr
                        // <= old | extend(din, WIDTH)`) changed nothing
                        // about correctness but needlessly rewrote already-
                        // correct shipped goldens.
                        _ => {
                            // `allow_shift: true` — BUG-47 (docs/audit/bugs.md).
                            //
                            // This was `false`, justified by the simulator
                            // threading THIS extend's own target width `n`
                            // into evaluating its argument via
                            // `eval_ctx(r, &args[0], Some(n))` — under which a
                            // shift argument really was context-determined in
                            // mimz's own model, and hoisting it would have
                            // disagreed with the reference semantics.
                            //
                            // That function no longer exists. BUG-34's
                            // fused-shift rework replaced it: a shift is now
                            // evaluated by `binary::eval_shift_chain`, which
                            // resolves the chain's own bottom-up width and
                            // never consults an ambient expected width. The
                            // guard outlived its reason, and what it left
                            // behind is a silent miscompile —
                            // `extend(p1 >> 2, 20)` with `p1: signed[4]` =
                            // `0b1111` is 3 in mimz (the shift happens within
                            // 4 bits) and 262143 in Verilog, which sign-
                            // extends `p1` to the surrounding 20 bits first.
                            //
                            // BUG-6's own guard (`examples/english/shift.mimz`:
                            // `extend(1 << 3, 8)` must stay 8, not collapse to
                            // 0) still holds through the hoist — `infer_kind`
                            // gives `1 << 3` a grown width of its own, so the
                            // hoisted wire is wide enough to hold 8.
                            //
                            // A shift as the LHS of ANOTHER shift stays
                            // un-hoisted: that position is gated separately by
                            // `allow_shift_lhs` in the `Binary` arm above and
                            // is a genuine fused-chain case (BUG-24/BUG-34),
                            // unlike this one.
                            //
                            // BUG-55 (docs/audit/bugs.md): this argument can
                            // itself be an `if`/`match` wrapping a shift one
                            // level deeper (`extend(match s {..., _ => p0 >>
                            // k}, N)`) — `render_shift_ctx_operand` recurses
                            // into that branch with the same self-determined
                            // treatment, not just this direct-child check.
                            let text = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                            format!("({text})")
                        }
                    }
                }
                // `allow_shift: true` for the remaining `Builtin` arms below
                // (`Trunc`/`Min`/`Max`/`Abs`/`Nand`/`Nor`/`Xnor`) — every one
                // of `call`'s own match arms for these evaluates its
                // argument(s) with plain `eval`, never `eval_ctx`, so a
                // shift argument is always self-determined here.
                Builtin::Trunc => {
                    let decls = Rc::clone(&self.cur_decls);
                    let x = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    // BUG-36 (docs/audit/bugs.md): `trunc` renders as an
                    // explicit part-select `x[N-1:0]` — the same BUG-20
                    // grammar constraint as `ExprKind::Slice`: Verilog's
                    // part-select only accepts a plain identifier as its
                    // base. `hoist_width_effect_operand` above only hoists
                    // a width-effect-binop/shift base (BUG-23/24's concern,
                    // value correctness); it left a non-identifier base of
                    // any OTHER shape (a concat, here) untouched. Hoist
                    // unconditionally on shape, mirroring `ExprKind::Slice`'s
                    // own `hoist_slice_base_if_needed` call exactly.
                    let base_kind =
                        crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env);
                    let x = match base_kind {
                        Some(k) => self.hoist_slice_base_if_needed(x, k.width, false, args[0].span),
                        None => self.hoist_unresolved(&args[0], "trunc base", x, true),
                    };
                    let n = self.expr_subst(&args[1], subst, arrays);
                    let sel = format!("{x}[({n})-1:0]");
                    // BUG-44 (docs/audit/bugs.md): `trunc` KEEPS its operand's
                    // signedness — the checker (`widths/ops/builtins.rs`:
                    // `Ty::Signed(_) => Ty::Signed(n)`), the simulator
                    // (`value/fn_eval.rs`: `Val::new(.., v.signed)`) and
                    // `kinds.rs`'s own `signed: base_signed` all agree. The
                    // part-select above does NOT: a part-select is
                    // unconditionally unsigned in Verilog-2005 (IEEE 1364-2005
                    // section 5.1.7) even off a `signed` wire. Unwrapped, the
                    // lost signedness both mis-extends into a wider signed
                    // target AND demotes any surrounding arithmetic to
                    // unsigned (mixing one unsigned operand makes the whole
                    // expression unsigned), discarding a sibling's `$signed`
                    // too — the shape the fuzz seed took.
                    //
                    // `ExprKind::Slice` deliberately needs no such wrap:
                    // `width_rules::slice_result` types a slice `signed:
                    // false` (BUG-21), which is exactly Verilog's own rule.
                    // `trunc` is the one construct where a mimz-signed value
                    // renders as an always-unsigned Verilog construct.
                    //
                    // An unresolvable `base_kind` keeps the pre-existing
                    // unsigned rendering — the same "leave the text as it
                    // was" residue every other `infer_kind` call site here
                    // already accepts, not a new failure mode.
                    if base_kind.is_some_and(|k| k.signed) {
                        format!("$signed({sel})")
                    } else {
                        sel
                    }
                }
                Builtin::Min => {
                    let a = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let b = self.render_shift_ctx_operand(&args[1], subst, arrays, true);
                    format!("(({a} < {b}) ? ({a}) : ({b}))")
                }
                Builtin::Max => {
                    let a = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let b = self.render_shift_ctx_operand(&args[1], subst, arrays, true);
                    format!("(({a} < {b}) ? ({b}) : ({a}))")
                }
                // Result is `signed[N+1]`; the assignment context sign-extends
                // both ternary arms (the operand is declared `signed`).
                Builtin::Abs => {
                    let x = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    format!("(({x} < 0) ? (-{x}) : ({x}))")
                }
                // Verilog-2005 negated reduction operators — one bit out.
                // BUG-60 (docs/audit/bugs.md): same operand hoist as
                // `ExprKind::Unary`'s `RedAnd|RedOr|RedXor` above — a
                // negated reduction's argument is self-determined at its
                // own rendered width, not the 1-bit result width
                // `verilog_self_determined_kind`'s `Nand|Nor|Xnor` arm
                // answers. `nand(extend(a, 8))` renders `(a)` unhoisted
                // without this, and-reduces over 4 bits instead of 8.
                Builtin::Nand => {
                    let x = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let decls = Rc::clone(&self.cur_decls);
                    let x =
                        match crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(&args[0], x, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(&args[0], &x, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(&args[0], "nand operand", x, false)
                                }),
                        };
                    format!("(~&({x}))")
                }
                Builtin::Nor => {
                    let x = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let decls = Rc::clone(&self.cur_decls);
                    let x =
                        match crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(&args[0], x, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(&args[0], &x, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(&args[0], "nor operand", x, false)
                                }),
                        };
                    format!("(~|({x}))")
                }
                Builtin::Xnor => {
                    let x = self.render_shift_ctx_operand(&args[0], subst, arrays, true);
                    let decls = Rc::clone(&self.cur_decls);
                    let x =
                        match crate::emit_verilog::kinds::infer_kind(&args[0], &decls, &self.env) {
                            Some(k) => self.hoist_if_needed(&args[0], x, k, &decls),
                            None => self
                                .try_widen_symbolic_extend(&args[0], &x, subst, arrays)
                                .unwrap_or_else(|| {
                                    self.hoist_unresolved(&args[0], "xnor operand", x, false)
                                }),
                        };
                    format!("(~^({x}))")
                }
                // `clog2(n)` folds to a literal when `n` is a constant (a literal
                // or `const`). Of a module PARAMETER it stays symbolic, so it
                // lowers to a call of the injected Verilog-2005 `clog2` constant
                // function — except in a port width, where that function (body-
                // scoped) can't be reached, so it is an honest error.
                Builtin::Clog2 => match consteval::eval(&args[0], &self.env) {
                    Ok(n) if n.to_i128_saturating() >= 1 => {
                        consteval::clog2_bits(n.to_i128_saturating() as u128).to_string()
                    }
                    Ok(_) => "1".to_string(), // n < 1: the checker already reported E0202
                    Err(_) if self.emitting_port => {
                        self.err(
                            args[0].span,
                            "`clog2` of a parameter cannot size a port — the Verilog-2005 \
                             constant function lives in the module body, out of the port list's reach",
                            "size a body `reg`/`wire` with it instead, or pass the width \
                             as its own parameter",
                        );
                        "1".to_string()
                    }
                    Err(_) => {
                        self.clog2_fn_used = true;
                        format!("clog2({})", self.expr_subst(&args[0], subst, arrays))
                    }
                },
                Builtin::SyncDoubleFlop | Builtin::SyncPulse => {
                    unreachable!(
                        "sync.double_flop/sync.pulse must be lowered by \
                         ast::sync_prim_lower::expand_sync_prims before reaching \
                         Verilog rendering — a later task wires that call in"
                    )
                }
            },
            // BundleLit is only valid as the direct RHS of a Drive or Wire init;
            // emit_drives handles it before calling expr(). Reaching here means
            // a bundle literal in an unsupported position (e.g. inside an operator).
            // Emit a safe placeholder — the checker should have caught this.
            ExprKind::BundleLit(_) => "0".into(),
            ExprKind::ArrayLit(_) => unreachable!("Task 8 or Task 9 wires this up"),
            ExprKind::EnumConstruct {
                enum_name,
                variant,
                args,
            } => {
                let edecl = self
                    .project
                    .first_enum(&enum_name.name)
                    .expect("checker already validated this enum exists");
                let total_w = edecl
                    .inferred_total_width
                    .get()
                    .expect("checker must run before emit") as u128;
                let tag_w = clog2(edecl.variants.len()) as u128;
                let max_payload_w = total_w - tag_w;
                let idx = edecl
                    .variants
                    .iter()
                    .position(|v| v.name.name == variant.name)
                    .expect("checker already validated this variant exists");
                if max_payload_w == 0 {
                    // Tag-only enum: same localparam a bare `Enum.Variant`
                    // reference (`ExprKind::Field`) already emits.
                    return enum_const(&enum_name.name, &variant.name);
                }
                let decl_v = &edecl.variants[idx];
                // Packs MSB-first in the payload region, padding the LOW
                // end — mirrors `arm_binding_exprs`'s slicing exactly, so
                // construction and pattern-match extraction agree on layout.
                //
                // Every part must be an explicitly-SIZED Verilog literal or
                // a self-sized signal reference: inside a `{}` concatenation
                // an unsized decimal literal defaults to 32 bits (LRM
                // §5.7.1), which would corrupt the tag/field/padding
                // boundaries — so any argument that folds to a compile-time
                // constant (a bare literal, `-3`, `2+1`, …) is rendered with
                // its field's own width prefix rather than left to
                // `expr_subst`'s ordinary (unsized) literal rendering. A
                // negative constant is masked to its field-width two's-
                // complement bit pattern first — concatenation is always an
                // unsigned/self-determined context, so the sign is encoded
                // in the bits, not the literal's base.
                let mut parts = Vec::new();
                // `tag_w` is `clog2(variant_count)`, which floors at 1 for
                // any legal (>=1-variant) enum — this branch always taken
                // in practice. Guarded anyway as defense in depth, matching
                // the padding guard below for the symmetric zero-width case.
                if tag_w > 0 {
                    parts.push(format!("{tag_w}'d{idx}"));
                }
                let mut used_w = 0u128;
                for (a, field) in args.iter().zip(decl_v.fields.iter()) {
                    let field_w: u128 = match &field.ty {
                        Type::Bit => 1,
                        Type::Bits(e) | Type::Signed(e) => consteval::eval(e, &self.env)
                            .map(|x| x.to_i128_saturating())
                            .unwrap_or(0)
                            as u128,
                        Type::Named(_) | Type::Bundle { .. } | Type::Array { .. } => 0,
                    };
                    used_w += field_w;
                    parts.push(match consteval::eval(a, &self.env) {
                        Ok(v) => {
                            let fw = field_w as u32;
                            // `wide::extend`, not a plain `to_limbs` reshape —
                            // a negative payload value must be SIGN-extended
                            // to `fw` bits before masking, not zero-padded
                            // (zero-padding a negative value's raw bit
                            // pattern silently drops its sign).
                            let mut limbs = crate::wide::extend(
                                &crate::bits::to_limbs(&v.bits, v.width),
                                v.width,
                                fw,
                                v.signed,
                            );
                            crate::wide::mask_to_width(&mut limbs, fw);
                            let masked = crate::bits::from_limbs(limbs, fw);
                            format!(
                                "{field_w}'d{}",
                                crate::bits::bits_to_decimal_string(&masked, fw, false)
                            )
                        }
                        Err(_) => {
                            // `allow_shift: true` — a payload field's value
                            // is evaluated independently of any enclosing
                            // width context (each field is separately
                            // masked to its own `field_w` right here).
                            self.render_shift_ctx_operand(a, subst, arrays, true)
                        }
                    });
                }
                let padding_w = max_payload_w - used_w;
                if padding_w > 0 {
                    parts.push(format!("{padding_w}'d0"));
                }
                format!("{{{}}}", parts.join(", "))
            }
        }
    }

    /// Render the condition for a `Pattern::Variant` match arm:
    /// - tag-only enum: `(s == ENUM_VARIANT)` (unchanged from before)
    /// - tagged enum: `(s[total-1:max_payload_w] == tag_w'd<index>)`
    fn variant_cond(&self, s: &str, enum_name: &str, variant_name: &str) -> String {
        let Some(edecl) = self.project.first_enum(enum_name) else {
            return format!("({s} == {})", enum_const(enum_name, variant_name));
        };
        let total_w = match edecl.inferred_total_width.get() {
            Some(w) => w as u128,
            None => return format!("({s} == {})", enum_const(enum_name, variant_name)),
        };
        let tag_w = clog2(edecl.variants.len()) as u128;
        let max_payload_w = total_w - tag_w;
        if max_payload_w == 0 {
            // Tag-only: compare the whole signal to the localparam.
            format!("({s} == {})", enum_const(enum_name, variant_name))
        } else {
            // Tagged: compare tag bits only (MSBs).
            let idx = edecl
                .variants
                .iter()
                .position(|v| v.name.name == variant_name)
                .expect("variant not found — checker must run before emitter");
            let hi = total_w - 1;
            let lo = max_payload_w;
            format!("({s}[{hi}:{lo}] == {tag_w}'d{idx})")
        }
    }

    /// For a tagged enum match arm, build a list of `(binding_name, slice_expr)`
    /// pairs that map each pattern binding to the payload slice of `scrutinee`.
    /// Returns empty if the arm has no variant pattern with bindings.
    fn arm_binding_exprs(&self, arm: &Arm, scrutinee: &Expr) -> Vec<(String, Expr)> {
        for pat in &arm.patterns {
            let Pattern::Variant {
                enum_name,
                variant,
                bindings,
            } = pat
            else {
                continue;
            };
            if bindings.is_empty() {
                continue;
            }
            let Some(edecl) = self.project.first_enum(&enum_name.name) else {
                continue;
            };
            let total_w = match edecl.inferred_total_width.get() {
                Some(w) => w as u128,
                None => continue,
            };
            let tag_w = clog2(edecl.variants.len()) as u128;
            let max_payload_w = total_w - tag_w;
            if max_payload_w == 0 {
                continue;
            }
            let Some(vdecl) = edecl.variants.iter().find(|v| v.name.name == variant.name) else {
                continue;
            };
            // Pack fields MSB-first in the payload region [max_payload_w-1 : 0].
            let mut cursor = max_payload_w;
            let mut out = Vec::new();
            debug_assert_eq!(
                bindings.len(),
                vdecl.fields.len(),
                "E0806 should have rejected this"
            );
            for (field, binding) in vdecl.fields.iter().zip(bindings.iter()) {
                let field_w: u128 = match &field.ty {
                    Type::Bit => 1,
                    Type::Bits(e) | Type::Signed(e) => consteval::eval(e, &self.env)
                        .map(|x| x.to_i128_saturating())
                        .unwrap_or(0)
                        as u128,
                    Type::Named(_) => 0, // E0807: already rejected by checker
                    // Bundles are not valid enum payload fields (checker enforces).
                    Type::Bundle { .. } => 0,
                    // Arrays sit in the SAME category as bundles here: neither is a
                    // scalar bit-vector payload field. An array field folds to 0
                    // (skipped below, like a bundle), matching the sibling arm
                    // exactly rather than inventing new behavior.
                    Type::Array { .. } => 0,
                };
                debug_assert!(
                    field_w > 0,
                    "E0807 should have rejected zero-width payload fields before emit/sim"
                );
                if field_w == 0 {
                    continue;
                }
                let hi = cursor - 1;
                let lo = cursor - field_w;
                cursor -= field_w;
                let sp = scrutinee.span;
                let slice_expr = Expr {
                    kind: ExprKind::Slice {
                        base: Box::new(scrutinee.clone()),
                        hi: Box::new(Expr {
                            kind: ExprKind::Int {
                                value: hi.into(),
                                raw: hi.to_string(),
                            },
                            span: sp,
                        }),
                        lo: Box::new(Expr {
                            kind: ExprKind::Int {
                                value: lo.into(),
                                raw: lo.to_string(),
                            },
                            span: sp,
                        }),
                    },
                    span: sp,
                };
                out.push((binding.name.clone(), slice_expr));
            }
            return out;
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_plain_identifier_accepts_and_rejects_correctly() {
        assert!(is_plain_identifier("p0"));
        assert!(is_plain_identifier("__mimz_sub_3"));
        assert!(!is_plain_identifier("(p0 & p1)"));
        assert!(!is_plain_identifier("3"));
        assert!(!is_plain_identifier(""));
    }
}
