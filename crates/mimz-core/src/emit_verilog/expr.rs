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

/// True iff `kinds::infer_kind` can resolve `expr`'s `Kind` against `decls`
/// WITHOUT panicking — the pre-check every hoist call site below runs
/// before ever calling `Emitter::hoist_if_needed` (which calls `infer_kind`
/// unconditionally, first thing, with no such check of its own).
///
/// Mirrors `infer_kind`'s own traversal shape (same `ExprKind`s, same
/// eagerness — e.g. a `Binary`'s `lhs` is checked even for `Eq`/`Ne`, since
/// `infer_kind`'s `l = infer_kind(lhs, decls)` runs before it looks at
/// `op`) and `kinds::const_fold`'s literal-only requirement for slice
/// bounds/extend-trunc widths, without ever panicking itself.
///
/// This exists because `decls` (`Emitter::cur_decls`, built once per
/// module from its own Port/Wire/Reg declarations) is deliberately NOT a
/// complete symbol table: a `fn` body's own params/`let`s, a module
/// `parameter` (kept symbolic by design — see `build_decls`'s own doc),
/// and the testbench emitter's signals (which never populate `cur_decls`
/// at all — the testbench `Emitter` never calls `module()`) are all real
/// `Ident`s `expr_subst` renders through the exact same Concat/Replicate/
/// Binary/Slice/Cast arms this task wires hoisting into, and none of them
/// are ever keys in `decls`. Without this guard, `hoist_if_needed` would
/// panic compiling any of those — a testbench `expect sum == 15`, a `fn`
/// body's own arithmetic, or a port width like `bits[W + 1]` — instead of
/// just rendering them exactly as before this task (the correct behavior:
/// hoisting is only ever needed for the module-body value expressions
/// BUG-19/BUG-20 are about).
fn kind_is_inferrable(expr: &Expr, decls: &HashMap<String, crate::width_rules::Kind>) -> bool {
    match &expr.kind {
        ExprKind::Int { .. } | ExprKind::Bool(_) => true,
        ExprKind::Ident(name) => decls.contains_key(name),
        ExprKind::Unary { expr: inner, .. } => kind_is_inferrable(inner, decls),
        // `infer_kind`'s `Binary` arm computes `l = infer_kind(lhs, decls)`
        // unconditionally before it even looks at `op` (and `r` too, for
        // every arm but `Eq`/`Ne`/`Lt`/`Le`/`Gt`/`Ge`/`LogicAnd`/`LogicOr`)
        // — checking both sides regardless of `op` is conservative (may
        // skip a hoist that would have been safe) but never wrong.
        ExprKind::Binary { lhs, rhs, .. } => {
            kind_is_inferrable(lhs, decls) && kind_is_inferrable(rhs, decls)
        }
        ExprKind::Concat(parts) | ExprKind::Replicate { parts, .. } => {
            parts.iter().all(|p| kind_is_inferrable(p, decls))
        }
        // `infer_kind`'s `Slice` arm never looks at `base` at all — only
        // `hi`/`lo`, via `const_fold`, which panics on anything but a
        // literal `Int` node (NOT just "constant-foldable": `W - 1` is
        // compile-time-constant but is a `Binary` node, not an `Int` one,
        // and `const_fold` does not evaluate it).
        ExprKind::Slice { hi, lo, .. } => {
            matches!(hi.kind, ExprKind::Int { .. }) && matches!(lo.kind, ExprKind::Int { .. })
        }
        ExprKind::Call { func, args } => match func {
            Builtin::Extend | Builtin::Trunc => {
                matches!(args[1].kind, ExprKind::Int { .. }) && kind_is_inferrable(&args[0], decls)
            }
            Builtin::SignedCast | Builtin::UnsignedCast => kind_is_inferrable(&args[0], decls),
            _ => false,
        },
        _ => false,
    }
}

