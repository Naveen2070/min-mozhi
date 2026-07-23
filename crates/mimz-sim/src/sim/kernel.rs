//! Event-driven, two-phase simulation kernel (Phase 1.5, step B2).
//!
//! Interprets a [`Design`] (from [`super::elaborate`]) over clock cycles.
//! Registers initialize to their reset value. Each rising edge of a clock:
//! settle the combinational layer from the current register state + inputs,
//! compute every register's next value (synchronous active-high reset wins,
//! else the `on`-block result), then **commit all registers at once**. That
//! two-phase update is what makes non-blocking `<-` semantics exact: a register
//! read during the cycle always sees the *old* value.
//!
//! Reset is synthesized exactly as the emitter does it — when a declared reset
//! is high, every register in that clock's domain takes its reset value — so a
//! run matches the Verilog the backend emits (the differential oracle, B8).
//!
//! [`Sim::snapshot`] returns every signal's current value: the per-cycle seam
//! the VCD writer and the console tracer (B5) both consume.

use std::collections::{BTreeMap, HashMap, HashSet};

use mimz_core::REPEAT_BUDGET;
use mimz_core::ast::{Edge, Expr, FuncDecl, SeqStmt};

use super::elaborate::{Design, Width};
use super::value::{self, Resolver, Val};

/// Re-mask `v`'s raw bits to width `w` (with `signed`) — a pure reinterpret
/// (truncate/zero-pad the limbs), NOT a sign-extending resize. Mirrors the
/// exact "reinterpret the same raw bits" semantics `Val::new(v.bits, w, s)`
/// had before `Bits` gained a `Wide` variant (Task 2's Copy-loss fallout,
/// Task 7); same pattern as `value.rs`'s own (private) `remask_to_width`.
fn remask_to_width(v: Val, w: u32, signed: bool) -> Val {
    let mut limbs = v.to_limbs();
    limbs.resize(super::wide::limb_count(w), 0);
    Val::new_wide(limbs, w, signed)
}

/// A running simulation of one elaborated [`Design`].
pub struct Sim {
    design: Design,
    /// Declared width of every signal — inputs, outputs, wires, registers, and
    /// the 1-bit clocks/resets — for masking set and computed values.
    widths: BTreeMap<String, Width>,
    /// Current values of the leaf signals the testbench drives: inputs, clocks,
    /// and resets. Wires/outputs are derived; registers live in `regs`.
    leaves: BTreeMap<String, Val>,
    /// Current register state.
    regs: BTreeMap<String, Val>,
    /// Written memory cells, keyed by (memory name, address). Sparse: a cell
    /// absent here reads as its memory's init value (power-on seed).
    mems: BTreeMap<(String, u128), Val>,
    /// Per-memory metadata for the resolver (element width, init, depth).
    mem_meta: BTreeMap<String, MemInfo>,
}

/// What the resolver and writer need to know about a memory without scanning
/// `design.mems`: the element width, the init value returned for an unwritten
/// or out-of-range cell, and the depth (writes past it are dropped, as in
/// Verilog).
#[derive(Clone, Copy)]
struct MemInfo {
    width: Width,
    init: i128,
    depth: u128,
}

impl Sim {
    /// Build a simulation: registers at their reset value, every drivable leaf
    /// (inputs/clocks/resets) at 0.
    pub fn new(design: Design) -> Sim {
        let one = Width {
            bits: 1,
            signed: false,
        };
        let mut widths = BTreeMap::new();
        for s in design
            .inputs
            .iter()
            .chain(&design.outputs)
            .chain(&design.wires)
        {
            widths.insert(s.name.clone(), s.width);
        }
        for r in &design.regs {
            widths.insert(r.name.clone(), r.width);
        }
        for c in &design.clocks {
            widths.insert(c.clone(), one);
        }
        for r in &design.resets {
            widths.insert(r.clone(), one);
        }

        let mut leaves = BTreeMap::new();
        for s in &design.inputs {
            leaves.insert(s.name.clone(), Val::new(0, s.width.bits, s.width.signed));
        }
        for c in &design.clocks {
            leaves.insert(c.clone(), Val::new(0, 1, false));
        }
        for r in &design.resets {
            leaves.insert(r.clone(), Val::new(0, 1, false));
        }
        let mut regs = BTreeMap::new();
        for r in &design.regs {
            regs.insert(
                r.name.clone(),
                Val::new(r.reset as u128, r.width.bits, r.width.signed),
            );
        }
        // Memories start empty (sparse): an unwritten cell reads its init value.
        let mut mem_meta = BTreeMap::new();
        for mem in &design.mems {
            mem_meta.insert(
                mem.name.clone(),
                MemInfo {
                    width: mem.width,
                    init: mem.init,
                    depth: mem.depth,
                },
            );
        }
        Sim {
            design,
            widths,
            leaves,
            regs,
            mems: BTreeMap::new(),
            mem_meta,
        }
    }

