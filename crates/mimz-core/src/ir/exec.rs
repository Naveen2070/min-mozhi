//! A minimal IR interpreter, used ONLY for differential validation against
//! `mimz-sim`'s AST-level kernel (Task 18) and per-cell unit tests — not a
//! performance-oriented simulator.
//!
//! Every arithmetic/logical cell evaluates through `value::binary_ctx` /
//! `value::unary`, the exact functions the AST evaluator uses, so masking,
//! lossless growth, wrapping, comparison, reduction and X-propagation rules
//! cannot drift between the two sides of the differential test.
//!
//! v1 limitations, all deliberate:
//! - **Unsigned only.** `ir::Bits` is a bare net vector with no signedness,
//!   so every value read out of the netlist is rebuilt as unsigned. A design
//!   whose source types are `signed` is not faithfully modeled here.
//! - **One global clock.** [`Executor::tick`] advances EVERY `Dff`/`Mem`
//!   regardless of which clock net it references (and regardless of its
//!   `edge`); the IR has no module-level clock list, and a genuinely
//!   multi-clock design is out of scope.
//! - **Narrow values only** in net reconstruction — `get_bits` and `set_bits`
//!   panic if a signal exceeds 128 nets (bits); wider signals are deferred to
//!   a future version.
//! - **`<<`/`>>` diverge from the AST kernel, three ways:** `lower_binop`
//!   sizes the `out` pin at `a.width()` so a growing shift truncates;
//!   `CellKind::Shl` carries no shift amount, so `const_amount` is always
//!   `None` here and growth is worst-case rather than exact; and a fused
//!   chain (`(p2 >> 4) << 7`) is one unit at one width for the AST side
//!   (`eval_shift_chain`, BUG-34) but two independent cells in the IR, which
//!   `exec` cannot re-fuse. See `docs/audit/gaps.md` GAP-1's sub-gap.

use super::{Bits, Cell, CellKind, Module, NetId};
use crate::ast::{BinOp, UnOp};
use crate::value::Val;
use std::collections::HashMap;

pub struct Executor<'a> {
    module: &'a Module,
    /// Current value of every DRIVEN net, one entry per net, each a 1-bit
    /// `Val`. A net that is absent is undriven, and reads back as part of a
    /// `Val::unknown` — that is the whole of `BlackBox` output handling.
    values: HashMap<NetId, Val>,
    /// Cell index -> that `Dff`'s current Q value, carried across ticks.
    dff_state: HashMap<usize, Val>,
    /// Cell index -> the words that memory has actually been WRITTEN with,
    /// sparse by address. Mirrors `mimz-sim`'s kernel exactly (`mem_cells`):
    /// an unwritten or out-of-range address reads the cell's `init` seed, so
    /// there is nothing to pre-fill and a huge `depth` costs nothing.
    mem_state: HashMap<usize, HashMap<u128, Val>>,
}

