//! Console trace rendering (Phase 1.5, step B5) — the opt-in `--trace` views.
//!
//! Two styles over the same [`Timeline`] the VCD writer uses: an **every-cycle
//! table** (`--trace`) and an **on-change** `$monitor`-style log
//! (`--trace=changes`). `scope` is the ordered list of signal names to show
//! (the command computes it from `--verbose` / `--signals` / the default
//! interface+state set). Default off — these only render when `--trace` is given.

use std::collections::BTreeMap;

use super::run::Timeline;

/// Render `tl` for the console. `style` is `"changes"` for the on-change log;
/// anything else (the default `"table"`) is the every-cycle table.
pub fn render(tl: &Timeline, style: &str, scope: &[String]) -> String {
    if style == "changes" {
        render_changes(tl, scope)
    } else {
        render_table(tl, scope)
    }
}

/// One row per rising-edge cycle; columns are `cycle` then each scope signal.
fn render_table(tl: &Timeline, scope: &[String]) -> String {
    let mut headers = vec!["cycle".to_string()];
    headers.extend(scope.iter().cloned());

    let mut rows: Vec<Vec<String>> = Vec::new();
    for f in tl.frames.iter().filter(|f| f.cycle.is_some()) {
        let mut row = vec![f.cycle.unwrap_or(0).to_string()];
        for s in scope {
            row.push(cell(f.values.get(s)));
        }
        rows.push(row);
    }

    // Column widths from the header and every cell.
    let mut w: Vec<usize> = headers.iter().map(String::len).collect();
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.len());
        }
    }

    let mut out = String::new();
    out.push_str(&join_row(&headers, &w));
    out.push('\n');
    out.push_str(
        &w.iter()
            .map(|x| "-".repeat(*x))
            .collect::<Vec<_>>()
            .join("-+-"),
    );
    out.push('\n');
    for r in &rows {
        out.push_str(&join_row(r, &w));
        out.push('\n');
    }
    out
}

/// A `$monitor`-style log: print every scope signal whenever any of them
/// changes (plus the first frame), tagged with the timestamp.
fn render_changes(tl: &Timeline, scope: &[String]) -> String {
    let mut out = String::new();
    let mut prev: BTreeMap<&str, u128> = BTreeMap::new();
    for (i, f) in tl.frames.iter().enumerate() {
        let changed = scope
            .iter()
            .any(|s| prev.get(s.as_str()) != Some(&f.values.get(s).copied().unwrap_or(0)));
        if i == 0 || changed {
            let cells: Vec<String> = scope
                .iter()
                .map(|s| format!("{s}={}", f.values.get(s).copied().unwrap_or(0)))
                .collect();
            out.push_str(&format!("#{}  {}\n", f.time, cells.join("  ")));
        }
        for s in scope {
            prev.insert(s.as_str(), f.values.get(s).copied().unwrap_or(0));
        }
    }
    out
}

fn cell(v: Option<&u128>) -> String {
    v.map(u128::to_string).unwrap_or_else(|| "?".into())
}

fn join_row(cells: &[String], widths: &[usize]) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{c:>width$}", width = widths[i]))
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::elaborate::elaborate;
    use crate::sim::run::{SimOpts, run};

    fn counter(cycles: u64) -> Timeline {
        let src = "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  \
                   out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
                   on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let d = elaborate(&f, None, &BTreeMap::new()).expect("elaborates");
        run(
            d,
            &SimOpts {
                clock: None,
                inputs: BTreeMap::new(),
                cycles,
                reset_cycles: 1,
            },
        )
        .expect("runs")
    }

    #[test]
    fn table_has_a_row_per_cycle() {
        let out = render(&counter(4), "table", &["count".into(), "value".into()]);
        assert!(out.contains("cycle"));
        assert!(out.contains("count"));
        // 1 header + 1 separator + 4 cycle rows
        assert_eq!(out.lines().count(), 6);
        // the last cycle row shows count = 3
        assert!(out.lines().last().unwrap().contains('3'));
    }

    #[test]
    fn changes_style_omits_unchanged_frames() {
        let out = render(&counter(4), "changes", &["count".into()]);
        // count changes on the 3 non-reset rising edges; plus the first frame.
        // Far fewer lines than the 8 total frames.
        let lines = out.lines().count();
        assert!((1..8).contains(&lines), "got {lines} lines:\n{out}");
        assert!(out.contains("count="));
    }
}