    /// Drive a leaf signal (an input, clock, or reset) to `value`, masked to its
    /// declared width. Errors if `name` is not a drivable leaf.
    pub fn set(&mut self, name: &str, value: value::Bits) -> Result<(), String> {
        match self.widths.get(name) {
            Some(w) if self.leaves.contains_key(name) => {
                let val = match value {
                    value::Bits::Small(b) if w.bits <= 128 => Val::new(b, w.bits, w.signed),
                    value::Bits::Small(b) => {
                        Val::new_wide(value::wide_limbs_from_u128(b, w.bits), w.bits, w.signed)
                    }
                    value::Bits::Wide(limbs) => Val::new_wide(limbs, w.bits, w.signed),
                };
                self.leaves.insert(name.to_string(), val);
                Ok(())
            }
            _ => Err(format!(
                "`{name}` is not a drivable input/clock/reset of `{}`",
                self.design.module
            )),
        }
    }

    /// Read the current value of any signal — a leaf (input/clock/reset), a
    /// register, or a combinational wire/output (settled on demand).
    pub fn peek(&self, name: &str) -> Result<value::Bits, String> {
        if let Some(v) = self.leaves.get(name) {
            return Ok(v.bits_masked());
        }
        if let Some(v) = self.regs.get(name) {
            return Ok(v.bits_masked());
        }
        let mut env = self.comb_env();
        Ok(env.signal(name)?.bits_masked())
    }

    /// Advance one full period of `clock`: the rising edge, then the falling
    /// edge. A register/process acts only in its own edge phase, so a
    /// pure-`rise` design behaves exactly as before (the fall phase is a no-op);
    /// a mixed design sees `posedge` regs update before `negedge` regs within the
    /// period, matching Verilog (and the Icarus differential).
    pub fn tick(&mut self, clock: &str) -> Result<(), String> {
        self.tick_edge(clock, Edge::Rise)?;
        self.tick_edge(clock, Edge::Fall)?;
        Ok(())
    }

    /// One edge of `clock` (two-phase commit, see the module doc): the regs and
    /// processes bound to `clock` whose `edge` matches update; everything else
    /// holds. Synchronous active-high reset wins over the `on`-block result.
    /// Public within the sim so [`super::run`] can sample BETWEEN the rising and
    /// falling edges (matching the Verilog testbench's mid-cycle sample point).
    pub(super) fn tick_edge(&mut self, clock: &str, edge: Edge) -> Result<(), String> {
        let reset_now = self
            .design
            .resets
            .iter()
            .any(|r| self.leaves.get(r).is_some_and(|v| v.lsb() == 1));

        // Start from the current registers (hold-by-default), overlay this
        // edge's updates, then commit. Memory holds across reset (power-on
        // init only — the reset line clears registers, not memory). A design
        // with no memories pays nothing here: no clone, no commit (keeps the
        // mem-free kernel hot path byte-for-byte as before A4).
        let has_mems = !self.mem_meta.is_empty();
        let mut next = self.regs.clone();
        let mut next_mems = if has_mems {
            self.mems.clone()
        } else {
            BTreeMap::new()
        };
        if reset_now {
            for reg in &self.design.regs {
                if reg.clock == clock && reg.edge == edge {
                    next.insert(
                        reg.name.clone(),
                        Val::new(reg.reset as u128, reg.width.bits, reg.width.signed),
                    );
                }
            }
        } else {
            let mut env = self.comb_env();
            for proc in &self.design.procs {
                if proc.clock == clock && proc.edge == edge {
                    run_seq(
                        &mut env,
                        &proc.body,
                        &mut next,
                        &mut next_mems,
                        &self.widths,
                    )?;
                }
            }
        }
        self.regs = next;
        if has_mems {
            self.mems = next_mems;
        }
        Ok(())
    }

    /// A snapshot of every signal's current value (low bits) with its width —
    /// the per-cycle data the VCD writer and console tracer consume. Order:
    /// leaves, then registers, then combinational signals (settled now).
    ///
    /// Returns `Bits` (not a raw `u128`) so a signal wider than 128 bits can
    /// be snapshotted without panicking — `Frame.values`' own element type
    /// (`run.rs`, Task 8) and `Val::bits_masked`'s "both `Small` and `Wide`"
    /// contract, mirroring `Sim::peek` immediately above.
    pub fn snapshot(&self) -> Result<Vec<(String, value::Bits, Width)>, String> {
        let mut out = Vec::new();
        for (name, v) in &self.leaves {
            out.push((name.clone(), v.bits_masked(), self.widths[name]));
        }
        for (name, v) in &self.regs {
            out.push((name.clone(), v.bits_masked(), self.widths[name]));
        }
        let mut env = self.comb_env();
        for name in self.design.comb.keys().chain(&self.design.unknown_signals) {
            let v = env.signal(name)?;
            let w = self.widths.get(name).copied().unwrap_or(Width {
                bits: v.width,
                signed: v.signed,
            });
            out.push((name.clone(), v.bits_masked(), w));
        }
        Ok(out)
    }

