use super::*;

impl Emitter<'_> {
    /// Render one user-defined function as a Verilog-2005
    /// `function automatic` block. Local `let` bindings are declared as
    /// `reg [W-1:0]` using the width inferred by the checker's width pass
    /// and stored in [`LocalLet::inferred_width`]; emitting `integer` would
    /// silently widen narrow wrapping values (e.g. an 8-bit `*%` product
    /// stored in a 32-bit `integer` would not wrap at 8 bits).
    ///
    /// Renders under the FILE-LEVEL const env (`file_env`) so file consts
    /// used in the function body fold to literals (e.g. `a >> SCALE` where
    /// `SCALE` is a file const folds to `a >> 3`), while module consts —
    /// which are not visible inside a `function automatic` body — are
    /// excluded so they cannot accidentally shadow a function parameter.
    ///
    /// `return` lowers via continuation-passing (see [`Self::emit_fn_stmts`])
    /// rather than a flat `funcname = expr;` per statement: Verilog function
    /// bodies execute sequentially with no early exit, so a naive flat
    /// lowering would let the mandatory tail's assignment silently overwrite
    /// an earlier `return` fired inside an `if` branch.
    pub(super) fn render_fn_decl(&mut self, decl: &FuncDecl, file_env: &Env) -> String {
        // Replace the module env with the file-level env: module consts are
        // out of scope inside a `function automatic`, but file consts must
        // fold so uses like `a >> SCALE` emit correct literals.
        let saved_env = std::mem::replace(&mut self.env, file_env.clone());
        // Task 2 (BUG-62(a), GAP-16, docs/plan/v0.2-class-closure-round6.local.md):
        // `cur_decls` was left as the ENCLOSING MODULE's map for the whole
        // body — a `fn`'s own params/`let`s were never in it, so every
        // hoist call site's `infer_kind` saw `None` for a bare fn-body
        // operand and silently rendered it unchanged (BUG-28's founding
        // shape, alive inside a `fn` at round 6). A fresh, empty map here
        // (not the module's own, and never merged with it — a `fn` cannot
        // read a module signal at all, so a name resolving there would
        // only ever be an accidental same-name collision, BUG-63's shape)
        // gets filled in below and installed for the duration of this
        // body; the module's own map is restored on exit exactly like
        // `self.env` above.
        let saved_decls = std::mem::replace(&mut self.cur_decls, std::rc::Rc::new(HashMap::new()));
        let saved_in_fn_body = std::mem::replace(&mut self.in_fn_body, true);
        // Task 4 real fix (BUG-63): a fresh, function-local hoist buffer/
        // counter, never shared with the enclosing module's own
        // `hoist_counter`/`hoisted_decls` — saved/restored exactly like
        // `cur_decls`/`in_fn_body` above, though in practice a `fn` is
        // never rendered while another `fn`'s body is mid-render, so these
        // always start fresh.
        let saved_fn_hoist_counter = std::mem::replace(&mut self.fn_hoist_counter, 0);
        let saved_fn_hoisted_regs = std::mem::take(&mut self.fn_hoisted_regs);
        debug_assert!(
            self.fn_hoisted_stmts.is_empty(),
            "fn_hoisted_stmts must be drained between statements, never carried across a fn"
        );

        // Array-typed names in scope for this body: each param or `let`-bound
        // array maps to `(element_width_string, length)` so a call argument
        // referring to it by name (or an array literal passed directly) can be
        // expanded to the `<name>_<i>` scalars the callee's array param
        // elaborated into (Task 7). Never mutated after construction — an
        // array is immutable once bound (matching the `fn` purity rule).
        let mut arrays: ArrayScope = HashMap::new();
        for param in &decl.params {
            if let Type::Array { elem, len } = &param.ty {
                let n = consteval::eval(len, &self.env)
                    .expect("checker already validated this array's length")
                    .to_i128_saturating() as u128;
                arrays.insert(param.name.name.clone(), (self.width(elem), n));
            }
        }
        for local in fn_all_locals(&decl.stmts, &decl.params, &self.env) {
            if let ExprKind::ArrayLit(elems) = &local.value.kind {
                // Element width comes from the checker's width pass, which sets
                // `inferred_width` to the array's ELEMENT width for an
                // array-typed `let` (widths/mod.rs `FnStmt::Let` arm). Mirror
                // `self.width`'s convention: a 1-bit element has no `[..]` range.
                let ew = match local.inferred_width.get() {
                    Some(1) => String::new(),
                    Some(w) => format!("[{}:0] ", w - 1),
                    None => unreachable!(
                        "array-typed let `{}` has no element width — checker must run first",
                        local.name.name
                    ),
                };
                arrays.insert(local.name.name.clone(), (ew, elems.len() as u128));
            }
        }

        // Build this `fn`'s own `decls` — every param/`let` that
        // `hoist_unresolved` (Task 1) can now resolve instead of asserting
        // on. Array-typed params/`let`s are deliberately absent: they
        // elaborate to `<name>_<i>` scalars via `arrays` above, never to a
        // single `decls` entry an `Ident`/`Index` lookup would find.
        let mut fn_decls: HashMap<String, crate::width_rules::Kind> = HashMap::new();
        for param in &decl.params {
            if matches!(param.ty, Type::Array { .. }) {
                continue;
            }
            self.insert_signal_kind(&mut fn_decls, &param.name, &param.ty);
        }
        for local in fn_all_locals(&decl.stmts, &decl.params, &self.env) {
            if matches!(local.value.kind, ExprKind::ArrayLit(_)) {
                continue;
            }
            if let Some(w) = local.inferred_width.get() {
                // `signed: false` matches the `reg` declaration loop below
                // exactly — a fn-local `let` is never declared `signed`
                // (only cast to `$signed` at use sites, same as `Trunc`'s
                // own part-select, BUG-44's note).
                fn_decls.insert(
                    local.name.name.clone(),
                    crate::width_rules::Kind {
                        width: w,
                        signed: false,
                    },
                );
            }
        }
        // Round-7 plan Task 4 (BUG-67): mirrors `build_decls`'s OWN
        // identical loop (`module/ports.rs`) — every project `fn`'s
        // declared return `Kind`, independent of any one call's arguments.
        // Without this, `infer_kind`'s `FnCall` arm (`kinds.rs`) resolves a
        // nested call one level up (module scope, where `build_decls` DOES
        // install these keys) but not inside a `fn` body, where this map
        // used to be the only source of truth and never carried them —
        // BUG-28's founding divergence, reached through a sixth context.
        // `mem_elem_decl_key` and the instance-port keys stay absent: a
        // `fn` can read neither.
        for (fname, other) in &self.project.funcs {
            if let Some(k) = self.scalar_kind_in_env(&other.ret, &self.env) {
                fn_decls.insert(crate::emit_verilog::kinds::fn_ret_decl_key(fname), k);
            }
        }
        self.cur_decls = std::rc::Rc::new(fn_decls);

        let ret_w = self.width(&decl.ret);
        let mut s = format!("    function automatic {ret_w}{};\n", decl.name.name);
        for param in &decl.params {
            match &param.ty {
                Type::Array { elem, len } => {
                    // An array parameter is never a real Verilog array port —
                    // it elaborates to N independent scalar `input` ports,
                    // named `<param>_<index>`, exactly like `repeat` elaborates
                    // to N copies of hardware rather than a real loop.
                    let n = consteval::eval(len, &self.env)
                        .expect(
                            "checker already validated this array's length is a positive compile-time constant",
                        )
                        .to_i128_saturating();
                    let ew = self.width(elem);
                    for i in 0..n {
                        s.push_str(&format!("        input {ew}{}_{i};\n", param.name.name));
                    }
                }
                // BUG-10 (docs/audit/bugs.md): a bundle-typed `fn` parameter
                // is never a single scalar `input` — it flattens to one
                // `input` per field, same convention module ports/wires use
                // (module.rs:60-78, 130-140). `expr.rs`'s `Field` arm
                // already renders `u.tx` as `u_tx` unconditionally, so the
                // body needs no change — only this declaration and the
                // call-site argument expansion below (`ExprKind::FnCall`
                // in expr.rs) were missing the flatten step.
                Type::Bundle { name: bname, args } => {
                    for (fname, fty) in self.resolve_bundle_fields(bname, args) {
                        let fw = self.width_resolved(&fty);
                        s.push_str(&format!("        input {fw}{}_{fname};\n", param.name.name));
                    }
                }
                Type::Named(id) if self.project.resolve_bundle(id).is_some() => {
                    for (fname, fty) in self.resolve_bundle_fields(id, &[]) {
                        let fw = self.width_resolved(&fty);
                        s.push_str(&format!("        input {fw}{}_{fname};\n", param.name.name));
                    }
                }
                other => {
                    let pw = self.width(other);
                    s.push_str(&format!("        input {pw}{};\n", param.name.name));
                }
            }
        }
        // Names already given a `reg`/`input` declaration this function —
        // seeded with the scalar params (an array param's `<name>_<i>`
        // scalars never collide with a plain `let`'s single name). BUG-9: a
        // `let` that shadows an earlier `let` or a param — the checker
        // (E0813) now guarantees any such shadow keeps the SAME width — so
        // it's the exact same Verilog identifier and needs declaring only
        // once; the emitter used to blindly emit one `reg` line per source
        // `let`, so a shadow re-declared the same name and real Verilog
        // rejected it.
        let mut declared: std::collections::HashSet<String> = decl
            .params
            .iter()
            .filter(|p| !matches!(p.ty, Type::Array { .. }))
            .map(|p| p.name.name.clone())
            .collect();
        for local in fn_all_locals(&decl.stmts, &decl.params, &self.env) {
            // An array-typed `let` is not one sized `reg` — it lowers to N
            // scalar `reg`s named `<name>_<i>`, the same convention an array
            // param uses (built into `arrays` above).
            if let ExprKind::ArrayLit(elems) = &local.value.kind {
                let (ew, _n) = &arrays[&local.name.name];
                for i in 0..elems.len() {
                    s.push_str(&format!("        reg {ew}{}_{i};\n", local.name.name));
                }
                continue;
            }
            if !declared.insert(local.name.name.clone()) {
                continue;
            }
            let decl_line = match local.inferred_width.get() {
                Some(1) => format!("        reg {};\n", local.name.name),
                Some(w) => format!("        reg [{}:0] {};\n", w - 1, local.name.name),
                None => unreachable!(
                    "LocalLet `{}` has no inferred_width — checker must run before emitter",
                    local.name.name
                ),
            };
            s.push_str(&decl_line);
        }
        // Task 4 real fix (BUG-63): any hoist inside the body below fills
        // `self.fn_hoisted_regs` with `reg` decl lines — Verilog-2005
        // requires every function `reg` declared before its first
        // statement, but a hoist is only discovered while rendering that
        // statement's own operand, so declare the splice point now and
        // fill it in only after the whole body is rendered (mirrors
        // `module()`'s own `insert_str` at a saved `fn_pos`).
        let regs_pos = s.len();
        s.push_str("        begin\n");
        let (tail_prefix, tail) = self.render_fn_operand(&decl.tail, &arrays, "            ");
        let tail_code = format!("{tail_prefix}            {} = {};\n", decl.name.name, tail);
        let body_code = self.emit_fn_stmts(
            &decl.stmts,
            &tail_code,
            &decl.name.name,
            3,
            &arrays,
            &decl.params,
        );
        s.push_str(&body_code);
        if !self.fn_hoisted_regs.is_empty() {
            s.insert_str(regs_pos, &self.fn_hoisted_regs);
        }
        s.push_str("        end\n");
        s.push_str("    endfunction\n");

        self.env = saved_env;
        self.cur_decls = saved_decls;
        self.in_fn_body = saved_in_fn_body;
        self.fn_hoist_counter = saved_fn_hoist_counter;
        self.fn_hoisted_regs = saved_fn_hoisted_regs;
        s
    }

    /// Renders one `fn`-body operand (a `let` value, a `return`/tail
    /// expression, an `if` condition — anywhere `emit_fn_stmts` currently
    /// calls `expr_subst` directly) and drains whatever hoist statements
    /// that rendering pushed into `self.fn_hoisted_stmts` (Task 4 real fix,
    /// BUG-63) into a properly-indented prefix. The prefix must be emitted
    /// immediately before the statement this operand belongs to — never
    /// batched across statements — so a hoisted assignment lands in the
    /// same control-flow branch as its use and executes before it, exactly
    /// what a `function automatic`'s sequential body requires.
    fn render_fn_operand(&mut self, e: &Expr, arrays: &ArrayScope, pad: &str) -> (String, String) {
        debug_assert!(
            self.fn_hoisted_stmts.is_empty(),
            "fn_hoisted_stmts must be drained before rendering the next operand"
        );
        let v = self.expr_subst(e, &HashMap::new(), arrays);
        let mut prefix = String::new();
        for stmt in self.fn_hoisted_stmts.drain(..) {
            prefix.push_str(pad);
            prefix.push_str(&stmt);
            prefix.push('\n');
        }
        (prefix, v)
    }

    /// Lower a `fn`-body statement list to Verilog, threading `rest` — the
    /// code for whatever comes after this list falls through to — as a
    /// continuation. A `return` inside an `if` branch must NOT reach
    /// `rest`: it terminates that branch's generated code outright. A
    /// branch that falls through (ends without a `return`) embeds `rest`
    /// as ITS continuation, so the code after the `if` only runs on the
    /// paths that didn't already return.
    #[allow(clippy::too_many_arguments)]
    fn emit_fn_stmts(
        &mut self,
        stmts: &[FnStmt],
        rest: &str,
        fname: &str,
        indent: usize,
        arrays: &ArrayScope,
        params: &[FnParam],
    ) -> String {
        let pad = "    ".repeat(indent);
        match stmts.split_first() {
            None => rest.to_string(),
            Some((FnStmt::Let(l), tail_stmts)) => {
                let mut out = String::new();
                if let ExprKind::ArrayLit(elems) = &l.value.kind {
                    // Array-typed `let`: assign each scalar reg `<name>_<i>`
                    // from its element (mirrors the N-reg declaration above,
                    // same `<name>_<i>` convention as an array param).
                    for (i, el) in elems.iter().enumerate() {
                        let (prefix, v) = self.render_fn_operand(el, arrays, &pad);
                        out.push_str(&prefix);
                        out.push_str(&format!("{pad}{}_{i} = {v};\n", l.name.name));
                    }
                } else {
                    let (prefix, v) = self.render_fn_operand(&l.value, arrays, &pad);
                    out.push_str(&prefix);
                    out.push_str(&format!("{pad}{} = {v};\n", l.name.name));
                }
                out.push_str(&self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params));
                out
            }
            Some((FnStmt::Return(e), _)) => {
                // E0812 already rejects any statement after an unconditional
                // `return` in the same block, so nothing after this one in
                // `stmts` is reachable for a program that passed the checker
                // — the continuation for a `return` is simply the return
                // value itself, never `rest`.
                let (prefix, v) = self.render_fn_operand(e, arrays, &pad);
                format!("{prefix}{pad}{fname} = {v};\n")
            }
            Some((FnStmt::If { cond, then, els }, tail_stmts)) => {
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params);
                let then_code = self.emit_fn_stmts(then, &cont, fname, indent + 1, arrays, params);
                let else_code = match els {
                    Some(els) => self.emit_fn_stmts(els, &cont, fname, indent + 1, arrays, params),
                    None => cont.clone(),
                };
                let (prefix, c) = self.render_fn_operand(cond, arrays, &pad);
                format!(
                    "{prefix}{pad}if ({c}) begin\n{then_code}{pad}end else begin\n{else_code}{pad}end\n"
                )
            }
            Some((
                FnStmt::Loop {
                    var,
                    lo,
                    hi,
                    body,
                    span,
                },
                tail_stmts,
            )) => {
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params);
                let (Some(lo_v), Some(hi_v)) = (self.eval_const(lo), self.eval_const(hi)) else {
                    return cont;
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
                    return cont;
                }
                self.repeat_budget -= count;
                self.emit_fn_loop_unroll(
                    &var.name, lo_v, hi_v, body, &cont, fname, indent, arrays, params,
                )
            }
            // `foreach` is pure sugar over `loop` — lower on the spot and
            // splice the result in as this statement's own continuation
            // chain: `cont` is "whatever comes after the foreach" (same
            // shape `If`'s `then`/`els` branches thread through), and the
            // lowered `[Loop]` re-derives its own per-iteration flow when
            // recursed into with `cont` as ITS `rest`.
            Some((
                FnStmt::ForEach {
                    var,
                    source,
                    body,
                    span,
                },
                tail_stmts,
            )) => {
                let cont = self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params);
                match crate::ast::lower_foreach_fn(var, source, body, *span, params) {
                    Some(lowered) => {
                        self.emit_fn_stmts(&lowered, &cont, fname, indent, arrays, params)
                    }
                    None => cont,
                }
            }
            Some((FnStmt::Error(_), tail_stmts)) => {
                self.emit_fn_stmts(tail_stmts, rest, fname, indent, arrays, params)
            }
        }
    }

    /// Unroll a `FnStmt::Loop`'s body `hi - lo` times, threading each
    /// iteration's continuation to the NEXT iteration (or, for the last
    /// iteration, to the loop's own `rest`) — mirrors `emit_fn_stmts`'s own
    /// continuation-passing shape so `return`'s first-match priority holds
    /// across iterations: iteration 0's `if` only falls through to
    /// iteration 1's check when iteration 0's own condition was false,
    /// never the other way around (see the design spec's "duplicate match"
    /// requirement — this is what makes that case correct).
    #[allow(clippy::too_many_arguments)]
    fn emit_fn_loop_unroll(
        &mut self,
        var: &str,
        i: i128,
        hi: i128,
        body: &[FnStmt],
        rest: &str,
        fname: &str,
        indent: usize,
        arrays: &ArrayScope,
        params: &[FnParam],
    ) -> String {
        if i >= hi {
            return rest.to_string();
        }
        let shadowed = self
            .env
            .insert(var.to_string(), consteval::ConstVal::from_i128(i));
        let inner_rest =
            self.emit_fn_loop_unroll(var, i + 1, hi, body, rest, fname, indent, arrays, params);
        let out = self.emit_fn_stmts(body, &inner_rest, fname, indent, arrays, params);
        match shadowed {
            Some(v) => self.env.insert(var.to_string(), v),
            None => self.env.remove(var),
        };
        out
    }
}

