//! Defense-in-depth IR validation: re-verifies invariants the AST checker
//! already enforced, catching bugs in the lowering pass itself before the
//! IR is trusted as equivalent to the source.
//!
//! Six checks, kept in six clearly separated passes over `module`:
//! driver/undriven nets, fixed-width pin contracts (e.g. `Mux.sel`),
//! same-width a/b pairs on bitwise/comparison/logical cells, combinational
//! cycles, black-box port shape against declared extern ports
//! (`Module::extern_decls`), and output port width against its source
//! declaration (`Module::port_declared_widths`). Every check appends
//! independently to the same `errors` list — one check finding something
//! never skips another.

use super::{CellKind, Module, NetId};
use std::collections::{HashMap, HashSet};

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    MultipleDrivers {
        net: NetId,
        cell_indices: Vec<usize>,
    },
    UndrivenNet {
        net: NetId,
    },
    WidthMismatch {
        cell_index: usize,
        pin: &'static str,
        expected: u32,
        found: u32,
    },
    CombinationalCycle {
        nets: Vec<NetId>,
    },
    BlackBoxPortMismatch {
        cell_index: usize,
        reason: String,
    },
    PortWidthMismatch {
        port: String,
        declared: u32,
        found: u32,
    },
}

/// Input pin names per `CellKind` and their required relationship to
/// output width — kept as one match so every cell kind's contract lives
/// in exactly one place (this table, mirrored by `exec.rs`'s evaluator).
/// Covers two shapes: a fixed constant (`Mux.sel` is always 1 bit) and a
/// formula over the cell's OWN other pins (`Dff.q` mirrors `d`'s width;
/// `Add`/`Sub`/`Mul`/`*Wrap`'s `out` mirrors `lower.rs`'s `lower_binop`
/// growth/wrap formula exactly — the two must never drift, since this
/// check exists specifically to catch a lowering bug that made them
/// disagree). `Shr` is absent: `lower_binop` defines its `out` width as
/// simply `a.width()` (right-shift never grows, `width_rules::
/// shift_result`'s own `grows: false` rule), so there's no independent
/// formula to cross-check against — any corruption of `out` would just as
/// trivially corrupt the "expected" value derived from `a` here, catching
/// nothing. `Shl` DOES have an independent formula
/// (`width_rules::shift_result` with `grows: true`, exact `const_amount`
/// when `b` is driven by a compile-time constant, worst-case
/// `const_amount: None` otherwise — see `lower_binop` and
/// `shl_const_amount` below) and is checked below.
fn expected_widths(
    kind: &CellKind,
    pins: &std::collections::BTreeMap<&'static str, super::Bits>,
    module: &Module,
    driver: &HashMap<NetId, Vec<usize>>,
) -> Vec<(&'static str, u32)> {
    match kind {
        CellKind::Mux => vec![("sel", 1)],
        CellKind::Dff { .. } => vec![("q", pins["d"].width())],
        CellKind::Add | CellKind::Sub => {
            vec![("out", pins["a"].width().max(pins["b"].width()) + 1)]
        }
        CellKind::Mul => vec![("out", pins["a"].width() + pins["b"].width())],
        CellKind::AddWrap | CellKind::SubWrap | CellKind::MulWrap => {
            vec![("out", pins["a"].width().max(pins["b"].width()))]
        }
        CellKind::Shl => {
            let out_width = crate::width_rules::shift_result(
                crate::width_rules::Kind {
                    width: pins["a"].width(),
                    signed: false,
                },
                crate::width_rules::Kind {
                    width: pins["b"].width(),
                    signed: false,
                },
                shl_const_amount(module, driver, &pins["b"]),
                true,
            )
            .expect(
                "Shl growth exceeded MAX_WIDTH — same pathological-input panic as \
                 ir::lower's identical formula (see docs/audit/gaps.md GAP-1); this \
                 validation pass exists to REPORT malformed IR, not crash on it, but this \
                 specific case has no fixture exercising it today, so it hasn't needed a \
                 non-panicking path yet",
            )
            .width;
            vec![("out", out_width)]
        }
        _ => Vec::new(), // remaining binary/unary cells only constrain relationships between their OWN pins, not a fixed output-width formula — checked separately below
    }
}

/// If `b` (a `Shl` cell's shift-amount pin) is driven ENTIRELY by one
/// `Const` cell — the shape `ir::lower` produces for a compile-time-
/// constant shift amount — return that constant's value so this
/// independently-computed formula sizes `out` exactly the same way
/// `lower_binop` did, instead of drifting to worst-case sizing the moment
/// `lower_binop` learned to do better (GAP-1's "narrower than originally
/// scoped" residual). Anything else — a runtime `b`, a constant reached
/// only indirectly (e.g. through a `Concat`/`Slice`), or a constant too
/// wide for `u128` — falls back to `None` (worst-case), which is always
/// the safe/wide side of `width_rules::shift_result`.
fn shl_const_amount(
    module: &Module,
    driver: &HashMap<NetId, Vec<usize>>,
    b: &super::Bits,
) -> Option<u128> {
    let &cell_idx = b.0.first().and_then(|n| driver.get(n))?.first()?;
    let single_driver = [cell_idx];
    if !b
        .0
        .iter()
        .all(|n| driver.get(n).map(Vec::as_slice) == Some(&single_driver[..]))
    {
        return None;
    }
    let CellKind::Const { value } = &module.cells.get(cell_idx)?.kind else {
        return None;
    };
    if module.cells[cell_idx].pins.get("out") != Some(b) {
        return None;
    }
    match &value.bits {
        crate::bits::Bits::Small(v) => Some(*v),
        crate::bits::Bits::Wide(_) => None,
    }
}

