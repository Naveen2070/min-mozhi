//! Parses the line-based IR text format (see `print_line`) back into a
//! `Module`. Deliberately simple (line-oriented, whitespace-split) —
//! this format exists to be hand-writable, so its parser has to be
//! forgiving of exactly the shape a human would type, not a full
//! tokenizer/grammar.

use super::{Bits, Cell, CellKind, Module, NetId, NetInfo};
use crate::ast::Dir;
use std::collections::{BTreeMap, HashMap};

pub fn parse(text: &str) -> Result<Module, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let module_line = lines.next().ok_or("empty input")?;
    let name = module_line
        .strip_prefix("module ")
        .ok_or("expected `module <name>` on the first line")?
        .trim()
        .to_string();

    let mut module = Module {
        name,
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: BTreeMap::new(), // text format doesn't round-trip declared extern shapes (v1 gap, see Module::extern_decls doc)
        signals: BTreeMap::new(), // nor the source-name -> Bits table (same v1 gap, see Module::signals doc)
        port_declared_widths: BTreeMap::new(), // nor each output's declared width (same v1 gap, see Module::port_declared_widths doc)
    };
    // Bracket-form pin values (`{17,18,19}`) embed the ORIGINAL module's
    // own NetId numbers as literal text, unlike a name-form reference —
    // reprinting one has to reproduce those SAME numbers verbatim, since
    // there's no symbolic name to regenerate them from under a different
    // numbering. Pre-reserve nets `0..=max` so every `{N}` this parse
    // meets can be used directly as `NetId(N)` instead of needing a
    // remap; every OTHER net this parser allocates (ports, name-form
    // pins, fabricated Dff/Mem placeholders) is appended after this
    // reserved range via `alloc_net`/`alloc_bits`, so it can never
    // collide with a literal bracket id.
    if let Some(max_id) = max_bracket_net_id(text) {
        // Cap before allocating: `max_id` comes straight from untrusted
        // text, and `vec![NetInfo::default(); max_id + 1]` is otherwise an
        // attacker/typo-controlled OOM (`{4294967295}` alone would ask for
        // ~100GB). No real hand-written or generated IR text (this repo's
        // largest designs are a tiny fraction of this) references a net id
        // anywhere near this high, so anything past it is malformed input.
        const MAX_PLAUSIBLE_NET_ID: u32 = 10_000_000;
        if max_id > MAX_PLAUSIBLE_NET_ID {
            return Err(format!(
                "bracket-form net id {max_id} exceeds the plausible maximum ({MAX_PLAUSIBLE_NET_ID}) — malformed input"
            ));
        }
        module.nets = vec![NetInfo::default(); max_id as usize + 1];
    }
    let mut named_bits: HashMap<String, Bits> = HashMap::new();

    for line in lines {
        if let Some(rest) = line.strip_prefix("port ") {
            let mut parts = rest.split_whitespace();
            let dir = match parts.next().ok_or("expected port direction")? {
                "in" => Dir::In,
                "out" => Dir::Out,
                other => return Err(format!("unknown port direction `{other}`")),
            };
            let spec = parts.next().ok_or("expected `name[0:width]`")?;
            let (name, width) = parse_name_width(spec)?;
            // `print`'s port line (`port {dir} {name}[0:{width}]`) only
            // ever consults `bits.width()` — never the underlying nets'
            // names — so a port's own bits carry no naming commitment.
            // Whether the underlying net counts as "named" for a LATER
            // cell-pin reference to reprint correctly is decided lazily,
            // the first time that reference actually shows up in
            // name-form (see `resolve_bits_spec`); allocating a name here
            // unconditionally would wrongly force name-form on a cell pin
            // whose original net was genuinely unnamed (e.g. an output
            // port that aliases a mux's own synthetic result net).
            let bits = module.alloc_bits(width, None);
            named_bits.insert(name.clone(), bits.clone());
            module.ports.push((name, bits, dir));
        } else if let Some(rest) = line.strip_prefix("cell ") {
            let mut parts = rest.split_whitespace();
            let op = parts.next().ok_or("expected a cell op, e.g. $add")?;
            let _index_tag = parts.next(); // the `:N` tag — human-readable only, not re-parsed into meaning
            let mut pins = BTreeMap::new();
            for pin_spec in parts {
                let (pin_name, bits_spec) = pin_spec
                    .split_once('=')
                    .ok_or_else(|| format!("expected `pin=value`, got `{pin_spec}`"))?;
                let bits = resolve_bits_spec(&mut module, &mut named_bits, bits_spec)?;
                pins.insert(leak_pin_name(pin_name), bits);
            }
            let kind = parse_cell_kind(&mut module, op, &pins)?;
            module.cells.push(Cell {
                kind,
                pins,
                span: crate::span::Span::default(),
            });
        } else {
            return Err(format!("unrecognized line: `{line}`"));
        }
    }
    Ok(module)
}

