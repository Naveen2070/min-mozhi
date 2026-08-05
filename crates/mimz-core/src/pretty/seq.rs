use super::*;

impl Pretty {
    pub(super) fn on_block(&mut self, on: &OnBlock) {
        let on_kw = self.kw(Kw::On);
        let edge = self.kw(match on.edge {
            crate::ast::Edge::Rise => Kw::Rise,
            crate::ast::Edge::Fall => Kw::Fall,
        });
        let head = match self.order {
            // code-order:  on rise(clk) {  /  on fall(clk) {
            Order::Code => format!("{on_kw} {edge}({}) {{", on.clock.name),
            // thamizh-order:  rise(clk) on {  /  fall(clk) on {
            Order::Thamizh => format!("{edge}({}) {on_kw} {{", on.clock.name),
        };
        self.line(&head);
        self.indent += 1;
        for st in &on.body {
            self.seq_stmt(st);
        }
        self.indent -= 1;
        self.line("}");
    }

    pub(super) fn seq_stmt(&mut self, st: &SeqStmt) {
        let ind = self.indent;
        match st {
            SeqStmt::Assign { lhs, rhs } => {
                let s = format!("{} <- {}", self.lvalue(lhs, ind), self.expr(rhs, ind));
                self.line(&s);
            }
            SeqStmt::If { cond, then, els } => {
                let if_kw = self.kw(Kw::If);
                let cond = self.operand(cond, ind);
                let head = match self.order {
                    Order::Code => format!("{if_kw} {cond} {{"),
                    Order::Thamizh => format!("{cond} {if_kw} {{"),
                };
                self.line(&head);
                self.indent += 1;
                for s in then {
                    self.seq_stmt(s);
                }
                self.indent -= 1;
                match els {
                    None => self.line("}"),
                    Some(else_body) => {
                        let s = format!("}} {} {{", self.kw(Kw::Else));
                        self.line(&s);
                        self.indent += 1;
                        for s in else_body {
                            self.seq_stmt(s);
                        }
                        self.indent -= 1;
                        self.line("}");
                    }
                }
            }
            SeqStmt::Assert(a) => self.assert_stmt(a),
            SeqStmt::Default { name, val, .. } => {
                let kw = self.kw(Kw::Default);
                let v = self.expr(val, ind);
                self.line(&format!("{kw} {} <- {v}", name.name));
            }
            SeqStmt::Loop {
                var, lo, hi, body, ..
            } => {
                let kw = self.kw(Kw::Loop);
                let lo_s = self.expr(lo, ind);
                let hi_s = self.expr(hi, ind);
                let head = format!("{kw} {}: {lo_s}..{hi_s} {{", var.name);
                self.line(&head);
                self.indent += 1;
                for s in body {
                    self.seq_stmt(s);
                }
                self.indent -= 1;
                self.line("}");
            }
            SeqStmt::ForEach {
                var, source, body, ..
            } => {
                let head = format!(
                    "{} {} {} {} {{",
                    self.kw(Kw::Foreach),
                    var.name,
                    self.kw(Kw::In),
                    self.foreach_source(source, ind)
                );
                self.line(&head);
                self.indent += 1;
                for s in body {
                    self.seq_stmt(s);
                }
                self.indent -= 1;
                self.line("}");
            }
            SeqStmt::Error(_) => {} // unreachable on a strict-parsed tree
        }
    }

    // ---------- tests (test HEADER + test `if` stay code-order; the test-form
    // flip is deferred to Phase 1.5, so they are not reorderable) ----------

