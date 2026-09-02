//! S-expression IR dump (printer only — no parser; see design doc's
//! "avoid maintaining two parsers for one piece of information" call).

use super::{Bits, Module};
use crate::ir::print_line::{cell_op_name, contiguous_same_name};
use std::fmt::Write;

/// Formats one pin's `Bits` as a bare net name (`a`, `sum`) when
/// [`contiguous_same_name`] finds one, falling back to the same
/// bracketed net-id list as `print_line`'s `format_bits`. Deliberately
/// *not* `print_line::format_bits` reused as-is: that format always
/// appends a `[lo:hi]` width suffix, but the s-expr dump — per the design
/// doc's own example (`(cell $add (a n3) (b n4) (out n5))`) — is meant to
/// read as terse, bare net references, so the positive case renders
/// differently even though the detection is shared.
fn format_pin(module: &Module, bits: &Bits) -> String {
    if bits.0.is_empty() {
        return "{}".to_string();
    }
    match contiguous_same_name(module, bits) {
        Some(name) => name,
        None => {
            let ids: Vec<String> = bits.0.iter().map(|n| n.0.to_string()).collect();
            format!("{{{}}}", ids.join(","))
        }
    }
}

/// Prints `module` as an s-expression dump — one `(port ...)` per port,
/// one `(cell ...)` per cell, pins in a stable (`BTreeMap`-ordered)
/// sequence so output is deterministic and diff-friendly.
pub fn print(module: &Module) -> String {
    let mut out = String::new();
    writeln!(out, "(module {}", module.name).unwrap();
    for (name, bits, dir) in &module.ports {
        let dir_str = match dir {
            crate::ast::Dir::In => "in",
            crate::ast::Dir::Out => "out",
        };
        writeln!(out, "  (port {dir_str} {name} {})", bits.width()).unwrap();
    }
    for cell in &module.cells {
        write!(out, "  (cell {}", cell_op_name(&cell.kind)).unwrap();
        for (pin_name, bits) in &cell.pins {
            write!(out, " ({pin_name} {})", format_pin(module, bits)).unwrap();
        }
        writeln!(out, ")").unwrap();
    }
    writeln!(out, ")").unwrap();
    out
}