impl<'a> Executor<'a> {
    pub fn new(module: &'a Module) -> Executor<'a> {
        let mut ex = Executor {
            module,
            values: HashMap::new(),
            dff_state: HashMap::new(),
            mem_state: HashMap::new(),
        };
        for (i, cell) in module.cells.iter().enumerate() {
            match &cell.kind {
                CellKind::Dff { .. } => {
                    // Power-on Q is 0, and it is published to the netlist
                    // right away so a read before the first `tick` sees a
                    // register's initial value rather than X.
                    let q = Val::new(0, cell.pins["q"].width(), false);
                    ex.set_bits(&cell.pins["q"], &q);
                    ex.dff_state.insert(i, q);
                }
                CellKind::Mem { .. } => {
                    ex.mem_state.insert(i, HashMap::new());
                }
                _ => {}
            }
        }
        ex
    }

    /// Drives a module input port. Panics on an unknown port name — this is
    /// a test/differential tool, not a diagnosable end-user path.
    pub fn set_input(&mut self, name: &str, value: Val) {
        let module = self.module;
        let bits = module
            .ports
            .iter()
            .find(|(n, ..)| n == name)
            .map(|(_, b, _)| b)
            .unwrap_or_else(|| panic!("no such input port `{name}`"));
        self.set_bits(bits, &value);
    }

    /// Reads any source-level signal: an output port, or — falling back —
    /// a wire, a register's Q, or a memory's read port. `lower()` only puts
    /// `design.inputs`/`outputs` into `module.ports`, so a plain wire is
    /// never a port; `Module::signals` is the exact lookup for those.
    ///
    /// Deliberately NOT a scan of `module.nets` by name: `lower()` only
    /// stamps a wire's name onto its driver's still-unnamed nets, so a name
    /// scan returns a partial, wrong-width value for a wire that reuses
    /// another signal's nets (`wire w = {p, q + 1}`) — silently. See
    /// `Module::signals`.
    pub fn get_output(&self, name: &str) -> Val {
        if let Some((_, bits, _)) = self.module.ports.iter().find(|(n, ..)| n == name) {
            return self.get_bits(bits);
        }
        let bits = self.module.signals.get(name).unwrap_or_else(|| {
            panic!(
                "no such port or signal `{name}` (a module parsed from IR text has no \
                 `signals` table — only its ports are addressable; see Module::signals)"
            )
        });
        self.get_bits(bits)
    }

    /// Settles combinational logic, advances every `Dff`/`Mem` one clock
    /// edge, then settles again so everything downstream of the new register
    /// and memory contents is consistent when this returns.
    ///
    /// The two capture/apply passes are what make a chain of registers see
    /// PRE-edge values: every D input and every memory write is read while
    /// the old Q values still stand, and only then committed.
    pub fn tick(&mut self) {
        self.settle();

        let module = self.module;
        let mut next_dff: Vec<(usize, Val)> = Vec::new();
        let mut writes: Vec<(usize, u128, Val)> = Vec::new();
        for (i, cell) in module.cells.iter().enumerate() {
            match &cell.kind {
                CellKind::Dff { .. } => next_dff.push((i, self.get_bits(&cell.pins["d"]))),
                CellKind::Mem { depth, .. } => {
                    // A ROM's `wen` is a constant 0 (see `lower`), so this
                    // needs no separate "has a clock pin?" test.
                    let wen = self.get_bits(&cell.pins["wen"]);
                    if wen.lsb() != 1 {
                        continue;
                    }
                    let addr = self.get_bits(&cell.pins["waddr"]).bits_small_or_zero();
                    if addr >= *depth {
                        continue; // a write past the end is dropped, same as the kernel
                    }
                    writes.push((i, addr, self.get_bits(&cell.pins["wdata"])));
                }
                _ => {}
            }
        }
        for (i, q) in next_dff {
            self.set_bits(&module.cells[i].pins["q"], &q);
            self.dff_state.insert(i, q);
        }
        for (i, addr, word) in writes {
            self.mem_state
                .get_mut(&i)
                .expect("every Mem cell got its storage in `new`")
                .insert(addr, word);
        }

        self.settle();
    }

    /// Evaluates every combinational cell to a fixed point.
    // ponytail: O(cells^2) worst case — a repeated full pass instead of a
    // precomputed topological order. Add a real topo-sort if this ever shows
    // up in a profile; v1's module sizes don't need one.
    fn settle(&mut self) {
        let module = self.module;
        for _ in 0..module.cells.len().max(1) {
            for (i, cell) in module.cells.iter().enumerate() {
                self.eval_comb_cell(i, cell);
            }
        }
    }

    fn eval_comb_cell(&mut self, index: usize, cell: &Cell) {
        match &cell.kind {
            // `Dff`'s Q is driven in `tick`. A `BlackBox`'s outputs are
            // driven by nothing at all — their nets simply stay absent from
            // `values`, which `get_bits` already reports as `Val::unknown`,
            // matching `mimz-sim`'s `Warn`-mode extern-output semantics with
            // no special case here.
            CellKind::Dff { .. } | CellKind::BlackBox { .. } => {}

            CellKind::Add => self.binop(cell, BinOp::Add),
            CellKind::Sub => self.binop(cell, BinOp::Sub),
            CellKind::Mul => self.binop(cell, BinOp::Mul),
            CellKind::AddWrap => self.binop(cell, BinOp::AddWrap),
            CellKind::SubWrap => self.binop(cell, BinOp::SubWrap),
            CellKind::MulWrap => self.binop(cell, BinOp::MulWrap),
            CellKind::Shl => self.binop(cell, BinOp::Shl),
            CellKind::Shr => self.binop(cell, BinOp::Shr),
            CellKind::And => self.binop(cell, BinOp::BitAnd),
            CellKind::Or => self.binop(cell, BinOp::BitOr),
            CellKind::Xor => self.binop(cell, BinOp::BitXor),
            CellKind::Eq => self.binop(cell, BinOp::Eq),
            CellKind::Ne => self.binop(cell, BinOp::Ne),
            CellKind::Lt => self.binop(cell, BinOp::Lt),
            CellKind::Le => self.binop(cell, BinOp::Le),
            CellKind::Gt => self.binop(cell, BinOp::Gt),
            CellKind::Ge => self.binop(cell, BinOp::Ge),
            CellKind::LogicAnd => self.binop(cell, BinOp::LogicAnd),
            CellKind::LogicOr => self.binop(cell, BinOp::LogicOr),

            CellKind::Not => self.unop(cell, UnOp::BitNot),
            CellKind::Neg => self.unop(cell, UnOp::Neg),
            CellKind::LogicNot => self.unop(cell, UnOp::LogicNot),
            CellKind::RedAnd => self.unop(cell, UnOp::RedAnd),
            CellKind::RedOr => self.unop(cell, UnOp::RedOr),
            CellKind::RedXor => self.unop(cell, UnOp::RedXor),

            CellKind::Mux => {
                let pin = if self.get_bits(&cell.pins["sel"]).lsb() == 1 {
                    "a"
                } else {
                    "b"
                };
                let v = self.get_bits(&cell.pins[pin]);
                self.set_bits(&cell.pins["out"], &v);
            }

            // `lower()` never emits a Concat or Slice cell — both are pure
            // `Bits` re-pointing at lowering time (Task 6), zero cells. These
            // two arms exist so a hand-written or round-tripped IR text file
            // (Tasks 12-13) using them still executes. Concat's input pins
            // are joined in `BTreeMap` key order, LSB pin first; since
            // nothing produces these cells yet, that ordering is this
            // executor's own convention rather than an established one.
            CellKind::Concat => {
                let nets: Vec<NetId> = cell
                    .pins
                    .iter()
                    .filter(|(name, _)| **name != "out")
                    .flat_map(|(_, bits)| bits.0.iter().copied())
                    .collect();
                let v = self.get_bits(&Bits(nets));
                self.set_bits(&cell.pins["out"], &v);
            }
            CellKind::Slice { lo, hi } => {
                let sub = Bits(cell.pins["a"].0[*lo as usize..=*hi as usize].to_vec());
                let v = self.get_bits(&sub);
                self.set_bits(&cell.pins["out"], &v);
            }

            // Combinational read of the PRE-tick contents: writes land in
            // `mem_state` only at `tick`, so a same-cycle read of a written
            // word still sees the old value. An unwritten or out-of-range
            // address reads the `init` seed, exactly like the kernel.
            CellKind::Mem { depth, init } => {
                let rdata = &cell.pins["rdata"];
                let addr = self.get_bits(&cell.pins["raddr"]).bits_small_or_zero();
                let word = if addr < *depth {
                    self.mem_state[&index].get(&addr).cloned()
                } else {
                    None
                };
                let word = word.unwrap_or_else(|| {
                    crate::value::from_const_at_width(init, rdata.width(), false)
                });
                self.set_bits(rdata, &word);
            }

            CellKind::Const { value } => {
                let out = &cell.pins["out"];
                let v = crate::value::from_const_at_width(value, out.width(), false);
                self.set_bits(out, &v);
            }
        }
    }

    /// One `a`/`b` -> `out` cell, evaluated by the AST evaluator's own
    /// operator implementation. A rejected operand pair here means `lower`
    /// produced malformed IR (an internal bug), not a runtime condition, so
    /// this panics rather than propagating a diagnostic.
    fn binop(&mut self, cell: &Cell, op: BinOp) {
        let a = self.get_bits(&cell.pins["a"]);
        let b = self.get_bits(&cell.pins["b"]);
        let v = crate::value::binary_ctx(op, a, b, None, cell.span).unwrap_or_else(|e| {
            panic!(
                "ir::exec: {op:?} cell rejected its own operands ({}) — malformed IR",
                e.msg
            )
        });
        self.set_bits(&cell.pins["out"], &v);
    }

    /// One `a` -> `out` cell. `value::unary` is infallible.
    fn unop(&mut self, cell: &Cell, op: UnOp) {
        let a = self.get_bits(&cell.pins["a"]);
        let v = crate::value::unary(op, a);
        self.set_bits(&cell.pins["out"], &v);
    }

    /// Publishes `value` onto `bits`, one net per bit. An X-state value
    /// REMOVES its nets instead: `Val`'s taint is whole-value, and an absent
    /// net is exactly how `get_bits` represents "unknown", so this is the
    /// same representation rather than a second one.
    fn set_bits(&mut self, bits: &Bits, value: &Val) {
        narrow_only(bits.width());
        if value.unknown {
            for net in &bits.0 {
                self.values.remove(net);
            }
            return;
        }
        for (i, &net) in bits.0.iter().enumerate() {
            self.values
                .insert(net, Val::new(bit_of(value, i as u32), 1, false));
        }
    }

    /// Reassembles a pin's nets into one value. If ANY net is undriven the
    /// WHOLE value is `Val::unknown` — `Val`'s taint is coarse by design, and
    /// a per-bit reconstruction would silently turn an undriven net into a
    /// real 0.
    fn get_bits(&self, bits: &Bits) -> Val {
        let width = bits.width();
        narrow_only(width);
        let mut acc: u128 = 0;
        for (i, &net) in bits.0.iter().enumerate() {
            match self.values.get(&net) {
                Some(v) => acc |= v.lsb() << i,
                None => return Val::unknown(width, false),
            }
        }
        Val::new(acc, width, false)
    }
}

/// Per-bit net reconstruction is `u128`-accumulator-based and genuinely
/// cannot represent more than 128 bits. Fail loudly rather than silently
/// truncate — nothing in v1 needs a wider net vector, and `binary_ctx`/
/// `unary` already handle wide VALUES internally, so only this boundary is
/// deferred.
fn narrow_only(width: u32) {
    if width > 128 {
        unimplemented!(
            "wide (>128-bit) net reconstruction not yet implemented; \
             needs mimz_core::wide's limb helpers"
        );
    }
}

fn bit_of(v: &Val, i: u32) -> u128 {
    match &v.bits {
        crate::bits::Bits::Small(b) => {
            if i >= 128 {
                0
            } else {
                (b >> i) & 1
            }
        }
        crate::bits::Bits::Wide(limbs) => crate::wide::bit_at(limbs, i) as u128,
    }
}