    pub(super) fn test_decl(&mut self, t: &TestDecl) {
        let test_kw = self.kw(Kw::Test);
        let for_kw = self.kw(Kw::For);
        let module = &t.module.to_dotted();
        let args = self.named_args(&t.args);
        let head = match self.order {
            // code-order:    test "name" for M(args) {
            Order::Code => format!("{test_kw} {:?} {for_kw} {module}{args} {{", t.name),
            // thamizh-order: M(args) kaaga "name" sodhanai {
            Order::Thamizh => format!("{module}{args} {for_kw} {:?} {test_kw} {{", t.name),
        };
        self.line(&head);
        self.indent += 1;
        for st in &t.body {
            self.test_stmt(st);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn named_args(&self, args: &[NamedArg]) -> String {
        if args.is_empty() {
            return String::new();
        }
        let a = args
            .iter()
            .map(|na| format!("{}: {}", na.name.name, self.expr(&na.value, self.indent)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("({a})")
    }

    fn test_stmt(&mut self, st: &TestStmt) {
        let ind = self.indent;
        match st {
            TestStmt::Tick { clock, count } => {
                let s = match count {
                    Some(c) => format!(
                        "{}({}, {})",
                        self.kw(Kw::Tick),
                        clock.name,
                        self.expr(c, ind)
                    ),
                    None => format!("{}({})", self.kw(Kw::Tick), clock.name),
                };
                self.line(&s);
            }
            TestStmt::Expect(e) => {
                let s = format!("{} {}", self.kw(Kw::Expect), self.expr(e, ind));
                self.line(&s);
            }
            TestStmt::Drive { name, value } => {
                let s = format!("{} = {}", name.name, self.expr(value, ind));
                self.line(&s);
            }
            TestStmt::If { cond, then, els } => {
                // Always code-order — the parser only flips `on`-block `if`.
                let head = format!("{} {} {{", self.kw(Kw::If), self.operand(cond, ind));
                self.line(&head);
                self.indent += 1;
                for s in then {
                    self.test_stmt(s);
                }
                self.indent -= 1;
                match els {
                    None => self.line("}"),
                    Some(else_body) => {
                        let s = format!("}} {} {{", self.kw(Kw::Else));
                        self.line(&s);
                        self.indent += 1;
                        for s in else_body {
                            self.test_stmt(s);
                        }
                        self.indent -= 1;
                        self.line("}");
                    }
                }
            }
            TestStmt::Sim(sim) => {
                let s = format!("{} {{", self.kw(Kw::Sim));
                self.line(&s);
                self.indent += 1;
                if let Some(speed) = &sim.speed {
                    let s = format!("{} {}", self.kw(Kw::Speed), self.speed_expr(speed));
                    self.line(&s);
                }
                for b in &sim.binds {
                    let args = b
                        .args
                        .iter()
                        .map(|a| {
                            let v = match &a.value {
                                BindArgValue::Ident(s) => s.clone(),
                                BindArgValue::Str(s) => format!("{s:?}"),
                                BindArgValue::Int(n) => n.to_string(),
                            };
                            format!("{}: {}", a.name.name, v)
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let s = format!(
                        "{} {} -> {}({})",
                        self.kw(Kw::Bind),
                        b.port.name,
                        b.peripheral.name,
                        args
                    );
                    self.line(&s);
                }
                self.indent -= 1;
                self.line("}");
            }
            TestStmt::Error(_) => {} // unreachable on a strict-parsed tree
        }
    }

    /// Re-sugars a `sim` block's `speed` field back into `hz(..)` /
    /// `khz(..)` / `mhz(..)` source. `speed_expr()` in
    /// `parser/items/test.rs` always desugars the `speed` clause to
    /// `Binary { op: Mul, lhs, rhs: Int { value: 1 | 1_000 | 1_000_000 } }`
    /// — this is the exact inverse. Printing the generic binary expr
    /// instead (`50 * 1000000`) doesn't match the `speed` clause's own
    /// grammar and fails to re-parse.
    fn speed_expr(&self, speed: &Expr) -> String {
        if let ExprKind::Binary {
            op: BinOp::Mul,
            lhs,
            rhs,
        } = &speed.kind
            && let ExprKind::Int { value, .. } = &rhs.kind
        {
            let unit = match value {
                crate::bits::Bits::Small(1) => Some("hz"),
                crate::bits::Bits::Small(1_000) => Some("khz"),
                crate::bits::Bits::Small(1_000_000) => Some("mhz"),
                _ => None,
            };
            if let Some(unit) = unit {
                return format!("{unit}({})", self.expr(lhs, self.indent));
            }
        }
        // Shouldn't happen given speed_expr()'s invariant above, but don't
        // panic — fall back to the plain expression printer.
        self.expr(speed, self.indent)
    }
}