/// Substitute ident values in a type-width expression: a numeric value from
/// `env` folds to a literal; a name with no numeric value but a `symbolic`
/// entry (a bundle param whose arg forwards an outer identifier `env`
/// doesn't have, e.g. the enclosing module's own `parameter`) is replaced
/// by that raw expression instead, so the width stays genuinely symbolic
/// rather than silently wrong. Anything neither map covers is left as-is.
pub(super) fn substitute_expr(
    e: &Expr,
    env: &consteval::Env,
    symbolic: &HashMap<String, Expr>,
) -> Expr {
    match &e.kind {
        ExprKind::Ident(name) => {
            if let Some(v) = env.get(name.as_str()) {
                Expr {
                    kind: ExprKind::Int {
                        value: v.bits.clone(),
                        raw: v.to_string(),
                    },
                    span: e.span,
                }
            } else if let Some(sub) = symbolic.get(name.as_str()) {
                sub.clone()
            } else {
                e.clone()
            }
        }
        ExprKind::Binary { op, lhs, rhs } => Expr {
            kind: ExprKind::Binary {
                op: *op,
                lhs: Box::new(substitute_expr(lhs, env, symbolic)),
                rhs: Box::new(substitute_expr(rhs, env, symbolic)),
            },
            span: e.span,
        },
        ExprKind::Unary { op, expr } => Expr {
            kind: ExprKind::Unary {
                op: *op,
                expr: Box::new(substitute_expr(expr, env, symbolic)),
            },
            span: e.span,
        },
        ExprKind::Call { func, args } => Expr {
            kind: ExprKind::Call {
                func: *func,
                args: args
                    .iter()
                    .map(|a| substitute_expr(a, env, symbolic))
                    .collect(),
            },
            span: e.span,
        },
        // Literals and other forms are already concrete — clone as-is.
        _ => e.clone(),
    }
}

