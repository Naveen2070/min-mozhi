//! Expression parsing: Rust-style precedence climbing (bitwise binds
//! tighter than comparison; comparisons are non-associative — spec/02 §3),
//! `if`/`match` expressions, patterns, and builtin calls.

use super::*;

/// Binary operator precedence, Rust-style. Higher binds tighter.
/// unary(9) → mul(8) → add(7) → shift(6) → & (5) → ^ (4) → | (3)
/// → comparison(2, non-assoc) → && (1) → || (0)
fn bin_op(kind: &TokKind) -> Option<(BinOp, u8)> {
    use TokKind::*;
    Some(match kind {
        Star => (BinOp::Mul, 8),
        StarPct => (BinOp::MulWrap, 8),
        Plus => (BinOp::Add, 7),
        Minus => (BinOp::Sub, 7),
        PlusPct => (BinOp::AddWrap, 7),
        MinusPct => (BinOp::SubWrap, 7),
        Shl => (BinOp::Shl, 6),
        Shr => (BinOp::Shr, 6),
        Amp => (BinOp::BitAnd, 5),
        Caret => (BinOp::BitXor, 4),
        Pipe => (BinOp::BitOr, 3),
        EqEq => (BinOp::Eq, 2),
        Ne => (BinOp::Ne, 2),
        Lt => (BinOp::Lt, 2),
        Le => (BinOp::Le, 2),
        Gt => (BinOp::Gt, 2),
        Ge => (BinOp::Ge, 2),
        AmpAmp => (BinOp::LogicAnd, 1),
        Kw(super::Kw::And) => (BinOp::LogicAnd, 1),
        PipePipe => (BinOp::LogicOr, 0),
        Kw(super::Kw::Or) => (BinOp::LogicOr, 0),
        _ => return None,
    })
}

impl Parser {
    pub(super) fn expr(&mut self) -> Option<Expr> {
        if self.at_kw(Kw::If) {
            return self.if_expr();
        }
        if self.at_kw(Kw::Match) {
            return self.match_expr();
        }
        self.binary(0)
    }

