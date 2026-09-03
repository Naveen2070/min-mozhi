//! The Min-Mozhi IR (Phase 2, v1): a typed netlist lowered from the flat
//! `elaborate::Design`. See `docs/plan/phase-2-ir-design.local.md`.

pub mod exec;
pub mod lower;
pub mod parse_line;
pub mod print_line;
pub mod print_sexpr;
pub mod validate;

#[cfg(test)]
mod tests;

use crate::ast::Dir;
use crate::span::Span;
use std::collections::BTreeMap;

/// A single abstract bit in the netlist. Dense, `Copy`, indexes directly
/// into `Module::nets`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetId(pub u32);

/// An ordered bit-vector — one pin's connection. Index 0 is the LSB, by
/// convention shared with `mimz_core::bits`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Bits(pub Vec<NetId>);

impl Bits {
    pub fn width(&self) -> u32 {
        self.0.len() as u32
    }
}

/// Per-net metadata. `name` is `Some(source_name)` when this net
/// corresponds to a real `.mimz` wire/port/reg bit; `None` for a purely
/// synthetic net a lowering pass invented (an intermediate arithmetic
/// result, for instance).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NetInfo {
    pub name: Option<String>,
}

/// Which clock edge a `Dff`/`Mem` write triggers on. Reuses `ast::Edge`'s
/// meaning; kept as a local re-export so `ir` doesn't need `ast::Edge` to
/// stay stable for unrelated reasons.
pub use crate::ast::Edge;

/// v1 cell kinds. Word-level (a cell's pins may be many bits wide) but
/// every pin is still an ordered per-bit `Bits` vector, so adding
/// gate-level (1-bit) cell kinds later is an additive change, not a
/// schema break. See design doc "Rationale for the three cell-kind
/// decisions".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellKind {
    // Lossless arithmetic (ast::BinOp::{Add,Sub,Mul}: result grows).
    Add,
    Sub,
    Mul,
    // Wrapping arithmetic (ast::BinOp::{AddWrap,SubWrap,MulWrap}: keeps width).
    AddWrap,
    SubWrap,
    MulWrap,
    Shl,
    Shr,
    And,
    Or,
    Xor,
    Not,
    RedAnd,
    RedOr,
    RedXor,
    Neg,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    LogicAnd,
    LogicOr,
    LogicNot,
    Mux,
    Concat,
    Slice {
        lo: u32,
        hi: u32,
    },
    Dff {
        clock: NetId,
        edge: Edge,
    },
    /// A memory array: `depth` words, each as wide as the `rdata`/`wdata`
    /// pins, seeded to `init` at power-on (carried as cell metadata per the
    /// design doc — a ROM is exactly a `Mem` whose `wen` is tied low, so the
    /// seed value is the whole of its behaviour).
    Mem {
        depth: u128,
        init: crate::checker::consteval::ConstVal,
    },
    BlackBox {
        module_name: String,
    },
    /// A compile-time-folded constant (an `ExprKind::Int`/`Bool` literal,
    /// or a synthesized register reset value). Zero input pins, one
    /// `out` pin — modeled as an ordinary cell (not a special net
    /// annotation) so validation's "every read net has a driver" check
    /// needs no special case for constants.
    Const {
        value: crate::checker::consteval::ConstVal,
    },
}

/// One cell: an operation reading some pins and writing others, tagged
/// with the source span it came from (diagnostics/differential-debugging
/// need to point back at real `.mimz` source).
#[derive(Clone, Debug)]
pub struct Cell {
    pub kind: CellKind,
    pub pins: BTreeMap<&'static str, Bits>,
    pub span: Span,
}

/// A fully lowered module: nets, ports, and the cells connecting them.
#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub ports: Vec<(String, Bits, Dir)>,
    pub cells: Vec<Cell>,
    pub nets: Vec<NetInfo>,
    /// Declared port shape (name, width) for each distinct extern module a
    /// `BlackBox` cell instantiates, keyed by `CellKind::BlackBox`'s
    /// `module_name` — populated by `lower()` from `design.extern_instances`
    /// (Task 11), consumed by `validate`'s black-box-port-shape check. Not
    /// currently round-tripped by the text format (`print_line`/`parse_line`
    /// don't emit/restore it) — a v1 scope boundary: validating a hand-parsed
    /// IR text file's `BlackBox` cells against declared shape is not yet
    /// possible; `validate` skips the check gracefully (no entry = no error)
    /// rather than treating a missing entry as a violation.
    pub extern_decls: std::collections::BTreeMap<String, Vec<(String, u32)>>,
    /// Every SOURCE-LEVEL signal name (input, output, wire, register Q,
    /// memory read port, extern-instance port net) mapped to the exact `Bits`
    /// it lowered to. Populated by `lower()` from its own resolution table,
    /// which is the only place this is knowable.
    ///
    /// Reconstructing this from `nets` by name does NOT work, and the two
    /// failure modes are silent: `lower()` stamps a wire's name onto only
    /// those of its driver's nets that are still unnamed, so (a) a wire whose
    /// value partly reuses another signal's nets (`wire w = {p, q + 1}`) has
    /// only PART of itself named `w`, and (b) a wire whose driver is a bare
    /// `Ident` gets no nets of its own at all. Anything needing a signal's
    /// real `Bits` — `exec`'s `get_output`, Task 18's differential harness —
    /// must come here, not to a name scan.
    ///
    /// Not round-tripped by the text format (`print_line`/`parse_line` don't
    /// emit/restore it), the same v1 scope boundary as `extern_decls`: a
    /// hand-parsed IR module can only be addressed by port name.
    pub signals: BTreeMap<String, Bits>,
}

impl Module {
    fn alloc_net(&mut self, name: Option<String>) -> NetId {
        let id = NetId(self.nets.len() as u32);
        self.nets.push(NetInfo { name });
        id
    }

    /// Allocates `width` fresh, sequentially-numbered nets, all sharing
    /// `name` (a multi-bit signal's bits all report the same source name;
    /// callers that need per-bit names, e.g. `a[3]`, format at print time,
    /// not here).
    pub(crate) fn alloc_bits(&mut self, width: u32, name: Option<&str>) -> Bits {
        let mut ids = Vec::with_capacity(width as usize);
        for _ in 0..width {
            ids.push(self.alloc_net(name.map(str::to_string)));
        }
        Bits(ids)
    }
}

pub use lower::lower;
