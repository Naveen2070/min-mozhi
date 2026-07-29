use super::*;

impl Pretty {
    pub(super) fn ty(&self, t: &Type, ind: usize) -> String {
        match t {
            Type::Bit => "bit".to_string(),
            Type::Bits(e) => format!("bits[{}]", self.expr(e, ind)),
            Type::Signed(e) => format!("signed[{}]", self.expr(e, ind)),
            Type::Named(id) => id.to_dotted(),
            Type::Bundle { name, args } => {
                if args.is_empty() {
                    name.to_dotted()
                } else {
                    let a = args
                        .iter()
                        .map(|a| format!("{}: {}", a.name.name, self.expr(&a.value, ind)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}({a})", name.to_dotted())
                }
            }
            Type::Array { elem, len } => {
                format!("{}[{}]", self.ty(elem, ind), self.expr(len, ind))
            }
        }
    }

    pub(super) fn lvalue(&self, lv: &LValue, ind: usize) -> String {
        match &lv.index {
            None => lv.base.name.clone(),
            Some((i, None)) => format!("{}[{}]", lv.base.name, self.expr(i, ind)),
            Some((hi, Some(lo))) => {
                format!(
                    "{}[{}:{}]",
                    lv.base.name,
                    self.expr(hi, ind),
                    self.expr(lo, ind)
                )
            }
        }
    }

    /// An operand in a precedence-sensitive position (binary/unary operand,
    /// `if` condition, `match` scrutinee). Parenthesize anything that is not
    /// atomic so the tree re-parses identically; atoms/postfix bind tightest
    /// and need no parens.
    pub(super) fn operand(&self, e: &Expr, ind: usize) -> String {
        match e.kind {
            ExprKind::Binary { .. } | ExprKind::IfExpr { .. } | ExprKind::Match { .. } => {
                format!("({})", self.expr(e, ind))
            }
            _ => self.expr(e, ind),
        }
    }

    /// Emit an expression. `ind` is the column level for any block this
    /// expression opens (only `match` uses it — its arms go one per line at
    /// `ind + 1`, closing brace at `ind`).
    pub(super) fn expr(&self, e: &Expr, ind: usize) -> String {
        match &e.kind {
            ExprKind::Int { raw, .. } => raw.clone(),
            ExprKind::Bool(b) => self.kw(if *b { Kw::True } else { Kw::False }).to_string(),
            ExprKind::Ident(name) => name.clone(),
            ExprKind::Field { base, field } => {
                format!("{}.{}", self.operand(base, ind), field.name)
            }
            ExprKind::Unary { op, expr } => format!("{}{}", un_op(*op), self.operand(expr, ind)),
            ExprKind::Binary { op, lhs, rhs } => {
                format!(
                    "{} {} {}",
                    self.operand(lhs, ind),
                    bin_op(*op),
                    self.operand(rhs, ind)
                )
            }
            ExprKind::IfExpr { cond, then, els } => {
                let if_kw = self.kw(Kw::If);
                let else_kw = self.kw(Kw::Else);
                let cond = self.operand(cond, ind);
                let then = self.expr(then, ind);
                let els = self.expr(els, ind);
                match self.order {
                    Order::Code => format!("{if_kw} {cond} {{ {then} }} {else_kw} {{ {els} }}"),
                    Order::Thamizh => {
                        format!("{cond} {if_kw} {{ {then} }} {else_kw} {{ {els} }}")
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                let match_kw = self.kw(Kw::Match);
                let scrut = self.operand(scrutinee, ind);
                // One arm per line (the parser separates arms by newlines, not
                // commas), indented one level deeper than the opening line.
                let inner = pad(ind + 1);
                let close = pad(ind);
                let arms_src: String = arms
                    .iter()
                    .map(|a| format!("{inner}{}\n", self.arm(a, ind + 1)))
                    .collect();
                let body = format!("{{\n{arms_src}{close}}}");
                match self.order {
                    Order::Code => format!("{match_kw} {scrut} {body}"),
                    Order::Thamizh => format!("{scrut} {match_kw} {body}"),
                }
            }
            ExprKind::Concat(parts) => {
                let p = parts
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{p}}}")
            }
            ExprKind::Replicate { count, parts } => {
                let p = parts
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}{{{p}}}}}", self.expr(count, ind))
            }
            ExprKind::Index { base, index } => {
                format!("{}[{}]", self.operand(base, ind), self.expr(index, ind))
            }
            ExprKind::Slice { base, hi, lo } => {
                format!(
                    "{}[{}:{}]",
                    self.operand(base, ind),
                    self.expr(hi, ind),
                    self.expr(lo, ind)
                )
            }
            ExprKind::Call { func, args } => {
                let a = args
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({a})", builtin(*func))
            }
            ExprKind::FnCall { name, args } => {
                let a = args
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({a})", name.name)
            }
            ExprKind::BundleLit(inits) => {
                let fields = inits
                    .iter()
                    .map(|fi| format!("{}: {}", fi.name.name, self.expr(&fi.value, ind)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {fields} }}")
            }
            ExprKind::ArrayLit(elems) => {
                let parts = elems
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{parts}]")
            }
            ExprKind::EnumConstruct {
                enum_name,
                variant,
                args,
            } => {
                let a = args
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}.{}({})", enum_name.name, variant.name, a)
            }
        }
    }

    fn arm(&self, a: &Arm, ind: usize) -> String {
        let pats = a
            .patterns
            .iter()
            .map(pattern)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{pats} => {}", self.expr(&a.value, ind))
    }
}

fn pattern(p: &Pattern) -> String {
    match p {
        Pattern::Int { raw, .. } => raw.clone(),
        Pattern::IntMask { raw, .. } => raw.clone(),
        Pattern::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Pattern::Variant {
            enum_name,
            variant,
            bindings,
        } => {
            if bindings.is_empty() {
                format!("{}.{}", enum_name.name, variant.name)
            } else {
                let bs = bindings
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}.{}({bs})", enum_name.name, variant.name)
            }
        }
        Pattern::Wildcard => "_".to_string(),
    }
}

fn un_op(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::BitNot => "~",
        UnOp::LogicNot => "!",
        UnOp::RedAnd => "&",
        UnOp::RedOr => "|",
        UnOp::RedXor => "^",
    }
}

fn bin_op(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::AddWrap => "+%",
        BinOp::SubWrap => "-%",
        BinOp::MulWrap => "*%",
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
        BinOp::Coalesce => "??",
    }
}

fn builtin(b: Builtin) -> &'static str {
    match b {
        Builtin::Extend => "extend",
        Builtin::Trunc => "trunc",
        Builtin::SignedCast => "signed",
        Builtin::UnsignedCast => "unsigned",
        Builtin::Min => "min",
        Builtin::Max => "max",
        Builtin::Abs => "abs",
        Builtin::Nand => "nand",
        Builtin::Nor => "nor",
        Builtin::Xnor => "xnor",
        Builtin::Clog2 => "clog2",
        // CDC sync primitives: dot-namespaced calls require "sync." prefix
        Builtin::SyncDoubleFlop => "sync.double_flop",
        Builtin::SyncPulse => "sync.pulse",
    }
}
