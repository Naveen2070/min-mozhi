//! Synthesis v1 design section 6.2: every corpus module's IR-emitted
//! Verilog, simulated in Icarus, matches `ir::exec` bit for bit over 8 ticks.
//! Modules this cannot check are pinned in `EXPECTED_SKIPS` with why.
//! Corpus: `examples/` + `tests/fixtures/extern/`, the set `ir_opt_corpus.rs`
//! walks (`demo/` is left out).

mod support;

use mimz_core::ast::Edge;
use mimz_core::ir::CellKind;
use std::path::PathBuf;
use support::ir_sim::*;

/// file:module skipped, with why.
const EXPECTED_SKIPS: &[&str] = &[
    // Mixed rising and falling registers (`on rise` feeds `on fall`):
    // `ir::exec` ignores `edge` (one global clock), so its trace differs
    // from a real-edge simulation by design (spec/07-ir.md section 3.6;
    // GAP-1 in docs/audit/gaps.md). The mixed-edge test below skips any such
    // module automatically, except `DualEdge`, which is checked shifted by
    // one tick; so none are listed here.
    // Extern fixtures: a `BlackBox` cell has no behavior to simulate.
    "tests/fixtures/extern/pll.mimz:ExternDemo",
    "tests/fixtures/extern/pll_alias.mimz:AliasDemo",
];

#[test]
fn ir_verilog_matches_ir_exec_on_the_corpus() {
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    let root = support::repo();
    let mut files: Vec<PathBuf> = Vec::new();
    mimz_files(&root.join("examples"), &mut files);
    mimz_files(&root.join("tests/fixtures/extern"), &mut files);
    files.sort();
    let mut skipped = Vec::new();
    let mut checked = 0;
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (name, module) in lowered_modules(path) {
            let at = format!("{rel}:{name}");
            let Some(mut m) = module else {
                skipped.push(at);
                continue;
            };
            mimz_core::ir::opt::optimize(&mut m);
            let opaque = m
                .cells
                .iter()
                .any(|c| matches!(c.kind, CellKind::BlackBox { .. }));
            let wide = m.ports.iter().any(|(_, b, _)| b.width() > 128);
            // `ir::exec` ticks every register on one clock and ignores
            // `edge`; the testbench's rise-then-fall lets a falling register
            // see a rising one's new value in the same tick.
            let edges = |want: Edge| {
                m.cells.iter().any(|c| match &c.kind {
                    CellKind::Dff { edge, .. } | CellKind::Adff { edge, .. } => *edge == want,
                    _ => false,
                })
            };
            let mixed_edges = edges(Edge::Rise) && edges(Edge::Fall);
            // `DualEdge` (`a` on rise <- d, `b` on fall <- a, `q = b`) is
            // checked, shifted: Icarus passes `d` through both edges in the
            // same tick (`q_t = d_t`), `ir::exec` updates `a` and `b` together
            // (`q_t = d_(t-1)`), so Icarus `q_t` equals exec `q_(t+1)` (GAP-1
            // in docs/audit/gaps.md). That holds only while `rst` is low at
            // t+1 (exec's reset lags a tick the same way), so `rst` is high
            // at tick 0 only. Any other mixed-edge module is still skipped.
            let shifted = name == "DualEdge" && mixed_edges;
            if opaque || wide || (mixed_edges && !shifted) {
                skipped.push(at);
                continue;
            }
            let e = mimz_core::backend::verilog::emit(&m);
            let mut stim = stimulus(&m, 8);
            if shifted {
                for (t, tick) in stim.iter_mut().enumerate() {
                    for (_, v) in tick.iter_mut().filter(|(n, _)| n == "rst") {
                        *v = u128::from(t == 0);
                    }
                }
            }
            let want = exec_trace(&m, &stim);
            let tb = testbench(&e, &clock_ports(&m), &stim);
            let v = std::env::temp_dir().join(format!("mimz_irv_{}.v", file_safe(&at)));
            std::fs::write(&v, &e.text).unwrap();
            let got = icarus_trace(
                &run_iverilog(&bin, &at, std::slice::from_ref(&v), &tb, &[]),
                &e,
            );
            let _ = std::fs::remove_file(&v);
            let (got, want) = if shifted {
                // The 7 overlapping ticks; the oracle must actually move.
                let qs: std::collections::BTreeSet<u128> =
                    want.iter().flatten().map(|(_, v)| *v).collect();
                assert!(qs.len() > 2, "{at}: q never moves");
                (got[..7].to_vec(), want[1..].to_vec())
            } else {
                (got, want)
            };
            assert_eq!(
                got, want,
                "{at}: Icarus trace differs from ir::exec\n--- verilog ---\n{}",
                e.text
            );
            checked += 1;
        }
    }
    eprintln!("checked {checked} modules");
    assert_eq!(skipped, EXPECTED_SKIPS, "IR-Verilog skip list changed");
    assert!(checked > 0);
}
