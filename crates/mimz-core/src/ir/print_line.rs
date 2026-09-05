//! Line-based IR text format: one cell per line, hand-writable and
//! diffable. `cell $<op> pin=name[lo:hi] ... -> out=name[lo:hi]`.

use super::{Bits, CellKind, Module};
use std::fmt::Write;

/// Returns the shared source name when every net in `bits` shares that
/// name and is contiguous in `module.nets` (the common case — a whole
/// signal, not a hand-assembled slice); `None` for anything else (empty,
/// unnamed, or non-contiguous). Shared between this module's
/// `name[lo:hi]` rendering and `print_sexpr`'s bare-name rendering — both
/// need the same detection, they just format the positive case
/// differently.
pub(super) fn contiguous_same_name(module: &Module, bits: &Bits) -> Option<String> {
    if bits.0.is_empty() {
        return None;
    }
    let first = bits.0[0];
    let name = module.nets[first.0 as usize].name.clone();
    let is_contiguous = name.is_some()
        && bits
            .0
            .windows(2)
            .all(|w| w[1].0 == w[0].0 + 1 && module.nets[w[1].0 as usize].name == name);
    if is_contiguous { name } else { None }
}

/// Formats one pin's `Bits` as `name[lo:hi]` when [`contiguous_same_name`]
/// finds a shared name; falls back to a bracketed net-id list (`{3,4,7}`)
/// for anything else, so the format never loses information even for a
/// purely synthetic net group.
fn format_bits(module: &Module, bits: &Bits) -> String {
    if bits.0.is_empty() {
        return "{}".to_string();
    }
    match contiguous_same_name(module, bits) {
        Some(name) => format!("{}[0:{}]", name, bits.0.len()),
        None => {
            let ids: Vec<String> = bits.0.iter().map(|n| n.0.to_string()).collect();
            format!("{{{}}}", ids.join(","))
        }
    }
}

fn cmp_op_name(base: &str, signed: bool) -> String {
    if signed {
        format!("{base}[signed]")
    } else {
        base.to_string()
    }
}

pub(super) fn cell_op_name(kind: &CellKind) -> String {
    match kind {
        CellKind::Add => "$add".to_string(),
        CellKind::Sub => "$sub".to_string(),
        CellKind::Mul => "$mul".to_string(),
        CellKind::AddWrap => "$addwrap".to_string(),
        CellKind::SubWrap => "$subwrap".to_string(),
        CellKind::MulWrap => "$mulwrap".to_string(),
        CellKind::Shl => "$shl".to_string(),
        CellKind::Shr => "$shr".to_string(),
        CellKind::And => "$and".to_string(),
        CellKind::Or => "$or".to_string(),
        CellKind::Xor => "$xor".to_string(),
        CellKind::Not => "$not".to_string(),
        CellKind::RedAnd => "$redand".to_string(),
        CellKind::RedOr => "$redor".to_string(),
        CellKind::RedXor => "$redxor".to_string(),
        CellKind::Neg => "$neg".to_string(),
        CellKind::Eq => "$eq".to_string(),
        CellKind::Ne => "$ne".to_string(),
        // Ordering comparisons: unsigned is the bare form (so every IR text
        // written before the `signed` field existed still means what it did),
        // signed gets a `[signed]` bracket argument in the same style as
        // `$dff[Rise]`.
        CellKind::Lt { signed } => cmp_op_name("$lt", *signed),
        CellKind::Le { signed } => cmp_op_name("$le", *signed),
        CellKind::Gt { signed } => cmp_op_name("$gt", *signed),
        CellKind::Ge { signed } => cmp_op_name("$ge", *signed),
        CellKind::LogicAnd => "$logic_and".to_string(),
        CellKind::LogicOr => "$logic_or".to_string(),
        CellKind::LogicNot => "$logic_not".to_string(),
        CellKind::Mux => "$mux".to_string(),
        CellKind::Concat => "$concat".to_string(),
        CellKind::Slice { lo, hi } => format!("$slice[{lo}:{hi}]"),
        CellKind::Dff { edge, .. } => format!("$dff[{edge:?}]"),
        CellKind::Mem { depth, .. } => format!("$mem[{depth}]"),
        CellKind::BlackBox { module_name } => format!("$blackbox[{module_name}]"),
        CellKind::Const { value } => format!(
            "$const[{}'d{}]",
            value.width,
            crate::bits::to_decimal_string(&value.bits)
        ),
    }
}

/// Prints `module` in the line-based format — one line per port, a blank
/// line, then one line per cell, pins in a stable (`BTreeMap`-ordered)
/// sequence so output is deterministic and diff-friendly.
pub fn print(module: &Module) -> String {
    let mut out = String::new();
    writeln!(out, "module {}", module.name).unwrap();
    for (name, bits, dir) in &module.ports {
        let dir_str = match dir {
            crate::ast::Dir::In => "in",
            crate::ast::Dir::Out => "out",
        };
        writeln!(out, "port {dir_str} {name}[0:{}]", bits.width()).unwrap();
    }
    writeln!(out).unwrap();
    for (i, cell) in module.cells.iter().enumerate() {
        let mut pins_str = String::new();
        for (pin_name, bits) in &cell.pins {
            if !pins_str.is_empty() {
                pins_str.push(' ');
            }
            write!(pins_str, "{pin_name}={}", format_bits(module, bits)).unwrap();
        }
        writeln!(out, "cell {} :{i} {pins_str}", cell_op_name(&cell.kind)).unwrap();
    }
    out
}
