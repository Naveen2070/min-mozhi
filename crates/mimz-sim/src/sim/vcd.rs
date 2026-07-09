//! Hand-written IEEE-1364 **2-state** VCD writer (Phase 1.5, step B5).
//!
//! No external crate (decision 2026-06-16): VCD is a small ASCII format and we
//! want full control over signal naming + scope. Consumes a [`Timeline`]; the
//! output opens in GTKWave and is validated against Icarus by the differential
//! suite (B8). 2-state only — every value is `0`/`1` bits, never `x`/`z`.

use std::collections::BTreeMap;

use super::run::Timeline;

/// Render `tl` as a VCD document.
pub fn to_vcd(tl: &Timeline) -> String {
    // Assign each signal a short identifier code (printable ASCII, base-94).
    let mut ids: BTreeMap<String, (String, u32)> = BTreeMap::new();
    let mut out = String::new();
    out.push_str("$timescale 1ns $end\n");
    out.push_str(&format!("$scope module {} $end\n", tl.module));
    for (i, sig) in tl.signals.iter().enumerate() {
        let id = id_code(i);
        out.push_str(&format!(
            "$var wire {} {} {} $end\n",
            sig.width.bits, id, sig.name
        ));
        ids.insert(sig.name.clone(), (id, sig.width.bits));
    }
    out.push_str("$upscope $end\n");
    out.push_str("$enddefinitions $end\n");

    // Initial full dump, then only changed values at each later timestamp.
    let mut prev: BTreeMap<String, u128> = BTreeMap::new();
    for (fi, frame) in tl.frames.iter().enumerate() {
        out.push_str(&format!("#{}\n", frame.time));
        if fi == 0 {
            out.push_str("$dumpvars\n");
        }
        for sig in &tl.signals {
            let v = *frame.values.get(&sig.name).unwrap_or(&0);
            if fi == 0 || prev.get(&sig.name) != Some(&v) {
                let (id, width) = &ids[&sig.name];
                out.push_str(&fmt_value(*width, v, id));
                out.push('\n');
            }
            prev.insert(sig.name.clone(), v);
        }
        if fi == 0 {
            out.push_str("$end\n");
        }
    }
    out
}

/// A VCD scalar (`0!`/`1!`) or vector (`b1010 !`) value-change line. `value` is
/// already masked to `width` by the snapshot, so it needs no further masking.
fn fmt_value(width: u32, value: u128, id: &str) -> String {
    if width <= 1 {
        format!("{}{}", value & 1, id)
    } else {
        format!("b{value:b} {id}")
    }
}

/// A unique identifier code for signal index `i`: positional base-94 over the
/// printable ASCII range `!`..`~` (33..=126), least-significant char first.
fn id_code(mut i: usize) -> String {
    const BASE: usize = 94;
    let mut s = String::new();
    loop {
        s.push((33 + (i % BASE) as u8) as char);
        i /= BASE;
        if i == 0 {
            break;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::elaborate::elaborate;
    use crate::sim::run::{SimOpts, run};
    use std::collections::BTreeMap;

    fn counter_vcd(cycles: u64) -> String {
        let src = "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  \
                   out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
                   on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let d = elaborate(&f, None, &BTreeMap::new()).expect("elaborates");
        let tl = run(
            d,
            &SimOpts {
                clock: None,
                inputs: BTreeMap::new(),
                cycles,
                reset_cycles: 1,
            },
        )
        .expect("runs");
        to_vcd(&tl)
    }

    #[test]
    fn header_scope_and_vars_present() {
        let v = counter_vcd(4);
        assert!(v.contains("$timescale 1ns $end"));
        assert!(v.contains("$scope module Counter $end"));
        assert!(v.contains(" count $end")); // a $var line for the output
        assert!(v.contains(" clk $end"));
        assert!(v.contains("$enddefinitions $end"));
    }

    #[test]
    fn has_initial_dump_and_timestamps() {
        let v = counter_vcd(3);
        assert!(v.contains("$dumpvars"));
        assert!(v.contains("#0\n"));
        assert!(v.contains("#10\n")); // second cycle's rising edge (period = 10)
        // a multi-bit vector value line for `count` (e.g. `b1 <id>`)
        assert!(v.lines().any(|l| l.starts_with('b')));
    }

    #[test]
    fn id_codes_are_unique() {
        let ids: Vec<String> = (0..200).map(id_code).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "id codes must be unique");
    }
}