    /// Evaluate an expression against the current state — settling the
    /// combinational layer on demand. The test harness (B6) uses this for
    /// `expect`/`if` conditions, input drives, and `tick` counts.
    pub(super) fn eval(&self, e: &Expr) -> Result<Val, String> {
        value::eval(&mut self.comb_env(), e)
    }

    /// A combinational resolver over the current state: registers and leaves are
    /// known leaf values; wires/outputs resolve through their drivers.
    fn comb_env(&self) -> CombEnv<'_> {
        let mut known = self.leaves.clone();
        known.extend(self.regs.iter().map(|(k, v)| (k.clone(), v.clone())));
        CombEnv {
            consts: &self.design.consts,
            known,
            comb: &self.design.comb,
            widths: &self.widths,
            memo: BTreeMap::new(),
            stack: Vec::new(),
            mem_cells: &self.mems,
            mem_meta: &self.mem_meta,
            funcs: &self.design.funcs,
            unknown: &self.design.unknown_signals,
        }
    }
}

/// Interpret a sequential body, writing register next-values into `next`. RHS
/// expressions read `env` (the *current* state), so non-blocking semantics hold;
/// `next` is write-only here. Last assignment on the taken path wins.
fn run_seq(
    env: &mut CombEnv,
    body: &[SeqStmt],
    next: &mut BTreeMap<String, Val>,
    next_mems: &mut BTreeMap<(String, u128), Val>,
    widths: &BTreeMap<String, Width>,
) -> Result<(), String> {
    // D-DEFAULT-3: defaults first so conditional assigns override them
    for s in body {
        if let SeqStmt::Default { name, val, .. } = s {
            // Target width is known up front — feed it in as context so a
            // `<<`/`>>` in `val` sees its real consuming width (BUG-11).
            let target_w = widths.get(&name.name).map(|w| w.bits);
            let v = value::eval_ctx(env, val, target_w)?;
            let w = widths.get(&name.name).copied().unwrap_or(Width {
                bits: v.width,
                signed: v.signed,
            });
            next.insert(name.name.clone(), remask_to_width(v, w.bits, w.signed));
        }
    }
    for s in body {
        match s {
            SeqStmt::Assign { lhs, rhs } => {
                match &lhs.index {
                    // Whole-register update.
                    None => {
                        // Target width known up front — feed it in as
                        // context so a `<<`/`>>` in `rhs` sees its real
                        // consuming width (BUG-11).
                        let target_w = widths.get(&lhs.base.name).map(|w| w.bits);
                        let v = value::eval_ctx(env, rhs, target_w)?;
                        let w = widths.get(&lhs.base.name).copied().unwrap_or(Width {
                            bits: v.width,
                            signed: v.signed,
                        });
                        next.insert(lhs.base.name.clone(), remask_to_width(v, w.bits, w.signed));
                    }
                    // Memory cell write `m[addr] <- v`. The address reads the
                    // CURRENT state (`env`); the write lands in `next_mems`, so
                    // a same-cycle read of the cell still sees the old value.
                    Some((addr_expr, None)) if env.is_mem(&lhs.base.name) => {
                        let info = env.mem_info(&lhs.base.name);
                        let addr = value::eval(env, addr_expr)?.bits_small_or_zero();
                        let v = value::eval_ctx(env, rhs, info.map(|i| i.width.bits))?;
                        // A write past the end is dropped (matches Verilog).
                        if let Some(info) = info
                            && addr < info.depth
                        {
                            next_mems.insert(
                                (lhs.base.name.clone(), addr),
                                remask_to_width(v, info.width.bits, info.width.signed),
                            );
                        }
                    }
                    // Bit-indexed (`reg[i] <- v`) or slice-indexed
                    // (`reg[hi:lo] <- v`) write into a plain register — BUG-8.
                    // Both index/slice bounds must be compile-time constants
                    // for a plain (non-array, non-mem) signal, same as the
                    // read path (`value::eval`'s `ExprKind::Index`/`Slice`
                    // arms), so no runtime-index case exists here to handle.
                    // The base value is the CURRENT register state, chained
                    // through `next` if an earlier statement this same cycle
                    // already patched a disjoint bit/slice of it (so two
                    // `reg[i] <- ..`/`reg[j] <- ..` writes in one `on` block
                    // combine, rather than the second clobbering the first).
                    Some((idx_or_hi, lo)) => {
                        let base = match next.get(&lhs.base.name) {
                            Some(v) => v.clone(),
                            None => env.signal(&lhs.base.name)?,
                        };
                        let v = value::eval(env, rhs)?;
                        let bits = match lo {
                            None => {
                                let i = value::checked_index(
                                    value::const_eval(idx_or_hi, env.ints())?,
                                    base.width,
                                    "bit index",
                                )?;
                                (base.masked() & !(1u128 << i)) | (v.lsb() << i)
                            }
                            Some(lo_expr) => {
                                let hi = value::checked_index(
                                    value::const_eval(idx_or_hi, env.ints())?,
                                    base.width,
                                    "slice high bound",
                                )?;
                                let lo = value::checked_index(
                                    value::const_eval(lo_expr, env.ints())?,
                                    base.width,
                                    "slice low bound",
                                )?;
                                if hi < lo {
                                    return Err(
                                        "slice bounds reversed (write `[hi:lo]`, msb first)".into(),
                                    );
                                }
                                let w = hi - lo + 1;
                                let clear = value::mask(w) << lo;
                                (base.masked() & !clear) | ((v.masked() & value::mask(w)) << lo)
                            }
                        };
                        next.insert(
                            lhs.base.name.clone(),
                            Val::new(bits, base.width, base.signed),
                        );
                    }
                }
            }
            SeqStmt::If { cond, then, els } => {
                if value::eval(env, cond)?.lsb() == 1 {
                    run_seq(env, then, next, next_mems, widths)?;
                } else if let Some(e) = els {
                    run_seq(env, e, next, next_mems, widths)?;
                }
            }
            SeqStmt::Default { .. } => {} // already processed above
            SeqStmt::Loop {
                var, lo, hi, body, ..
            } => {
                let lo_v = value::eval(env, lo)?.masked() as i128;
                let hi_v = value::eval(env, hi)?.masked() as i128;
                let count = (hi_v - lo_v).max(0);
                if count > REPEAT_BUDGET {
                    return Err(format!(
                        "`loop` would unroll {count} times, over the limit of {REPEAT_BUDGET}"
                    ));
                }
                // Bind the loop variable into `known` (owned, mutable leaf
                // storage) for each iteration — `consts` is an immutable
                // borrow and can't host a per-iteration value. Shadow the
                // previous binding (if any) and restore it after, same
                // discipline as every other compile-time loop variable here.
                let mut i = lo_v;
                while i < hi_v {
                    let shadowed = env.known.insert(var.name.clone(), Val::from_int(i));
                    run_seq(env, body, next, next_mems, widths)?;
                    match shadowed {
                        Some(v) => {
                            env.known.insert(var.name.clone(), v);
                        }
                        None => {
                            env.known.remove(&var.name);
                        }
                    }
                    i += 1;
                }
            }
            // Unreachable: every `SeqStmt::ForEach` in an `on`-block body is
            // lowered before `run_seq` ever runs — see `elaborate.rs`'s
            // `elaborate_module`'s `ModuleItem::On` arm.
            SeqStmt::ForEach { .. } => unreachable!(
                "ForEach is lowered before Rw::seq/assigns/run_seq ever run — see elaborate_module's ModuleItem::On arm"
            ),
            // Unreachable: the kernel runs on a strict-parsed tree, which
            // carries no `Error` placeholder.
            SeqStmt::Error(_) => {}
        }
    }
    Ok(())
}

