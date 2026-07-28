use super::*;

impl Emitter<'_> {
    /// Emit the body of an always-block. `depth` is the nesting level
    /// inside the block (0 = directly under `always`), used for
    /// indentation only. `module_items` is the enclosing module's item
    /// list — needed only to resolve a `foreach` Elements-form source
    /// (`array_like_len`); threaded through so the `ForEach` arm below can
    /// lower on the spot, same as `emit_drives`'s `SyncLoop`/`ForEach` arms.
    pub(super) fn seq_stmts(
        &mut self,
        stmts: &[SeqStmt],
        depth: usize,
        module_items: &[ModuleItem],
    ) {
        let pad = "    ".repeat(depth + 1);
        // D-DEFAULT-3: defaults first so conditional assigns override (NB last-wins)
        for s in stmts {
            if let SeqStmt::Default { name, val, .. } = s {
                let v = self.expr(val);
                self.out.push_str(&format!("{pad}{} <= {v};\n", name.name));
            }
        }
        for s in stmts {
            match s {
                SeqStmt::Assign { lhs, rhs } => {
                    let l = self.lvalue(lhs);
                    let r = self.expr(rhs);
                    self.out.push_str(&format!("{pad}{l} <= {r};\n"));
                }
                SeqStmt::If { cond, then, els } => {
                    let c = self.expr(cond);
                    self.out.push_str(&format!("{pad}if ({c}) begin\n"));
                    self.seq_stmts(then, depth + 1, module_items);
                    if let Some(els) = els {
                        self.out.push_str(&format!("{pad}end else begin\n"));
                        self.seq_stmts(els, depth + 1, module_items);
                    }
                    self.out.push_str(&format!("{pad}end\n"));
                }
                SeqStmt::Default { .. } => {} // already emitted above
                SeqStmt::Loop {
                    var,
                    lo,
                    hi,
                    body,
                    span,
                } => {
                    let (Some(lo_v), Some(hi_v)) = (self.eval_const(lo), self.eval_const(hi))
                    else {
                        continue;
                    };
                    let count = (hi_v - lo_v).max(0);
                    if count > self.repeat_budget {
                        self.err(
                            *span,
                            format!(
                                "`loop` would unroll {count} times, over the limit of {}",
                                crate::REPEAT_BUDGET
                            ),
                            "this is compile-time hardware generation, not a runtime loop — \
                             narrow the range (a datapath this wide is almost certainly a typo)",
                        );
                        continue;
                    }
                    self.repeat_budget -= count;
                    let mut i = lo_v;
                    while i < hi_v {
                        let shadowed = self
                            .env
                            .insert(var.name.clone(), consteval::ConstVal::from_i128(i));
                        // Same `depth`, not `depth + 1`: unlike `SeqStmt::If`,
                        // a `loop` introduces no new Verilog block — its body
                        // is a literal textual duplicate of hand-written code,
                        // not a nested scope.
                        self.seq_stmts(body, depth, module_items);
                        match shadowed {
                            Some(v) => self.env.insert(var.name.clone(), v),
                            None => self.env.remove(&var.name),
                        };
                        i += 1;
                    }
                }
                // `foreach` is pure sugar over `loop` — lower on the spot
                // and recurse at the SAME `depth` (the lowered `Loop` arm
                // above re-derives its own per-iteration `depth`, same as
                // any hand-written `loop` would).
                SeqStmt::ForEach {
                    var,
                    source,
                    body,
                    span,
                } => {
                    if let Some(lowered) =
                        crate::ast::lower_foreach_seq(var, source, body, *span, module_items)
                    {
                        self.seq_stmts(&lowered, depth, module_items);
                    }
                }
                // Unreachable on the codegen path: `parse` rejects a tree with
                // any `Error` node, so emission never sees one.
                SeqStmt::Error(_) => {}
            }
        }
    }
}