/// Every register name assigned anywhere in this statement tree (both `if`
/// branches included), deduplicated in first-seen order. Drives the
/// generated reset branch: only the regs an `on` block writes are reset
/// in its always-block.
///
/// Owned `String`s (not `&str`) because a `foreach` arm lowers into a
/// temporary `Vec<SeqStmt>` that doesn't outlive this call — same reason
/// `fn_all_locals` returns owned `LocalLet`s instead of borrowing. Cheap:
/// `on`-block bodies are small (see the O(n²) note below).
///
/// NOTE(deferred): O(n²) — `Vec::contains` on every push. Acceptable because
/// on-blocks are small in practice (typically <10 statements). If on-blocks
/// ever grow large, switch to a `HashSet` or `IndexSet`.
pub(super) fn collect_assigned(
    stmts: &[SeqStmt],
    out: &mut Vec<String>,
    module_items: &[ModuleItem],
) {
    for s in stmts {
        match s {
            SeqStmt::Assign { lhs, .. } => {
                if !out.iter().any(|n| n == &lhs.base.name) {
                    out.push(lhs.base.name.clone());
                }
            }
            SeqStmt::If { then, els, .. } => {
                collect_assigned(then, out, module_items);
                if let Some(els) = els {
                    collect_assigned(els, out, module_items);
                }
            }
            SeqStmt::Default { name, .. } => {
                if !out.iter().any(|n| n == &name.name) {
                    out.push(name.name.clone());
                }
            }
            SeqStmt::Loop { body, .. } => {
                collect_assigned(body, out, module_items);
            }
            SeqStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                if let Some(lowered) =
                    crate::ast::lower_foreach_seq(var, source, body, *span, module_items)
                {
                    collect_assigned(&lowered, out, module_items);
                }
            }
            SeqStmt::Assert(_) => {} // assigns no reg
            SeqStmt::Cover(_) => {}  // assigns no reg
            SeqStmt::Error(_) => {}  // unreachable on the codegen path
        }
    }
}

