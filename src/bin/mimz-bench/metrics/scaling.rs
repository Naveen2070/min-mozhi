//! Complexity scaling: emit the SAME synthetic module at several sizes and
//! report the cost of each doubling.
//!
//! Every other section here samples one workload size, which is why GAP-12
//! (`mimz compile` superlinear in module size) shipped and sat undetected —
//! an absolute millisecond figure moves with the machine, so a trend line
//! over a single point cannot separate "slower runner" from "worse
//! complexity". The **ratio between adjacent doublings** can: it is ~2.0 for
//! a linear emitter on any machine, and grows without bound for a
//! superlinear one. That ratio is what this records.
//!
//! The workload has to be chosen with care. GAP-12's own stated benchmark —
//! a module of N registers chained `r_i <- r_{i-1}` — does not exhibit the
//! gap at all: a bare identifier right-hand side never enters a hoist path,
//! so it never reaches the per-expression `cur_decls` snapshot that was the
//! actual cost. Measured before and after the `Rc` fix, that shape is
//! unchanged (0.36 s vs 0.33 s at N=8,000). The drives below therefore carry
//! `trunc(extend(r_i, W) * extend(3, W), 8)`, so declaration count and
//! hoist-site count both scale with N — the shape that showed 7.59 s -> 0.28 s.

use std::time::Instant;

use mimz::{ast, checker, emit_verilog, lexer, parser};

use super::{Scaling, ScalingPoint, median};

/// Module sizes to emit, each double the last. Kept small on purpose: this
/// runs on every `mimz-bench` invocation including CI's per-PR gate, and the
/// ratio — not the absolute time — is the signal, so buying a bigger N adds
/// seconds without adding information.
const SIZES: [usize; 3] = [250, 500, 1000];

/// One synthetic module of `n` registers whose drives all hoist.
///
/// Deliberately NOT a plain `r_i <- r_{i-1}` chain — see the module docs for
/// why that shape measures nothing.
fn gen_source(n: usize) -> String {
    let mut s = String::with_capacity(n * 80);
    s.push_str("module Scaling {\n  clock clk\n  reset rst\n  in d: bits[8]\n  out y: bits[8]\n");
    for i in 0..n {
        s.push_str(&format!("  reg r{i}: bits[8] = 0\n"));
    }
    for i in 0..n {
        s.push_str(&format!(
            "  wire w{i}: bits[8] = trunc(extend(r{i}, 16) * extend(3, 16), 8)\n"
        ));
    }
    s.push_str("  on rise(clk) {\n    r0 <- d\n");
    for i in 1..n {
        s.push_str(&format!("    r{i} <- w{}\n", i - 1));
    }
    s.push_str(&format!("  }}\n  y = w{}\n}}\n", n - 1));
    s
}

/// Median emit time at each size in [`SIZES`], plus the worst adjacent
/// doubling ratio.
///
/// Times the check+emit half only: lexing and parsing are done once per size,
/// outside the timer, because the cost this exists to catch lives in the
/// emitter. `iterations` is capped — the work is already O(n) per run and the
/// perf batch passes 500, which would take minutes here for no extra signal.
pub fn measure_scaling(iterations: usize) -> Scaling {
    let reps = iterations.clamp(1, 5);
    let mut points: Vec<ScalingPoint> = Vec::new();

    for n in SIZES {
        let src = gen_source(n);
        let loc = src.lines().count();
        let tokens = lexer::lex(&src).expect("generated module lexes");
        let file = parser::parse(tokens).expect("generated module parses");

        // Warm-up, untimed: same convention as `measure_speed`.
        {
            let mut asts = vec![file.clone()];
            checker::check(&asts).expect("generated module checks clean");
            emit_verilog::transliterate(&mut asts);
            let proj = emit_verilog::Project::from_files(&asts).expect("project builds");
            emit_verilog::emit(&proj, &asts).expect("generated module emits");
        }

        let mut times = Vec::with_capacity(reps);
        for _ in 0..reps {
            let mut asts: Vec<ast::File> = vec![file.clone()];
            let t = Instant::now();
            checker::check(&asts).expect("generated module checks clean");
            emit_verilog::transliterate(&mut asts);
            let proj = emit_verilog::Project::from_files(&asts).expect("project builds");
            emit_verilog::emit(&proj, &asts).expect("generated module emits");
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }

        points.push(ScalingPoint {
            regs: n,
            loc,
            emit_ms: median(&mut times),
        });
    }

    // Cost of each doubling. A linear emitter sits at ~2.0 regardless of
    // machine; the pre-`Rc` emitter measured 2.71 / 3.90 / 5.20 and climbing.
    let ratios: Vec<f64> = points
        .windows(2)
        .map(|w| {
            if w[0].emit_ms > 0.0 {
                w[1].emit_ms / w[0].emit_ms
            } else {
                0.0
            }
        })
        .collect();
    let worst_doubling_ratio = ratios.iter().copied().fold(0.0_f64, f64::max);

    Scaling {
        points,
        ratios,
        worst_doubling_ratio,
    }
}
