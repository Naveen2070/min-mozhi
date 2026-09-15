//! `elaborate::Design` -> `ir::Module` lowering.

use super::{Bits, Cell, CellKind, Module};
use crate::ast::{BinOp, Builtin, Dir, Expr, ExprKind, FnStmt, LValue, SeqStmt, Type, UnOp};
use crate::elaborate::Design;
use std::collections::{BTreeMap, HashMap};

/// An unsigned constant of an exact width — the shape every synthesized
/// literal in this file wants.
fn const_val(value: u128, width: u32) -> crate::checker::consteval::ConstVal {
    crate::checker::consteval::ConstVal {
        bits: crate::bits::Bits::Small(value),
        width,
        signed: false,
    }
}

/// Mirrors `validate::requires_matched_ab`'s exact set of ops, keyed on
/// `BinOp` (available here, in `lower_expr`'s `ExprKind::Binary` arm,
/// before any `CellKind` exists) instead of `CellKind`. Deliberately
/// DUPLICATED rather than shared: `CellKind` doesn't retain which `BinOp`
/// produced it, so unifying the two would mean inventing a translation
/// layer neither file has today, for an 11-arm boolean lookup that isn't a
/// numeric formula (unlike `Shl`'s growth formula, which both files call
/// through `width_rules::shift_result` because THAT part is genuinely
/// shared) — see `validate.rs`'s `expected_widths` doc comment on `Shl` for
/// the same "the two must never drift" principle, which it also accepts a
/// duplicated formula for (Add/Sub/Mul) rather than sharing.
fn requires_matched_ab(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge
            | BinOp::LogicAnd
            | BinOp::LogicOr
    )
}

/// The three synthetic `env` keys a memory's write port occupies while
/// `lower_seq_stmts` folds its `if`s. They live in the same key space as
/// register names, and the `__mem_` prefix keeps them out of the way of
/// real signal names.
fn mem_write_keys(mem: &str) -> (String, String, String) {
    (
        format!("__mem_wen_{mem}"),
        format!("__mem_waddr_{mem}"),
        format!("__mem_wdata_{mem}"),
    )
}

/// Lowering state threaded through one module's lowering: which
/// signal names have already been turned into `Bits`, memoized so a wire
/// referenced by more than one comb expression is lowered exactly once
/// (the AST checker already guarantees `comb` forms a DAG, so plain
/// memoized recursion terminates — no separate topo-sort needed).
struct LowerCtx<'a> {
    design: &'a Design,
    resolved: HashMap<String, Bits>,
    /// Per-memory `(raddr, rdata)` nets, one entry per DISTINCT lowered read
    /// address, allocated lazily on each new `m[addr]` read and picked up
    /// later by `lower`'s cell-emitting pass, which is the only place the
    /// `Mem` cell itself is pushed (it needs the read and write pin sets
    /// filled in together, and the write side isn't knowable until the
    /// writing process is walked).
    ///
    /// MULTIPLE READ PORTS (GAP-1 residual Task 6): a read at the SAME
    /// lowered address as an existing entry reuses that port for free; a
    /// read at a genuinely different address grows a new entry instead of
    /// panicking (the v1 ceiling this used to hit). Read ports live on
    /// `CellKind::Mem::read_ports`, not `Cell::pins` — see that field's doc.
    mem_read: HashMap<String, Vec<(Bits, Bits)>>,
    /// One TOP-LEVEL expression node's lowered result, keyed by the ADDRESS
    /// of its `Expr` node.
    ///
    /// `lower_seq_stmts` walks one shared process body once per target (once
    /// per register, once per memory write port), and `SeqStmt::If` lowers
    /// its condition on every one of those walks. Without memoization every
    /// walk re-lowers that condition from scratch, allocating fresh nets for
    /// anything past a bare `Ident` — which duplicates cells, and makes one
    /// source-level `m[0]` read look like two rival read ports to
    /// `mem_read`'s address comparison. `design` is borrowed immutably for
    /// the whole pass, and every node keyed here belongs to it (never to a
    /// temporary), so a node's address is a stable identity for "this exact
    /// expression site": re-encountering one returns its `Bits` with no
    /// re-lowering at all — nothing inside it, an inlined `fn` body
    /// included, gets a second chance to allocate.
    ///
    /// Only populated/consulted when `locals.is_none()`, i.e. never inside an
    /// inlined `fn` body, where one node legitimately means different values
    /// on different calls (`fn f(i) { ram[i] }` called as `f(a)` and `f(b)`).
    ///
    /// FUTURE RE-WALK MECHANISMS: the same caveat binds anything else that
    /// re-walks one body at `locals: None` with DIFFERING per-iteration
    /// semantics. Specifically, `on`-block `Loop`/`ForEach` unrolling
    /// (`unimplemented!` in `lower_seq_stmts` today) must either bypass this
    /// memo per iteration or scope it per iteration — otherwise every
    /// iteration silently reuses iteration 0's nets.
    expr_memo: HashMap<usize, Bits>,
}

impl<'a> LowerCtx<'a> {
    /// Resolves a signal name to its `Bits`, lowering its driving comb
    /// expression on first use if it's a wire/output (a plain input or a
    /// register's Q is already in `resolved` from module-scaffold time).
    fn resolve(&mut self, module: &mut Module, name: &str) -> Bits {
        if let Some(bits) = self.resolved.get(name) {
            return bits.clone();
        }
        let expr = self.design.comb.get(name).unwrap_or_else(|| {
            panic!("no driver recorded for signal `{name}` (checker should have caught this)")
        });
        // `design.comb` only ever drives an output or a wire (never a reg —
        // see its own doc comment), so a declared width is always found here
        // in practice; the `None` fallback (plain `lower_expr`, today's
        // behaviour) is just defensive, not expected to fire on any real
        // `Design`. Threading the width through lets a bare-literal-or-const
        // driver (`out = 0;`) size itself to the PORT's declared width
        // instead of its own natural width — the `debug_wrapper.mimz` shape
        // (`dbg_out = 0` inside a `const if` branch, folded away before a
        // `Design` exists, leaving `0` as the whole `comb` entry).
        let declared_width = self
            .design
            .outputs
            .iter()
            .chain(&self.design.wires)
            .find(|s| s.name == name)
            .map(|s| s.width.bits);
        // Top-level signal resolution is never inside a `fn` body, so there
        // are no call-local bindings in scope here.
        let bits = match declared_width {
            Some(w) => self.lower_expr_sized(module, expr, None, None, w),
            None => self.lower_expr(module, expr, None, None),
        };
        self.resolved.insert(name.to_string(), bits.clone());
        bits
    }

