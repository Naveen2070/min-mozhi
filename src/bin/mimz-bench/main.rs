//! mimz-bench — the Min-Mozhi benchmark & validation harness.
//!
//! A REPO tool, not a user-facing `mimz` subcommand (decision 2026-06-12):
//! it measures the corpora under examples/ and tests/, which only exist in
//! a checkout. Run from the repo root:
//!
//! ```text
//! cargo run --release --bin mimz-bench            # full run (llvm-cov + Icarus)
//! cargo run --release --bin mimz-bench -- --no-cov   # skip the slow coverage pass
//! ```
//!
//! One run measures four sections — speed (per-phase compile timings),
//! accuracy (goldens, flavor identity, Icarus), safety (error corpus,
//! false positives), coverage (corpus + cargo-llvm-cov) — appends a
//! history line, and writes `bench-report.html` (Chart.js graphs) plus
//! `bench-report.json`. Exits non-zero on any accuracy/safety failure,
//! so it can gate CI. Docs: docs/code/12-benchmark.md.

mod html;
mod metrics;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// CLI for the benchmark harness (`///` docs double as `--help`).
#[derive(Parser)]
#[command(
    name = "mimz-bench",
    version,
    about = "Min-Mozhi benchmark: speed, accuracy, safety, coverage — with an HTML graph report"
)]
struct Cli {
    /// Where the HTML report is written
    #[arg(long, default_value = "bench-report.html")]
    out: PathBuf,
    /// Where the machine-readable JSON report is written
    #[arg(long, default_value = "bench-report.json")]
    json: PathBuf,
    /// Run history (one JSON line per run; feeds the trend chart)
    #[arg(long, default_value = "bench-history.jsonl")]
    history: PathBuf,
    /// Timing iterations per example (median is reported)
    #[arg(long, default_value_t = 5)]
    iterations: usize,
    /// Skip cargo-llvm-cov (the slowest section — it reruns the test suite)
    #[arg(long)]
    no_cov: bool,
    /// Skip the Icarus Verilog layers even when iverilog is installed
    #[arg(long)]
    no_icarus: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    println!(
        "mimz-bench: measuring speed ({} iterations)...",
        cli.iterations
    );
    let speed = metrics::measure_speed(cli.iterations);

    println!("mimz-bench: measuring accuracy (goldens, flavors, Icarus)...");
    if !cli.no_icarus && metrics::iverilog_bin().is_none() {
        println!("  note: iverilog not found — Icarus layers will be skipped");
    }
    let accuracy = metrics::measure_accuracy(!cli.no_icarus);

    println!("mimz-bench: measuring safety (error corpus, false positives)...");
    let safety = metrics::measure_safety();

    if cli.no_cov {
        println!("mimz-bench: coverage (corpus only — --no-cov)...");
    } else {
        println!("mimz-bench: coverage (corpus + cargo-llvm-cov; this reruns the test suite)...");
    }
    let coverage = metrics::measure_coverage(!cli.no_cov);

    let report = metrics::BenchReport {
        meta: metrics::collect_meta(cli.iterations),
        speed,
        accuracy,
        safety,
        coverage,
    };

    // History first (append), then read it ALL back so the trend chart
    // includes this run as its newest point.
    let entry = metrics::HistoryEntry::from_report(&report);
    let line = serde_json::to_string(&entry).expect("history entry serializes");
    if let Err(e) = append_line(&cli.history, &line) {
        eprintln!("warning: could not append {}: {e}", cli.history.display());
    }
    let history = read_history(&cli.history);

    let json = serde_json::to_string_pretty(&report).expect("report serializes");
    if let Err(e) = std::fs::write(&cli.json, json) {
        eprintln!("error: cannot write {}: {e}", cli.json.display());
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(&cli.out, html::render(&report, &history)) {
        eprintln!("error: cannot write {}: {e}", cli.out.display());
        return ExitCode::FAILURE;
    }

    print_summary(&report, &cli);
    if report.all_validations_pass() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn append_line(path: &PathBuf, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")
}

/// Every parseable line of the history file (unparseable lines are
/// skipped — an old schema must not kill the report).
fn read_history(path: &PathBuf) -> Vec<metrics::HistoryEntry> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// The console version of the report's summary cards.
fn print_summary(report: &metrics::BenchReport, cli: &Cli) {
    let rate = |label: &str, r: &metrics::Rate| {
        println!(
            "  {label:<44} {:>3}/{:<3} ({:.1}%)",
            r.passed,
            r.total,
            r.percent()
        );
    };
    println!();
    println!(
        "== mimz-bench summary ({} examples, {} fixtures) ==",
        report.meta.example_files, report.meta.fixture_files
    );
    println!(
        "  pipeline total {:.1} ms (median of {}), {:.0} LOC/s",
        report.speed.total_ms, report.meta.iterations, report.speed.loc_per_sec
    );
    rate("golden-file match", &report.accuracy.golden);
    rate("flavor byte-identity", &report.accuracy.flavor_identity);
    match &report.accuracy.iverilog_syntax {
        Some(r) => rate("iverilog syntax accept", r),
        None => println!("  {:<44} skipped", "iverilog syntax accept"),
    }
    match &report.accuracy.testbenches {
        Some(r) => rate("self-checking testbenches", r),
        None => println!("  {:<44} skipped", "self-checking testbenches"),
    }
    rate("error fixtures fire their code", &report.safety.fixtures);
    rate("diagnostics carry a help line", &report.safety.help_lines);
    rate(
        "examples check clean (no false positives)",
        &report.safety.clean_examples,
    );
    rate(
        "checker codes with a fixture",
        &report.coverage.corpus.codes_with_fixture,
    );
    match &report.coverage.llvm {
        Some(l) => println!(
            "  {:<44} {:.1}% lines, {:.1}% functions",
            "code coverage (cargo-llvm-cov)", l.line_percent, l.function_percent
        ),
        None => println!(
            "  {:<44} {}",
            "code coverage (cargo-llvm-cov)",
            report.coverage.llvm_note.as_deref().unwrap_or("skipped")
        ),
    }
    for f in report
        .accuracy
        .failures
        .iter()
        .chain(&report.safety.failures)
    {
        println!("  FAIL: {f}");
    }
    println!();
    println!("report: {} (+ {})", cli.out.display(), cli.json.display());
}
