// --------------------------------------------------------------- memory

use super::{Memory, all_example_files, compile_to_verilog};

/// Peak process RSS observed while compiling the whole corpus in one pass.
/// Every emitted string is retained in a sink so the allocator can't reclaim
/// it between files — we want the corpus's true high-water mark, sampled
/// after each compile. Lightweight (no allocator swap), so it's safe to run
/// in a normal `mimz-bench` invocation.
pub fn measure_memory() -> Memory {
    let mut peak = current_rss_mb();
    let mut sink: Vec<String> = Vec::new();
    for path in all_example_files() {
        if let Ok(v) = compile_to_verilog(&path) {
            sink.push(v);
        }
        peak = peak.max(current_rss_mb());
    }
    std::hint::black_box(&sink);
    Memory { peak_rss_mb: peak }
}

fn current_rss_mb() -> f64 {
    memory_stats::memory_stats()
        .map(|m| m.physical_mem as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}
