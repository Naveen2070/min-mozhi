# Min-Mozhi Benchmark & Performance Plan

This document outlines the strategic roadmap for evolving the `mimz-bench`
harness. The current harness already does macro-level tracking well (median
over N iterations, historical LOC/s throughput, and a hard correctness gate),
so the phases below add micro-level precision and resource tracking on top of
it — in order of return on effort.

---

## 1. Current State & Strengths

The current `mimz-bench` already uses the macro-benchmarking gold standard:
**batching and medians**, plus a **correctness gate**.

### Why batching is better

Running a compiler once to measure speed is dangerous. The operating system
might schedule a background task, or the CPU might thermally throttle, causing
an artificial spike. `mimz-bench` instead runs each example's pipeline a batch
of times (`--iterations`, default 5) and keeps the **median** per phase, which
ignores one-off spikes and yields a stable steady-state number.

### It already gates correctness

`mimz-bench` exits **non-zero** on any accuracy or safety failure (see
`src/bin/mimz-bench/main.rs`). So the harness is already CI-usable as a
correctness gate today — the work below is about _performance_ precision and
_resource_ tracking, not about adding gating.

### Cumulative data tracking

Every run appends one line to `bench-history.jsonl`, building a cumulative
dataset over the project's lifetime. This is what powers the Chart.js trend
graphs in `bench-report.html`.

### How to run massive batches today

```bash
cargo run --release --bin mimz-bench -- --no-cov --iterations 100
```

This runs the pipeline 100 times per example, keeps the median, and logs the
cumulative throughput (LOC/s) to history.

---

## Phase 1: Cache Warm-up & I/O Isolation (highest ROI, smallest change)

**The goal:** decouple disk speed from compiler speed.
Reading `.mimz` files off an SSD/HDD introduces statistical noise. We only want
to measure the _compiler's_ work, not the OS's disk-read speed — and we want
`--iterations 1` to be honest, not just the default of 5.

**Implementation strategy:**

- Before starting the `std::time::Instant` timer, run the full pipeline for
  that example **once without recording the time**.
- This forces the OS to load the source into the RAM cache and warms branch
  predictors, so the timed iterations read from memory and produce stable
  LOC/s.

This is a ~5-line change inside `measure_speed` and directly improves the
metric the harness already reports, which is why it leads the roadmap.

---

## Phase 2: Micro-Benchmarking (`criterion`, a separate harness)

**The goal:** isolate specific compiler phases to detect micro-regressions.
`mimz-bench` is a macro benchmark — if compilation slows by 2 ms, it can't tell
you whether the lexer, parser, or checker caused it.

**Important:** this is a **separate harness**, not part of `mimz-bench`.
`criterion` benchmarks live under `benches/` and run via `cargo bench`; they do
not share code or output with `mimz-bench`. The two tools are complementary:
`mimz-bench` tracks the end-to-end corpus and gates correctness; `criterion`
does statistical per-phase timing.

**Implementation strategy:**

- Add `criterion` as a dev-dependency and a `[[bench]]` target with
  `harness = false`.
- Write micro-benchmarks that exercise the **lexer**, **parser**, **checker**,
  and **emitter** in isolation over a representative example, using the public
  library entry points (`mimz::lexer::lex`, `mimz::parser::parse`,
  `mimz::checker::check`, `mimz::emit_verilog::*`).
- `criterion` performs statistical warmup, outlier detection, and saves a
  local baseline under `target/criterion/`.

**CI note:** `criterion` prints regression deltas locally but does **not** fail
a build on its own. To fail a PR that slows a phase, add a comparison tool such
as `critcmp` or a benchmark-threshold GitHub Action on top.

---

## Phase 3: Memory Profiling

**The goal:** a fast compiler is useless if it consumes gigabytes of RAM. ASTs
are notorious for memory bloat, so track peak heap/RSS alongside speed.

**Implementation strategy (two tiers, kept separate):**

- **Default (`memory-stats`, peak RSS):** measure peak resident set around a
  full-corpus compile and record it in `bench-report.json` /
  `bench-history.jsonl` plus a memory-trend chart. `memory-stats` is
  lightweight and **can coexist with a normal run**, so it rides the existing
  `mimz-bench` flow.
- **Opt-in (`dhat`, precise heap):** for detailed allocation profiles. `dhat`
  installs a custom `#[global_allocator]` and slows execution by roughly 10×,
  so it can **never** share a timed run — gate it behind a dedicated
  `--profile-mem` build/feature and never report its timings as speed.

If a developer accidentally clones heavy `String`s instead of borrowing, the
peak-RSS trend flags the regression.

---

## Phase 4: Parallelization & Scale (`rayon`) — deferred

**The goal:** keep the benchmark fast if the corpus ever scales to 1,000+ files.

**Status: deferred.** Today's corpus is ~56 files and a full run is sub-second,
so this is premature; revisit when scale is real.

**Hard rule when it does land:** parallelize **only the untimed validation
sweeps** (accuracy, safety, coverage — they are pass/fail). The speed pass
(`measure_speed`) must stay **single-threaded**: running compiles concurrently
makes them contend for CPU and cache, which corrupts the LOC/s measurement.

**Implementation strategy:**

- Integrate `rayon` and switch `.iter()` to `.par_iter()` in the validation
  sweeps only.
- Leave `measure_speed` sequential and explicitly commented as such.

---

## 5. Execution Strategy for CI/CD

**Wired now** (`.github/workflows/ci.yml`):

- **Every push / PR:**
  - the `check` job runs the full R8 gate — `fmt --check`, `clippy
--all-targets -D warnings`, **rustdoc `-D warnings`**, `cargo test` (with
    `REQUIRE_IVERILOG=1`), `cargo build`, and **`cargo bench --no-run`** so the
    `criterion` harness is type-checked but never gated on noisy timings;
  - the `bench` job runs `cargo run --release --bin mimz-bench -- --no-cov
--no-icarus` — its non-zero exit is a hard **correctness gate** (goldens,
    flavor byte-identity, fixtures, no-false-positives).
- **Perf batch (`nightly-bench` job):** runs `mimz-bench --no-cov --iterations
100`, accumulates `bench-history.jsonl` across runs via a rolling
  `actions/cache` (restore-keys prefix), and uploads `bench-report.html` /
  `.json` as artifacts — a growing perf trend without committing the gitignored
  history. Triggered **manually** today (Actions tab → CI → Run workflow, i.e.
  `workflow_dispatch`); the nightly `schedule:` cron is committed-out in
  `ci.yml` and can be uncommented to run it automatically at 03:00 UTC (the job
  already accepts that trigger).

**Future:**

- A **public dashboard** (publish the nightly `bench-report.html` to GitHub
  Pages) instead of per-run artifacts.
- **PR timing gate:** `cargo bench` + `critcmp`/a threshold action to fail PRs
  that slow a phase — deferred until run-to-run noise on shared runners is
  characterized (today the benches are only compile-checked).