    /// Lowers `e` at exactly `target_width`, when `e` is a bare integer
    /// literal, a reference to a `design.consts` entry (module parameter or
    /// file-level `const`), or a larger compile-time-constant EXPRESSION
    /// built only from those two shapes (e.g. `sync_loop_lower.rs`'s
    /// desugared `hi - 1` loop bound, both `hi` and `1` literals) — all
    /// values with no width of their own, which the checker types
    /// contextually (`Ty::CtInt`), promoted to whatever the use-context
    /// requires, rather than at their own natural/lowered-arithmetic width.
    /// Anything else falls through to plain `lower_expr` unchanged: a real
    /// signal's width is already reconciled by the checker, and re-sizing it
    /// here would silently truncate or mis-extend a value that is NOT a
    /// compile-time constant.
    ///
    /// The `Int` arm deliberately does NOT route through
    /// `crate::value::const_eval`, unlike this file's other `const_eval`
    /// call (`Slice`'s `hi`/`lo`, above) and unlike the fallback arm below —
    /// `ExprKind::Int`'s own `value` field is already the exact
    /// arbitrary-width `bits::Bits` the checker const-folded, so reusing it
    /// directly (as the sibling `ExprKind::Int` arm in `lower_expr` does)
    /// avoids a needless round trip for the single most common case.
    ///
    /// The fallback arm (anything else) tries `const_eval_wide` on the WHOLE
    /// expression, gated to `locals.is_none()`. Deliberately
    /// `const_eval_wide`, NOT the plain `i128`-narrowing `const_eval` this
    /// file's other call sites use (`Slice`'s `hi`/`lo`, `shl_const_amount`,
    /// below) — a compile-time-constant EXPRESSION (unlike a bare literal,
    /// or a `design.consts` entry, which is always an already-`i128`
    /// module-parameter/`const` value) can genuinely evaluate past `i128`'s
    /// range (`bits[200] w = 1 << 190;` is checker-legal, `Bits::Wide`
    /// exists exactly for this — BUG-13 layer 2), and `const_eval`'s
    /// `i128`-saturating narrowing does not error on that, it silently
    /// returns `i128::MAX`/`MIN` (`ConstVal::to_i128_saturating`'s own doc).
    /// `const_eval_wide` returns the checker's arbitrary-width `ConstVal`
    /// directly instead, so the fold is exact regardless of magnitude — the
    /// resize to `target_width` below then goes through
    /// `crate::value::from_const_at_width`, the SAME sign-aware
    /// extend/truncate `ir::exec`'s own `Const`-cell evaluation uses, so a
    /// negative or over-128-bit folded value widens/narrows exactly as
    /// correctly as executing the constant would. `const_eval_wide` still
    /// has no notion of `locals` (same as `const_eval`), so it inherits the
    /// same `locals.is_none()` gate as before: inside an inlined `fn` body
    /// it could otherwise silently resolve a LOCAL param/`let` name that
    /// collides with an unrelated design-level const/param. Every real call
    /// site today (`resolve()`'s comb driver, `ExprKind::Binary`'s siblings
    /// reached from module-level seq/comb lowering) already passes
    /// `locals: None`, so this costs nothing in practice; a compound
    /// constant expression inside a `fn` body simply isn't re-sized —
    /// today's pre-existing behaviour, not a regression.
    fn lower_expr_sized(
        &mut self,
        module: &mut Module,
        e: &Expr,
        locals: Option<&HashMap<String, Bits>>,
        arrays: Option<&HashMap<String, u32>>,
        target_width: u32,
    ) -> Bits {
        // `ExprKind::Int` is kept as its own arm rather than folded into the
        // `ConstVal` path below: its `value` field is already the exact
        // arbitrary-width `bits::Bits` the checker const-folded, so this
        // avoids a needless round trip for the single most common case.
        if let ExprKind::Int { value, .. } = &e.kind {
            let cv = crate::checker::consteval::ConstVal {
                bits: value.clone(),
                width: target_width,
                signed: false,
            };
            return self.lower_const(module, &cv, e.span);
        }
        // `design.consts` entries are always already-folded `i128`s (module
        // parameters/file-level `const`s never exceed that range by
        // construction), so the native `i128 -> u128` two's-complement cast
        // is exact here — no wide-magnitude risk, unlike the fallback arm.
        let folded: Option<crate::checker::consteval::ConstVal> = match &e.kind {
            ExprKind::Ident(name) if locals.is_none_or(|l| !l.contains_key(name)) => self
                .design
                .consts
                .get(name)
                .copied()
                .map(|v| crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(v as u128),
                    width: target_width,
                    signed: v < 0,
                }),
            _ if locals.is_none() => crate::value::const_eval_wide(e, &self.design.consts)
                .ok()
                .map(|cv| {
                    let resized = crate::value::from_const_at_width(&cv, target_width, cv.signed);
                    crate::checker::consteval::ConstVal {
                        bits: resized.bits,
                        width: target_width,
                        signed: cv.signed,
                    }
                }),
            _ => None,
        };
        match folded {
            Some(cv) => self.lower_const(module, &cv, e.span),
            None => self.lower_expr(module, e, locals, arrays),
        }
    }

    /// Whether `lower_expr_sized` would treat `e` as a compile-time
    /// constant it can re-size (a literal, a const/param `Ident`, or a
    /// larger constant expression), rather than falling through to plain
    /// `lower_expr`. Used by `ExprKind::Binary`'s arm to decide WHICH side
    /// of a width mismatch to re-lower: unlike a bare literal (always
    /// narrower than a sized sibling, since a literal's own natural width is
    /// its tightest representation), a constant EXPRESSION can come out
    /// WIDER than the real signal it's compared against (`lower_binop`'s
    /// arithmetic growth formulas apply uniformly whether or not the
    /// operands are literals — `hi - 1` lowers to `in_width + 1` bits
    /// regardless), so "re-lower whichever side is narrower" is the wrong
    /// test in general; "re-lower whichever side is const-foldable" is not.
    fn is_const_foldable(&self, e: &Expr, locals: Option<&HashMap<String, Bits>>) -> bool {
        match &e.kind {
            ExprKind::Int { .. } => true,
            ExprKind::Ident(name) if locals.is_none_or(|l| !l.contains_key(name)) => {
                self.design.consts.contains_key(name)
            }
            // Matches `lower_expr_sized`'s own fallback arm exactly —
            // `const_eval_wide`, not `const_eval`, so this predicate and the
            // resize it gates never disagree on what folds.
            _ if locals.is_none() => crate::value::const_eval_wide(e, &self.design.consts).is_ok(),
            _ => false,
        }
    }

    /// Lowers one expression to its `Bits`. `locals` carries the current
    /// `fn` call's param/`let` bindings (params zipped with lowered args,
    /// see the `FnCall` arm below) when lowering is happening INSIDE an
    /// inlined function body; `None` everywhere else (module-level
    /// wire/output/register lowering) — a plain `Ident` then always
    /// resolves as a module signal via `self.resolve`. `arrays` is `locals`'s
    /// sibling: `Some` exactly when `locals` is (an array-typed `fn` param
    /// only ever exists alongside its call's own `locals`), mapping each
    /// in-scope flattened array's bare name to its element count so
    /// `ExprKind::Index` can tell "this is `vals[i]` on an array param" from
    /// "this is a plain-vector bit-select" (GAP-1 residual Task 6).
    ///
    /// At top level (`locals: None`) the whole thing is memoized on the
    /// node's address, so a body walked once per target lowers each of its
    /// expressions exactly once. See `LowerCtx::expr_memo` for why the
    /// `fn`-body path is deliberately excluded.
    fn lower_expr(
        &mut self,
        module: &mut Module,
        e: &Expr,
        locals: Option<&HashMap<String, Bits>>,
        arrays: Option<&HashMap<String, u32>>,
    ) -> Bits {
        let site = std::ptr::from_ref(e) as usize;
        if locals.is_none()
            && let Some(bits) = self.expr_memo.get(&site)
        {
            return bits.clone();
        }
        let result = match &e.kind {
            ExprKind::Ident(name) => {
                let local = locals.and_then(|l| l.get(name)).cloned();
                match local {
                    Some(bits) => bits,
                    // Module parameters and file-level `const`s are neither
                    // `locals` nor comb-driven signals — the elaborator folds
                    // both into `design.consts` (same folded-`i128` map
                    // `value::build_env` converts via `ConstVal::from_i128`,
                    // reused here rather than re-deriving the two's-
                    // complement/width convention for a negative value).
                    None => match self.design.consts.get(name) {
                        Some(&v) => {
                            let cv = crate::checker::consteval::ConstVal::from_i128(v);
                            self.lower_const(module, &cv, e.span)
                        }
                        None => self.resolve(module, name),
                    },
                }
            }
            ExprKind::Bool(b) => {
                let const_val = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*b as u128),
                    width: 1,
                    signed: false,
                };
                self.lower_const(module, &const_val, e.span)
            }
            ExprKind::Int { value, .. } => {
                let width = crate::bits::natural_width(value).max(1);
                let const_val = crate::checker::consteval::ConstVal {
                    bits: value.clone(),
                    width,
                    signed: false,
                };
                self.lower_const(module, &const_val, e.span)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let mut a = self.lower_expr(module, lhs, locals, arrays);
                let mut b = self.lower_expr(module, rhs, locals, arrays);
                // A width mismatch here can only be a compile-time-constant
                // expression (a bare literal, a const/param identifier, or a
                // larger constant expression built from those — see
                // `is_const_foldable`) on one side: the checker's untyped
                // `Ty::CtInt`, sized by `lower_expr`'s own arms to its
                // NATURAL/arithmetic-growth width rather than the sibling's.
                // Every other shape is already reconciled to matching widths
                // by the checker. Re-lower whichever side IS const-foldable
                // at the OTHER side's width — deliberately NOT "whichever
                // side is narrower": a bare literal is always narrower than
                // its sized sibling, but a constant EXPRESSION can come out
                // WIDER instead (`lower_binop`'s arithmetic growth formulas
                // apply the same whether or not the operands are literals),
                // so testing width alone picks the wrong side for that
                // shape (see `is_const_foldable`'s own doc). If NEITHER side
                // is const-foldable, both `a`/`b` come back unchanged and
                // the mismatch reaches `lower_binop`/`validate` untouched —
                // a genuine checker/lowering disagreement, which stays a
                // loud validation failure rather than a silent truncation.
                if requires_matched_ab(*op) && a.width() != b.width() {
                    if self.is_const_foldable(lhs, locals) {
                        a = self.lower_expr_sized(module, lhs, locals, arrays, b.width());
                    } else if self.is_const_foldable(rhs, locals) {
                        b = self.lower_expr_sized(module, rhs, locals, arrays, a.width());
                    }
                }
                let shl_const_amount = (*op == BinOp::Shl)
                    .then(|| crate::value::const_eval(rhs, &self.design.consts).ok())
                    .flatten()
                    .and_then(|v| u128::try_from(v).ok());
                // Signedness of an ORDERING comparison has to come from the
                // source `Expr`s — `Bits` carries no sign bit — so it is
                // computed here, at the one call site that still has them,
                // and gated on the op the same way `shl_const_amount` is
                // above (otherwise every node of an expression tree walks its
                // own subtree, making lowering quadratic for no benefit).
                let cmp_signed = matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
                    && (self.expr_is_definitely_signed(lhs, locals)
                        || self.expr_is_definitely_signed(rhs, locals));
                self.lower_binop(module, *op, a, b, shl_const_amount, cmp_signed, e.span)
            }
            ExprKind::Unary { op, expr } => {
                let a = self.lower_expr(module, expr, locals, arrays);
                let (kind, out_width) = match op {
                    UnOp::Neg => (CellKind::Neg, a.width()),
                    UnOp::BitNot => (CellKind::Not, a.width()),
                    UnOp::LogicNot => (CellKind::LogicNot, 1),
                    UnOp::RedAnd => (CellKind::RedAnd, 1),
                    UnOp::RedOr => (CellKind::RedOr, 1),
                    UnOp::RedXor => (CellKind::RedXor, 1),
                };
                self.push_unary_cell(module, kind, a, out_width, e.span)
            }
            // `{a, b}` is Verilog-style: the first (source-order) part is
            // the MOST-significant. `ir::Bits` is index-0-is-LSB, so the
            // last part in source order goes at the low indices. No cell:
            // this is pure bit-vector reassembly, not a new value.
            ExprKind::Concat(parts) => {
                let mut ids = Vec::new();
                for part in parts.iter().rev() {
                    let bits = self.lower_expr(module, part, locals, arrays);
                    ids.extend(bits.0);
                }
                Bits(ids)
            }
            // `base[hi:lo]`, both bounds inclusive. `hi`/`lo` always
            // const-fold (checker-enforced) — the same const_eval promoted
            // from mimz-sim in Task 1. No cell: a sub-range of existing
            // nets, not a new value.
            ExprKind::Slice { base, hi, lo } => {
                let base_bits = self.lower_expr(module, base, locals, arrays);
                let hi_val = crate::value::const_eval(hi, &self.design.consts)
                    .expect("checker guarantees slice bounds const-fold")
                    as usize;
                let lo_val = crate::value::const_eval(lo, &self.design.consts)
                    .expect("checker guarantees slice bounds const-fold")
                    as usize;
                Bits(base_bits.0[lo_val..=hi_val].to_vec())
            }
            ExprKind::IfExpr { cond, then, els } => {
                let sel = self.lower_expr(module, cond, locals, arrays);
                let a = self.lower_expr(module, then, locals, arrays);
                let b = self.lower_expr(module, els, locals, arrays);
                let out_width = a.width().max(b.width());
                let out = module.alloc_bits(out_width, None);
                module.cells.push(Cell {
                    kind: CellKind::Mux,
                    pins: [("sel", sel), ("a", a), ("b", b), ("out", out.clone())]
                        .into_iter()
                        .collect(),
                    span: e.span,
                });
                out
            }
            // `base[i]` is dual-use: a full-width memory-word read when
            // `base` names a `design.mems` entry, a single-bit select
            // otherwise. Only the name tells them apart — the parser can't,
            // and the width checker branches on exactly this same test.
            ExprKind::Index { base, index } if self.indexed_mem(base).is_some() => {
                let (mem, width) = self.indexed_mem(base).unwrap();
                // A repeat encounter of THIS read node never gets here —
                // `expr_memo` intercepts it above. So reaching the
                // comparison below means a genuinely distinct read site (or
                // a re-inlined `fn` body, where the address really can
                // differ per call).
                let addr = self.lower_expr(module, index, locals, arrays);
                let ports = self.mem_read.entry(mem.clone()).or_default();
                // A DIFFERENT read site landing on the same memory. Same
                // LOWERED address: share that port. Different address: grow
                // a new one (GAP-1 residual Task 6 — this used to panic,
                // one read port per memory being the v1 ceiling).
                match ports.iter().find(|(prev, _)| *prev == addr) {
                    Some((_, rdata)) => rdata.clone(),
                    None => {
                        let rdata = module.alloc_bits(width, Some(&mem));
                        ports.push((addr, rdata.clone()));
                        rdata
                    }
                }
            }
            // `base[index]` is triple-use: a full-width memory-word read
            // handled above, an ARRAY-ELEMENT select when `base` is a bare
            // `Ident` naming an in-scope flattened array (GAP-1 residual
            // Task 6: `arrays` maps that name to its element count, set up
            // by the `FnCall` arm below exactly where `call_locals` binds
            // the `<name>_<i>` elements themselves), and a single-BIT select
            // of a plain vector otherwise. Mirrors `value::mod.rs`'s own
            // `ExprKind::Index` arm, which resolves the identical ambiguity
            // the identical way: check `array_len(name)` FIRST, fall back to
            // bit-select only when that fails.
            //
            // Unlike `Slice`'s `hi`/`lo`, the checker does NOT require a
            // plain-vector index to const-fold (`checker/widths/expr/
            // lvalue.rs` `index_in_range`: only `Ty::CtInt` is range-checked;
            // a runtime signal passes through unchecked), so both a constant
            // and a runtime `i` are real inputs in both the array and
            // plain-vector cases.
            ExprKind::Index { base, index } => {
                if let ExprKind::Ident(n) = &base.kind
                    && let Some(&len) = arrays.and_then(|a| a.get(n))
                {
                    let call_locals = locals
                        .expect("an array-scope entry only ever exists alongside call_locals");
                    match crate::value::const_eval(index, &self.design.consts) {
                        // Constant index: pure re-pointing to the flattened
                        // element's own `Bits` — no cell, same shape as
                        // `Slice`/the plain-vector constant case below, just
                        // at element width instead of 1 bit.
                        Ok(i) => call_locals[&format!("{n}_{i}")].clone(),
                        // Runtime index: `if idx==0 {name_0} else if idx==1
                        // {name_1} else ... {name_{len-1}}`, folded from the
                        // LAST element backward exactly like `lower_match`'s
                        // reverse fold — so an out-of-range index falls
                        // through every `Eq` to the unconditional last
                        // element, matching the emitter's ternary-chain
                        // default (spec/02 §1.14) and `value::mod.rs`'s own
                        // clamp-to-last behaviour. Built entirely from this
                        // file's existing `Eq`/`Mux` cells — no new CellKind.
                        Err(_) => {
                            let idx_bits = self.lower_expr(module, index, locals, arrays);
                            let mut acc = call_locals[&format!("{n}_{}", len - 1)].clone();
                            for i in (0..len - 1).rev() {
                                let elem = call_locals[&format!("{n}_{i}")].clone();
                                let i_bits = self.lower_const(
                                    module,
                                    &const_val(i as u128, idx_bits.width()),
                                    e.span,
                                );
                                let eq = self.push_binary_cell(
                                    module,
                                    CellKind::Eq,
                                    idx_bits.clone(),
                                    i_bits,
                                    1,
                                    e.span,
                                );
                                let out_width = elem.width().max(acc.width());
                                let out = module.alloc_bits(out_width, None);
                                module.cells.push(Cell {
                                    kind: CellKind::Mux,
                                    pins: [
                                        ("sel", eq),
                                        ("a", elem),
                                        ("b", acc),
                                        ("out", out.clone()),
                                    ]
                                    .into_iter()
                                    .collect(),
                                    span: e.span,
                                });
                                acc = out;
                            }
                            acc
                        }
                    }
                } else {
                    // Plain vector: a constant `i` is a pure re-pointing, no
                    // cell, exactly like `Slice` above. A runtime `i`
                    // composes the existing `BinOp::Shr` lowering (`v >> i`,
                    // via `lower_binop`) with a `Slice{0,0}` on ITS OWN
                    // result: `Shr` never grows past `v`'s width regardless
                    // of `i`'s runtime value (`lower_binop`'s `BinOp::Shr`
                    // arm), so bit 0 of the shift's freshly-allocated `Bits`
                    // is always net-index 0 — a CONSTANT slice despite `i`
                    // itself being runtime, needing no bit-level indexing
                    // machinery of its own.
                    let base_bits = self.lower_expr(module, base, locals, arrays);
                    match crate::value::const_eval(index, &self.design.consts) {
                        Ok(i) => Bits(vec![base_bits.0[i as usize]]),
                        Err(_) => {
                            let idx_bits = self.lower_expr(module, index, locals, arrays);
                            let shifted = self.lower_binop(
                                module,
                                BinOp::Shr,
                                base_bits,
                                idx_bits,
                                None,
                                false,
                                e.span,
                            );
                            Bits(vec![shifted.0[0]])
                        }
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.lower_match(module, scrutinee, arms, e.span, locals, arrays)
            }
            ExprKind::FnCall { name, args } => {
                let func = self.design.funcs.get(&name.name).unwrap_or_else(|| {
                    panic!(
                        "unknown function `{}` (checker should have caught this)",
                        name.name
                    )
                });
                // An array-typed param never gets ONE `call_locals` entry —
                // it flattens to N, one per element, keyed `"{param}_{i}"`
                // (i in 0..N) exactly like `emit_verilog`'s own scalar-port
                // convention (`emit_verilog/module/funcs.rs`'s `arrays`
                // bookkeeping and per-element `input` loop) and the AST/value
                // evaluator's `eval_fn_call` (`value/fn_eval.rs`, same
                // `"{}_{i}"` key). The call-site argument for such a param is
                // always an array literal (`search([a, b, c, d], x)` —
                // there's no surface syntax for anything else, since a module
                // signal can never itself be array-typed, E0416): lower each
                // element individually rather than trying to lower the
                // `ArrayLit` as one expression (`lower_expr`'s catch-all does
                // not handle `ExprKind::ArrayLit`, it's never a real value in
                // its own right in this scheme). `call_arrays` records each
                // flattened name's element count so the callee's OWN body
                // can resolve `vals[i]` back through `ExprKind::Index`'s
                // array branch above.
                let mut call_locals: HashMap<String, Bits> = HashMap::new();
                let mut call_arrays: HashMap<String, u32> = HashMap::new();
                for (param, arg) in func.params.iter().zip(args) {
                    if matches!(param.ty, Type::Array { .. }) {
                        let ExprKind::ArrayLit(elems) = &arg.kind else {
                            unimplemented!(
                                "array-typed fn-call argument must be an array literal (the \
                                 only shape a module signal can produce, E0416); fn `{}`, \
                                 param `{}`, got {:?}",
                                name.name,
                                param.name.name,
                                arg.kind
                            );
                        };
                        for (i, el) in elems.iter().enumerate() {
                            call_locals.insert(
                                format!("{}_{i}", param.name.name),
                                self.lower_expr(module, el, locals, arrays),
                            );
                        }
                        call_arrays.insert(param.name.name.clone(), elems.len() as u32);
                    } else {
                        call_locals.insert(
                            param.name.name.clone(),
                            self.lower_expr(module, arg, locals, arrays),
                        );
                    }
                }
                let (ret_width, _ret_signed) =
                    crate::value::type_width(&func.ret, &self.design.consts, func.span)
                        .expect("checker guarantees a fn's declared return type resolves");
                self.lower_fn_stmts(
                    module,
                    &func.stmts,
                    &func.tail,
                    &call_locals,
                    Some(&call_arrays),
                    ret_width,
                )
            }
            ExprKind::Call { func, args } => match func {
                Builtin::Extend => {
                    let base = self.lower_expr(module, &args[0], locals, arrays);
                    let target = crate::value::const_eval(&args[1], &self.design.consts)
                        .expect("checker guarantees extend's width folds")
                        as u32;
                    if target <= base.width() {
                        base
                    } else {
                        if !self.arg_is_definitely_unsigned(&args[0], locals) {
                            unimplemented!(
                                "ir::lower cannot prove `extend`'s argument is unsigned, \
                                 so it cannot choose between zero- and sign-extension \
                                 (ir::Bits has no signed bit in v1); span: {:?}",
                                args[0].span
                            );
                        }
                        let pad =
                            self.lower_const(module, &const_val(0, target - base.width()), e.span);
                        Bits([base.0, pad.0].concat())
                    }
                }
                Builtin::Trunc => {
                    let base = self.lower_expr(module, &args[0], locals, arrays);
                    let target = crate::value::const_eval(&args[1], &self.design.consts)
                        .expect("checker guarantees trunc's width folds")
                        as u32;
                    debug_assert!(
                        target <= base.width(),
                        "checker (E0407) guarantees trunc never widens"
                    );
                    Bits(base.0[..(target.min(base.width())) as usize].to_vec())
                }
                Builtin::SignedCast | Builtin::UnsignedCast | Builtin::Encoding => {
                    self.lower_expr(module, &args[0], locals, arrays)
                }
                Builtin::Nand | Builtin::Nor | Builtin::Xnor => {
                    let a = self.lower_expr(module, &args[0], locals, arrays);
                    let reduce_kind = match func {
                        Builtin::Nand => CellKind::RedAnd,
                        Builtin::Nor => CellKind::RedOr,
                        Builtin::Xnor => CellKind::RedXor,
                        _ => unreachable!("outer match already narrowed `func` to Nand|Nor|Xnor"),
                    };
                    // `nand(x)` = `!(&x)`, `nor(x)` = `!(|x)`, `xnor(x)` =
                    // `!(^x)` — composing the existing reduction cell with
                    // `LogicNot` needs no new `CellKind` and no signedness
                    // (a reduction's result is always 1 bit unsigned).
                    let reduced = self.push_unary_cell(module, reduce_kind, a, 1, e.span);
                    self.push_unary_cell(module, CellKind::LogicNot, reduced, 1, e.span)
                }
                Builtin::Min | Builtin::Max | Builtin::Abs => unimplemented!(
                    "ir::lower does not lower `{func:?}` — its correctness depends on \
                     signed interpretation, and ir::Bits has no signed bit in v1 (see \
                     docs/audit/gaps.md GAP-1); span: {:?}",
                    e.span
                ),
                // `clog2` always const-folds before a checked Design exists
                // (checker rejects it in a runtime value position, E0407);
                // `sync.double_flop`/`sync.pulse` are always desugared by
                // `ast::sync_prim_lower::expand_sync_prims` before
                // elaboration produces a `Design` — same guarantee
                // `value::fn_eval::call`'s matching arm already relies on.
                Builtin::Clog2 => unreachable!(
                    "clog2 is compile-time only and always const-folds before a \
                     checked Design exists"
                ),
                Builtin::SyncDoubleFlop | Builtin::SyncPulse => unreachable!(
                    "sync.double_flop/sync.pulse are always desugared by \
                     ast::sync_prim_lower::expand_sync_prims before elaborate() \
                     produces a Design"
                ),
            },
            // `{N{a, b}}` is Verilog-style replication: the inner concatenation
            // `{a, b}` repeated `count` times. `count` always const-folds
            // (checker-enforced) — the same guarantee `Slice`'s `hi`/`lo` already
            // rely on. No cell: this is pure bit-vector reassembly (parts' nets
            // are reused, not reallocated), same as `Concat`.
            ExprKind::Replicate { count, parts } => {
                let n = crate::value::const_eval(count, &self.design.consts)
                    .expect("checker guarantees a replicate count const-folds")
                    as usize;
                let mut ids = Vec::new();
                for _ in 0..n {
                    for part in parts.iter().rev() {
                        let bits = self.lower_expr(module, part, locals, arrays);
                        ids.extend(bits.0);
                    }
                }
                Bits(ids)
            }
            other => unimplemented!(
                "expression form not yet lowered by Task 6 (see later tasks for \
                 field access): {other:?}"
            ),
        };
        if locals.is_none() {
            self.expr_memo.insert(site, result.clone());
        }
        result
    }

    /// Classifies an assignment target. A bit-select/slice write to a
    /// plain signal is a `Target::BitSelect` — the actual per-bit merge
    /// happens in `lower_seq_stmts`'s `SeqStmt::Assign` arm, which still
    /// has `lhs.index` in scope.
    fn assign_target(&self, lhs: &LValue) -> Target {
        if lhs.index.is_none() {
            return Target::Signal(lhs.base.name.clone());
        }
        if self.design.mems.iter().any(|m| m.name == lhs.base.name) {
            return Target::MemWrite(lhs.base.name.clone());
        }
        Target::BitSelect(lhs.base.name.clone())
    }

    /// Merges `rhs` into `base_bits` at the position(s) `first`/`second`
    /// describe, keeping every other bit of `base_bits` unchanged.
    /// `first`/`second: None` is a single-bit write (`q[i] <- v`, `i` may
    /// be constant or a runtime signal); `second: Some(lo)` is a slice
    /// write (`q[hi:lo] <- v`), whose bounds ALWAYS const-fold (checker-
    /// enforced, same guarantee a plain `Slice` read relies on).
    fn lower_bitselect_write(
        &mut self,
        module: &mut Module,
        base_bits: &Bits,
        first: &Expr,
        second: Option<&Expr>,
        rhs: Bits,
        span: crate::span::Span,
    ) -> Bits {
        match second {
            Some(lo) => {
                let hi = crate::value::const_eval(first, &self.design.consts)
                    .expect("checker guarantees a slice write's hi bound const-folds")
                    as usize;
                let lo = crate::value::const_eval(lo, &self.design.consts)
                    .expect("checker guarantees a slice write's lo bound const-folds")
                    as usize;
                let mut nets = base_bits.0.clone();
                nets[lo..=hi].clone_from_slice(&rhs.0);
                Bits(nets)
            }
            None => match crate::value::const_eval(first, &self.design.consts) {
                Ok(i) => {
                    let mut nets = base_bits.0.clone();
                    nets[i as usize] = rhs.0[0];
                    Bits(nets)
                }
                Err(_) => {
                    let idx_bits = self.lower_expr(module, first, None, None);
                    let width = base_bits.width();
                    let mut nets = Vec::with_capacity(width as usize);
                    for i in 0..width {
                        let i_const =
                            self.lower_const(module, &const_val(i as u128, idx_bits.width()), span);
                        let is_i = self.push_binary_cell(
                            module,
                            CellKind::Eq,
                            idx_bits.clone(),
                            i_const,
                            1,
                            span,
                        );
                        let base_bit = Bits(vec![base_bits.0[i as usize]]);
                        let out = module.alloc_bits(1, None);
                        module.cells.push(Cell {
                            kind: CellKind::Mux,
                            pins: [
                                ("sel", is_i),
                                ("a", rhs.clone()),
                                ("b", base_bit),
                                ("out", out.clone()),
                            ]
                            .into_iter()
                            .collect(),
                            span,
                        });
                        nets.push(out.0[0]);
                    }
                    Bits(nets)
                }
            },
        }
    }

    /// `(name, word width)` of the memory `base` indexes, if `base` is a
    /// bare identifier naming a `design.mems` entry.
    fn indexed_mem(&self, base: &Expr) -> Option<(String, u32)> {
        let ExprKind::Ident(name) = &base.kind else {
            return None;
        };
        self.design
            .mems
            .iter()
            .find(|m| m.name == *name)
            .map(|m| (m.name.clone(), m.width.bits))
    }

    /// The declared signedness of a module-level signal named `name`, or
    /// `None` if `name` isn't an input/output/wire/register (a memory name,
    /// or a name this function simply doesn't know — never expected for a
    /// real `Design`, since the checker already resolved every such name).
    fn declared_signed(&self, name: &str) -> Option<bool> {
        self.design
            .inputs
            .iter()
            .chain(&self.design.outputs)
            .chain(&self.design.wires)
            .find(|s| s.name == name)
            .map(|s| s.width.signed)
            .or_else(|| {
                self.design
                    .regs
                    .iter()
                    .find(|r| r.name == name)
                    .map(|r| r.width.signed)
            })
    }

    /// Best-effort proof that `e`'s value is definitely UNSIGNED, so
    /// `extend`'s zero-fill is the correct extension for it. `ir::Bits`
    /// carries no signed bit in v1 (`ir/exec.rs`'s own documented "unsigned
    /// only" limitation) — this function exists so `extend` NEVER silently
    /// zero-extends a value it cannot prove is unsigned; anything it
    /// returns `false` for must be refused loudly by the caller, not
    /// assumed safe. Recognizes exactly the shapes real programs use to
    /// size a literal (`extend(1, N)`, `extend(x, N)` for a plain unsigned
    /// signal, `extend(unsigned(x), N)`/`extend(encoding(e), N)`); anything
    /// else — including any reference inside an inlined `fn` body, where a
    /// param's declared signedness isn't threaded through `locals` in v1 —
    /// is conservatively `false`. See `docs/audit/gaps.md` GAP-1.
    fn arg_is_definitely_unsigned(&self, e: &Expr, locals: Option<&HashMap<String, Bits>>) -> bool {
        match &e.kind {
            ExprKind::Int { .. } | ExprKind::Bool(_) => true,
            ExprKind::Call {
                func: Builtin::UnsignedCast | Builtin::Encoding,
                ..
            } => true,
            // Only consult `declared_signed` when `name` is NOT shadowed by
            // a `fn` param/`let` binding — `lower_expr`'s own `Ident` arm
            // resolves a shadowed name through `locals` first, so a
            // module-level signal of the same name would be the WRONG
            // thing to ask about here. Strictly more permissive than the
            // old `locals.is_none()` guard (which refused every `Ident`
            // inside any inlined `fn` body, even an unshadowed reference to
            // a module signal), with no loss of soundness.
            ExprKind::Ident(name) if locals.is_none_or(|l| !l.contains_key(name)) => {
                self.declared_signed(name) == Some(false)
            }
            _ => false,
        }
    }

    /// Best-effort proof that `e`'s value is DEFINITELY signed AND that
    /// `lower` sizes it at exactly the width the CHECKER types it at — the
    /// mirror image of `arg_is_definitely_unsigned` above, and the input to
    /// `CellKind::{Lt,Le,Gt,Ge}`'s `signed` flag.
    ///
    /// That second half is not decoration, it is the whole soundness
    /// argument. `lower_binop` only trusts this answer when the two operand
    /// pins came out the SAME width, and that guard is only meaningful if a
    /// `true` here implies "this pin's width IS the checker's type width".
    /// Wherever `ir::lower`'s width formula and the checker's disagree, a
    /// literal on the other side can match the LOWERED width while the
    /// checker sized it from the (different) TYPE width, and reinterpreting
    /// those bits as two's complement silently changes the answer. Two such
    /// divergences exist today:
    ///
    /// * `UnOp::Neg` — `checker::widths::ops` grows it (`Signed(n)` ->
    ///   `Signed(n + 1)`, gaining the carry bit) but `lower_expr`'s `Neg` arm
    ///   keeps `a.width()`. So `-a < 200` for `a: signed[8]` has an 8-bit
    ///   pin against a checker type of `Signed(9)`, the 8-bit literal `200`
    ///   matches the pin width, and reading it as signed flips half the
    ///   input domain. See `docs/audit/gaps.md`.
    /// * `BinOp::Mul` with a literal operand — `lower_binop` sizes it
    ///   `a.width() + b.width()` from the literal's NATURAL width, while the
    ///   checker first adapts the literal to the sized side's type: `a * 2`
    ///   for `a: signed[8]` is 10 bits lowered but `Signed(16)` typed.
    ///
    /// So this recognizes ONLY the two shapes whose lowered width is, by
    /// construction, the declared width of a module-level signal: a bare
    /// `Ident`, and `signed(<Ident>)` (a free reinterpret — `lower`'s
    /// `SignedCast` arm repoints at its argument's `Bits` and allocates
    /// nothing). Everything else is conservatively `false`, i.e. an unsigned
    /// comparison — today's behaviour, so an unrecognized shape is never a
    /// regression. In particular a bare literal is the checker's untyped
    /// `Ty::CtInt`: it INHERITS the other operand's type
    /// (`checker::widths::ops::matched_ty`) rather than deciding signedness
    /// itself, so it answers `false` and lets the sized side decide.
    ///
    /// Two non-literal operands cannot disagree: `matched_ty` rejects a
    /// genuinely mixed comparison outright (E0403 "cannot mix X and Y"), so
    /// the caller ORs the two answers rather than reconciling them.
    fn expr_is_definitely_signed(&self, e: &Expr, locals: Option<&HashMap<String, Bits>>) -> bool {
        match &e.kind {
            ExprKind::Ident(name) => self.unshadowed_signal_signed(name, locals) == Some(true),
            // `signed(x)` over a bare identifier. The cast is free — `lower`'s
            // `SignedCast` arm repoints at `x`'s `Bits` and allocates nothing
            // — so the pin keeps `x`'s DECLARED width whatever `x`'s own
            // declared signedness was, which is the point: `signed(a) <
            // signed(b)` over two UNSIGNED-declared signals is GAP-1's
            // headline case. Hence `.is_some()`, not `== Some(true)`.
            ExprKind::Call {
                func: Builtin::SignedCast,
                args,
            } => match args.first().map(|a| &a.kind) {
                Some(ExprKind::Ident(name)) => {
                    self.unshadowed_signal_signed(name, locals).is_some()
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// The declared signedness of `name`, but ONLY when `name` really is a
    /// module-level signal and is not shadowed by a `fn` param/`let` binding
    /// — `lower_expr`'s own `Ident` arm resolves a shadowed name through
    /// `locals` first, so a module-level signal of the same name would be the
    /// wrong thing to ask about (the same guard `arg_is_definitely_unsigned`
    /// uses). `None` therefore means "no declared width to reason about",
    /// which is exactly what `expr_is_definitely_signed`'s callers need.
    fn unshadowed_signal_signed(
        &self,
        name: &str,
        locals: Option<&HashMap<String, Bits>>,
    ) -> Option<bool> {
        locals
            .is_none_or(|l| !l.contains_key(name))
            .then(|| self.declared_signed(name))
            .flatten()
    }

    /// Walks one `fn` body's statement list, evaluating against `locals`
    /// (params + `let`s bound so far). Mirrors
    /// `emit_verilog::module::funcs::emit_fn_stmts`'s continuation-passing
    /// shape: an unconditional `Return` short-circuits before any later
    /// statement or `tail` is ever reached, exactly like that renderer's
    /// `rest` threading — checker-guaranteed (E0812) so no reachability
    /// analysis is needed here either.
    fn lower_fn_stmts(
        &mut self,
        module: &mut Module,
        stmts: &[FnStmt],
        tail: &Expr,
        locals: &HashMap<String, Bits>,
        arrays: Option<&HashMap<String, u32>>,
        target_width: u32,
    ) -> Bits {
        match stmts.split_first() {
            None => self.lower_expr_sized(module, tail, Some(locals), arrays, target_width),
            Some((FnStmt::Let(l), rest)) => {
                let v = self.lower_expr(module, &l.value, Some(locals), arrays);
                let mut locals2 = locals.clone();
                locals2.insert(l.name.name.clone(), v);
                self.lower_fn_stmts(module, rest, tail, &locals2, arrays, target_width)
            }
            Some((FnStmt::Return(e), _rest)) => {
                self.lower_expr_sized(module, e, Some(locals), arrays, target_width)
            }
            Some((FnStmt::If { cond, then, els }, rest)) => {
                let sel = self.lower_expr(module, cond, Some(locals), arrays);
                let then_full: Vec<FnStmt> = then.iter().chain(rest.iter()).cloned().collect();
                let then_val =
                    self.lower_fn_stmts(module, &then_full, tail, locals, arrays, target_width);
                let els_slice: &[FnStmt] = els.as_deref().unwrap_or(&[]);
                let els_full: Vec<FnStmt> = els_slice.iter().chain(rest.iter()).cloned().collect();
                let else_val =
                    self.lower_fn_stmts(module, &els_full, tail, locals, arrays, target_width);
                // Both branches are now already sized to target_width by the
                // Return/tail arms above — out_width no longer needs
                // max(then, else); it IS target_width, by construction.
                let out = module.alloc_bits(target_width, None);
                module.cells.push(Cell {
                    kind: CellKind::Mux,
                    pins: [
                        ("sel", sel),
                        ("a", then_val),
                        ("b", else_val),
                        ("out", out.clone()),
                    ]
                    .into_iter()
                    .collect(),
                    span: cond.span,
                });
                out
            }
            Some((FnStmt::Loop { span, .. }, _)) | Some((FnStmt::ForEach { span, .. }, _)) => {
                unimplemented!(
                    "loop/foreach unrolling inside fn bodies not yet lowered (needs \
                     const-var-substitution machinery); span: {span:?}"
                )
            }
            Some((FnStmt::Error(_), rest)) => {
                self.lower_fn_stmts(module, rest, tail, locals, arrays, target_width)
            }
        }
    }

    /// Emits a `CellKind::Const` cell and returns its `out` pin — the one
    /// path every literal (Task 5) and synthesized reset value (Task 8)
    /// goes through, so there's exactly one place a constant becomes a
    /// net.
    fn lower_const(
        &mut self,
        module: &mut Module,
        value: &crate::checker::consteval::ConstVal,
        span: crate::span::Span,
    ) -> Bits {
        let out = module.alloc_bits(value.width, None);
        module.cells.push(Cell {
            kind: CellKind::Const {
                value: value.clone(),
            },
            pins: [("out", out.clone())].into_iter().collect(),
            span,
        });
        out
    }

    // `shl_const_amount` and `cmp_signed` are both facts read off the SOURCE
    // `Expr`s that `Bits` cannot carry, so they have to arrive as parameters;
    // bundling them into a struct would relocate the argument count, not
    // reduce it.
    #[allow(clippy::too_many_arguments)]
    fn lower_binop(
        &mut self,
        module: &mut Module,
        op: BinOp,
        a: Bits,
        b: Bits,
        shl_const_amount: Option<u128>,
        cmp_signed: bool,
        span: crate::span::Span,
    ) -> Bits {
        let in_width = a.width().max(b.width());
        // An ordering comparison is lowered as SIGNED only when its operands
        // agree on WIDTH as well as sign. Two non-literal operands always do
        // (`checker::widths::ops::matched_ty` rejects a genuinely mixed
        // comparison outright, E0403), so this costs nothing there. A width
        // MISMATCH means the narrow side is a bare literal, which the checker
        // types as untyped `Ty::CtInt` inheriting the sized side's type —
        // but `lower_expr`'s `Int` arm sizes it at its own NATURAL width
        // instead (`5` -> 3 bits), and reinterpreting those 3 bits as two's
        // complement would read `5` as `-3`. Until literals are sized from
        // their comparison context (a separate residual, see
        // `docs/audit/gaps.md` GAP-1), those cases stay unsigned exactly as
        // they are today rather than becoming newly, differently wrong.
        let cmp_signed = cmp_signed && a.width() == b.width();
        let (kind, out_width) = match op {
            BinOp::Add => (CellKind::Add, in_width + 1),
            BinOp::Sub => (CellKind::Sub, in_width + 1),
            BinOp::Mul => (CellKind::Mul, a.width() + b.width()),
            BinOp::AddWrap => (CellKind::AddWrap, in_width),
            BinOp::SubWrap => (CellKind::SubWrap, in_width),
            BinOp::MulWrap => (CellKind::MulWrap, in_width),
            BinOp::Shl => {
                // `shl_const_amount` is threaded in from the one call site
                // (`ExprKind::Binary`, above) which const-evals the source
                // `rhs` `Expr` before it's lowered to `Bits` — so when the
                // shift amount is a compile-time constant, `out` is sized
                // exactly (`a.width() + k`), matching the checker's own
                // `shift_ty` (`checker/widths/ops/mod.rs`) and the AST
                // evaluator's `eval_shift_chain` (`value/binary.rs`). For a
                // genuinely RUNTIME (non-constant) shift amount,
                // `shl_const_amount` is `None` and this falls back to
                // worst-case growth, exactly matching the AST evaluator's
                // own fallback (`value::binary::shl`'s `width_rules::
                // shift_result` call with the same `None`). `Shr` (below)
                // never grows, so it needs no change (`width_rules::
                // shift_result`'s own doc: "grows: false" keeps the left
                // operand's width).
                let out_width = crate::width_rules::shift_result(
                    crate::width_rules::Kind {
                        width: a.width(),
                        signed: false,
                    },
                    crate::width_rules::Kind {
                        width: b.width(),
                        signed: false,
                    },
                    shl_const_amount,
                    true,
                )
                .expect(
                    "Shl growth exceeded MAX_WIDTH — the checker independently sizes this \
                     the same way (exact constant growth for a compile-time shift amount, \
                     worst-case growth otherwise), so a checker-accepted program is not \
                     expected to panic here; a pathological shift amount could still \
                     legitimately do so (see docs/audit/gaps.md GAP-1)",
                )
                .width;
                (CellKind::Shl, out_width)
            }
            BinOp::Shr => (CellKind::Shr, a.width()),
            BinOp::BitAnd => (CellKind::And, in_width),
            BinOp::BitOr => (CellKind::Or, in_width),
            BinOp::BitXor => (CellKind::Xor, in_width),
            BinOp::Eq => (CellKind::Eq, 1),
            BinOp::Ne => (CellKind::Ne, 1),
            // `Eq`/`Ne` above stay sign-agnostic on purpose; only the
            // ORDERING comparisons read `cmp_signed`.
            BinOp::Lt => (CellKind::Lt { signed: cmp_signed }, 1),
            BinOp::Le => (CellKind::Le { signed: cmp_signed }, 1),
            BinOp::Gt => (CellKind::Gt { signed: cmp_signed }, 1),
            BinOp::Ge => (CellKind::Ge { signed: cmp_signed }, 1),
            BinOp::LogicAnd => (CellKind::LogicAnd, 1),
            BinOp::LogicOr => (CellKind::LogicOr, 1),
            // `??` never reaches `ir::lower` at MODULE level (wire/reg
            // declarations, assignments, instance connections, fn-call
            // arguments) in either of its two source forms — both are
            // eliminated by `elaborate` before a `Design` exists, the same
            // "checker-legal but always eliminated earlier" situation as
            // `Builtin::Clog2`/`SyncDoubleFlop`/`SyncPulse` above
            // (`ExprKind::Call` match). Verified empirically (not just by
            // re-reading these citations) by
            // `lower_coalesce_is_unreachable_for_both_source_forms` below,
            // which runs a real `??` fixture through the full lex -> parse ->
            // check -> elaborate_project -> lower pipeline and confirms no
            // `BinOp::Coalesce` node survives into the `Design`:
            // - unwrap form (`raw ?? 0`, scalar result): rewritten to an
            //   `ExprKind::IfExpr` (`if raw.valid { raw.data } else { 0 }`)
            //   by `Rw::expr`'s dedicated `Binary{Coalesce}` arm in
            //   `elaborate/rewrite.rs` (`crates/mimz-core/src/elaborate/
            //   rewrite.rs:58-91`), recursed into immediately.
            // - OR-mux form (`x ?? y`, both sides and the result stay
            //   bundle-typed): intercepted earlier still, at bundle-typed
            //   signal-declaration/assignment/argument time, by
            //   `bundle_field_expr` in `elaborate/bundle.rs`
            //   (`crates/mimz-core/src/elaborate/bundle.rs:47-92`) — its own
            //   doc comment states the OR-mux form "never reaches [`Rw::
            //   expr`'s] generic scalar-expression rewrite".
            //
            // Also proven (2026-09-15) for a bundle-typed `fn` PARAMETER
            // used BARE (not via `.field`) inside the fn's own body, e.g.
            // `fn f(h: Handshake) -> bits[8] { h ?? 0 }` — the UNWRAP form
            // only. `flatten_bundle_refs_expr` (`elaborate/bundle.rs`) now
            // has its own `Binary{Coalesce}` case mirroring `Rw::expr`'s
            // desugaring exactly (`.valid`/`.data` field access + `IfExpr`,
            // recursed back through itself so the resulting `Field` nodes
            // flatten to `h_valid`/`h_data`). The OR-mux form (`x ?? y`,
            // bundle-typed result) for a bare bundle-typed fn TAIL is still
            // unproven — no fn-body equivalent of `bundle_field_expr`'s
            // signal-declaration-time interception exists — see
            // `docs/audit/gaps.md`'s resolution note for the (closed)
            // "bare bundle-typed fn parameter" sub-gap.
            BinOp::Coalesce => unreachable!(
                "`??` (BinOp::Coalesce) never reaches ir::lower for a MODULE-level use \
                 (wire/reg decl, assignment, instance connection, fn-call argument) — both \
                 forms are eliminated by elaborate before a Design exists there: the unwrap \
                 form (`raw ?? 0`) becomes an IfExpr in Rw::expr's own Binary{{Coalesce}} arm \
                 (elaborate/rewrite.rs), and the OR-mux form (`x ?? y`, bundle-typed) is \
                 intercepted at bundle-typed signal-declaration time by bundle_field_expr \
                 (elaborate/bundle.rs) before it ever reaches a generic expression rewrite. \
                 Also eliminated for a bare bundle-typed fn PARAMETER's unwrap-form use \
                 (`h ?? 0` inside the fn's own body) by flatten_bundle_refs_expr's own \
                 Binary{{Coalesce}} case (elaborate/bundle.rs) — see \
                 docs/audit/gaps.md's \"bare bundle-typed fn parameter\" sub-gap for what's \
                 still NOT covered (the OR-mux form for a bundle-typed fn tail); if you hit \
                 this panic from that shape, it's a real gap, not a bug in this assertion"
            ),
            // No catch-all left: `BinOp` has exactly 20 variants (see
            // `ast/expr.rs`) and every one is now matched explicitly above —
            // a residual `other => unimplemented!(...)` arm here would be
            // dead code (compiler-flagged `unreachable_patterns`), not a
            // safety net for a future variant.
        };
        let out = module.alloc_bits(out_width, None);
        module.cells.push(Cell {
            kind,
            pins: [("a", a), ("b", b), ("out", out.clone())]
                .into_iter()
                .collect(),
            span,
        });
        out
    }

    /// Lowers `match scrutinee { arms }` as a reverse fold of nested `Mux`
    /// cells: the last arm (or any arm containing `Pattern::Wildcard`) is
    /// the unconditional default, folded in first, then earlier arms are
    /// wrapped around it in reverse declaration order so the FIRST matching
    /// pattern wins — matching `emit_verilog::expr::match_subst`'s
    /// `is_last || is_wild` priority exactly, so IR execution and Verilog
    /// output never disagree on tie-breaking.
    fn lower_match(
        &mut self,
        module: &mut Module,
        scrutinee: &Expr,
        arms: &[crate::ast::Arm],
        span: crate::span::Span,
        locals: Option<&HashMap<String, Bits>>,
        arrays: Option<&HashMap<String, u32>>,
    ) -> Bits {
        let scrutinee_bits = self.lower_expr(module, scrutinee, locals, arrays);
        let n = arms.len();
        let mut acc = self.lower_expr(module, &arms[n - 1].value, locals, arrays);
        for arm in arms[..n - 1].iter().rev() {
            let sel = self.lower_pattern_conds(module, &scrutinee_bits, &arm.patterns, span);
            let arm_value = self.lower_expr(module, &arm.value, locals, arrays);
            let out_width = arm_value.width().max(acc.width());
            let out = module.alloc_bits(out_width, None);
            module.cells.push(Cell {
                kind: CellKind::Mux,
                pins: [
                    ("sel", sel),
                    ("a", arm_value),
                    ("b", acc),
                    ("out", out.clone()),
                ]
                .into_iter()
                .collect(),
                span,
            });
            acc = out;
        }
        acc
    }

    /// One arm's patterns OR'd together into a single 1-bit selector.
    fn lower_pattern_conds(
        &mut self,
        module: &mut Module,
        scrutinee: &Bits,
        patterns: &[crate::ast::Pattern],
        span: crate::span::Span,
    ) -> Bits {
        let mut acc: Option<Bits> = None;
        for p in patterns {
            let cond = self.lower_pattern_eq(module, scrutinee, p, span);
            acc = Some(match acc {
                None => cond,
                Some(prev) => self.push_binary_cell(module, CellKind::LogicOr, prev, cond, 1, span),
            });
        }
        acc.expect("checker guarantees every match arm has at least one pattern")
    }

    /// Lowers a single pattern to the 1-bit "does `scrutinee` match this
    /// pattern" condition.
    fn lower_pattern_eq(
        &mut self,
        module: &mut Module,
        scrutinee: &Bits,
        p: &crate::ast::Pattern,
        span: crate::span::Span,
    ) -> Bits {
        use crate::ast::Pattern;
        match p {
            Pattern::Wildcard => {
                let cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(1),
                    width: 1,
                    signed: false,
                };
                self.lower_const(module, &cv, span)
            }
            Pattern::Bool(b) => {
                let cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*b as u128),
                    width: scrutinee.width(),
                    signed: false,
                };
                let const_bits = self.lower_const(module, &cv, span);
                self.push_binary_cell(module, CellKind::Eq, scrutinee.clone(), const_bits, 1, span)
            }
            Pattern::Int { value, .. } => {
                let cv = crate::checker::consteval::ConstVal {
                    bits: value.clone(),
                    width: scrutinee.width(),
                    signed: false,
                };
                let const_bits = self.lower_const(module, &cv, span);
                self.push_binary_cell(module, CellKind::Eq, scrutinee.clone(), const_bits, 1, span)
            }
            Pattern::IntMask {
                value, mask, width, ..
            } => {
                // `(scrutinee & mask) == value`, both sized to the
                // pattern's own width.
                let mask_cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*mask),
                    width: *width,
                    signed: false,
                };
                let mask_bits = self.lower_const(module, &mask_cv, span);
                let masked = self.push_binary_cell(
                    module,
                    CellKind::And,
                    scrutinee.clone(),
                    mask_bits,
                    *width,
                    span,
                );
                let value_cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*value),
                    width: *width,
                    signed: false,
                };
                let value_bits = self.lower_const(module, &value_cv, span);
                self.push_binary_cell(module, CellKind::Eq, masked, value_bits, 1, span)
            }
            Pattern::Variant { .. } => unreachable!(
                "Pattern::Variant never reaches ir::lower — elaborate::rewrite.rs already rewrites every \
                 Variant pattern to Int (tag-only) or IntMask (tagged+payload) before Design exists; its own \
                 comment states \"the runtime evaluator never sees Pattern::Variant\""
            ),
        }
    }

    /// Shared 2-input/1-output cell constructor — used by the pattern-matching
    /// helpers above (Eq/And/LogicOr all have this exact shape).
    fn push_binary_cell(
        &mut self,
        module: &mut Module,
        kind: CellKind,
        a: Bits,
        b: Bits,
        out_width: u32,
        span: crate::span::Span,
    ) -> Bits {
        let out = module.alloc_bits(out_width, None);
        module.cells.push(Cell {
            kind,
            pins: [("a", a), ("b", b), ("out", out.clone())]
                .into_iter()
                .collect(),
            span,
        });
        out
    }

    /// Shared 1-input/1-output cell constructor — used by `ExprKind::Unary`'s
    /// own arm (Neg/BitNot/LogicNot/RedAnd/RedOr/RedXor all have this exact
    /// shape) and by `Builtin::Nand`/`Nor`/`Xnor`'s RedAnd/RedOr/RedXor-then-
    /// LogicNot composition (in `lower_expr`'s `ExprKind::Call` match, above),
    /// which pushes both of ITS cells through this one code path.
    fn push_unary_cell(
        &mut self,
        module: &mut Module,
        kind: CellKind,
        a: Bits,
        out_width: u32,
        span: crate::span::Span,
    ) -> Bits {
        let out = module.alloc_bits(out_width, None);
        module.cells.push(Cell {
            kind,
            pins: [("a", a), ("out", out.clone())].into_iter().collect(),
            span,
        });
        out
    }

    /// Walks one `on`-block body, folding last-write-wins per-target
    /// assignment through `if`/`else`, mirroring
    /// `emit_verilog::module::seq::seq_stmts`'s two-pass structure
    /// (D-DEFAULT-3: every `Default` is seeded into `env` before any
    /// `Assign`/`If` is processed, so a conditional assign always wins over
    /// a default for the same target).
    fn lower_seq_stmts(
        &mut self,
        module: &mut Module,
        stmts: &[SeqStmt],
        env: &mut HashMap<String, Bits>,
    ) {
        for stmt in stmts {
            if let SeqStmt::Default { name, val, .. } = stmt {
                // Only targets the caller pre-seeded are tracked: one walk
                // of the body is done per register AND per memory write
                // port, and each walk must leave `env`'s key set untouched
                // or the branch merge below indexes a key that was never
                // seeded. Assignments to everything else are simply not
                // this walk's business.
                if env.contains_key(&name.name) {
                    // `on`-block register lowering is always top-level, never
                    // inside a `fn` body — no call-local bindings in scope.
                    let bits = self.lower_expr(module, val, None, None);
                    env.insert(name.name.clone(), bits);
                }
            }
        }
        for stmt in stmts {
            match stmt {
                SeqStmt::Assign { lhs, rhs } => match self.assign_target(lhs) {
                    Target::Signal(name) => {
                        if env.contains_key(&name) {
                            let bits = self.lower_expr(module, rhs, None, None);
                            env.insert(name, bits);
                        }
                    }
                    // `m[addr] <- v` drives three write-port values at once;
                    // they then fold through the `if` merge below exactly
                    // like any register's, so a conditional write comes out
                    // as `wen = Mux(cond, 1, 0)` for free.
                    Target::MemWrite(mem) => {
                        let (wen_k, waddr_k, wdata_k) = mem_write_keys(&mem);
                        if env.contains_key(&wen_k) {
                            // `.1` is the ranged form's second bound
                            // (`x[hi:lo] <- ..`); a memory write is always a
                            // whole-word `m[addr] <- v`, so the checker never
                            // lets a range reach a `design.mems` base.
                            let addr_expr = &lhs.index.as_ref().expect("MemWrite implies index").0;
                            let addr = self.lower_expr(module, addr_expr, None, None);
                            let data = self.lower_expr(module, rhs, None, None);
                            let one = self.lower_const(module, &const_val(1, 1), lhs.span);
                            env.insert(wen_k, one);
                            env.insert(waddr_k, addr);
                            env.insert(wdata_k, data);
                        }
                    }
                    Target::BitSelect(name) => {
                        if env.contains_key(&name) {
                            let (first, second) =
                                lhs.index.as_ref().expect("BitSelect implies index");
                            let base_bits = env[&name].clone();
                            let rhs_bits = self.lower_expr(module, rhs, None, None);
                            let merged = self.lower_bitselect_write(
                                module,
                                &base_bits,
                                first,
                                second.as_ref(),
                                rhs_bits,
                                lhs.span,
                            );
                            env.insert(name, merged);
                        }
                    }
                },
                SeqStmt::If { cond, then, els } => {
                    let sel = self.lower_expr(module, cond, None, None);
                    let mut then_env = env.clone();
                    self.lower_seq_stmts(module, then, &mut then_env);
                    let mut else_env = env.clone();
                    if let Some(else_stmts) = els {
                        self.lower_seq_stmts(module, else_stmts, &mut else_env);
                    }
                    let mut changed: Vec<String> =
                        then_env.keys().chain(else_env.keys()).cloned().collect();
                    changed.sort();
                    changed.dedup();
                    for name in changed {
                        let before = env[&name].clone();
                        let a = then_env
                            .get(&name)
                            .cloned()
                            .unwrap_or_else(|| before.clone());
                        let b = else_env.get(&name).cloned().unwrap_or(before);
                        if a == b {
                            env.insert(name, a);
                            continue;
                        }
                        let out_width = a.width().max(b.width());
                        let out = module.alloc_bits(out_width, None);
                        module.cells.push(Cell {
                            kind: CellKind::Mux,
                            pins: [
                                ("sel", sel.clone()),
                                ("a", a),
                                ("b", b),
                                ("out", out.clone()),
                            ]
                            .into_iter()
                            .collect(),
                            span: crate::span::Span::default(),
                        });
                        env.insert(name, out);
                    }
                }
                SeqStmt::Default { .. } => {} // already seeded above
                SeqStmt::Loop { span, .. } | SeqStmt::ForEach { span, .. } => {
                    unimplemented!(
                        "loop/foreach unrolling inside on-blocks not yet lowered by \
                         Task 8 (needs const-var-substitution machinery); span: {span:?}"
                    )
                }
                // assert/cover never synthesized (design doc decision);
                // Error is parser-recovery-only, unreachable on the
                // elaborated-Design path.
                SeqStmt::Assert(_) | SeqStmt::Cover(_) | SeqStmt::Error(_) => {}
            }
        }
    }
}