/// Resolver over a frozen state: `known` (regs + leaves) are leaf values;
/// combinational signals resolve through `comb` (memoized, cycle-detected).
struct CombEnv<'a> {
    consts: &'a BTreeMap<String, i128>,
    known: BTreeMap<String, Val>,
    comb: &'a BTreeMap<String, Expr>,
    widths: &'a BTreeMap<String, Width>,
    memo: BTreeMap<String, Val>,
    stack: Vec<String>,
    mem_cells: &'a BTreeMap<(String, u128), Val>,
    mem_meta: &'a BTreeMap<String, MemInfo>,
    funcs: &'a HashMap<String, FuncDecl>,
    /// Driverless-by-design signals (extern-instance outputs in `warn`
    /// `SimMode`) — resolved straight to `Val::unknown`, checked before
    /// `comb` since these names have no entry there. See
    /// `elaborate::Design::unknown_signals`.
    unknown: &'a HashSet<String>,
}

impl CombEnv<'_> {
    /// Metadata of memory `name`, if it is one (`Copy`, so the caller is free
    /// of the borrow afterward).
    fn mem_info(&self, name: &str) -> Option<MemInfo> {
        self.mem_meta.get(name).copied()
    }
}

impl Resolver for CombEnv<'_> {
    fn is_mem(&self, name: &str) -> bool {
        self.mem_meta.contains_key(name)
    }

    fn mem_read(&mut self, name: &str, addr: u128) -> Result<Val, String> {
        let info = self
            .mem_meta
            .get(name)
            .ok_or_else(|| format!("`{name}` is not a memory"))?;
        // An unwritten or out-of-range cell reads the memory's init value.
        Ok(self
            .mem_cells
            .get(&(name.to_string(), addr))
            .cloned()
            .unwrap_or_else(|| Val::new(info.init as u128, info.width.bits, info.width.signed)))
    }

    fn signal(&mut self, name: &str) -> Result<Val, String> {
        if let Some(v) = self.known.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.memo.get(name) {
            return Ok(v.clone());
        }
        if self.unknown.contains(name) {
            // Extern-instance output in `warn` mode: no driver by design.
            let w = self.widths.get(name).copied().unwrap_or(Width {
                bits: 1,
                signed: false,
            });
            let v = Val::unknown(w.bits, w.signed);
            self.memo.insert(name.to_string(), v.clone());
            return Ok(v);
        }
        if let Some(driver) = self.comb.get(name) {
            if self.stack.iter().any(|n| n == name) {
                return Err(format!(
                    "combinational cycle through `{name}` — feedback must pass through a register"
                ));
            }
            self.stack.push(name.to_string());
            // Target width known up front — feed it in as context so a
            // `<<`/`>>` in `driver` sees its real consuming width (BUG-11).
            let target_w = self.widths.get(name).map(|w| w.bits);
            let v = value::eval_ctx(self, driver, target_w)?;
            self.stack.pop();
            let v = match self.widths.get(name) {
                // `Val::new` always clears `unknown` — re-widthing a driver's
                // result must not silently launder an extern-fed taint away
                // (BUG found by Task 8: `ok = u.locked` re-masked to `ok`'s
                // declared width here and lost `unknown` in the process).
                Some(w) => {
                    let unknown = v.unknown;
                    Val {
                        unknown,
                        ..remask_to_width(v, w.bits, w.signed)
                    }
                }
                None => v,
            };
            self.memo.insert(name.to_string(), v.clone());
            return Ok(v);
        }
        if let Some(i) = self.consts.get(name) {
            return Ok(Val::from_int(*i));
        }
        Err(format!("unknown signal `{name}`"))
    }
    fn ints(&self) -> &BTreeMap<String, i128> {
        self.consts
    }
    fn funcs(&self) -> Option<&HashMap<String, FuncDecl>> {
        Some(self.funcs)
    }
}

