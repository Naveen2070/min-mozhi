# 12 — Benchmark Harness (`mimz-bench`)

The repo's benchmark & validation harness: one run measures **speed,
accuracy, safety, and coverage**, then writes an **HTML report with
graphs** plus a machine-readable JSON twin. It is a separate binary
(decision 2026-06-12) — NOT a `mimz` subcommand — because it measures
the corpora under `examples/` and `tests/`, which only exist in a
checkout.

```text
cargo run --release --bin mimz-bench              # full run (Icarus + cargo-llvm-cov)
cargo run --release --bin mimz-bench -- --no-cov  # skip the slow coverage pass
cargo run --release --bin mimz-bench -- --help    # all flags
```

Always run `--release` — debug-build timings measure the optimizer's
absence, not the compiler.

## What it measures

| Section      | Metrics                                                                                                                                                | Source of truth                                                          |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| **Speed**    | per-phase wall time (load+parse / check / emit) per base example, **median** of N iterations (`--iterations`, default 5); total LOC/s throughput       | the lib pipeline, timed with `std::time::Instant`                        |
| **Accuracy** | golden-file match rate, 4-flavor byte-identity rate, `iverilog -t null` accept rate, self-checking testbench PASS rate                                 | `tests/golden/`, `tests/icarus/` (Icarus skipped gracefully if missing)  |
| **Safety**   | error-fixture rate (each fixture's diagnostics contain its declared E-code **with** a help line), false-positive rate (every example must check clean) | `tests/fixtures/errors/`, `mimz::diag::ALL_CHECKER_CODES`                |
| **Coverage** | corpus: codes-with-fixture, examples-with-golden/-testbench, flavor completeness; code: line/function/region % via cargo-llvm-cov                      | computed internally; `cargo llvm-cov --json --summary-only` for real cov |

The harness **re-measures what the test suite asserts**: `cargo test`
answers pass/fail; `mimz-bench` answers _how fast, how complete, and is
it trending the right way_ — and renders it for humans.

## Outputs (all gitignored — regenerate any time)

| File                  | Contents                                                                                                       |
| --------------------- | -------------------------------------------------------------------------------------------------------------- |
| `bench-report.html`   | The graph report: verdict banner, summary cards, stacked timing bars, rate bars, coverage doughnut, trend line |
| `bench-report.json`   | The full `BenchReport`, machine-readable (same data the HTML embeds)                                           |
| `bench-history.jsonl` | One JSON line per run (timestamp, git rev, headline numbers) — feeds the trend chart                           |

The HTML pulls Chart.js from the jsDelivr CDN (user decision
2026-06-12): the file is a single portable page, but drawing the charts
needs internet. The tables and verdict render without it.

## Exit code = validation gate

`mimz-bench` exits **non-zero if any accuracy or safety rate is below
100%** (each failure is named in the console, the report, and the JSON).
Speed and coverage inform but never fail a run — so it can later gate CI
without flaking on machine speed.

## Code coverage needs cargo-llvm-cov (optional)

```text
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
```

Not installed → the coverage card says so (with this hint) and the run
still succeeds. Note the coverage pass **reruns the whole instrumented
test suite** — minutes, not seconds; `--no-cov` skips it.

## Code layout (`src/bin/mimz-bench/`)

| File         | Role                                                                                                                   |
| ------------ | ---------------------------------------------------------------------------------------------------------------------- |
| `main.rs`    | clap CLI, section orchestration, history append, console summary, exit code                                            |
| `metrics.rs` | the measurement engine — every section returns plain serializable structs; mirrors the corpus conventions of the tests |
| `html.rs`    | `BenchReport` + history → the single-file Chart.js report                                                              |

Corpus constants (`BASE_EXAMPLES`, `TESTBENCHES`, the fixture-header
convention, iverilog detection) intentionally mirror
`tests/examples.rs` / `tests/errors.rs` / `tests/icarus.rs`; the checker
code list is shared for real via `mimz::diag::ALL_CHECKER_CODES`. If a
corpus convention changes, update the harness in the same session
(RULES R1).
