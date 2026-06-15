// ---------------------------------------------------------------- speed

use std::time::Instant;

use mimz::{ast, checker, emit_verilog};

use super::{BASE_EXAMPLES, ExampleTiming, Speed, load, median, repo};

/// Time the pipeline phases for every base example (english flavor),
/// `iterations` runs each, keeping the per-phase MEDIAN (steady-state
/// number, robust to one cold file-cache run).
pub fn measure_speed(iterations: usize) -> Speed {
    let mut per_example = Vec::new();
    let mut total_loc = 0usize;
    for base in BASE_EXAMPLES {
        let path = repo()
            .join("examples")
            .join("english")
            .join(format!("{base}.mimz"));
        let mut loads = Vec::new();
        let mut checks = Vec::new();
        let mut emits = Vec::new();
        let mut loc = 0usize;

        // Warm-up: one untimed full pipeline so the OS file cache and branch
        // predictors are hot before the timer starts. Decouples disk-read
        // noise from compiler speed and makes `--iterations 1` honest.
        {
            let files = load(&path).expect("examples compile — gated by cargo test");
            let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
            checker::check(&asts).expect("examples check clean");
            emit_verilog::transliterate(&mut asts);
            let proj = emit_verilog::Project::from_files(&asts).expect("project builds");
            emit_verilog::emit(&proj, &asts).expect("examples emit");
        }

        for _ in 0..iterations.max(1) {
            let t = Instant::now();
            let files = load(&path).expect("examples compile — gated by cargo test");
            loads.push(t.elapsed().as_secs_f64() * 1000.0);
            loc = files.iter().map(|f| f.src.lines().count()).sum();

            let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
            let t = Instant::now();
            checker::check(&asts).expect("examples check clean");
            checks.push(t.elapsed().as_secs_f64() * 1000.0);

            let t = Instant::now();
            emit_verilog::transliterate(&mut asts);
            let proj = emit_verilog::Project::from_files(&asts).expect("project builds");
            emit_verilog::emit(&proj, &asts).expect("examples emit");
            emits.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        total_loc += loc;
        per_example.push(ExampleTiming {
            name: base.to_string(),
            loc,
            load_ms: median(&mut loads),
            check_ms: median(&mut checks),
            emit_ms: median(&mut emits),
        });
    }
    let total_ms: f64 = per_example
        .iter()
        .map(|e| e.load_ms + e.check_ms + e.emit_ms)
        .sum();
    Speed {
        per_example,
        total_loc,
        total_ms,
        loc_per_sec: if total_ms > 0.0 {
            total_loc as f64 / (total_ms / 1000.0)
        } else {
            0.0
        },
    }
}
