//! Synthesis backends (synthesis v1 design, section 3): pure functions from
//! a validated, optimized `ir::Module` to the text a synthesis tool reads.
//! No file or process access; `mimz build` (shell crate) does the I/O.

pub mod verilog;

#[cfg(test)]
mod tests;

use crate::ast::Dir;
use crate::ir::{Bits, Cell, CellKind, Module, NetId};
use std::collections::{HashMap, HashSet};

/// Where a net's value comes from: bit `bit` of input port `port`, or bit
/// `bit` of cell `cell`'s output pin `pin` (`rdataN` for a memory read port).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Port { port: usize, bit: u32 },
    Cell { cell: usize, pin: String, bit: u32 },
}

/// A cell's output pins: `out`/`q` for ordinary cells, `rdataN` for a
/// memory's read ports, a black box's declared outputs.
pub(crate) fn output_pins<'a>(m: &'a Module, cell: &'a Cell) -> Vec<(String, &'a Bits)> {
    match &cell.kind {
        CellKind::Mem { read_ports, .. } => read_ports
            .iter()
            .enumerate()
            .map(|(i, (_, rdata))| (format!("rdata{i}"), rdata))
            .collect(),
        CellKind::BlackBox { module_name, .. } => {
            let decl = m.extern_decls.get(module_name).unwrap_or_else(|| {
                crate::ir::failure::limitation(format!(
                    "black box `{module_name}` has no declared ports (IR parsed from text?)"
                ))
            });
            decl.iter()
                .filter(|(_, _, dir)| *dir == Dir::Out)
                .filter_map(|(n, _, _)| cell.pins.get(n.as_str()).map(|b| (n.clone(), b)))
                .collect()
        }
        _ => cell
            .pins
            .iter()
            .filter(|(p, _)| matches!(**p, "out" | "q"))
            .map(|(p, b)| (p.to_string(), b))
            .collect(),
    }
}

/// The driver of every driven net (`validate` guarantees one per read net).
pub(crate) fn drivers(m: &Module) -> HashMap<NetId, Source> {
    let mut d = HashMap::new();
    for (port, (_, bits, dir)) in m.ports.iter().enumerate() {
        if *dir == Dir::In {
            for (bit, n) in bits.nets.iter().enumerate() {
                d.insert(
                    *n,
                    Source::Port {
                        port,
                        bit: bit as u32,
                    },
                );
            }
        }
    }
    for (cell, c) in m.cells.iter().enumerate() {
        for (pin, bits) in output_pins(m, c) {
            for (bit, n) in bits.nets.iter().enumerate() {
                d.insert(
                    *n,
                    Source::Cell {
                        cell,
                        pin: pin.clone(),
                        bit: bit as u32,
                    },
                );
            }
        }
    }
    d
}

/// Every reserved word of IEEE 1364-2005 (Annex B), sorted.
#[rustfmt::skip]
pub(crate) const VERILOG_KEYWORDS: &[&str] = &[
    "always", "and", "assign", "automatic", "begin", "buf", "bufif0", "bufif1",
    "case", "casex", "casez", "cell", "cmos", "config", "deassign", "default",
    "defparam", "design", "disable", "edge", "else", "end", "endcase",
    "endconfig", "endfunction", "endgenerate", "endmodule", "endprimitive",
    "endspecify", "endtable", "endtask", "event", "for", "force", "forever",
    "fork", "function", "generate", "genvar", "highz0", "highz1", "if",
    "ifnone", "incdir", "include", "initial", "inout", "input", "instance",
    "integer", "join", "large", "liblist", "library", "localparam",
    "macromodule", "medium", "module", "nand", "negedge", "nmos", "nor",
    "noshowcancelled", "not", "notif0", "notif1", "or", "output", "parameter",
    "pmos", "posedge", "primitive", "pull0", "pull1", "pulldown", "pullup",
    "pulsestyle_ondetect", "pulsestyle_onevent", "rcmos", "real", "realtime",
    "reg", "release", "repeat", "rnmos", "rpmos", "rtran", "rtranif0",
    "rtranif1", "scalared", "showcancelled", "signed", "small", "specify",
    "specparam", "strong0", "strong1", "supply0", "supply1", "table", "task",
    "time", "tran", "tranif0", "tranif1", "tri", "tri0", "tri1", "triand",
    "trior", "trireg", "unsigned", "use", "uwire", "vectored", "wait", "wand",
    "weak0", "weak1", "while", "wire", "wor", "xnor", "xor",
];

/// A legal, unique Verilog identifier for `name`: Tamil romanized with the
/// AST emitter's scheme, other illegal characters `_`, a leading digit
/// prefixed `_`, a reserved word suffixed `_`, then `_1`, `_2`, ... until
/// unused in `used`.
pub fn legal_name(name: &str, used: &mut HashSet<String>) -> String {
    let romanized = crate::emit_verilog::translit::romanize(name);
    let mut s: String = romanized
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    if VERILOG_KEYWORDS.contains(&s.as_str()) {
        s.push('_');
    }
    let mut candidate = s.clone();
    let mut k = 1;
    while !used.insert(candidate.clone()) {
        candidate = format!("{s}_{k}");
        k += 1;
    }
    candidate
}
