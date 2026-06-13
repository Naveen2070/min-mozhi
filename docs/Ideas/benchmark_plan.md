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

## Phase 1: Cache Warm-up & I/O Isolation — ✅ DONE 2026-06-13

Implemented: `measure_speed` (`src/bin/mimz-bench/metrics.rs`) runs one untimed
full pipeline per example before the timed loop.

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

## Phase 2: Micro-Benchmarking (`criterion`, a separate harness) — ✅ DONE 2026-06-13

Implemented: `benches/compile.rs` (`[[bench]]`, `harness = false`) isolates
lexer / parser / checker / emit over `traffic_light`; run with `cargo bench`,
compile-checked in CI via `cargo bench --no-run`.

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

## Phase 3: Memory Profiling — ✅ DONE 2026-06-13 (tier 1; dhat tier deferred)

The goal: a fast compiler is useless if it consumes gigabytes of RAM. ASTs are
notorious for memory bloat, so track peak heap/RSS alongside speed.

**Two tiers, kept separate:**

- **Default (`memory-stats`, peak RSS) — ✅ DONE:** `measure_memory` records
  peak resident set over a full-corpus compile into `bench-report.json` /
  `bench-history.jsonl`, surfaced as a card + memory-trend chart. `memory-stats`
  is lightweight (no allocator swap), so it rides a normal `mimz-bench` run.
- **Opt-in (`dhat`, precise heap) — deferred:** for detailed allocation
  profiles. `dhat` installs a custom `#[global_allocator]` and slows execution
  ~10×, so it can **never** share a timed run — gate it behind a dedicated
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

The full CI strategy, security model, and hardening roadmap now live in
[`ci_plan.md`](ci_plan.md). Benchmark-relevant summary:

- **push / PR — `bench` job:** `mimz-bench --no-cov --no-icarus` as a hard
  correctness gate; `--history` routed to a temp path so it records no point.
  The `check` job also `cargo bench --no-run`s the `criterion` harness.
- **Perf batch — `nightly-bench` job:** `mimz-bench --no-cov --iterations 500`,
  then **commits the appended `bench-history.jsonl` back to the repo**
  (`[skip ci]`) and uploads the report as an artifact. The committed JSONL is
  the canonical, version-controlled trend. Triggered **manually** today
  (`workflow_dispatch`); the nightly cron is commented out in `ci.yml`.

Deferred CI items (a public GitHub Pages dashboard, a `critcmp` PR timing gate,
commit-SHA action pinning) are tracked in [`ci_plan.md`](ci_plan.md).