/// Scans every `cell` line's pin values for bracket-form net-id lists
/// (`{17,18,19}`) and returns the largest id seen anywhere, so `parse`
/// can pre-size `module.nets` to cover them all before allocating
/// anything else. `None` when no cell line uses bracket form at all.
fn max_bracket_net_id(text: &str) -> Option<u32> {
    let mut max: Option<u32> = None;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("cell ") else {
            continue;
        };
        for tok in rest.split_whitespace() {
            // The op and `:N` tokens never contain `=`, so this also
            // naturally skips them without needing to special-case them.
            let Some((_, bits_spec)) = tok.split_once('=') else {
                continue;
            };
            let Some(inner) = bits_spec
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
            else {
                continue;
            };
            for id_str in inner.split(',').filter(|s| !s.is_empty()) {
                if let Ok(id) = id_str.parse::<u32>() {
                    max = Some(max.map_or(id, |m| m.max(id)));
                }
            }
        }
    }
    max
}

fn parse_name_width(spec: &str) -> Result<(String, u32), String> {
    let (name, range) = spec
        .split_once('[')
        .ok_or_else(|| format!("expected `name[lo:hi]`, got `{spec}`"))?;
    let range = range.trim_end_matches(']');
    let (_lo, hi) = range
        .split_once(':')
        .ok_or_else(|| format!("expected `lo:hi` inside brackets, got `{range}`"))?;
    let width: u32 = hi.parse().map_err(|_| format!("bad width in `{spec}`"))?;
    Ok((name.to_string(), width))
}

fn resolve_bits_spec(
    module: &mut Module,
    named_bits: &mut HashMap<String, Bits>,
    spec: &str,
) -> Result<Bits, String> {
    if let Some(inner) = spec.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        if inner.is_empty() {
            return Ok(Bits(Vec::new()));
        }
        // Safe to use these literal ids directly as `NetId`s (rather than
        // remapping) because `parse` pre-sized `module.nets` to cover
        // every one that appears anywhere in the text — see the comment
        // there.
        let ids: Result<Vec<NetId>, String> = inner
            .split(',')
            .map(|s| {
                s.parse::<u32>()
                    .map(NetId)
                    .map_err(|_| format!("bad net id `{s}`"))
            })
            .collect();
        return Ok(Bits(ids?));
    }
    let (name, width) = parse_name_width(spec)?;
    let bits = match named_bits.get(&name) {
        Some(existing) => existing.clone(),
        None => {
            // A pin referencing a net group that hasn't been declared as
            // a port yet (a purely internal/synthetic net a cell writes
            // for the first time as an output pin) — allocate it fresh
            // and remember it so a LATER line reading the same name
            // resolves to the same nets.
            let bits = module.alloc_bits(width, Some(&name));
            named_bits.insert(name.clone(), bits.clone());
            bits
        }
    };
    // A name-form reference (as opposed to a bracket-form one) is exactly
    // the condition `print_line::format_bits` uses to choose name-form
    // over bracket-form on the next print — set it every time, not just
    // on first allocation: a port's own bits are allocated unnamed (port
    // lines never encode net identity, only width — see the `port`
    // branch above), so this may be the sighting that first earns the
    // name for nets a port allocated earlier.
    for id in &bits.0 {
        module.nets[id.0 as usize].name = Some(name.clone());
    }
    Ok(bits)
}

fn leak_pin_name(name: &str) -> &'static str {
    // Pin names lowering actually produces are a small fixed set (see
    // `lower.rs`) — match against that set and return the matching
    // 'static str so `Cell::pins`'s `BTreeMap<&'static str, Bits>`
    // doesn't need owned `String` keys just for the parser's sake.
    match name {
        "a" => "a",
        "b" => "b",
        "out" => "out",
        "sel" => "sel",
        "d" => "d",
        "q" => "q",
        "clock" => "clock",
        "raddr" => "raddr",
        "waddr" => "waddr",
        "rdata" => "rdata",
        "wdata" => "wdata",
        "wen" => "wen",
        // `BlackBox` cells are the one exception: their pin names are the
        // extern module's own port names, arbitrary and known only at
        // lowering/print time — leak a fresh 'static copy rather than
        // reject them (mirrors `lower.rs`'s own `Box::leak` for exactly
        // the same reason).
        other => Box::leak(other.to_string().into_boxed_str()),
    }
}

