//! Expression rendering: operators, literals (base-preserving), enum
//! constants, instance-port wires, if → ternary, match → ternary chains,
//! and builtin casts. `subst` replaces child-module parameter names with
//! instance arguments when rendering child port widths.

use super::*;

impl Emitter<'_> {
    /// Render an expression with no substitutions (the common case).
    pub(super) fn expr(&mut self, e: &Expr) -> String {
        self.expr_subst(e, &HashMap::new())
    }

    /// Render an index or slice bound. A non-literal that folds at compile
    /// time — a `repeat` variable or arithmetic over one, like `i + 1` —
    /// collapses to its decimal value (`sum[i] → sum[2]`); plain literals
    /// keep their written base, and anything symbolic (a parameter, a
    /// dynamic signal index) renders unchanged.
    pub(super) fn index_expr(&mut self, e: &Expr, subst: &HashMap<&str, &Expr>) -> String {
        if !matches!(e.kind, ExprKind::Int { .. })
            && let Ok(v) = consteval::eval(e, &self.env)
        {
            return v.to_string();
        }
        self.expr_subst(e, subst)
    }

    /// Render an expression to Verilog text. Compound results are wrapped
    /// in parentheses unconditionally — correctness over prettiness; a
    /// future emitter can use real precedence (architecture invariant #6).
    /// `subst` maps child-module parameter names to instance arguments.
    pub(super) fn expr_subst(&mut self, e: &Expr, subst: &HashMap<&str, &Expr>) -> String {
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
                let x = self.expr_subst(expr, subst);
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
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr_subst(lhs, subst);
                let r = self.expr_subst(rhs, subst);
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
                        self.expr_subst(then, subst)
                    } else {
                        self.expr_subst(els, subst)
                    };
                }
                let c = self.expr_subst(cond, subst);
                let t = self.expr_subst(then, subst);
                let f = self.expr_subst(els, subst);
                format!("(({c}) ? ({t}) : ({f}))")
            }
            ExprKind::Match { scrutinee, arms } => {
                // Nested ternaries; the final arm becomes the default.
                let s = self.expr_subst(scrutinee, subst);
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

                    let v = self.expr_subst(&arm.value, &arm_subst);
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
                let ps: Vec<String> = parts.iter().map(|p| self.expr_subst(p, subst)).collect();
                format!("{{{}}}", ps.join(", "))
            }
            ExprKind::Replicate { count, parts } => {
                let c = self.index_expr(count, subst);
                let ps: Vec<String> = parts.iter().map(|p| self.expr_subst(p, subst)).collect();
                format!("{{{c}{{{}}}}}", ps.join(", "))
            }
            ExprKind::Index { base, index } => {
                let b = self.expr_subst(base, subst);
                let i = self.index_expr(index, subst);
                format!("{b}[{i}]")
            }
            ExprKind::Slice { base, hi, lo } => {
                let b = self.expr_subst(base, subst);
                let h = self.index_expr(hi, subst);
                let l = self.index_expr(lo, subst);
                format!("{b}[{h}:{l}]")
            }
            ExprKind::FnCall { name, args } => {
                // Mark this function (and all transitive callees) for injection
                // at module-body top; then render as a Verilog function call.
                self.mark_fn_used(&name.name);
                let args_str: Vec<String> =
                    args.iter().map(|a| self.expr_subst(a, subst)).collect();
                format!("{}({})", name.name, args_str.join(", "))
            }
            ExprKind::Call { func, args } => match func {
                Builtin::SignedCast => format!("$signed({})", self.expr_subst(&args[0], subst)),
                Builtin::UnsignedCast => {
                    format!("$unsigned({})", self.expr_subst(&args[0], subst))
                }
                // Extension is context-automatic in Verilog assignments:
                // unsigned operands zero-extend; `signed`-declared ones
                // SIGN-extend (declarations carry `signed`, see
                // `width_subst`). The checker has already verified widths.
                Builtin::Extend => format!("({})", self.expr_subst(&args[0], subst)),
                Builtin::Trunc => {
                    let x = self.expr_subst(&args[0], subst);
                    let n = self.expr_subst(&args[1], subst);
                    format!("{x}[({n})-1:0]")
                }
                Builtin::Min => {
                    let a = self.expr_subst(&args[0], subst);
                    let b = self.expr_subst(&args[1], subst);
                    format!("(({a} < {b}) ? ({a}) : ({b}))")
                }
                Builtin::Max => {
                    let a = self.expr_subst(&args[0], subst);
                    let b = self.expr_subst(&args[1], subst);
                    format!("(({a} < {b}) ? ({b}) : ({a}))")
                }
                // Result is `signed[N+1]`; the assignment context sign-extends
                // both ternary arms (the operand is declared `signed`).
                Builtin::Abs => {
                    let x = self.expr_subst(&args[0], subst);
                    format!("(({x} < 0) ? (-{x}) : ({x}))")
                }
                // Verilog-2005 negated reduction operators — one bit out.
                Builtin::Nand => format!("(~&({}))", self.expr_subst(&args[0], subst)),
                Builtin::Nor => format!("(~|({}))", self.expr_subst(&args[0], subst)),
                Builtin::Xnor => format!("(~^({}))", self.expr_subst(&args[0], subst)),
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
                        format!("clog2({})", self.expr_subst(&args[0], subst))
                    }
                },
            },
        }
    }

    /// Render the condition for a `Pattern::Variant` match arm:
    /// - tag-only enum: `(s == ENUM_VARIANT)` (unchanged from before)
    /// - tagged enum: `(s[total-1:max_payload_w] == tag_w'd<index>)`
    fn variant_cond(&self, s: &str, enum_name: &str, variant_name: &str) -> String {
        let Some(edecl) = self.project.enums.get(enum_name) else {
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
            let Some(edecl) = self.project.enums.get(&enum_name.name) else {
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
                };
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