#[cfg(test)]
mod tests {
    use super::value::Bits;
    use super::*;
    use crate::sim::elaborate::{SimMode, elaborate, elaborate_with_mode};
    use mimz_core::ast::ExprKind;

    fn sim(src: &str) -> Sim {
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        Sim::new(elaborate(&f, None, &BTreeMap::new()).expect("elaborates"))
    }

    const EXTERN_PLL_SRC: &str = "extern module Pll {\n  in clk_in: bit\n  out locked: bit\n}\n\
        module M {\n  clock sysclk\n  out ok: bit\n  \
        let u = Pll() { clk_in: sysclk, locked: ok }\n  \
        ok = u.locked\n}\n";

    #[test]
    fn extern_instance_output_is_unknown_tainted_in_warn_mode() {
        let f = mimz_core::parser::parse(mimz_core::lexer::lex(EXTERN_PLL_SRC).expect("lexes"))
            .expect("parses");
        let design = elaborate_with_mode(&f, None, &BTreeMap::new(), SimMode::Warn)
            .expect("warn mode elaborates without error — the design just runs with X outputs");
        let s = Sim::new(design);
        let ok = Expr {
            kind: ExprKind::Ident("ok".to_string()),
            span: mimz_core::span::Span::new(0, 0),
        };
        let v = s
            .eval(&ok)
            .expect("reading an extern-fed signal in warn mode must not error");
        assert!(
            v.unknown,
            "`ok` is driven by `u.locked` (an extern output) — it must read as unknown-tainted"
        );
    }

    #[test]
    fn extern_instance_is_a_hard_error_in_strict_mode() {
        let f = mimz_core::parser::parse(mimz_core::lexer::lex(EXTERN_PLL_SRC).expect("lexes"))
            .expect("parses");
        let err = elaborate_with_mode(&f, None, &BTreeMap::new(), SimMode::Strict)
            .expect_err("strict mode must reject an extern instance before any cycle runs");
        assert!(err.contains('u'), "error should name the instance: {err}");
        assert!(
            err.contains("Pll"),
            "error should name the extern module: {err}"
        );
    }

