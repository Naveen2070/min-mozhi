# Min-Mozhi Benchmark & Performance Plan

This document outlines the strategic roadmap for evolving the `mimz-bench` harness. While the current harness is already highly effective at macro-level tracking (reporting medians over N iterations and tracking historical LOC/s throughput), the following phases will bring it to enterprise-grade compiler performance tracking.

---

## 1. Current State & Strengths: The Power of Batched Benchmarks

The current `mimz-bench` architecture is already using the "gold standard" for macro-benchmarking: **Batching and Medians**.

### Why Batching is Better

Running a compiler once to test speed is dangerous. The Operating System might suddenly schedule a background task, or the CPU might thermally throttle, causing a massive artificial spike in execution time.
To solve this, `mimz-bench` runs the compiler in a continuous batch loop and takes the **median** time. The median completely ignores sudden CPU spikes, yielding the true, clean speed of the compiler.

### Cumulative Data Tracking

Because `mimz-bench` appends every single run to `bench-history.jsonl`, you aren't just batching data per run—you are building a **cumulative historical dataset** over the lifetime of the project. This is what currently powers the Chart.js trend graphs.

### How to Run Massive Batches Today

By default, the benchmark runs a batch of 5 (`--iterations 5`). To generate rigorous cumulative data for deep insights, you can force the tool to run massive back-to-back batches:

```bash
cargo run --release --bin mimz-bench -- --no-cov --iterations 100
```

This command runs the entire pipeline 100 times, filters out all noise, takes the exact median, and permanently logs the cumulative throughput (LOC/s) to your history.

---

## 2. Phase 1: Micro-Benchmarking (`criterion.rs`)

**The Goal:** Isolate specific compiler phases to detect micro-regressions.
Currently, `mimz-bench` is a "Macro Benchmark." If the compiler slows down by 2 milliseconds, we don't know if the Lexer, Parser, or Checker caused it.

**Implementation Strategy:**

- Introduce the `criterion` crate (or `iai` for instruction-counting).
- Write specific micro-benchmarks that test the **Lexer**, **Parser**, and **Checker** in complete isolation.
- `criterion` automatically performs statistical warmup passes, outlier detection, and warns us if a specific Pull Request degrades a specific phase by even a few nanoseconds.

---

## 3. Phase 2: Parallelization & Scale (`rayon`)

**The Goal:** Keep the benchmark execution fast even when the standard library scales to 1,000+ files.
Right now, the benchmark likely iterates through the `examples/` and `tests/` directories sequentially.

**Implementation Strategy:**

- Integrate the `rayon` crate into `mimz-bench`.
- Switch `.iter()` over the corpus to `.par_iter()`.
- Load, parse, and verify all `.mimz` test files concurrently utilizing all available CPU cores. This will drop total benchmark suite times from minutes to seconds as the project grows.

---

## 4. Phase 3: Hardware & Memory Profiling (`dhat`)

**The Goal:** A fast compiler is useless if it consumes gigabytes of RAM. Abstract Syntax Trees (ASTs) are notorious for causing memory bloat. We must track peak heap allocation.

**Implementation Strategy:**

- Integrate a memory profiling crate like `dhat` or `memory-stats` into `mimz-bench`.
- Track **Peak RAM Usage** (in Megabytes) alongside Speed (in LOC/s).
- Add a new graph to `bench-report.html` showing Memory Usage over time. If a developer accidentally copies heavy Strings instead of using lightweight references, the benchmark will instantly flag the memory spike.

---

## 5. Phase 4: Cache Warm-ups & I/O Isolation

**The Goal:** Decouple Disk Speed from Compiler Speed.
Reading files off a physical SSD/HDD introduces heavy statistical noise. We only want to measure the _compiler's_ parsing and checking speed, not the operating system's disk read speed.

**Implementation Strategy:**

- Implement a "Warm-Up" pass in `mimz-bench`.
- Before starting the `std::time::Instant` timer, execute the entire benchmark once _without recording the time_.
- This forces the Operating System to load the `.mimz` files into the RAM cache.
- When the actual timed iterations begin, the files will be read directly from memory, resulting in incredibly stable and accurate LOC/s metrics.

---

## 6. Execution Strategy for CI/CD

To ensure performance never degrades over time, we will run batch benchmarks automatically:

- **Pull Requests:** Run `cargo bench` (Criterion micro-benchmarks) to instantly fail PRs that slow down the parser.
- **Nightly Batch:** Run `cargo run --release --bin mimz-bench -- --no-cov --iterations 100` on the `main` branch every night via GitHub Actions.
- Append the nightly results to a remote `bench-history.jsonl` to generate a public-facing performance dashboard for the Min-Mozhi community.