fn parse_cell_kind(
    module: &mut Module,
    op: &str,
    pins: &BTreeMap<&'static str, Bits>,
) -> Result<CellKind, String> {
    // Mirrors print_line::cell_op_name's mapping, in reverse, in the same
    // order: plain no-arg ops first, then the bracketed-argument ops
    // ($slice[lo:hi], $dff[Rise|Fall], $mem[N], $blackbox[Name],
    // $const[width'dvalue]).
    Ok(match op {
        "$add" => CellKind::Add,
        "$sub" => CellKind::Sub,
        "$mul" => CellKind::Mul,
        "$addwrap" => CellKind::AddWrap,
        "$subwrap" => CellKind::SubWrap,
        "$mulwrap" => CellKind::MulWrap,
        "$shl" => CellKind::Shl,
        "$shr" => CellKind::Shr,
        "$and" => CellKind::And,
        "$or" => CellKind::Or,
        "$xor" => CellKind::Xor,
        "$not" => CellKind::Not,
        "$redand" => CellKind::RedAnd,
        "$redor" => CellKind::RedOr,
        "$redxor" => CellKind::RedXor,
        "$neg" => CellKind::Neg,
        "$eq" => CellKind::Eq,
        "$ne" => CellKind::Ne,
        "$lt" => CellKind::Lt,
        "$le" => CellKind::Le,
        "$gt" => CellKind::Gt,
        "$ge" => CellKind::Ge,
        "$logic_and" => CellKind::LogicAnd,
        "$logic_or" => CellKind::LogicOr,
        "$logic_not" => CellKind::LogicNot,
        "$mux" => CellKind::Mux,
        "$concat" => CellKind::Concat,

        other if other.starts_with("$slice[") => {
            let inner = bracket_arg(other, "$slice[")?;
            let (lo, hi) = inner
                .split_once(':')
                .ok_or_else(|| format!("expected `lo:hi` inside `{other}`"))?;
            let lo: u32 = lo
                .parse()
                .map_err(|_| format!("bad slice lo in `{other}`"))?;
            let hi: u32 = hi
                .parse()
                .map_err(|_| format!("bad slice hi in `{other}`"))?;
            CellKind::Slice { lo, hi }
        }
        other if other.starts_with("$dff[") => {
            let inner = bracket_arg(other, "$dff[")?;
            let edge = match inner {
                "Rise" => crate::ast::Edge::Rise,
                "Fall" => crate::ast::Edge::Fall,
                other_edge => {
                    return Err(format!("unknown clock edge `{other_edge}` in `{other}`"));
                }
            };
            // `Dff::clock` is a `NetId` struct field, never printed by
            // `print_line` (only `edge` is) — a known, pre-approved
            // text-format lossiness (see Task 13 brief). Fabricate a
            // fresh net; the round-trip text can't observe this because
            // `cell_op_name` never reads `clock` back.
            let clock = module.alloc_net(None);
            CellKind::Dff { clock, edge }
        }
        other if other.starts_with("$mem[") => {
            let inner = bracket_arg(other, "$mem[")?;
            let depth: u128 = inner
                .parse()
                .map_err(|_| format!("bad mem depth in `{other}`"))?;
            // `Mem::init` is likewise never printed by `print_line` (only
            // `depth` is) — same known, pre-approved lossiness. Fabricate
            // a zero-valued placeholder at the cell's own data width
            // (read off the already-parsed `rdata` pin); print never
            // reads `init` back so this can't desync the round-trip text.
            // A `$mem[...]` line missing `rdata` entirely is itself
            // malformed (every real `Mem` cell has one) — falls back to
            // width 1 rather than erroring here; `validate.rs` (a later
            // task) is the place that's meant to catch a pinless Mem cell.
            let width = pins.get("rdata").map(|b| b.width()).unwrap_or(1);
            let init = crate::checker::consteval::ConstVal {
                bits: crate::bits::Bits::Small(0),
                width,
                signed: false,
            };
            CellKind::Mem { depth, init }
        }
        other if other.starts_with("$blackbox[") => {
            let module_name = bracket_arg(other, "$blackbox[")?.to_string();
            CellKind::BlackBox { module_name }
        }
        other if other.starts_with("$const[") => {
            // `$const[<width>'d<decimal>]` — parse both fields back out.
            let inner = bracket_arg(other, "$const[")?;
            let (width_str, dec) = inner
                .split_once("'d")
                .ok_or_else(|| format!("malformed const literal `{other}`"))?;
            let width: u32 = width_str
                .parse()
                .map_err(|_| format!("bad const width in `{other}`"))?;
            let value: u128 = dec.parse().map_err(|_| format!("bad const value in `{other}` (wide constants beyond u128 aren't parseable yet — extend this once Task 12's to_decimal_string's wide path has a matching from-decimal-string parser)"))?;
            CellKind::Const {
                value: crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(value),
                    width,
                    signed: false,
                },
            }
        }
        other => {
            return Err(format!(
                "unrecognized (or not-yet-parseable) cell op `{other}` — extend parse_cell_kind to match print_line::cell_op_name's full list"
            ));
        }
    })
}

/// Strips `prefix` and a trailing `]` off a bracketed op like
/// `$slice[3:9]`, returning just the inner text (`3:9`).
fn bracket_arg<'a>(op: &'a str, prefix: &str) -> Result<&'a str, String> {
    op.strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| format!("malformed bracketed op `{op}`"))
}