    #[test]
    fn extern_taint_survives_one_level_of_real_module_nesting() {
        // C wraps an extern Pll instance and re-exposes its unknown-tainted
        // output as C's own output port; Top instantiates C (not the extern
        // directly). `flatten_instance`'s child->parent copy loop used to only
        // copy a child wire/output that has a `comb` driver, silently dropping
        // the driverless (extern-tainted) `u_locked` wire when C was flattened
        // into Top — leaving `c_c_out`'s driver expression referencing a wire
        // that was never declared or marked unknown in Top's own Design, which
        // then blew up at read time with "unknown signal `c_u_locked`" instead
        // of just reading as X like the direct (one-level) case does.
        const SRC: &str = "extern module Pll {\n  in clk_in: bit\n  out locked: bit\n}\n\
            module C {\n  clock sysclk\n  out c_out: bit\n  \
            let u = Pll() { clk_in: sysclk, locked: c_out }\n  \
            c_out = u.locked\n}\n\
            module Top {\n  clock sysclk\n  out final_out: bit\n  \
            let c = C() { sysclk: sysclk, c_out: final_out }\n  \
            final_out = c.c_out\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(SRC).expect("lexes")).expect("parses");
        let design = elaborate_with_mode(&f, Some("Top"), &BTreeMap::new(), SimMode::Warn)
            .expect("warn mode elaborates a nested extern-tainted design without error");
        let s = Sim::new(design);
        let final_out = Expr {
            kind: ExprKind::Ident("final_out".to_string()),
            span: mimz_core::span::Span::new(0, 0),
        };
        let v = s.eval(&final_out).expect(
            "reading a signal fed by an extern output through one level of real-module \
             nesting must not error with \"unknown signal\" — it must just read as X",
        );
        assert!(
            v.unknown,
            "`final_out` is fed by `C`'s `c_out`, itself fed by extern `Pll.locked` — it \
             must read as unknown-tainted"
        );
    }

    const COUNTER: &str = "module Counter(WIDTH: int = 8) {\n  \
        clock clk\n  reset rst\n  out count: bits[WIDTH]\n  \
        reg value: bits[WIDTH] = 0\n  on rise(clk) { value <- value +% 1 }\n  \
        count = value\n}\n";

    #[test]
    fn counter_counts_and_resets() {
        let mut s = sim(COUNTER);
        // A reset cycle holds the register at 0.
        s.set("rst", Bits::Small(1)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("count").unwrap(), Bits::Small(0));
        // Then it counts up each rising edge.
        s.set("rst", Bits::Small(0)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("count").unwrap(), Bits::Small(1));
        s.tick("clk").unwrap();
        assert_eq!(s.peek("value").unwrap(), Bits::Small(2));
        s.tick("clk").unwrap();
        assert_eq!(s.peek("count").unwrap(), Bits::Small(3));
        // Asserting reset forces it back to 0.
        s.set("rst", Bits::Small(1)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("count").unwrap(), Bits::Small(0));
    }

    #[test]
    fn dual_edge_negedge_reg_captures_posedge_within_a_period() {
        // a (posedge) <- d; b (negedge) <- a. The rise-then-fall tick lets the
        // negedge `b` see the NEW `a` in the same period — matching Verilog (the
        // `dual_edge` example proves this bit-for-bit vs Icarus).
        let mut s = sim(
            "module M {\n  clock clk\n  reset rst\n  in d: bits[8]\n  out q: bits[8]\n  reg a: bits[8] = 0\n  reg b: bits[8] = 0\n  on rise(clk) { a <- d }\n  on fall(clk) { b <- a }\n  q = b\n}\n",
        );
        s.set("d", Bits::Small(5)).unwrap();
        s.set("rst", Bits::Small(0)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("a").unwrap(), Bits::Small(5));
        assert_eq!(
            s.peek("q").unwrap(),
            Bits::Small(5),
            "negedge `b` captured the new `a`"
        );
    }

