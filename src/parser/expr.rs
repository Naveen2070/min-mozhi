//! Expression parsing: Rust-style precedence climbing (bitwise binds
//! tighter than comparison; comparisons allow a monotonic one-direction
//! chain and reject the confusing forms — spec/02 section 3),
//! `if`/`match` expressions, patterns, and builtin calls.

use super::*;

/// Binary operator precedence, Rust-style. Higher binds tighter.
/// unary(9) → mul(8) → add(7) → shift(6) → & (5) → ^ (4) → | (3)
/// → comparison(2, chain via `comparison_chain`) → && (1) → || (0)
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
    /// `expr = ifExpr | matchExpr | binExpr` — the expression entry point
    /// used by everything in `items.rs`. In `thamizh` order the clause head
    /// trails the operand, so we parse the operand first and let the trailing
    /// keyword decide (see `expr_thamizh`).
    pub(super) fn expr(&mut self) -> Option<Expr> {
        if self.profile == Profile::Thamizh {
            return self.expr_thamizh();
        }
        if self.at_kw(Kw::If) {
            return self.if_expr();
        }
        if self.at_kw(Kw::Match) {
            return self.match_expr();
        }
        self.binary(0)
    }

    /// `thamizh`-order expression entry: parse the operand with `binary(0)`,
    /// then one-token lookahead on the trailing clause head — `endral` makes
    /// it an if-expression over that operand as the condition, `poruthu` a
    /// match over it as the scrutinee. No backtracking (spec/04). A nested
    /// `if`/`match` as the condition/scrutinee needs parens, exactly as the
    /// code-order match scrutinee already requires.
    fn expr_thamizh(&mut self) -> Option<Expr> {
        let head = self.binary(0)?;
        if self.at_kw(Kw::If) {
            return self.if_expr_thamizh(head);
        }
        if self.at_kw(Kw::Match) {
            return self.match_expr_thamizh(head);
        }
        Some(head)
    }

    /// `ifExpr = "if" expr "{" expr "}" "else" ("{" expr "}" | ifExpr)` —
    /// `else` is MANDATORY: an if-expression drives a value, and a missing
    /// branch is how latches are born (safety rule, spec/02 section 1.3).
    fn if_expr(&mut self) -> Option<Expr> {
        self.enter()?;
        let r = self.if_expr_inner();
        self.leave();
        r
    }

    fn if_expr_inner(&mut self) -> Option<Expr> {
        let start = self.bump().span; // if
        let cond = self.expr()?;
        self.expect(TokKind::LBrace, "`{` then the value when true")?;
        self.skip_newlines();
        let then = self.expr()?;
        self.skip_newlines();
        self.expect(TokKind::RBrace, "`}` after the value")?;
        if !self.at_kw(Kw::Else) {
            let span = self.peek().span;
            self.error(
                span,
                "E1108",
                "this `if` drives a value, so `else` is mandatory",
            );
            self.help(
                "without `else` the wire would be undriven in some cycles — that is how latches are born (spec/02 section 1.3)",
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

    /// `thamizh`-order if-expression: `<cond> "endral" "{" expr "}" "illaiyel"
    /// ("{" expr "}" | <cond> "endral" …)`. The condition is already parsed
    /// (the `head` from `expr_thamizh`); everything from `endral` onward mirrors
    /// `if_expr` and builds the SAME `ExprKind::IfExpr`. `else` (`illaiyel`) is
    /// still mandatory — an if-expression drives a value (spec/02 section 1.3).
    fn if_expr_thamizh(&mut self, cond: Expr) -> Option<Expr> {
        self.enter()?;
        let r = self.if_expr_thamizh_inner(cond);
        self.leave();
        r
    }

    fn if_expr_thamizh_inner(&mut self, cond: Expr) -> Option<Expr> {
        let start = cond.span;
        self.bump(); // endral (Kw::If)
        self.expect(TokKind::LBrace, "`{` then the value when true")?;
        self.skip_newlines();
        let then = self.expr()?;
        self.skip_newlines();
        self.expect(TokKind::RBrace, "`}` after the value")?;
        if !self.at_kw(Kw::Else) {
            let span = self.peek().span;
            self.error(
                span,
                "E1108",
                "this `if` drives a value, so `else` is mandatory",
            );
            self.help(
                "without `else` the wire would be undriven in some cycles — that is how latches are born (spec/02 section 1.3)",
            );
            return None;
        }
        self.bump(); // illaiyel (else)
        // A chained alternative in thamizh order is `illaiyel <cond> endral …`,
        // so anything other than a `{` starts another condition.
        let els = if !self.at(&TokKind::LBrace) {
            let head = self.binary(0)?;
            self.if_expr_thamizh(head)?
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

    /// `matchExpr = "match" binExpr "{" { arm } "}"` — the scrutinee is
    /// `binary(0)`, not `expr()`, so a nested `if`/`match` head needs
    /// parentheses (avoids `match if ...` ambiguity).
    fn match_expr(&mut self) -> Option<Expr> {
        let start = self.bump().span; // match
        let scrutinee = self.binary(0)?;
        self.finish_match(scrutinee, start)
    }

    /// `thamizh`-order match: `<expr> "poruthu" "{" { arm } "}"`. The scrutinee
    /// is already parsed (the `head` from `expr_thamizh`); from the `poruthu`
    /// keyword onward it is identical to code order, building the SAME
    /// `ExprKind::Match`.
    fn match_expr_thamizh(&mut self, scrutinee: Expr) -> Option<Expr> {
        let start = scrutinee.span;
        self.bump(); // poruthu (Kw::Match)
        self.finish_match(scrutinee, start)
    }

    /// Shared arm loop for both word-order profiles. Called with the cursor at
    /// the opening `{`. `start` anchors the node span (the `match` keyword in
    /// code order, the scrutinee in thamizh order).
    fn finish_match(&mut self, scrutinee: Expr, start: Span) -> Option<Expr> {
        self.expect(TokKind::LBrace, "`{` to start the match arms")?;
        let mut arms = Vec::new();
        let end = loop {
            self.skip_newlines();
            if let TokKind::RBrace = self.peek_kind() {
                break self.bump().span;
            }
            if let TokKind::Eof = self.peek_kind() {
                let span = self.peek().span;
                self.error(span, "E1101", "`match` is missing its closing `}`");
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

    /// `arm = pattern { "," pattern } "=>" expr`
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

    /// `pattern = int | "true" | "false" | "_" | ident "." ident`
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
                    "E1101",
                    format!("expected a pattern (number, `Enum.Variant`, or `_`), found {found}"),
                );
                None
            }
        }
    }

    /// Precedence climbing: parse a unary operand, then greedily fold
    /// operators of precedence ≥ `min_prec`. Recursing with `prec + 1`
    /// makes every level left-associative. The comparison level (prec 2) is
    /// special: it routes to [`Self::comparison_chain`], which allows a
    /// monotonic one-direction chain (`0 <= x < 100`) but rejects the
    /// confusing forms — spec/02 section 3.
    ///
    /// `pub(super)` so the thamizh-order item parsers (`items.rs`) can parse a
    /// clause head as an operand before the trailing keyword decides the form.
    pub(super) fn binary(&mut self, min_prec: u8) -> Option<Expr> {
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = bin_op(self.peek_kind()) {
            if prec < min_prec {
                break;
            }
            if prec == 2 {
                // Parse the whole (possibly chained) comparison here, then
                // re-check the loop — only lower-prec `&&`/`||` may follow.
                lhs = self.comparison_chain(lhs)?;
                continue;
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

    /// Parse a comparison, possibly a Python-style chain (`first` is the
    /// already-parsed left operand). A single comparison (`a < b`) is one
    /// `Binary`. A **monotonic** chain in ONE direction (`0 <= x < 100`)
    /// desugars to `&&` of the pairwise comparisons, sharing the middle
    /// operands — a combinational value read twice is identical in hardware,
    /// so there is no evaluation-order subtlety (unlike software). The
    /// genuinely confusing forms stay rejected (E1109): mixed-direction
    /// (`a < b > c`) and any chain involving `==`/`!=`. spec/02 section 3.
    fn comparison_chain(&mut self, first: Expr) -> Option<Expr> {
        let chain_start = self.peek().span; // first comparison op (for errors)
        let mut operands = vec![first];
        let mut ops: Vec<BinOp> = Vec::new();
        while let Some((op, 2)) = bin_op(self.peek_kind()) {
            self.bump();
            self.skip_newlines();
            operands.push(self.binary(3)?); // operands never contain a comparison
            ops.push(op);
        }

        // The common case: a single comparison, no desugaring.
        if ops.len() == 1 {
            let rhs = operands.pop().unwrap();
            let lhs = operands.pop().unwrap();
            let span = lhs.span.join(rhs.span);
            return Some(Expr {
                kind: ExprKind::Binary {
                    op: ops[0],
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            });
        }

        // A chain — only monotonic ordering chains are allowed. Direction:
        // +1 ascending (`<`,`<=`), -1 descending (`>`,`>=`), 0 = `==`/`!=`.
        let dir = |op: BinOp| match op {
            BinOp::Lt | BinOp::Le => 1i8,
            BinOp::Gt | BinOp::Ge => -1i8,
            _ => 0i8,
        };
        if ops.iter().any(|o| dir(*o) == 0) {
            self.error(
                chain_start,
                "E1109",
                "`==`/`!=` cannot be part of a comparison chain",
            );
            self.help("compare equality on its own, e.g. `(a == b) && (b < c)`");
            return None;
        }
        let first_dir = dir(ops[0]);
        if ops.iter().any(|o| dir(*o) != first_dir) {
            self.error(
                chain_start,
                "E1109",
                "a comparison chain must point in one direction",
            );
            self.help("keep one direction, e.g. `0 <= x <= 100` — or split with `&&`");
            return None;
        }

        // Desugar to `(a op b) && (b op c) && …`, cloning the shared middle
        // operands (each interior operand appears in two comparisons).
        let mut acc: Option<Expr> = None;
        for (i, op) in ops.iter().enumerate() {
            let lhs = operands[i].clone();
            let rhs = operands[i + 1].clone();
            let span = lhs.span.join(rhs.span);
            let cmp = Expr {
                kind: ExprKind::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
            acc = Some(match acc {
                None => cmp,
                Some(prev) => {
                    let span = prev.span.join(cmp.span);
                    Expr {
                        kind: ExprKind::Binary {
                            op: BinOp::LogicAnd,
                            lhs: Box::new(prev),
                            rhs: Box::new(cmp),
                        },
                        span,
                    }
                }
            });
        }
        acc
    }

    /// `unary = [ "-" | "~" | "!" | "not" | "&" | "|" | "^" ] unary | postfix`
    /// — prefix `&`/`|`/`^` are the reduction operators (fold a vector to
    /// one bit), same symbols as the binary bitwise ops.
    fn unary(&mut self) -> Option<Expr> {
        self.enter()?;
        let r = self.unary_inner();
        self.leave();
        r
    }

    fn unary_inner(&mut self) -> Option<Expr> {
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

    /// `postfix = primary { "[" expr [":" expr] "]" | "." ident }` —
    /// indexing, slicing, and field access chain left-to-right.
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

    /// `primary = int | "true" | "false" | "(" expr ")" | "{" exprList "}"
    ///          | ifExpr | matchExpr | ident | builtinCall` —
    /// `{a, b}` is bit concatenation, not a block.
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
            // In code order the clause head LEADS, so `if`/`match` start a
            // primary. In thamizh order it TRAILS the operand (handled in
            // `expr_thamizh`), so a leading `endral`/`poruthu` here is the
            // wrong order — reject it cleanly rather than silently parsing it
            // as code order inside a thamizh file.
            TokKind::Kw(Kw::If) if self.profile == Profile::Thamizh => {
                let span = self.peek().span;
                self.error(
                    span,
                    "E1101",
                    "in thamizh order the condition comes first: `<cond> endral { … }` — parenthesize if you need it as a value",
                );
                None
            }
            TokKind::Kw(Kw::Match) if self.profile == Profile::Thamizh => {
                let span = self.peek().span;
                self.error(
                    span,
                    "E1101",
                    "in thamizh order the scrutinee comes first: `<expr> poruthu { … }` — parenthesize if you need it as a value",
                );
                None
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
                self.error(
                    span,
                    "E1101",
                    format!("expected a value here, found {found}"),
                );
                None
            }
        }
    }

    /// `builtinCall = ("extend" | "trunc" | "signed" | "unsigned") "(" args ")"`
    /// — arity-checked here; any other `name(...)` gets the "no user
    /// functions, only modules" teaching error.
    fn builtin_call(&mut self, id: Ident, name: &str) -> Option<Expr> {
        let (func, arity) = match name {
            "extend" => (Builtin::Extend, 2),
            "trunc" => (Builtin::Trunc, 2),
            "signed" => (Builtin::SignedCast, 1),
            "unsigned" => (Builtin::UnsignedCast, 1),
            "min" => (Builtin::Min, 2),
            "max" => (Builtin::Max, 2),
            "abs" => (Builtin::Abs, 1),
            "nand" => (Builtin::Nand, 1),
            "nor" => (Builtin::Nor, 1),
            "xnor" => (Builtin::Xnor, 1),
            other => {
                self.error(
                    id.span,
                    "E1110",
                    format!(
                        "`{other}` is not a function — Min-Mozhi has no user functions, only modules"
                    ),
                );
                self.help(
                    "built-ins are `extend(x, N)`, `trunc(x, N)`, `signed(x)`, `unsigned(x)`, `min(a, b)`, `max(a, b)`, `abs(x)`, `nand|nor|xnor(x)`; modules are instantiated with `let` (spec/02 section 1.5)",
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
                "E1110",
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