    fn if_expr(&mut self) -> Option<Expr> {
        let start = self.bump().span; // if
        let cond = self.expr()?;
        self.expect(TokKind::LBrace, "`{` then the value when true")?;
        self.skip_newlines();
        let then = self.expr()?;
        self.skip_newlines();
        self.expect(TokKind::RBrace, "`}` after the value")?;
        if !self.at_kw(Kw::Else) {
            let span = self.peek().span;
            self.error(span, "this `if` drives a value, so `else` is mandatory");
            self.help(
                "without `else` the wire would be undriven in some cycles — that is how latches are born (spec/02 §1.3)",
            );
            return None;
        }
        self.bump(); // else
        let els = if self.at_kw(Kw::If) {
            self.if_expr()?
        } else {
            self.expect(TokKind::LBrace, "`{` then the value when false")?;
            self.skip_newlines();
            let e = self.expr()?;
            self.skip_newlines();
            let t = self.expect(TokKind::RBrace, "`}` after the value")?;
            Expr {
                span: e.span.join(t.span),
                ..e
            }
        };
        let span = start.join(els.span);
        Some(Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            span,
        })
    }

    fn match_expr(&mut self) -> Option<Expr> {
        let start = self.bump().span; // match
        let scrutinee = self.binary(0)?;
        self.expect(TokKind::LBrace, "`{` to start the match arms")?;
        let mut arms = Vec::new();
        let end = loop {
            self.skip_newlines();
            if let TokKind::RBrace = self.peek_kind() {
                break self.bump().span;
            }
            if let TokKind::Eof = self.peek_kind() {
                let span = self.peek().span;
                self.error(span, "`match` is missing its closing `}`");
                break span;
            }
            match self.arm() {
                Some(a) => arms.push(a),
                None => self.sync_to_newline(),
            }
        };
        Some(Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            span: start.join(end),
        })
    }

    fn arm(&mut self) -> Option<Arm> {
        let mut patterns = vec![self.pattern()?];
        while self.eat(&TokKind::Comma) {
            patterns.push(self.pattern()?);
        }
        self.expect(TokKind::FatArrow, "`=>` then the arm's value")?;
        let value = self.expr()?;
        self.terminator();
        Some(Arm { patterns, value })
    }

    fn pattern(&mut self) -> Option<Pattern> {
        match self.peek_kind().clone() {
            TokKind::Int { value, raw } => {
                self.bump();
                Some(Pattern::Int { value, raw })
            }
            TokKind::Kw(Kw::True) => {
                self.bump();
                Some(Pattern::Bool(true))
            }
            TokKind::Kw(Kw::False) => {
                self.bump();
                Some(Pattern::Bool(false))
            }
            TokKind::Ident(name) if name == "_" => {
                self.bump();
                Some(Pattern::Wildcard)
            }
            TokKind::Ident(_) => {
                let enum_name = self.ident("a pattern")?;
                self.expect(TokKind::Dot, "`.` — enum patterns are written `State.Red`")?;
                let variant = self.ident("a variant name")?;
                Some(Pattern::Variant { enum_name, variant })
            }
            other => {
                let found = kind_name(&other);
                let span = self.peek().span;
                self.error(
                    span,
                    format!("expected a pattern (number, `Enum.Variant`, or `_`), found {found}"),
                );
                None
            }
        }
    }

    fn binary(&mut self, min_prec: u8) -> Option<Expr> {
        let mut lhs = self.unary()?;
        let mut comparison_seen = false;
        while let Some((op, prec)) = bin_op(self.peek_kind()) {
            if prec < min_prec {
                break;
            }
            // Comparisons are non-associative (spec/02 §3).
            if prec == 2 {
                if comparison_seen {
                    let span = self.peek().span;
                    self.error(span, "comparisons cannot be chained");
                    self.help(
                        "write `(a < b) && (b < c)` — each comparison produces a single `bit`",
                    );
                    return None;
                }
                comparison_seen = true;
            }
            self.bump();
            self.skip_newlines();
            let rhs = self.binary(prec + 1)?;
            let span = lhs.span.join(rhs.span);
            lhs = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Some(lhs)
    }

    fn unary(&mut self) -> Option<Expr> {
        let op = match self.peek_kind() {
            TokKind::Minus => Some(UnOp::Neg),
            TokKind::Tilde => Some(UnOp::BitNot),
            TokKind::Bang | TokKind::Kw(Kw::Not) => Some(UnOp::LogicNot),
            TokKind::Amp => Some(UnOp::RedAnd),
            TokKind::Pipe => Some(UnOp::RedOr),
            TokKind::Caret => Some(UnOp::RedXor),
            _ => None,
        };
        if let Some(op) = op {
            let start = self.bump().span;
            let expr = self.unary()?;
            let span = start.join(expr.span);
            return Some(Expr {
                kind: ExprKind::Unary {
                    op,
                    expr: Box::new(expr),
                },
                span,
            });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Option<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&TokKind::LBracket) {
                let first = self.expr()?;
                if self.eat(&TokKind::Colon) {
                    let lo = self.expr()?;
                    let t = self.expect(TokKind::RBracket, "`]` after the slice")?;
                    let span = e.span.join(t.span);
                    e = Expr {
                        kind: ExprKind::Slice {
                            base: Box::new(e),
                            hi: Box::new(first),
                            lo: Box::new(lo),
                        },
                        span,
                    };
                } else {
                    let t = self.expect(TokKind::RBracket, "`]` after the index")?;
                    let span = e.span.join(t.span);
                    e = Expr {
                        kind: ExprKind::Index {
                            base: Box::new(e),
                            index: Box::new(first),
                        },
                        span,
                    };
                }
            } else if self.eat(&TokKind::Dot) {
                let field = self.ident("a field — `Enum.Variant` or `instance.port`")?;
                let span = e.span.join(field.span);
                e = Expr {
                    kind: ExprKind::Field {
                        base: Box::new(e),
                        field,
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Some(e)
    }

    fn primary(&mut self) -> Option<Expr> {
        match self.peek_kind().clone() {
            TokKind::Int { value, raw } => {
                let t = self.bump();
                Some(Expr {
                    kind: ExprKind::Int { value, raw },
                    span: t.span,
                })
            }
            TokKind::Kw(Kw::True) => {
                let t = self.bump();
                Some(Expr {
                    kind: ExprKind::Bool(true),
                    span: t.span,
                })
            }
            TokKind::Kw(Kw::False) => {
                let t = self.bump();
                Some(Expr {
                    kind: ExprKind::Bool(false),
                    span: t.span,
                })
            }
            TokKind::LParen => {
                self.bump();
                self.skip_newlines();
                let e = self.expr()?;
                self.skip_newlines();
                self.expect(TokKind::RParen, "`)`")?;
                Some(e)
            }
            TokKind::LBrace => {
                let start = self.bump().span;
                let mut parts = Vec::new();
                loop {
                    self.skip_newlines();
                    parts.push(self.expr()?);
                    self.skip_newlines();
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                }
                let t = self.expect(TokKind::RBrace, "`}` to close the concatenation")?;
                Some(Expr {
                    kind: ExprKind::Concat(parts),
                    span: start.join(t.span),
                })
            }
            TokKind::Kw(Kw::If) => self.if_expr(),
            TokKind::Kw(Kw::Match) => self.match_expr(),
            TokKind::Ident(name) => {
                let id = self.ident("a name")?;
                if self.at(&TokKind::LParen) {
                    return self.builtin_call(id, &name);
                }
                Some(Expr {
                    kind: ExprKind::Ident(name),
                    span: id.span,
                })
            }
            other => {
                let found = kind_name(&other);
                let span = self.peek().span;
                self.error(span, format!("expected a value here, found {found}"));
                None
            }
        }
    }

    fn builtin_call(&mut self, id: Ident, name: &str) -> Option<Expr> {
        let (func, arity) = match name {
            "extend" => (Builtin::Extend, 2),
            "trunc" => (Builtin::Trunc, 2),
            "signed" => (Builtin::SignedCast, 1),
            "unsigned" => (Builtin::UnsignedCast, 1),
            other => {
                self.error(
                    id.span,
                    format!(
                        "`{other}` is not a function — Min-Mozhi has no user functions, only modules"
                    ),
                );
                self.help(
                    "built-ins are `extend(x, N)`, `trunc(x, N)`, `signed(x)`, `unsigned(x)`; modules are instantiated with `let` (spec/02 §1.5)",
                );
                return None;
            }
        };
        self.bump(); // (
        let mut args = Vec::new();
        loop {
            self.skip_newlines();
            args.push(self.expr()?);
            self.skip_newlines();
            if !self.eat(&TokKind::Comma) {
                break;
            }
        }
        let t = self.expect(TokKind::RParen, "`)` to close the call")?;
        if args.len() != arity {
            self.error(
                id.span.join(t.span),
                format!("`{name}` takes {arity} argument(s), got {}", args.len()),
            );
            return None;
        }
        Some(Expr {
            kind: ExprKind::Call { func, args },
            span: id.span.join(t.span),
        })
    }
}