    #[test]
    fn memory_write_then_read_round_trips_a_cell() {
        let mut s = sim(
            "module RF {\n  clock clk\n  in we: bit\n  in waddr: bits[2]\n  in wdata: bits[8]\n  in raddr: bits[2]\n  out rdata: bits[8]\n  mem m: bits[8][4] = 0\n  on rise(clk) {\n    if we {\n      m[waddr] <- wdata\n    }\n  }\n  rdata = m[raddr]\n}\n",
        );
        // Every cell starts at the init value (power-on seed).
        s.set("raddr", Bits::Small(2)).unwrap();
        assert_eq!(
            s.peek("rdata").unwrap(),
            Bits::Small(0),
            "an unwritten cell reads init"
        );
        // Write 165 to cell 2, then read it back.
        s.set("we", Bits::Small(1)).unwrap();
        s.set("waddr", Bits::Small(2)).unwrap();
        s.set("wdata", Bits::Small(165)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(
            s.peek("rdata").unwrap(),
            Bits::Small(165),
            "cell 2 holds the written value"
        );
        // A different, never-written cell still reads init.
        s.set("raddr", Bits::Small(1)).unwrap();
        assert_eq!(
            s.peek("rdata").unwrap(),
            Bits::Small(0),
            "cell 1 was never written"
        );
    }

    #[test]
    fn bit_indexed_register_write_sets_one_bit() {
        // BUG-8 regression: `shift[i] <- v` on a plain register used to
        // error ("assigning a slice/bit of ... is not supported by the
        // simulator yet"). `i` must be a compile-time constant for a plain
        // (non-array/mem) signal, same as the read path.
        let mut s = sim(
            "module M {\n  clock clk\n  reset rst\n  in v: bit\n  out y: bits[4]\n  \
             reg shift: bits[4] = 0\n  on rise(clk) { shift[2] <- v }\n  y = shift\n}\n",
        );
        s.set("rst", Bits::Small(0)).unwrap();
        s.set("v", Bits::Small(1)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(
            s.peek("y").unwrap(),
            Bits::Small(0b0100),
            "bit 2 set, others untouched"
        );
    }

    #[test]
    fn slice_indexed_register_write_sets_a_range() {
        let mut s = sim(
            "module M {\n  clock clk\n  reset rst\n  in v: bits[2]\n  out y: bits[4]\n  \
             reg r: bits[4] = 0b1001\n  on rise(clk) { r[2:1] <- v }\n  y = r\n}\n",
        );
        s.set("rst", Bits::Small(0)).unwrap();
        s.set("v", Bits::Small(0b11)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(
            s.peek("y").unwrap(),
            Bits::Small(0b1111),
            "bits [2:1] replaced, bits 3 and 0 kept from the reset value"
        );
    }

    #[test]
    fn disjoint_bit_indexed_writes_in_one_on_block_combine() {
        // Two separate `reg[i] <- v` statements to disjoint bits of the
        // SAME register in one `on` block must both take effect — the
        // second must see the first's already-patched value (via `next`),
        // not clobber it by re-reading the stale pre-cycle `env` value.
        let mut s = sim(
            "module M {\n  clock clk\n  reset rst\n  in a: bit\n  in b: bit\n  out y: bits[2]\n  \
             reg r: bits[2] = 0\n  on rise(clk) {\n    r[0] <- a\n    r[1] <- b\n  }\n  y = r\n}\n",
        );
        s.set("rst", Bits::Small(0)).unwrap();
        s.set("a", Bits::Small(1)).unwrap();
        s.set("b", Bits::Small(1)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("y").unwrap(), Bits::Small(0b11));
    }

    #[test]
    fn regs_init_to_their_reset_value() {
        // Before any tick, a reg holds its (non-zero) reset value.
        let s = sim("module R {\n  clock clk\n  reset rst\n  out y: bits[8]\n  \
             reg r: bits[8] = 5\n  on rise(clk) { r <- r +% 1 }\n  y = r\n}\n");
        assert_eq!(s.peek("y").unwrap(), Bits::Small(5));
    }

    #[test]
    fn wraps_at_declared_width() {
        let mut s = sim("module W {\n  clock clk\n  reset rst\n  out y: bits[2]\n  \
             reg r: bits[2] = 0\n  on rise(clk) { r <- r +% 1 }\n  y = r\n}\n");
        s.set("rst", Bits::Small(0)).unwrap();
        for _ in 0..3 {
            s.tick("clk").unwrap(); // 1, 2, 3
        }
        assert_eq!(s.peek("y").unwrap(), Bits::Small(3));
        s.tick("clk").unwrap(); // 3 +% 1 wraps to 0 in bits[2]
        assert_eq!(s.peek("y").unwrap(), Bits::Small(0));
    }

    #[test]
    fn two_phase_commit_swaps_registers() {
        // `a <- b; b <- a` must SWAP (non-blocking), not collapse both to b.
        let mut s = sim(
            "module Swap {\n  clock clk\n  reset rst\n  out oa: bits[8]\n  out ob: bits[8]\n  \
             reg a: bits[8] = 1\n  reg b: bits[8] = 2\n  \
             on rise(clk) {\n    a <- b\n    b <- a\n  }\n  oa = a\n  ob = b\n}\n",
        );
        s.set("rst", Bits::Small(0)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("oa").unwrap(), Bits::Small(2)); // a took the OLD b
        assert_eq!(s.peek("ob").unwrap(), Bits::Small(1)); // b took the OLD a, not the new a
    }

    #[test]
    fn statement_if_picks_the_next_value() {
        let mut s = sim(
            "module C {\n  clock clk\n  reset rst\n  in up: bit\n  out y: bits[8]\n  \
             reg r: bits[8] = 0\n  \
             on rise(clk) {\n    if up { r <- r +% 1 } else { r <- r -% 1 }\n  }\n  y = r\n}\n",
        );
        s.set("rst", Bits::Small(0)).unwrap();
        s.set("up", Bits::Small(1)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("y").unwrap(), Bits::Small(1));
        s.set("up", Bits::Small(0)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("y").unwrap(), Bits::Small(0)); // 1 -% 1
    }

    #[test]
    fn snapshot_covers_every_signal() {
        let snap = sim(COUNTER).snapshot().unwrap();
        let names: Vec<&str> = snap.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"count")); // combinational output
        assert!(names.contains(&"value")); // register
        assert!(names.contains(&"clk")); // clock leaf
        assert!(names.contains(&"rst")); // reset leaf
    }

    #[test]
    fn set_rejects_a_non_leaf() {
        let mut s = sim(COUNTER);
        assert!(s.set("count", Bits::Small(1)).is_err()); // an output, not drivable
        assert!(s.set("nope", Bits::Small(1)).is_err()); // unknown
    }

    #[test]
    fn combinational_chain_propagates_in_order() {
        // y ← b ← a ← x, and b also reads reg r: a multi-level comb chain plus a
        // register input. The lazy memoized resolver must settle a, then b, then
        // y each cycle: y = ((x +% 1) +% r) +% 1 = x + r + 2.
        let mut s = sim(
            "module Chain {\n  clock clk\n  reset rst\n  in x: bits[8]\n  out y: bits[8]\n  \
             reg r: bits[8] = 0\n  wire a: bits[8] = x +% 1\n  wire b: bits[8] = a +% r\n  \
             on rise(clk) { r <- r +% 1 }\n  y = b +% 1\n}\n",
        );
        s.set("rst", Bits::Small(0)).unwrap();
        s.set("x", Bits::Small(10)).unwrap();
        assert_eq!(s.peek("y").unwrap(), Bits::Small(12)); // r = 0
        s.tick("clk").unwrap();
        assert_eq!(s.peek("y").unwrap(), Bits::Small(13)); // r = 1
        s.tick("clk").unwrap();
        assert_eq!(s.peek("y").unwrap(), Bits::Small(14)); // r = 2
    }

    #[test]
    fn on_block_loop_unrolls_at_runtime() {
        // Last unrolled copy wins (ordinary on-block last-write-wins
        // semantics, no early exit): i runs 0,1,2,3 and `acc <- i` on the
        // final iteration (i = 3) is what survives.
        let mut s = sim(
            "module M {\n  clock clk\n  reset rst\n  reg acc: bits[8] = 0\n  \
             on rise(clk) {\n    loop i: 0..4 {\n      acc <- i\n    }\n  }\n}\n",
        );
        s.set("rst", Bits::Small(0)).unwrap();
        s.tick("clk").unwrap();
        assert_eq!(s.peek("acc").unwrap(), Bits::Small(3));
    }

    #[test]
    fn on_block_loop_over_budget_errors_at_runtime() {
        let src = format!(
            "module M {{\n  clock clk\n  reset rst\n  in v0: bits[8]\n  reg acc: bits[8] = 0\n  \
             on rise(clk) {{\n    loop i: 0..{} {{\n      acc <- v0\n    }}\n  }}\n}}\n",
            mimz_core::REPEAT_BUDGET + 1
        );
        let mut s = sim(&src);
        s.set("rst", Bits::Small(0)).unwrap();
        s.set("v0", Bits::Small(1)).unwrap();
        let err = s
            .tick("clk")
            .expect_err("over-budget loop must error, not hang or overflow");
        assert!(err.contains("`loop` would unroll"), "got: {err}");
    }

    #[test]
    fn set_and_peek_round_trip_a_wide_value() {
        let src =
            "module M(WIDTH: int = 200) {\n  in a: bits[WIDTH]\n  out b: bits[WIDTH]\n  b = a\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let design =
            super::super::elaborate::elaborate(&f, None, &std::collections::BTreeMap::new())
                .expect("elaborates");
        let mut sim = Sim::new(design);
        let wide_val =
            super::super::value::Bits::Wide(super::super::wide::from_u128(0xDEADBEEF, 200));
        sim.set("a", wide_val.clone()).expect("a is drivable");
        assert_eq!(sim.peek("a").expect("a is readable"), wide_val);
    }

    #[test]
    fn combinational_cycle_is_reported() {
        // `a = b; b = a` is a pure combinational loop. Elaboration does not
        // reject it (that is the checker's job); the kernel's resolver must catch
        // it at settle time rather than spin.
        let s = sim(
            "module Cyc {\n  out y: bits[8]\n  wire a: bits[8] = b\n  wire b: bits[8] = a\n  y = a\n}\n",
        );
        let err = s.peek("y").unwrap_err();
        assert!(err.contains("cycle"), "expected a cycle error, got: {err}");
    }
}