/// What an `on`-block assignment writes to. `LValue` is dual-use in the
/// same way `ExprKind::Index` is: `m[addr] <- v` (a memory write) and
/// `q[3] <- v` (a bit-select register write) are the same AST shape, and
/// only whether the base names a `design.mems` entry separates them.
enum Target {
    Signal(String),
    MemWrite(String),
    BitSelect(String),
}

/// Lowers an elaborated `Design` into a `Module`.
///
/// `design.asserts` / `design.covers` are intentionally never read here —
/// they're verification-only and never synthesized (design doc, "assert/
/// cover... dropped entirely at lowering").
pub fn lower(design: &Design) -> Module {
    let mut module = Module {
        name: design.module.clone(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: BTreeMap::new(),
        signals: BTreeMap::new(),
        port_declared_widths: BTreeMap::new(),
    };
    let mut ctx = LowerCtx {
        design,
        resolved: HashMap::new(),
        mem_read: HashMap::new(),
        expr_memo: HashMap::new(),
    };

    for input in &design.inputs {
        let bits = module.alloc_bits(input.width.bits, Some(&input.name));
        ctx.resolved.insert(input.name.clone(), bits.clone());
        module.ports.push((input.name.clone(), bits, Dir::In));
    }
    // `clock`/`reset` declarations are NOT in `design.inputs` (the real
    // elaborator files them under `design.clocks`/`design.resets` instead —
    // see `elaborate::module.rs`'s `ModuleItem::Clock`/`ModuleItem::Reset`
    // arms), but every `Dff`/`Mem` cell built below resolves its clock (and
    // any reset mux resolves its reset) by name via `ctx.resolve`, same as
    // any other signal — so they need a net and a `ctx.resolved` entry same
    // as a plain input. Without this, `lower()` panics on every real
    // elaborated `Design` with a clock or reset (Task 17 Finding 1); it only
    // ever worked for hand-built test `Design`s that stuffed `clk`/`rst` into
    // BOTH `design.inputs` and `design.clocks`/`design.resets` themselves.
    // The `contains_key` guard keeps those fixtures behaving identically
    // (no double allocation) while fixing the real (non-hand-built) case.
    for name in design.clocks.iter().chain(&design.resets) {
        if ctx.resolved.contains_key(name) {
            continue;
        }
        let bits = module.alloc_bits(1, Some(name));
        ctx.resolved.insert(name.clone(), bits.clone());
        module.ports.push((name.clone(), bits, Dir::In));
    }
    // Registers must exist in `ctx.resolved` before any comb expression that
    // reads a register's current value is lowered.
    for reg in &design.regs {
        let q_bits = module.alloc_bits(reg.width.bits, Some(&reg.name));
        ctx.resolved.insert(reg.name.clone(), q_bits);
    }
    // Extern-instance outputs (`design.unknown_signals`) are driverless by
    // design — no `comb` entry, so `ctx.resolve`'s panic path would fire on
    // them. Pre-populate their `Bits` here too, same as inputs/reg Qs above,
    // so both the wire-resolution loop below and the `BlackBox`-cell loop
    // (after it) — and any ordinary wire that happens to read one — see an
    // already-allocated net instead of a missing driver.
    for name in &design.unknown_signals {
        let width = design
            .wires
            .iter()
            .find(|w| w.name == *name)
            .unwrap_or_else(|| panic!("unknown_signals entry `{name}` has no matching wire"))
            .width
            .bits;
        let bits = module.alloc_bits(width, Some(name));
        ctx.resolved.insert(name.clone(), bits);
    }
    for output in &design.outputs {
        let bits = ctx.resolve(&mut module, &output.name);
        module
            .port_declared_widths
            .insert(output.name.clone(), output.width.bits);
        module.ports.push((output.name.clone(), bits, Dir::Out));
    }
    // Force every wire to be lowered even if no output reads it (keeps
    // dead-wire diagnostics/validation meaningful; the optimizer's future
    // dead-signal-elimination pass is the place that actually removes it).
    // Also assign the wire's name to all of its unnamed nets (a pure
    // arithmetic result's nets start with no name; assigning the wire's
    // name here makes the printer output readable: `out=sum[0:9]` instead
    // of `out={16,17,18,19,20,21,22,23,24}`).
    for wire in &design.wires {
        let bits = ctx.resolve(&mut module, &wire.name);
        for net_id in &bits.0 {
            if module.nets[net_id.0 as usize].name.is_none() {
                module.nets[net_id.0 as usize].name = Some(wire.name.clone());
            }
        }
    }

    // Extern-module instances (Task 11): one `BlackBox` cell per
    // `design.extern_instances` entry. Every port was already resolved
    // above — an input via its synthesized comb driver (just-run
    // wire-resolution loop), an output via the `unknown_signals`
    // pre-population — so this is a pure by-name pin lookup.
    //
    // `Cell::pins` keys are `&'static str` (every other cell kind's pin
    // names are literals baked into this file); a `BlackBox`'s pin names
    // are the extern module's own port names instead, known only at
    // lowering time. `Box::leak` is the standard way to mint a `'static`
    // str from a runtime `String` — ponytail: leaked bytes are bounded by
    // one design's extern-instance port count, not per-run growth, and a
    // compiler invocation is short-lived, so this never accumulates.
    for ext in &design.extern_instances {
        let pins: BTreeMap<&'static str, Bits> = ext
            .ports
            .iter()
            .map(|(port_name, sig)| {
                let bits = ctx.resolved.get(&sig.name).unwrap_or_else(|| {
                    panic!(
                        "extern instance port `{port_name}` (net `{}`) was not pre-resolved",
                        sig.name
                    )
                });
                let leaked: &'static str = Box::leak(port_name.clone().into_boxed_str());
                (leaked, bits.clone())
            })
            .collect();
        module.cells.push(Cell {
            kind: CellKind::BlackBox {
                module_name: ext.module_name.clone(),
            },
            pins,
            span: ext.span,
        });
        module.extern_decls.insert(
            ext.module_name.clone(),
            ext.ports
                .iter()
                .map(|(n, s)| (n.clone(), s.width.bits))
                .collect(),
        );
    }

    // Build each reg's D input from its driving `Process` and emit the Dff
    // cell — after wires/outputs are resolved, so any wire reading a
    // register's Q sees the net already allocated above.
    for reg in &design.regs {
        let q_bits = ctx.resolved[&reg.name].clone();
        if reg.clock.is_empty() {
            continue; // unassigned reg: holds its reset value forever, no Dff
        }
        let proc = design
            .procs
            .iter()
            .find(|p| p.clock == reg.clock && p.edge == reg.edge)
            .expect("checker guarantees exactly one process per (clock, edge) pair");
        let mut env: HashMap<String, Bits> = HashMap::new();
        env.insert(reg.name.clone(), q_bits.clone()); // unassigned path: keep current value
        ctx.lower_seq_stmts(&mut module, &proc.body, &mut env);
        let mut d_bits = env
            .remove(&reg.name)
            .expect("lower_seq_stmts always re-inserts every target it started with");

        if let Some(reset_name) = design.resets.first() {
            let reset_sel = ctx.resolve(&mut module, reset_name);
            let reset_const =
                ctx.lower_const(&mut module, &reg.reset, crate::span::Span::default());
            let out = module.alloc_bits(reg.width.bits, None);
            module.cells.push(Cell {
                kind: CellKind::Mux,
                pins: [
                    ("sel", reset_sel),
                    ("a", reset_const),
                    ("b", d_bits),
                    ("out", out.clone()),
                ]
                .into_iter()
                .collect(),
                span: crate::span::Span::default(),
            });
            d_bits = out;
        }

        let clock_bits = ctx.resolve(&mut module, &reg.clock);
        assert_eq!(clock_bits.width(), 1, "a clock signal is always 1 bit");
        module.cells.push(Cell {
            kind: CellKind::Dff {
                clock: clock_bits.0[0],
                edge: reg.edge,
            },
            pins: [("d", d_bits), ("q", q_bits)].into_iter().collect(),
            span: crate::span::Span::default(),
        });
    }

    // Memories take TWO passes. Pass A walks every writing process, which is
    // itself a place `m[addr]` reads are discovered (`ram[wa] <- ram[ra]`, or
    // a read of one memory inside another's writer) — the register pass never
    // reaches them, because its `env.contains_key` guard skips every
    // `MemWrite` statement. Only once all of that has run is `ctx.mem_read`
    // complete, so pass B is the one that emits the cells.
    let mut writes: HashMap<String, (Bits, Bits, Bits, Bits)> = HashMap::new();
    for mem in &design.mems {
        if mem.clock.is_empty() {
            continue; // a ROM: no writing `on` block, so no write port
        }
        let proc = design
            .procs
            .iter()
            .find(|p| p.clock == mem.clock && p.edge == mem.edge)
            .expect("checker guarantees exactly one process per (clock, edge) pair");
        let (wen_k, waddr_k, wdata_k) = mem_write_keys(&mem.name);
        // Seeded with "no write on this edge"; every `if` the write sits
        // under folds against that seed, so an unguarded write comes out as a
        // constant-1 `wen` and a guarded one as `Mux(cond, 1, 0)`.
        let mut env: HashMap<String, Bits> = HashMap::new();
        let seed_wen = ctx.lower_const(&mut module, &const_val(0, 1), crate::span::Span::default());
        let seed_addr = ctx.lower_const(
            &mut module,
            &const_val(0, crate::checker::consteval::clog2_bits(mem.depth)),
            crate::span::Span::default(),
        );
        let seed_data = ctx.lower_const(
            &mut module,
            &const_val(0, mem.width.bits),
            crate::span::Span::default(),
        );
        env.insert(wen_k.clone(), seed_wen);
        env.insert(waddr_k.clone(), seed_addr);
        env.insert(wdata_k.clone(), seed_data);
        ctx.lower_seq_stmts(&mut module, &proc.body, &mut env);
        let expect = "lower_seq_stmts always re-inserts every target it started with";
        let wen = env.remove(&wen_k).expect(expect);
        let waddr = env.remove(&waddr_k).expect(expect);
        let wdata = env.remove(&wdata_k).expect(expect);
        let clock = ctx.resolve(&mut module, &mem.clock);
        assert_eq!(clock.width(), 1, "a clock signal is always 1 bit");
        writes.insert(mem.name.clone(), (wen, waddr, wdata, clock));
    }

    // Pass B: one `Mem` cell per `design.mems` entry. Read and write
    // addresses are INDEPENDENT pins (`raddr`/`waddr`), matching the
    // simulator kernel, which reads combinationally from the pre-tick array
    // while a write lands in `next_mems` — a same-cycle read of the written
    // cell still sees the old value. Sharing one address bus would make the
    // read follow the write address whenever `wen` is high and diverge from
    // the kernel on the canonical register-file shape.
    for mem in &design.mems {
        let addr_width = crate::checker::consteval::clog2_bits(mem.depth);
        let read_ports = match ctx.mem_read.remove(&mem.name) {
            Some(ports) if !ports.is_empty() => ports,
            // Never read anywhere: the cell still exists (a write-only memory
            // is legal), it just has one constant read address and a read
            // port that goes nowhere.
            _ => {
                let raddr = ctx.lower_const(
                    &mut module,
                    &const_val(0, addr_width),
                    crate::span::Span::default(),
                );
                let rdata = module.alloc_bits(mem.width.bits, Some(&mem.name));
                vec![(raddr, rdata)]
            }
        };
        let mut pins: BTreeMap<&'static str, Bits> = BTreeMap::new();

        match writes.remove(&mem.name) {
            Some((wen, waddr, wdata, clock)) => {
                pins.insert("waddr", waddr);
                pins.insert("wdata", wdata);
                pins.insert("wen", wen);
                pins.insert("clock", clock);
            }
            // A ROM: write port tied off, and no clock SIGNAL to point a
            // `clock` pin at. `CellKind::Mem` (unlike `Dff`) carries its
            // clock in `pins` rather than as a struct field, so "absent" is
            // directly expressible — leave the pin out rather than invent a
            // NetId for a clock that isn't there.
            None => {
                let waddr = ctx.lower_const(
                    &mut module,
                    &const_val(0, addr_width),
                    crate::span::Span::default(),
                );
                let wdata = ctx.lower_const(
                    &mut module,
                    &const_val(0, mem.width.bits),
                    crate::span::Span::default(),
                );
                let wen =
                    ctx.lower_const(&mut module, &const_val(0, 1), crate::span::Span::default());
                pins.insert("waddr", waddr);
                pins.insert("wdata", wdata);
                pins.insert("wen", wen);
            }
        }

        // `Module::signals` addresses a memory by NAME alone, with no way to
        // pick a port — sound only when there's exactly one, so that's the
        // only case this registers (see `Module::signals`'s own doc). A
        // multi-port memory's individual reads simply aren't addressable by
        // name; nothing in this codebase needs that today.
        if let [(_, rdata)] = read_ports.as_slice() {
            module.signals.insert(mem.name.clone(), rdata.clone());
        }

        module.cells.push(Cell {
            kind: CellKind::Mem {
                depth: mem.depth,
                init: mem.init.clone(),
                read_ports,
            },
            pins,
            span: crate::span::Span::default(),
        });
    }

    // `ctx.resolved` IS the source-name -> Bits table, built up by every pass
    // above; merging it in (rather than assigning outright) preserves the
    // per-memory entries the loop above just inserted.
    module.signals.extend(ctx.resolved);

    module
}