/// True iff `kind`'s own top-level operator is one whose spec-defined
/// result depends on the width it was originally evaluated at — lossless
/// growth (`+`/`-`/`*`) or wrap-modulus (`+%`/`-%`/`*%`) — BUG-19's own
/// documented class (`docs/audit/bugs.md`). Every OTHER operator
/// (`<<`/`>>`/`&`/`|`/`^`/comparisons/…) gives the SAME value no matter
/// what width Verilog happens to (re)compute it at, so `Builtin::Extend`'s
/// argument-hoist (below) only needs to isolate THIS family, not every
/// non-identifier shape.
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

    /// Try to resolve an expression to a constant integer value, seeing
    /// through `extend`/`trunc` that wrap a literal (BUG-18). `extend` widens
    /// without changing the value; `trunc` masks it to its low N bits. Returns
    /// `None` for anything with a runtime component (a signal or a computed
    /// expression) — those already carry a definite Verilog width from their
    /// own declaration, so the `Builtin::Extend` passthrough is safe for them.
    /// Values are always non-negative (source literals are unsized and
    /// non-negative; masking preserves that), so callers render `W'd{v}`.
    fn resolve_const_value(&self, e: &Expr) -> Option<i128> {
        match &e.kind {
            ExprKind::Int { value, .. } => i128::try_from(*value).ok(),
            ExprKind::Call {
                func: Builtin::Extend,
                args,
            } => self.resolve_const_value(&args[0]),
            ExprKind::Call {
                func: Builtin::Trunc,
                args,
            } => {
                let v = self.resolve_const_value(&args[0])?;
                let n = u32::try_from(consteval::eval(&args[1], &self.env).ok()?).ok()?;
                Some(if n >= 127 { v } else { v & ((1i128 << n) - 1) })
            }
            _ => None,
        }
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
            ExprKind::Int { value, raw } => verilog_literal(*value, raw),
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
            ExprKind::Unary { op, expr } => {
                let x = self.expr_subst(expr, subst, arrays);
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
                    let decls = self.cur_decls.clone();
                    let l = {
                        let text = self.expr_subst(lhs, subst, arrays);
                        if kind_is_inferrable(lhs, &decls) {
                            self.hoist_if_needed(lhs, text, &decls)
                        } else {
                            text
                        }
                    };
                    let r = {
                        let text = self.expr_subst(rhs, subst, arrays);
                        if kind_is_inferrable(rhs, &decls) {
                            self.hoist_if_needed(rhs, text, &decls)
                        } else {
                            text
                        }
                    };
                    (l, r)
                } else {
                    (
                        self.expr_subst(lhs, subst, arrays),
                        self.expr_subst(rhs, subst, arrays),
                    )
                };
                // Wrapping ops: same-width Verilog arithmetic already wraps.
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
            ExprKind::IfExpr { cond, then, els } => {
                // A condition that folds at compile time (typically on a
                // `repeat` variable) collapses to the taken branch — this
                // is what keeps `if i == 0 { cin } else { fa[i-1].cout }`
                // from emitting the dead `fa[-1]` arm at i == 0.
                if let Ok(c) = consteval::eval(cond, &self.env) {
                    return if c != 0 {
                        self.expr_subst(then, subst, arrays)
                    } else {
                        self.expr_subst(els, subst, arrays)
                    };
                }
                let c = self.expr_subst(cond, subst, arrays);
                let t = self.expr_subst(then, subst, arrays);
                let f = self.expr_subst(els, subst, arrays);
                format!("(({c}) ? ({t}) : ({f}))")
            }
            ExprKind::Match { scrutinee, arms } => {
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
                                format!("({s} == {})", verilog_literal(*value, raw))
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
            ExprKind::Concat(parts) => {
                let decls = self.cur_decls.clone();
                let ps: Vec<String> = parts
                    .iter()
                    .map(|p| {
                        let text = self.expr_subst(p, subst, arrays);
                        if kind_is_inferrable(p, &decls) {
                            self.hoist_if_needed(p, text, &decls)
                        } else {
                            text
                        }
                    })
                    .collect();
                format!("{{{}}}", ps.join(", "))
            }
            ExprKind::Replicate { count, parts } => {
                let decls = self.cur_decls.clone();
                let c = self.index_expr(count, subst, arrays);
                let ps: Vec<String> = parts
                    .iter()
                    .map(|p| {
                        let text = self.expr_subst(p, subst, arrays);
                        if kind_is_inferrable(p, &decls) {
                            self.hoist_if_needed(p, text, &decls)
                        } else {
                            text
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
                let b = self.expr_subst(base, subst, arrays);
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
                // mismatch. `kind_is_inferrable` still guards the
                // `infer_kind` call needed to size the hoisted wire — a
                // `fn`-body/testbench/width slice base is left exactly as
                // before this task.
                let b = self.expr_subst(base, subst, arrays);
                let decls = self.cur_decls.clone();
                let b = if kind_is_inferrable(base, &decls) {
                    let base_width = crate::emit_verilog::kinds::infer_kind(base, &decls).width;
                    self.hoist_slice_base_if_needed(b, base_width)
                } else {
                    b
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
                        _ => args_str.push(self.expr_subst(a, subst, arrays)),
                    }
                }
                format!("{}({})", name.name, args_str.join(", "))
            }
            ExprKind::Call { func, args } => match func {
                Builtin::SignedCast => {
                    let decls = self.cur_decls.clone();
                    let text = self.expr_subst(&args[0], subst, arrays);
                    let hoisted = if kind_is_inferrable(&args[0], &decls) {
                        self.hoist_if_needed(&args[0], text, &decls)
                    } else {
                        text
                    };
                    format!("$signed({hoisted})")
                }
                Builtin::UnsignedCast => {
                    let decls = self.cur_decls.clone();
                    let text = self.expr_subst(&args[0], subst, arrays);
                    let hoisted = if kind_is_inferrable(&args[0], &decls) {
                        self.hoist_if_needed(&args[0], text, &decls)
                    } else {
                        text
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
                // the width it was originally evaluated at"). Only THAT
                // family is unsound this way — `1 << 3`, `p0 & p1`, a bare
                // signal, all give the SAME value no matter what width
                // Verilog happens to (re)compute them at, so hoisting them
                // here would be pure noise (confirmed empirically: doing
                // this unconditionally for any non-identifier `args[0]`
                // spuriously hoisted `extend(1 << 3, N)`, changing golden
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
                        (Some(v), Some(w)) => format!("{}'d{v}", w as u128),
                        _ => {
                            let text = self.expr_subst(&args[0], subst, arrays);
                            let decls = self.cur_decls.clone();
                            let text = if is_width_effect_binop(&args[0].kind)
                                && kind_is_inferrable(&args[0], &decls)
                            {
                                let w =
                                    crate::emit_verilog::kinds::infer_kind(&args[0], &decls).width;
                                self.hoist_slice_base_if_needed(text, w)
                            } else {
                                text
                            };
                            format!("({text})")
                        }
                    }
                }
                Builtin::Trunc => {
                    let x = self.expr_subst(&args[0], subst, arrays);
                    let n = self.expr_subst(&args[1], subst, arrays);
                    format!("{x}[({n})-1:0]")
                }
                Builtin::Min => {
                    let a = self.expr_subst(&args[0], subst, arrays);
                    let b = self.expr_subst(&args[1], subst, arrays);
                    format!("(({a} < {b}) ? ({a}) : ({b}))")
                }
                Builtin::Max => {
                    let a = self.expr_subst(&args[0], subst, arrays);
                    let b = self.expr_subst(&args[1], subst, arrays);
                    format!("(({a} < {b}) ? ({b}) : ({a}))")
                }
                // Result is `signed[N+1]`; the assignment context sign-extends
                // both ternary arms (the operand is declared `signed`).
                Builtin::Abs => {
                    let x = self.expr_subst(&args[0], subst, arrays);
                    format!("(({x} < 0) ? (-{x}) : ({x}))")
                }
                // Verilog-2005 negated reduction operators — one bit out.
                Builtin::Nand => format!("(~&({}))", self.expr_subst(&args[0], subst, arrays)),
                Builtin::Nor => format!("(~|({}))", self.expr_subst(&args[0], subst, arrays)),
                Builtin::Xnor => format!("(~^({}))", self.expr_subst(&args[0], subst, arrays)),
                // `clog2(n)` folds to a literal when `n` is a constant (a literal
                // or `const`). Of a module PARAMETER it stays symbolic, so it
                // lowers to a call of the injected Verilog-2005 `clog2` constant
                // function — except in a port width, where that function (body-
                // scoped) can't be reached, so it is an honest error.
                Builtin::Clog2 => match consteval::eval(&args[0], &self.env) {
                    Ok(n) if n >= 1 => consteval::clog2_bits(n as u128).to_string(),
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
                        Type::Bits(e) | Type::Signed(e) => {
                            consteval::eval(e, &self.env).unwrap_or(0) as u128
                        }
                        Type::Named(_) | Type::Bundle { .. } | Type::Array { .. } => 0,
                    };
                    used_w += field_w;
                    parts.push(match consteval::eval(a, &self.env) {
                        Ok(v) => {
                            let bits = if field_w >= 128 {
                                v as u128
                            } else {
                                (v as u128) & ((1u128 << field_w) - 1)
                            };
                            format!("{field_w}'d{bits}")
                        }
                        Err(_) => self.expr_subst(a, subst, arrays),
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
                    Type::Bits(e) | Type::Signed(e) => {
                        consteval::eval(e, &self.env).unwrap_or(0) as u128
                    }
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
                                value: hi,
                                raw: hi.to_string(),
                            },
                            span: sp,
                        }),
                        lo: Box::new(Expr {
                            kind: ExprKind::Int {
                                value: lo,
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