/// `CellKind`s whose `a`/`b` pins must be the SAME width: bitwise
/// (`And`/`Or`/`Xor`), comparison (`Eq`/`Ne`/`Lt`/`Le`/`Gt`/`Ge`), and
/// logical (`LogicAnd`/`LogicOr`) ops — mirroring
/// `width_rules::matched_result`'s contract at the AST level.
/// `Add`/`Sub`/`Mul`/`*Wrap`/`Shl`/`Shr` are deliberately absent: per
/// `width_rules::lossless_result` and `lower.rs`'s own `lower_binop`,
/// those ops either legitimately allow differing operand widths (lossless
/// growth) or don't define an a/b width relationship at all (shifts), so
/// flagging them here would be a false positive. `Mux`'s `a`/`b` are also
/// excluded on purpose — `lower.rs` widens `Mux`'s output to
/// `a.width().max(b.width())`, so a `Mux` legitimately takes differently
/// sized arms.
fn requires_matched_ab(kind: &CellKind) -> bool {
    matches!(
        kind,
        CellKind::And
            | CellKind::Or
            | CellKind::Xor
            | CellKind::Eq
            | CellKind::Ne
            | CellKind::Lt
            | CellKind::Le
            | CellKind::Gt
            | CellKind::Ge
            | CellKind::LogicAnd
            | CellKind::LogicOr
    )
}

pub fn validate(module: &Module) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut driver: HashMap<NetId, Vec<usize>> = HashMap::new();
    let mut driven: HashSet<NetId> = HashSet::new();

    // --- Check 1: multiple-drivers / undriven-net --------------------
    for (i, cell) in module.cells.iter().enumerate() {
        for (pin_name, bits) in &cell.pins {
            let is_output = matches!(*pin_name, "out" | "q" | "rdata");
            if is_output {
                for net in &bits.0 {
                    driver.entry(*net).or_default().push(i);
                    driven.insert(*net);
                }
            }
        }
    }
    for (name, bits, dir) in &module.ports {
        let _ = name;
        // Only an INPUT port is a primary driver — an output port's nets
        // must actually be driven by some cell's `out`/`q`/`rdata` pin,
        // exactly like any other net. The previous version of this loop
        // destructured `_dir` and never read it, so an `out` port's nets
        // were marked "driven" unconditionally, letting a module with a
        // declared but never-written output port pass validation silently
        // (docs/audit/gaps.md GAP-1's "driven-set seeding is
        // direction-blind" sub-gap).
        if *dir == crate::ast::Dir::In {
            for net in &bits.0 {
                driven.insert(*net);
            }
        }
    }

    for (&net, cells) in &driver {
        if cells.len() > 1 {
            errors.push(ValidationError::MultipleDrivers {
                net,
                cell_indices: cells.clone(),
            });
        }
    }
    // Every net the module has allocated must have a driver — not just
    // ones some pin happens to read. A net that's neither an input port
    // nor any cell's output (e.g. a lowering bug that repointed a cell's
    // `out` pin elsewhere, orphaning the net it used to drive) is exactly
    // the class of bug this defense-in-depth pass exists to catch, read
    // or not.
    for i in 0..module.nets.len() {
        let net = NetId(i as u32);
        if !driven.contains(&net) {
            errors.push(ValidationError::UndrivenNet { net });
        }
    }

    // --- Checks 2 & 3: fixed-width contracts + same-width a/b pairs --
    for (i, cell) in module.cells.iter().enumerate() {
        for (pin_name, expected_width) in expected_widths(&cell.kind, &cell.pins, module, &driver) {
            let found = cell.pins[pin_name].width();
            if found != expected_width {
                errors.push(ValidationError::WidthMismatch {
                    cell_index: i,
                    pin: pin_name,
                    expected: expected_width,
                    found,
                });
            }
        }
        if requires_matched_ab(&cell.kind)
            && let (Some(a_bits), Some(b_bits)) = (cell.pins.get("a"), cell.pins.get("b"))
        {
            let (expected, found) = (a_bits.width(), b_bits.width());
            if expected != found {
                errors.push(ValidationError::WidthMismatch {
                    cell_index: i,
                    pin: "b",
                    expected,
                    found,
                });
            }
        }
    }

    // --- Check 4: combinational cycle ---------------------------------
    if let Some(cycle) = find_combinational_cycle(module) {
        errors.push(ValidationError::CombinationalCycle { nets: cycle });
    }

    // --- Check 5: black-box port shape ---------------------------------
    for (i, cell) in module.cells.iter().enumerate() {
        let CellKind::BlackBox { module_name } = &cell.kind else {
            continue;
        };
        // v1 text-format gap (see `Module::extern_decls` doc): no entry
        // means this module_name's declared shape isn't on record — skip
        // rather than treat a missing entry as a violation.
        let Some(decl) = module.extern_decls.get(module_name) else {
            continue;
        };
        for (port_name, expected_width) in decl {
            match cell.pins.get(port_name.as_str()) {
                None => errors.push(ValidationError::BlackBoxPortMismatch {
                    cell_index: i,
                    reason: format!(
                        "declared port `{port_name}` ({expected_width} bits) is missing from the instance"
                    ),
                }),
                Some(bits) if bits.width() != *expected_width => {
                    errors.push(ValidationError::BlackBoxPortMismatch {
                        cell_index: i,
                        reason: format!(
                            "port `{port_name}`: declared {expected_width} bits, found {} bits",
                            bits.width()
                        ),
                    })
                }
                _ => {}
            }
        }
        let declared_names: HashSet<&str> = decl.iter().map(|(n, _)| n.as_str()).collect();
        for pin_name in cell.pins.keys() {
            if !declared_names.contains(pin_name) {
                errors.push(ValidationError::BlackBoxPortMismatch {
                    cell_index: i,
                    reason: format!("pin `{pin_name}` is not a declared port of `{module_name}`"),
                });
            }
        }
    }

    // --- Check 6: output port width matches its source declaration ----
    for (name, bits, dir) in &module.ports {
        if *dir != crate::ast::Dir::Out {
            continue;
        }
        if let Some(&declared) = module.port_declared_widths.get(name)
            && bits.width() != declared
        {
            errors.push(ValidationError::PortWidthMismatch {
                port: name.clone(),
                declared,
                found: bits.width(),
            });
        }
    }

    errors
}

