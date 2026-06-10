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

    /// Render an expression to Verilog text. Compound results are wrapped
    /// in parentheses unconditionally — correctness over prettiness; a
    /// future emitter can use real precedence (architecture invariant #6).
    /// `subst` maps child-module parameter names to instance arguments.
    pub(super) fn expr_subst(&mut self, e: &Expr, subst: &HashMap<&str, &Expr>) -> String {
        match &e.kind {
            ExprKind::Int { value, raw } => verilog_literal(*value, raw),
            ExprKind::Bool(b) => if *b { "1'b1" } else { "1'b0" }.to_string(),
            ExprKind::Ident(name) => {
                if let Some(replacement) = subst.get(name.as_str()) {
                    let r = self.expr(replacement);
                    format!("({r})")
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
                for (i, arm) in arms.iter().enumerate() {
                    let v = self.expr_subst(&arm.value, subst);
                    let is_last = i == arms.len() - 1;
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
                            Pattern::Bool(b) => {
                                format!("({s} == {})", if *b { "1'b1" } else { "1'b0" })
                            }
                            Pattern::Variant { enum_name, variant } => {
                                format!("({s} == {})", enum_const(&enum_name.name, &variant.name))
                            }
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
            ExprKind::Index { base, index } => {
                let b = self.expr_subst(base, subst);
                let i = self.expr_subst(index, subst);
                format!("{b}[{i}]")
            }
            ExprKind::Slice { base, hi, lo } => {
                let b = self.expr_subst(base, subst);
                let h = self.expr_subst(hi, subst);
                let l = self.expr_subst(lo, subst);
                format!("{b}[{h}:{l}]")
            }
            ExprKind::Call { func, args } => match func {
                Builtin::SignedCast => format!("$signed({})", self.expr_subst(&args[0], subst)),
                Builtin::UnsignedCast => {
                    format!("$unsigned({})", self.expr_subst(&args[0], subst))
                }
                // Zero/sign extension is context-automatic in Verilog
                // assignments; the checker (work item 4) will verify widths.
                Builtin::Extend => format!("({})", self.expr_subst(&args[0], subst)),
                Builtin::Trunc => {
                    let x = self.expr_subst(&args[0], subst);
                    let n = self.expr_subst(&args[1], subst);
                    format!("{x}[({n})-1:0]")
                }
            },
        }
    }
}
