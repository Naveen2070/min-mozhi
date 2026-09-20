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

/// Widens a SIGNED `bits` to `width` by replicating its MSB — the same
/// sign-extension `Builtin::Extend` does, as pure net re-pointing with no
/// cell. Narrowing is not this function's job: `width <= bits.width()`
/// returns `bits` untouched.
fn sign_extended(bits: Bits, width: u32) -> Bits {
    debug_assert!(
        bits.signed || width <= bits.width(),
        "sign-extending an UNSIGNED value needs a zero constant, not MSB replication"
    );
    if width <= bits.width() {
        return bits;
    }
    let msb = *bits
        .nets
        .last()
        .expect("a zero-width value never reaches lowering");
    let pad = vec![msb; (width - bits.width()) as usize];
    Bits {
        nets: [bits.nets, pad].concat(),
        signed: bits.signed,
    }
}

/// Whether `op` lets an untyped compile-time-constant operand ADAPT to its
/// sibling's width and signedness — the union of the checker's
/// `matched_ty` family (`requires_matched_ab` above, plus the `+%`/`-%`/`*%`
/// wrapping ops) and its `adapt_lossless` family (`+`/`-`/`*`). Both give a
/// `Ty::CtInt` operand the SIZED side's `Ty` outright, so lowering must
/// re-size and re-sign the constant the same way.
///
/// `requires_matched_ab` is a narrower question — "must `a`/`b` end up the
/// same width, on pain of a `validate` error" — and stays separate: the
/// lossless ops legitimately take differently-sized REAL operands, they just
/// don't take an un-adapted CONSTANT one. Gating the resize on the narrower
/// predicate is exactly what left `a + (-3)` lowering its `-3` at a 2-bit
/// natural width (GAP-1 Task 6 round 4, F6).
///
/// `Shl`/`Shr` are excluded on purpose: the right operand is a shift AMOUNT,
/// not a sibling value — `width_rules::shift_result` derives `<<`'s growth
/// from its width, so widening a constant amount to the shifted value's
/// width would inflate the result for no reason. `Coalesce` never reaches
/// lowering at all (see `lower_binop`).
fn adapts_const_operand(op: BinOp) -> bool {
    requires_matched_ab(op)
        || matches!(
            op,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap
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
    /// `fn`-body `let` bindings whose value is a COMPILE-TIME CONSTANT,
    /// folded to its value. `ast::LocalLet` carries no type annotation, so
    /// the checker types `let m = 1` as an untyped `Ty::CtInt` that adapts
    /// to whatever uses it — but in the IR `m` is an already-lowered `Bits`
    /// in `locals`, an `Expr` no longer, so `is_const_foldable`/
    /// `lower_expr_sized` cannot see through the binding on their own and
    /// `v & m` reached `validate` as a 1-bit `Const` on an 8-bit pin (GAP-1
    /// Task 6 round 4, systematic pass). Merging these into
    /// `visible_consts` lets both see the value again.
    ///
    /// Scoped by save/restore around `lower_fn_stmts`'s own `Let` recursion,
    /// so a nested or sibling call's identically-named binding can't leak.
    /// `i128` (via `value::const_eval`, not `const_eval_wide`) to match
    /// `design.consts`, which these are merged into — a `let` bound past
    /// `i128` is not a shape v1 needs.
    local_consts: BTreeMap<String, i128>,
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
        // The declared WIDTH and the declared SIGNEDNESS are one fact, applied
        // together: `lower_expr_sized`'s const arms always build an UNSIGNED
        // `ConstVal` (a `Ty::CtInt` has no sign of its own), so sizing without
        // stamping left `wire w: signed[8] = -1` unsigned and a downstream
        // `extend` zero-padded it (GAP-1 Task 6 round 4, F1). Stamping is
        // unconditional — the declaration is the authority for a signal's kind
        // here exactly as it already is for an input/reg/mem element below.
        let declared = self
            .design
            .outputs
            .iter()
            .chain(&self.design.wires)
            .find(|s| s.name == name)
            .map(|s| s.width);
        // Top-level signal resolution is never inside a `fn` body, so there
        // are no call-local bindings in scope here.
        let bits = match declared {
            Some(w) => {
                let mut b = self.lower_expr_sized(module, expr, None, None, w.bits);
                b.signed = w.signed;
                b
            }
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
    /// has no notion of `locals` (same as `const_eval`), so it is handed
    /// `visible_consts(locals)` rather than `design.consts` directly: inside
    /// an inlined `fn` body it could otherwise silently resolve a LOCAL
    /// param/`let` name that collides with an unrelated design-level
    /// const/param, and hiding exactly those names makes it fail on them
    /// instead. This USED to be a blanket `locals.is_none()` gate, which
    /// also refused every honest compound constant inside a `fn` body —
    /// `return -1` in a `-> signed[8]` fn then lowered at its operand's own
    /// 1-bit natural width and returned `1` (GAP-1 Task 6 round 3, F6).
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
            _ => crate::value::const_eval_wide(e, &self.visible_consts(locals))
                .ok()
                .map(|cv| {
                    let resized = crate::value::from_const_at_width(&cv, target_width, cv.signed);
                    crate::checker::consteval::ConstVal {
                        bits: resized.bits,
                        width: target_width,
                        signed: cv.signed,
                    }
                }),
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
            // `const_eval_wide`, not `const_eval`, and the same
            // `visible_consts` shadowing, so this predicate and the resize it
            // gates never disagree on what folds.
            _ => crate::value::const_eval_wide(e, &self.visible_consts(locals)).is_ok(),
        }
    }

    /// Lowers `e` as the SIBLING of an already-lowered `other` — a mux
    /// branch opposite its other branch, a `match` arm opposite the arm
    /// chain it folds into. The checker unifies both to one `Ty`, so an
    /// untyped compile-time constant here adopts `other`'s width AND
    /// signedness; anything else is already reconciled and lowers unchanged.
    ///
    /// Without this, `if c { a } else { -1 }` lowered its `-1` at its own
    /// 1-bit natural width and the mux returned the raw literal `1` —
    /// SILENTLY, because `validate`'s `Mux` rule only checked the `sel` pin
    /// (GAP-1 Task 6 round 4, F2; the `a`/`b`-vs-`out` rule that makes this
    /// class loud was added in the same round).
    /// `other: None` means "no sibling with a real type exists" (every
    /// branch/arm is itself constant), which degrades to plain `lower_expr`.
    fn lower_sibling(
        &mut self,
        module: &mut Module,
        e: &Expr,
        other: Option<&Bits>,
        locals: Option<&HashMap<String, Bits>>,
        arrays: Option<&HashMap<String, u32>>,
    ) -> Bits {
        match other {
            Some(o) if self.is_const_foldable(e, locals) => {
                let (width, signed) = (o.width(), o.signed);
                let mut bits = self.lower_expr_sized(module, e, locals, arrays, width);
                bits.signed = signed;
                bits
            }
            _ => self.lower_expr(module, e, locals, arrays),
        }
    }

    /// `design.consts` with every name SHADOWED by an in-scope `fn`-body
    /// local (param or `let`) removed. `const_eval_wide` has no notion of
    /// `locals`, so without this a compound constant expression inside an
    /// inlined `fn` body could silently resolve a LOCAL name that collides
    /// with an unrelated design-level `const`/parameter. Hiding the
    /// collisions makes `const_eval_wide` fail on exactly those expressions
    /// (unknown identifier) instead, which is the "don't fold it" answer.
    ///
    /// This replaces the old blanket `locals.is_none()` gate, which refused
    /// to fold ANY compound constant inside a `fn` body — so a signed fn's
    /// `return -1` (an `Unary{Neg}`, not a bare `Int`, hence not covered by
    /// the arms above) was lowered at its operand's own 1-bit natural width
    /// and returned `1` instead of `-1` (GAP-1 Task 6 round 3, F6).
    ///
    /// Borrows in the common case — module-level lowering has no locals at
    /// all, and a `fn` body whose locals shadow nothing clones nothing.
    fn visible_consts(
        &self,
        locals: Option<&HashMap<String, Bits>>,
    ) -> std::borrow::Cow<'_, BTreeMap<String, i128>> {
        let shadows = locals.is_some_and(|l| l.keys().any(|k| self.design.consts.contains_key(k)));
        if !shadows && self.local_consts.is_empty() {
            return std::borrow::Cow::Borrowed(&self.design.consts);
        }
        let mut visible = self.design.consts.clone();
        if let Some(l) = locals {
            visible.retain(|k, _| !l.contains_key(k));
        }
        // AFTER the shadow removal: a `let` that both shadows a design-level
        // `const` AND folds to a constant of its own must resolve to the
        // LOCAL value, not the hidden design one.
        visible.extend(self.local_consts.iter().map(|(k, v)| (k.clone(), *v)));
        std::borrow::Cow::Owned(visible)
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
                let (c_lhs, c_rhs) = (
                    self.is_const_foldable(lhs, locals),
                    self.is_const_foldable(rhs, locals),
                );
                // Exactly ONE side being a compile-time constant (a bare
                // literal, a const/param identifier, or a larger constant
                // expression — see `is_const_foldable`) is the adapting case:
                // the checker types it `Ty::CtInt`, which has neither a width
                // nor a signedness of its own and simply BECOMES the sized
                // side's `Ty` (`matched_ty` for the comparison/bitwise/wrap
                // family, `adapt_lossless` for `+`/`-`/`*`). So the real side
                // is lowered first and the constant side is lowered ONCE,
                // already at that width — `lower_expr`'s own arms would
                // otherwise size it at its NATURAL/arithmetic-growth width,
                // and re-lowering it afterwards would leave the first,
                // wrongly-sized `Const` cell orphaned in the netlist.
                //
                // The sign stamp matters because `lower_expr_sized`'s const
                // arms always build an UNSIGNED `ConstVal`; `lower_binop`'s
                // `out_signed`/`cmp_signed` and `ir::exec`'s sign-aware
                // arithmetic all read these two flags. Before round 4 this
                // whole block was gated on `requires_matched_ab`, so a
                // constant operand of a LOSSLESS op was never sized or signed
                // at all — `a + (-3)` lowered its `-3` at a 2-bit natural
                // width (GAP-1 Task 6 round 4, F6).
                //
                // `is_const_foldable`, not a bare `ExprKind::Int` pattern:
                // the narrower shape `Mul` used to test missed `a * -1` (an
                // `Unary{Neg}`) and `a * K` (a named `const`), both of which
                // then hit a `PortWidthMismatch` at `validate` on a
                // checker-valid program (F5).
                let adapts = adapts_const_operand(*op);
                let (mut a, b) = if adapts && c_lhs && !c_rhs {
                    let b = self.lower_expr(module, rhs, locals, arrays);
                    let mut a = self.lower_expr_sized(module, lhs, locals, arrays, b.width());
                    a.signed = b.signed;
                    (a, b)
                } else if adapts && c_rhs && !c_lhs {
                    let a = self.lower_expr(module, lhs, locals, arrays);
                    let mut b = self.lower_expr_sized(module, rhs, locals, arrays, a.width());
                    b.signed = a.signed;
                    (a, b)
                } else {
                    (
                        self.lower_expr(module, lhs, locals, arrays),
                        self.lower_expr(module, rhs, locals, arrays),
                    )
                };
                // BOTH sides constant, in a position with no width context of
                // its own (a `Concat` part, say — a sized position folds the
                // whole expression through `lower_expr_sized` long before
                // here). Neither side has a real type to adopt, so this only
                // reconciles the two so the cell stays well-formed; it is the
                // one path that can still orphan a natural-width `Const`.
                // If NEITHER side is const-foldable the mismatch is a genuine
                // checker/lowering disagreement and reaches `validate` as a
                // loud failure, deliberately untouched.
                if adapts && c_lhs && c_rhs && a.width() != b.width() {
                    a = self.lower_expr_sized(module, lhs, locals, arrays, b.width());
                    a.signed = b.signed;
                }
                let shl_const_amount = (*op == BinOp::Shl)
                    .then(|| crate::value::const_eval(rhs, &self.design.consts).ok())
                    .flatten()
                    .and_then(|v| u128::try_from(v).ok());
                // Signedness of an ORDERING comparison now reads straight off
                // `a`/`b`'s own `Bits::signed` (computed once at construction,
                // per `ir::Bits`'s own doc) instead of re-deriving it from the
                // source `Expr` shapes — the two heuristic functions that used
                // to do that (`expr_is_definitely_signed`/
                // `unshadowed_signal_signed`) are gone (GAP-1 Task 6).
                //
                // `||`, not `&&` — the same rule `lower_binop`'s own
                // `out_signed` and `Min`/`Max`'s `cmp_signed` already use, for
                // the same reason: `matched_ty` (`checker/widths/ops/mod.rs`)
                // forces two REAL operands to the identical `Kind` (E0403), so
                // `||` and `&&` agree there; the only case they differ on is a
                // compile-time-constant operand, which the checker gives NO
                // signedness of its own (`Ty::CtInt` simply becomes the sized
                // side's `Ty`) and which `lower_const` therefore always builds
                // unsigned. `&&` let that unsigned literal veto its sibling's
                // sign — and the stamp below only runs on a WIDTH mismatch, so
                // `a < -128` over a `signed[8]` (literal already 8 bits wide,
                // no resize) compared unsigned (GAP-1 Task 6 round 3, F1).
                let cmp_signed = matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
                    && (a.signed || b.signed);
                self.lower_binop(module, *op, a, b, shl_const_amount, cmp_signed, e.span)
            }
            ExprKind::Unary { op, expr } => {
                let a = self.lower_expr(module, expr, locals, arrays);
                let (kind, out_width) = match op {
                    // `-x` grows by one bit when `x` is signed — the same
                    // "room for the MIN-value carry bit" rule `Abs` needs —
                    // matching `checker::widths::ops`'s `Signed(n) ->
                    // Signed(n+1)` growth exactly (previously a known width
                    // divergence, see docs/audit/gaps.md GAP-1).
                    UnOp::Neg => {
                        let out_w = if a.signed { a.width() + 1 } else { a.width() };
                        (CellKind::Neg, out_w)
                    }
                    UnOp::BitNot => (CellKind::Not, a.width()),
                    UnOp::LogicNot => (CellKind::LogicNot, 1),
                    UnOp::RedAnd => (CellKind::RedAnd, 1),
                    UnOp::RedOr => (CellKind::RedOr, 1),
                    UnOp::RedXor => (CellKind::RedXor, 1),
                };
                // Negating a signed value stays signed, and so does `~` (the
                // checker's `UnOp::BitNot` arm returns the operand's own `Ty`
                // unchanged). The reductions and `!` are always 1-bit
                // unsigned, so they never inherit.
                let signed = matches!(op, UnOp::Neg | UnOp::BitNot) && a.signed;
                let out = self.push_unary_cell(module, kind, a, out_width, e.span);
                Bits {
                    nets: out.nets,
                    signed,
                }
            }
            // `{a, b}` is Verilog-style: the first (source-order) part is
            // the MOST-significant. `ir::Bits` is index-0-is-LSB, so the
            // last part in source order goes at the low indices. No cell:
            // this is pure bit-vector reassembly, not a new value.
            ExprKind::Concat(parts) => {
                let mut ids = Vec::new();
                for part in parts.iter().rev() {
                    let bits = self.lower_expr(module, part, locals, arrays);
                    ids.extend(bits.nets);
                }
                Bits::unsigned(ids)
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
                // A slice is UNSIGNED whatever its base was —
                // `width_rules::slice_result` returns `signed: false`
                // unconditionally (the single shared rule that keeps BUG-21
                // from coming back), and `emit_verilog/kinds.rs` agrees. It
                // does NOT inherit `base_bits.signed`.
                Bits::unsigned(base_bits.nets[lo_val..=hi_val].to_vec())
            }
            // Both branches of an `if`-expression are unified to one `Ty` by
            // the checker, so `push_mux_cell`'s `a.signed || b.signed` is
            // exactly the branches' shared signedness (`emit_verilog/
            // kinds.rs` derives the same `Kind` the same way) — see that
            // helper's own doc for why a constant branch needs `||`.
            ExprKind::IfExpr { cond, then, els } => {
                let sel = self.lower_expr(module, cond, locals, arrays);
                // Lower the NON-constant branch first so the constant one has
                // a real sibling to adopt — `lower_sibling`'s whole job (F2).
                // Two REAL branches are already reconciled by the checker, and
                // two CONSTANT branches have no sibling to adopt from at all,
                // so both of those fall through unchanged.
                let (a, b) = match (
                    self.is_const_foldable(then, locals),
                    self.is_const_foldable(els, locals),
                ) {
                    (true, false) => {
                        let b = self.lower_expr(module, els, locals, arrays);
                        let a = self.lower_sibling(module, then, Some(&b), locals, arrays);
                        (a, b)
                    }
                    (false, true) => {
                        let a = self.lower_expr(module, then, locals, arrays);
                        let b = self.lower_sibling(module, els, Some(&a), locals, arrays);
                        (a, b)
                    }
                    _ => (
                        self.lower_expr(module, then, locals, arrays),
                        self.lower_expr(module, els, locals, arrays),
                    ),
                };
                self.push_mux_cell(module, sel, a, b, e.span)
            }
            // `base[i]` is dual-use: a full-width memory-word read when
            // `base` names a `design.mems` entry, a single-bit select
            // otherwise. Only the name tells them apart — the parser can't,
            // and the width checker branches on exactly this same test.
            ExprKind::Index { base, index } if self.indexed_mem(base).is_some() => {
                let (mem, width, signed) = self.indexed_mem(base).unwrap();
                let addr_width = self
                    .mem_kind(&mem)
                    .expect("indexed_mem already matched this memory")
                    .1;
                // A repeat encounter of THIS read node never gets here —
                // `expr_memo` intercepts it above. So reaching the
                // comparison below means a genuinely distinct read site (or
                // a re-inlined `fn` body, where the address really can
                // differ per call).
                // A CONSTANT address (`m[0]`) has no width of its own — size
                // it to the memory's own `clog2(depth)` address width so
                // every read port and the write port agree, instead of each
                // literal's natural width (round 4 systematic pass).
                let addr = self.lower_expr_sized(module, index, locals, arrays, addr_width);
                let ports = self.mem_read.entry(mem.clone()).or_default();
                // A DIFFERENT read site landing on the same memory. Same
                // LOWERED address: share that port. Different address: grow
                // a new one (GAP-1 residual Task 6 — this used to panic,
                // one read port per memory being the v1 ceiling).
                match ports.iter().find(|(prev, _)| *prev == addr) {
                    Some((_, rdata)) => rdata.clone(),
                    None => {
                        let mut rdata = module.alloc_bits(width, Some(&mem));
                        // The read value's kind is the memory's ELEMENT kind
                        // (`indexed_mem`'s doc cites both the checker and the
                        // emitter); `alloc_bits` is always unsigned, so a
                        // `mem m: signed[N][D]` read back zero-extended
                        // (GAP-1 Task 6 round 3, F4).
                        rdata.signed = signed;
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
                                acc = self.push_mux_cell(module, eq, elem, acc, e.span);
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
                        // A bit-select is a BIT: unsigned whatever the base
                        // was (`checker/widths/expr/lvalue.rs` types it
                        // `Ty::Bit`, `width_rules::slice_result` returns
                        // `signed: false`). Same for the runtime case below —
                        // it rides on a `Shr`, which DOES keep its left
                        // operand's signedness, so the 1-bit result has to
                        // drop that flag explicitly.
                        Ok(i) => Bits::unsigned(vec![base_bits.nets[i as usize]]),
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
                            Bits::unsigned(vec![shifted.nets[0]])
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
                // A parameter's DECLARED type is the argument's context, the
                // same way a fn's declared return type is its result's: an
                // argument that is a compile-time constant carries neither a
                // width nor a sign of its own (`Ty::CtInt`), so binding it
                // with a plain `lower_expr` handed the callee a literal at its
                // own natural width AND unsigned — `ext(-1)` for a
                // `x: signed[8]` param bound a 1-bit `1` (GAP-1 Task 6 round
                // 3, F8, found by probing beyond the reported findings). A
                // non-constant argument is already reconciled by the checker,
                // so `lower_expr_sized` falls straight through for it.
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
                        let Type::Array { elem, .. } = &param.ty else {
                            unreachable!("just matched Type::Array");
                        };
                        for (i, el) in elems.iter().enumerate() {
                            let bound =
                                self.lower_param_arg(module, el, elem, locals, arrays, func.span);
                            call_locals.insert(format!("{}_{i}", param.name.name), bound);
                        }
                        call_arrays.insert(param.name.name.clone(), elems.len() as u32);
                    } else {
                        let bound =
                            self.lower_param_arg(module, arg, &param.ty, locals, arrays, func.span);
                        call_locals.insert(param.name.name.clone(), bound);
                    }
                }
                let (ret_width, ret_signed) =
                    crate::value::type_width(&func.ret, &self.design.consts, func.span)
                        .expect("checker guarantees a fn's declared return type resolves");
                let mut out = self.lower_fn_stmts(
                    module,
                    &func.stmts,
                    &func.tail,
                    &call_locals,
                    Some(&call_arrays),
                    ret_width,
                );
                // A call's `Ty` IS the fn's DECLARED return type — the checker
                // forces the body to match it, so this is an assignment, not a
                // merge. It matters because a return/tail path can be a bare
                // compile-time constant (`if x < 0 { return -1 }`), which has
                // no signedness of its own and no sibling to inherit one from:
                // `lower_expr_sized` sizes it to `ret_width` but `lower_const`
                // always builds it unsigned, so the call came back disagreeing
                // with `-> signed[N]` and a downstream `extend` zero-padded it
                // (GAP-1 Task 6 round 3, F3).
                out.signed = ret_signed;
                out
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
                        // `base.signed` REPLACES the old
                        // `arg_is_definitely_unsigned` heuristic entirely —
                        // computed once at construction (GAP-1 Task 6), it's
                        // correct no matter how `base` was produced, instead
                        // of a best-effort proof over a handful of `Expr`
                        // shapes.
                        let pad_net = if base.signed {
                            // Sign-extend: replicate the MSB, not zero.
                            *base
                                .nets
                                .last()
                                .expect("checker guarantees a non-empty operand")
                        } else {
                            self.lower_const(module, &const_val(0, 1), e.span).nets[0]
                        };
                        let pad_width = target - base.width();
                        let pad = vec![pad_net; pad_width as usize];
                        Bits {
                            nets: [base.nets, pad].concat(),
                            signed: base.signed,
                        }
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
                    Bits {
                        nets: base.nets[..(target.min(base.width())) as usize].to_vec(),
                        signed: base.signed,
                    }
                }
                // `signed(x)`/`unsigned(x)` are free reinterprets — repoint
                // at `x`'s own `Bits`, allocating nothing — but unlike a
                // plain pass-through they MUST flip the `signed` flag: that
                // is the entire point of the cast (GAP-1 Task 6; `ir::Bits`
                // used to carry no sign bit at all, so this was previously
                // inexpressible). `encoding(e)` is not a sign cast, so it
                // stays a pure pass-through.
                Builtin::SignedCast => {
                    let a = self.lower_expr(module, &args[0], locals, arrays);
                    Bits {
                        nets: a.nets,
                        signed: true,
                    }
                }
                Builtin::UnsignedCast => {
                    let a = self.lower_expr(module, &args[0], locals, arrays);
                    Bits {
                        nets: a.nets,
                        signed: false,
                    }
                }
                Builtin::Encoding => self.lower_expr(module, &args[0], locals, arrays),
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
                Builtin::Min | Builtin::Max | Builtin::Abs => {
                    match func {
                        Builtin::Abs => {
                            let a = self.lower_expr(module, &args[0], locals, arrays);
                            // Lossless like unary `-`: `abs(MIN)` needs the
                            // extra bit — the checker types it
                            // `Signed(n) -> Signed(n+1)`. The checker only
                            // accepts a `Signed` argument here, so `a.signed`
                            // is always true in practice; the `else` branch
                            // is defensive, mirroring `UnOp::Neg`'s formula.
                            let neg_width = if a.signed { a.width() + 1 } else { a.width() };
                            let neg = self.push_unary_cell(
                                module,
                                CellKind::Neg,
                                a.clone(),
                                neg_width,
                                e.span,
                            );
                            // Sign bit of `a` selects between `a` and its
                            // negation — a pure bit-select, no cell, the same
                            // convention `ExprKind::Slice`/`Index` already use.
                            let is_neg = Bits::unsigned(vec![
                                *a.nets
                                    .last()
                                    .expect("checker guarantees a non-empty operand"),
                            ]);
                            let a_signed = a.signed;
                            // Both mux arms at `neg_width`: `-x` grew by a
                            // bit, so the untouched `x` arm has to be
                            // sign-extended to match. Semantically a no-op
                            // (that arm is only selected when `x >= 0`, where
                            // the replicated sign bit is 0), but it keeps the
                            // `Mux`'s arms at its `out` width, which
                            // `validate` now enforces for every mux (round 4
                            // guard-rail).
                            let a = sign_extended(a, neg_width);
                            let out = self.push_mux_cell(module, is_neg, neg, a, e.span);
                            Bits {
                                nets: out.nets,
                                // The checker types `abs(x: signed[n])` as
                                // `signed[n+1]`, so the IR flag says `signed`
                                // too — the invariant is "`Bits::signed`
                                // equals the checker's own `Ty` signedness",
                                // not "this value could be negative". (It
                                // can't: `|x|`'s top bit is always 0 in n+1
                                // bits, so a downstream sign-extend and a
                                // zero-extend agree here anyway.)
                                signed: a_signed,
                            }
                        }
                        Builtin::Min | Builtin::Max => {
                            // `min`/`max` take a COMPARISON's operand rule
                            // (`checker/widths/ops/builtins.rs` routes both
                            // through `matched_ty`): equal width and
                            // signedness, with an untyped literal adapting to
                            // the sized side. So this needs exactly the same
                            // const-foldable resize + sign inheritance
                            // `ExprKind::Binary` already does for
                            // `<`/`<=`/`>`/`>=` — without it `min(x, 3)`
                            // feeds a 2-bit `Const` to an 8-bit `Lt` pin
                            // (`WidthMismatch`, an invalid module), and
                            // `max(x, 0)` — the stock clamp idiom — never
                            // lowers at all. The foldable side is lowered ONCE,
                            // already at its sibling's width — lowering it
                            // naturally first and re-lowering it after would
                            // orphan that first natural-width `Const` cell.
                            let c0 = self.is_const_foldable(&args[0], locals);
                            let c1 = self.is_const_foldable(&args[1], locals);
                            let (mut a, mut b) = match (c0, c1) {
                                (true, false) => {
                                    let b = self.lower_expr(module, &args[1], locals, arrays);
                                    let mut a = self.lower_expr_sized(
                                        module,
                                        &args[0],
                                        locals,
                                        arrays,
                                        b.width(),
                                    );
                                    a.signed = b.signed;
                                    (a, b)
                                }
                                (false, true) => {
                                    let a = self.lower_expr(module, &args[0], locals, arrays);
                                    let mut b = self.lower_expr_sized(
                                        module,
                                        &args[1],
                                        locals,
                                        arrays,
                                        a.width(),
                                    );
                                    b.signed = a.signed;
                                    (a, b)
                                }
                                _ => (
                                    self.lower_expr(module, &args[0], locals, arrays),
                                    self.lower_expr(module, &args[1], locals, arrays),
                                ),
                            };
                            // Two literals (`min(3, 5)`) can still differ in
                            // natural width; widen the narrower one. Two real
                            // signals can't — the checker matched them.
                            if c0 && c1 && a.width() != b.width() {
                                if a.width() < b.width() {
                                    a = self.lower_expr_sized(
                                        module,
                                        &args[0],
                                        locals,
                                        arrays,
                                        b.width(),
                                    );
                                } else {
                                    b = self.lower_expr_sized(
                                        module,
                                        &args[1],
                                        locals,
                                        arrays,
                                        a.width(),
                                    );
                                }
                            }
                            // Checker forbids mixed signed/unsigned operands
                            // (`matched_ty`), so `a.signed == b.signed` for
                            // two real signals; `||` covers the remaining
                            // case, a literal whose `ConstVal` is always
                            // built unsigned.
                            let cmp_signed = a.signed || b.signed;
                            let a_lt_b = self.push_binary_cell(
                                module,
                                CellKind::Lt { signed: cmp_signed },
                                a.clone(),
                                b.clone(),
                                1,
                                e.span,
                            );
                            let (sel, first, second) = if matches!(func, Builtin::Min) {
                                (a_lt_b, a, b)
                            } else {
                                (a_lt_b, b, a) // Max picks b when a < b
                            };
                            self.push_mux_cell(module, sel, first, second, e.span)
                        }
                        _ => unreachable!("outer match already narrowed `func` to Min|Max|Abs"),
                    }
                }
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
                        ids.extend(bits.nets);
                    }
                }
                Bits::unsigned(ids)
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

    /// A register's declared `Width`, if `name` is one.
    fn reg_width(&self, name: &str) -> Option<crate::elaborate::Width> {
        self.design
            .regs
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.width)
    }

    /// A memory's `(element Width, address width)`, if `name` is one.
    fn mem_kind(&self, name: &str) -> Option<(crate::elaborate::Width, u32)> {
        self.design
            .mems
            .iter()
            .find(|m| m.name == name)
            .map(|m| (m.width, crate::checker::consteval::clog2_bits(m.depth)))
    }

    /// Lowers an `on`-block assignment's RHS at its TARGET's declared type.
    /// A compile-time constant has no width or signedness of its own
    /// (`Ty::CtInt`), and a sequential target — unlike a binary operator's
    /// operand — has no sibling to adopt from: the DECLARATION is the only
    /// context, so it is both sized and signed from it, exactly as
    /// `lower_param_arg` already does for a `fn` parameter and `resolve` for
    /// a wire/output. `None` (a target with no declared type on record)
    /// falls back to plain `lower_expr`, today's behaviour.
    fn lower_target_rhs(
        &mut self,
        module: &mut Module,
        e: &Expr,
        declared: Option<crate::elaborate::Width>,
    ) -> Bits {
        match declared {
            // `on`-block lowering is always top-level, never inside a `fn`
            // body — no call-local bindings in scope.
            Some(w) => {
                let mut bits = self.lower_expr_sized(module, e, None, None, w.bits);
                bits.signed = w.signed;
                bits
            }
            None => self.lower_expr(module, e, None, None),
        }
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
    ///
    /// `rhs` arrives as an `Expr`, not an already-lowered `Bits`: the target
    /// SLOT's width (the slice's `hi - lo + 1`, or 1 bit) is the only width
    /// context an untyped constant RHS has, and it is only knowable after
    /// the bounds below are folded. Lowered without it, `q[7:4] <- 3` came
    /// out as a 2-bit `Const` and the splice below panicked outright
    /// (`clone_from_slice` length mismatch) — GAP-1 Task 6 round 4,
    /// systematic pass.
    fn lower_bitselect_write(
        &mut self,
        module: &mut Module,
        base_bits: &Bits,
        first: &Expr,
        second: Option<&Expr>,
        rhs: &Expr,
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
                let rhs = self.lower_expr_sized(module, rhs, None, None, (hi - lo + 1) as u32);
                let mut nets = base_bits.nets.clone();
                nets[lo..=hi].clone_from_slice(&rhs.nets);
                Bits {
                    nets,
                    signed: base_bits.signed,
                }
            }
            // A single-bit write's RHS is exactly 1 bit wide (the checker
            // types `q[i]` as `Ty::Bit`).
            None => match crate::value::const_eval(first, &self.design.consts) {
                Ok(i) => {
                    let rhs = self.lower_expr_sized(module, rhs, None, None, 1);
                    let mut nets = base_bits.nets.clone();
                    nets[i as usize] = rhs.nets[0];
                    Bits {
                        nets,
                        signed: base_bits.signed,
                    }
                }
                Err(_) => {
                    let rhs = self.lower_expr_sized(module, rhs, None, None, 1);
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
                        let base_bit = Bits::unsigned(vec![base_bits.nets[i as usize]]);
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
                        nets.push(out.nets[0]);
                    }
                    Bits {
                        nets,
                        signed: base_bits.signed,
                    }
                }
            },
        }
    }

    /// `(name, word width, word signedness)` of the memory `base` indexes, if
    /// `base` is a bare identifier naming a `design.mems` entry. The
    /// signedness is the memory's ELEMENT type: `checker/widths/expr/
    /// lvalue.rs` types `m[addr]` as `Ty::Signed(width)` for a
    /// `mem m: signed[N][D]`, and `emit_verilog/kinds.rs` reads the same
    /// element `Kind` back out of its `mem_elem_decl_key` entry.
    fn indexed_mem(&self, base: &Expr) -> Option<(String, u32, bool)> {
        let ExprKind::Ident(name) = &base.kind else {
            return None;
        };
        self.design
            .mems
            .iter()
            .find(|m| m.name == *name)
            .map(|m| (m.name.clone(), m.width.bits, m.width.signed))
    }

    /// Walks one `fn` body's statement list, evaluating against `locals`
    /// (params + `let`s bound so far). Mirrors
    /// `emit_verilog::module::funcs::emit_fn_stmts`'s continuation-passing
    /// shape: an unconditional `Return` short-circuits before any later
    /// statement or `tail` is ever reached, exactly like that renderer's
    /// `rest` threading — checker-guaranteed (E0812) so no reachability
    /// analysis is needed here either.
    /// Lowers one `fn`-call argument against its parameter's DECLARED type
    /// (`ty` is the scalar param's own type, or an array param's ELEMENT
    /// type — arrays flatten to one binding per element). Sizes and signs it
    /// the way the checker does: a `Ty::CtInt` argument has neither width nor
    /// signedness of its own and simply becomes the parameter's type, while a
    /// real signal is already reconciled and passes through untouched.
    fn lower_param_arg(
        &mut self,
        module: &mut Module,
        arg: &Expr,
        ty: &Type,
        locals: Option<&HashMap<String, Bits>>,
        arrays: Option<&HashMap<String, u32>>,
        span: crate::span::Span,
    ) -> Bits {
        let Ok((width, signed)) = crate::value::type_width(ty, &self.design.consts, span) else {
            // Not a scalar bit-vector type (an enum-typed param, say) — no
            // width/sign context to apply, so lower it as it stands.
            return self.lower_expr(module, arg, locals, arrays);
        };
        let mut bits = self.lower_expr_sized(module, arg, locals, arrays, width);
        bits.signed = signed;
        bits
    }

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
                // A `let` bound to a compile-time constant stays visible AS a
                // constant for the rest of the body, so a use site can size
                // and sign it to its own context (`local_consts`). The
                // lowered `Bits` is still bound too: a use with no width
                // context of its own reads it straight out of `locals`.
                let folded = crate::value::const_eval(&l.value, &self.visible_consts(Some(locals)))
                    .ok()
                    .filter(|_| self.is_const_foldable(&l.value, Some(locals)));
                let name = l.name.name.clone();
                let shadowed = match folded {
                    Some(k) => self.local_consts.insert(name.clone(), k),
                    None => self.local_consts.remove(&name),
                };
                let mut locals2 = locals.clone();
                locals2.insert(name.clone(), v);
                let out = self.lower_fn_stmts(module, rest, tail, &locals2, arrays, target_width);
                match shadowed {
                    Some(prev) => self.local_consts.insert(name, prev),
                    None => self.local_consts.remove(&name),
                };
                out
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
                // Return/tail arms above, so `push_mux_cell`'s own
                // max(then, else) formula lands on target_width too.
                self.push_mux_cell(module, sel, then_val, else_val, cond.span)
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
        // The RESULT's own signedness, mirroring the checker's rules one for
        // one (`checker/widths/ops/mod.rs` -> `width_rules`):
        // - `lossless_result` (`+`/`-`/`*`) and `matched_result` (the `+%`
        //   family, bitwise) both return the shared operand `Kind`, which the
        //   checker has already forced to agree on signedness (E0403). `||`
        //   rather than `&&` because ONE operand may be a re-sized literal,
        //   which `lower_expr_sized` always builds unsigned (the checker's
        //   `Ty::CtInt` adapts to its sibling instead).
        // - `shift_result` keeps the LEFT operand's kind; a shift amount is
        //   never signed.
        // - comparisons and `&&`/`||` are always 1-bit unsigned.
        // Without this, EVERY derived value came back `signed: false` and the
        // Task 6 features that read the flag (`extend`, `min`/`max`, `abs`,
        // unary `-`, comparison signedness) silently reverted to their
        // unsigned reading whenever an operand was an expression rather than
        // a bare signed port (GAP-1 Task 6 fix round, findings 1-4).
        let out_signed = match op {
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::AddWrap
            | BinOp::SubWrap
            | BinOp::MulWrap
            | BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor => a.signed || b.signed,
            BinOp::Shl | BinOp::Shr => a.signed,
            _ => false,
        };
        let mut out = module.alloc_bits(out_width, None);
        out.signed = out_signed;
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
        // The checker unifies every arm to ONE `Ty`, so the first arm with a
        // real type of its own is the sibling every untyped-constant arm
        // adopts its width and signedness from (`lower_sibling`). Without it
        // each constant arm kept its own natural width and the fold returned
        // the raw literal — `match s { 0 => a  _ => -1 }` gave `1`, not `0xFF`
        // (GAP-1 Task 6 round 4, F2). If EVERY arm is constant there is no
        // sibling to adopt, and `lower_sibling` degrades to plain `lower_expr`
        // for all of them, exactly as before.
        let reference = arms
            .iter()
            .position(|a| !self.is_const_foldable(&a.value, locals))
            .map(|i| self.lower_expr(module, &arms[i].value, locals, arrays));
        let mut acc = self.lower_sibling(
            module,
            &arms[n - 1].value,
            reference.as_ref(),
            locals,
            arrays,
        );
        for arm in arms[..n - 1].iter().rev() {
            let sel = self.lower_pattern_conds(module, &scrutinee_bits, &arm.patterns, span);
            let arm_value =
                self.lower_sibling(module, &arm.value, reference.as_ref(), locals, arrays);
            // Every arm is unified to one `Ty` by the checker, so each fold
            // step's `a.signed || b.signed` (inside `push_mux_cell`) carries
            // the arms' shared signedness all the way down the chain — `||`
            // so a constant arm can't veto it, see `push_mux_cell`'s doc.
            acc = self.push_mux_cell(module, sel, arm_value, acc, span);
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

    /// Shared `sel ? a : b` cell constructor — used by `FnStmt::If`, the
    /// register reset mux, and `Min`/`Max`/`Abs`'s own `Mux`, so this literal
    /// isn't written out a fourth time. `out`'s width is
    /// `a.width().max(b.width())`, matching every existing inline `Mux`
    /// builder in this file; its `signed` is `a.signed || b.signed`.
    ///
    /// `||`, not `&&`: the checker unifies every branch/arm of a mux to ONE
    /// `Ty` (`emit_verilog/kinds.rs` derives the mux's `Kind` from its
    /// branches the same way), so two REAL branches always agree and the two
    /// operators are equivalent there. They differ only when a branch is a
    /// compile-time constant — `if c { x } else { 0 }`, the stock
    /// clamp/default idiom, or `match s { 0 => 0  1 => a }` — which the
    /// checker gives no signedness of its own (`Ty::CtInt` adopts the sized
    /// branch's `Ty`) and which `lower_const` always builds unsigned. Under
    /// `&&` that literal branch vetoed the whole mux's sign and a downstream
    /// `extend` zero-padded a genuinely signed value (GAP-1 Task 6 round 3,
    /// F2). A caller that knows better (`Abs`'s "the result is never
    /// negative" case) overrides the returned `Bits` afterward.
    fn push_mux_cell(
        &mut self,
        module: &mut Module,
        sel: Bits,
        a: Bits,
        b: Bits,
        span: crate::span::Span,
    ) -> Bits {
        let out_width = a.width().max(b.width());
        let mut out = module.alloc_bits(out_width, None);
        out.signed = a.signed || b.signed;
        module.cells.push(Cell {
            kind: CellKind::Mux,
            pins: [("sel", sel), ("a", a), ("b", b), ("out", out.clone())]
                .into_iter()
                .collect(),
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
                    let bits = self.lower_target_rhs(module, val, self.reg_width(&name.name));
                    env.insert(name.name.clone(), bits);
                }
            }
        }
        for stmt in stmts {
            match stmt {
                SeqStmt::Assign { lhs, rhs } => match self.assign_target(lhs) {
                    Target::Signal(name) => {
                        if env.contains_key(&name) {
                            let bits = self.lower_target_rhs(module, rhs, self.reg_width(&name));
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
                            // Both the ADDRESS and the DATA are constant-
                            // context positions: the address adopts the
                            // memory's own `clog2(depth)` address width, the
                            // data its declared element type. Left at their
                            // own natural widths, `m[0] <- -1` stored the raw
                            // 1-bit literal `1` and the `Mem` cell's `waddr`
                            // pin disagreed with its `raddr` pins (GAP-1
                            // Task 6 round 4, F3).
                            let (elem, addr_width) = match self.mem_kind(&mem) {
                                Some((elem, aw)) => (Some(elem), Some(aw)),
                                None => (None, None),
                            };
                            let addr = self.lower_target_rhs(
                                module,
                                addr_expr,
                                addr_width.map(|bits| crate::elaborate::Width {
                                    bits,
                                    signed: false,
                                }),
                            );
                            let data = self.lower_target_rhs(module, rhs, elem);
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
                            let merged = self.lower_bitselect_write(
                                module,
                                &base_bits,
                                first,
                                second.as_ref(),
                                rhs,
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
                        let out = self.push_mux_cell(
                            module,
                            sel.clone(),
                            a,
                            b,
                            crate::span::Span::default(),
                        );
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
        local_consts: BTreeMap::new(),
        expr_memo: HashMap::new(),
    };

    for input in &design.inputs {
        // A signed-declared input's `Bits` must carry `signed: true` from the
        // moment it's allocated — everything downstream (`Neg`, `extend`,
        // `min`/`max`/`abs`, comparison signedness) reads this flag straight
        // off the `Bits`, never re-deriving it from the source `Expr` (GAP-1
        // Task 6). Nothing else re-populates this for a plain input, since a
        // bare `Ident` resolves straight to this cached `Bits` with no
        // further lowering (`LowerCtx::resolve`).
        let mut bits = module.alloc_bits(input.width.bits, Some(&input.name));
        bits.signed = input.width.signed;
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
        let mut q_bits = module.alloc_bits(reg.width.bits, Some(&reg.name));
        q_bits.signed = reg.width.signed;
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
            .width;
        let mut bits = module.alloc_bits(width.bits, Some(name));
        bits.signed = width.signed;
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
        for net_id in &bits.nets {
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
            // `elaborate::module`'s `const_eval_wide(&reset_expr)` stores the
            // reset value at the folded constant's OWN natural width and
            // natural signedness, not the register's — so `reg q: signed[8] =
            // -1` arrived here as the 1-bit constant `1`. Re-size it through
            // the same sign-aware `from_const_at_width` `lower_expr_sized` and
            // `ir::exec`'s own `Const` evaluation use (GAP-1 Task 6 round 4,
            // F4).
            let resized =
                crate::value::from_const_at_width(&reg.reset, reg.width.bits, reg.reset.signed);
            let reset_cv = crate::checker::consteval::ConstVal {
                bits: resized.bits,
                width: reg.width.bits,
                signed: reg.width.signed,
            };
            let reset_const = ctx.lower_const(&mut module, &reset_cv, crate::span::Span::default());
            d_bits = ctx.push_mux_cell(
                &mut module,
                reset_sel,
                reset_const,
                d_bits,
                crate::span::Span::default(),
            );
        }

        let clock_bits = ctx.resolve(&mut module, &reg.clock);
        assert_eq!(clock_bits.width(), 1, "a clock signal is always 1 bit");
        module.cells.push(Cell {
            kind: CellKind::Dff {
                clock: clock_bits.nets[0],
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
                let mut rdata = module.alloc_bits(mem.width.bits, Some(&mem.name));
                rdata.signed = mem.width.signed; // same element-kind rule as a real read
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