/// Collect every `Let` binding across a `fn`-body statement list, recursing
/// into BOTH arms of nested `if`s — Verilog-2005 `function` declarations
/// must all sit before `begin`, regardless of which branch actually assigns
/// them at runtime. `params` and `env` are needed only by the `ForEach`
/// arm: `params` to lower an Elements-form source (`ast::lower_foreach_fn`
/// needs the enclosing `fn`'s own array-typed parameters — a `fn` is
/// always project-top-level, so there's no module to resolve against),
/// `env` to backfill the synthesized binding's `inferred_width` via
/// `elem_width`. Returns owned `LocalLet`s (not `&LocalLet`) because a
/// lowered `foreach` produces a temporary `Vec<FnStmt>` that doesn't
/// outlive this call — same reason `collect_assigned` returns owned
/// `String`s instead of borrowing.
fn fn_all_locals(stmts: &[FnStmt], params: &[FnParam], env: &Env) -> Vec<LocalLet> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            FnStmt::Let(l) => out.push(l.clone()),
            FnStmt::If { then, els, .. } => {
                out.extend(fn_all_locals(then, params, env));
                if let Some(els) = els {
                    out.extend(fn_all_locals(els, params, env));
                }
            }
            FnStmt::Loop { body, .. } => {
                out.extend(fn_all_locals(body, params, env));
            }
            FnStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                if let Some(lowered) =
                    crate::ast::lower_foreach_fn(var, source, body, *span, params)
                {
                    // The Elements form's synthesized `var` binding (see
                    // `lower_foreach_fn`) never gets its `inferred_width`
                    // set by the checker — backfill it here from the
                    // array's element type before it's collected below.
                    if let ForEachSource::Elements(arr) = source
                        && let Some((elem_ty, _)) = crate::ast::array_like_len_fn(&arr.name, params)
                        && let FnStmt::Loop { body: inner, .. } = &lowered[0]
                        && let Some(FnStmt::Let(synth)) = inner.first()
                    {
                        synth
                            .inferred_width
                            .set(Some(bundle_fields::elem_width(&elem_ty, env)));
                    }
                    out.extend(fn_all_locals(&lowered, params, env));
                }
            }
            FnStmt::Return(_) | FnStmt::Error(_) => {}
        }
    }
    out
}