/// DFS for a cycle among cell input->output edges, treating a `Dff`/`Mem`
/// cell's `d`/`wdata` -> `q`/`rdata` edge as ABSENT (a register breaks
/// the combinational path by definition) — everything else's every
/// input pin has an edge to every output pin.
///
/// Classic white/grey/black DFS: `on_stack` marks nodes on the CURRENT
/// path (grey), `visited` marks nodes fully explored with no cycle found
/// (black) so they're never re-walked from a later start node. (The
/// brief's original starter code tracked only one global "seen" set and
/// checked `path.contains(&node)` — trivially true, since `path` always
/// ends in `node` itself — so it flagged a false cycle on any node
/// reached twice at all, e.g. two input nets of one plain `Add` cell
/// fanning into the same output width; that broke even the accepting
/// adder-module test, so this rewrite keeps the same edge-building shape
/// but fixes the traversal.)
// ponytail: recursive DFS, fine for the module sizes v1 targets — switch
// to Tarjan's SCC (or an explicit stack) if a large design's recursion
// depth ever becomes a problem.
fn find_combinational_cycle(module: &Module) -> Option<Vec<NetId>> {
    let mut edges: HashMap<NetId, Vec<NetId>> = HashMap::new();
    for cell in &module.cells {
        if matches!(cell.kind, CellKind::Dff { .. } | CellKind::Mem { .. }) {
            continue; // sequential cells break combinational cycles by construction
        }
        let inputs: Vec<NetId> = cell
            .pins
            .iter()
            .filter(|(name, _)| !matches!(**name, "out" | "q" | "rdata"))
            .flat_map(|(_, bits)| bits.0.iter().copied())
            .collect();
        let outputs: Vec<NetId> = cell
            .pins
            .iter()
            .filter(|(name, _)| matches!(**name, "out" | "q" | "rdata"))
            .flat_map(|(_, bits)| bits.0.iter().copied())
            .collect();
        for &i in &inputs {
            edges.entry(i).or_default().extend(outputs.iter().copied());
        }
    }

    fn dfs(
        node: NetId,
        edges: &HashMap<NetId, Vec<NetId>>,
        visited: &mut HashSet<NetId>,
        on_stack: &mut HashSet<NetId>,
        path: &mut Vec<NetId>,
    ) -> Option<Vec<NetId>> {
        if on_stack.contains(&node) {
            let start = path.iter().position(|&n| n == node).unwrap();
            let mut cycle = path[start..].to_vec();
            cycle.push(node);
            return Some(cycle);
        }
        if visited.contains(&node) {
            return None;
        }
        on_stack.insert(node);
        path.push(node);
        if let Some(next) = edges.get(&node) {
            for &n in next {
                if let Some(cycle) = dfs(n, edges, visited, on_stack, path) {
                    return Some(cycle);
                }
            }
        }
        path.pop();
        on_stack.remove(&node);
        visited.insert(node);
        None
    }

    let mut visited = HashSet::new();
    let mut on_stack = HashSet::new();
    let mut path = Vec::new();
    for &start in edges.keys() {
        if !visited.contains(&start)
            && let Some(cycle) = dfs(start, &edges, &mut visited, &mut on_stack, &mut path)
        {
            return Some(cycle);
        }
    }
    None
}
