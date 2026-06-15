//! The measurement engine: every benchmark section produces plain
//! serializable data here; rendering (HTML/console) happens elsewhere.
//! Mirrors the corpus conventions of the integration tests
//! (tests/examples.rs, tests/errors.rs, tests/icarus.rs) — the few small
//! private helpers they keep (banner strip, fixture headers, iverilog
//! detection) are re-implemented here, byte-compatible.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use mimz::project::LoadError;
use mimz::{ast, checker, emit_verilog, project};

mod accuracy;
mod coverage;
mod memory;
mod safety;
mod speed;

pub use accuracy::{iverilog_bin, measure_accuracy};
pub use coverage::measure_coverage;
pub use memory::measure_memory;
pub use safety::measure_safety;
pub use speed::measure_speed;

/// The flavor folders under examples/ (same as tests/examples.rs).
pub const FLAVORS: [&str; 4] = ["english", "tanglish", "tamil", "mixed"];

/// Every base example (same list the integration tests pin).
pub const BASE_EXAMPLES: [&str; 15] = [
    "adder",
    "alu",
    "blinker",
    "chained",
    "comparator",
    "counter",
    "edge_detector",
    "lib/full_adder",
    "mux4",
    "ripple_adder",
    "shift_register",
    "signed_math",
    "traffic_light",
    "vilakku",
    "window",
];

/// Testbench file (tests/icarus/) -> the example it simulates
/// (same table as tests/icarus.rs).
pub const TESTBENCHES: [(&str, &str); 14] = [
    ("adder_tb.v", "english/adder.mimz"),
    ("alu_tb.v", "english/alu.mimz"),
    ("blinker_tb.v", "english/blinker.mimz"),
    ("chained_tb.v", "english/chained.mimz"),
    ("comparator_tb.v", "english/comparator.mimz"),
    ("counter_tb.v", "english/counter.mimz"),
    ("edge_detector_tb.v", "english/edge_detector.mimz"),
    ("full_adder_tb.v", "english/lib/full_adder.mimz"),
    ("mux4_tb.v", "english/mux4.mimz"),
    ("ripple_adder_tb.v", "english/ripple_adder.mimz"),
    ("shift_register_tb.v", "english/shift_register.mimz"),
    ("signed_math_tb.v", "english/signed_math.mimz"),
    ("traffic_light_tb.v", "english/traffic_light.mimz"),
    ("window_tb.v", "english/window.mimz"),
];

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------- report

/// Everything one benchmark run measured. Serialized verbatim into
/// `bench-report.json` and embedded into the HTML report.
#[derive(Serialize)]
pub struct BenchReport {
    pub meta: Meta,
    pub speed: Speed,
    pub memory: Memory,
    pub accuracy: Accuracy,
    pub safety: Safety,
    pub coverage: Coverage,
}

impl BenchReport {
    /// The gate: accuracy + safety must be perfect; speed and coverage
    /// inform but never fail a run.
    pub fn all_validations_pass(&self) -> bool {
        self.accuracy.failures.is_empty() && self.safety.failures.is_empty()
    }
}

#[derive(Serialize)]
pub struct Meta {
    /// Unix epoch milliseconds (the HTML renders it as a local date).
    pub timestamp_ms: u128,
    pub git_rev: String,
    pub rustc: String,
    pub iterations: usize,
    pub example_files: usize,
    pub fixture_files: usize,
}

/// passed / total, the unit every validation section speaks in.
#[derive(Serialize, Clone, Copy)]
pub struct Rate {
    pub passed: usize,
    pub total: usize,
}

impl Rate {
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            self.passed as f64 * 100.0 / self.total as f64
        }
    }
}

#[derive(Serialize)]
pub struct Speed {
    /// One row per base example (english flavor), phase-split medians.
    pub per_example: Vec<ExampleTiming>,
    pub total_loc: usize,
    /// Sum of the per-example median pipeline times.
    pub total_ms: f64,
    pub loc_per_sec: f64,
}

#[derive(Serialize)]
pub struct ExampleTiming {
    pub name: String,
    /// Source lines across the example and its imports.
    pub loc: usize,
    pub load_ms: f64,
    pub check_ms: f64,
    pub emit_ms: f64,
}

#[derive(Serialize)]
pub struct Memory {
    /// Peak process resident set (MB) observed while compiling the whole
    /// corpus in one pass. Coarse but honest — the real OS-reported RSS
    /// high-water mark, not a per-allocation heap figure (that's the opt-in
    /// dhat profile, docs/Ideas/benchmark_plan.md Phase 3). 0.0 if the
    /// platform doesn't report RSS.
    pub peak_rss_mb: f64,
}

#[derive(Serialize)]
pub struct Accuracy {
    /// Emitted Verilog == tests/golden/<base>.v (banner stripped).
    pub golden: Rate,
    /// tanglish/tamil/mixed emit byte-identical Verilog to english.
    pub flavor_identity: Rate,
    /// `iverilog -t null` accepts every emitted file (None = not installed).
    pub iverilog_syntax: Option<Rate>,
    /// Self-checking testbenches reach PASS under vvp (None = not installed).
    pub testbenches: Option<Rate>,
    pub failures: Vec<String>,
}

#[derive(Serialize)]
pub struct Safety {
    /// Every error fixture produces its declared E-code.
    pub fixtures: Rate,
    /// ...and the diagnostic carries a teaching help line.
    pub help_lines: Rate,
    /// Every example checks CLEAN — the false-positive guard.
    pub clean_examples: Rate,
    pub failures: Vec<String>,
}

#[derive(Serialize)]
pub struct Coverage {
    /// Corpus completeness, computed with zero external tools.
    pub corpus: CorpusCoverage,
    /// Real line/function/region coverage from cargo-llvm-cov.
    pub llvm: Option<LlvmCov>,
    /// Why `llvm` is None (skipped / tool missing), shown in the report.
    pub llvm_note: Option<String>,
}

#[derive(Serialize)]
pub struct CorpusCoverage {
    /// Checker codes with at least one end-to-end fixture (n / 36).
    pub codes_with_fixture: Rate,
    pub examples_with_golden: Rate,
    pub examples_with_testbench: Rate,
    /// base × flavor files actually present on disk.
    pub flavor_completeness: Rate,
}

#[derive(Serialize)]
pub struct LlvmCov {
    pub line_percent: f64,
    pub function_percent: f64,
    pub region_percent: f64,
}

/// One line of `bench-history.jsonl` — the trend chart's data points.
#[derive(Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp_ms: u128,
    pub git_rev: String,
    pub total_ms: f64,
    pub loc_per_sec: f64,
    pub golden_pct: f64,
    pub fixture_pct: f64,
    pub llvm_line_pct: Option<f64>,
    /// Peak RSS (MB). `#[serde(default)]` so history lines written before
    /// this field existed still parse (they read back as None).
    #[serde(default)]
    pub peak_rss_mb: Option<f64>,
    // More validation rates for the trend chart. `#[serde(default)]` (None)
    // keeps pre-existing history lines parseable — they show as gaps.
    #[serde(default)]
    pub flavor_identity_pct: Option<f64>,
    #[serde(default)]
    pub clean_pct: Option<f64>,
    #[serde(default)]
    pub help_pct: Option<f64>,
}

impl HistoryEntry {
    pub fn from_report(r: &BenchReport) -> Self {
        HistoryEntry {
            timestamp_ms: r.meta.timestamp_ms,
            git_rev: r.meta.git_rev.clone(),
            total_ms: r.speed.total_ms,
            loc_per_sec: r.speed.loc_per_sec,
            golden_pct: r.accuracy.golden.percent(),
            fixture_pct: r.safety.fixtures.percent(),
            llvm_line_pct: r.coverage.llvm.as_ref().map(|l| l.line_percent),
            peak_rss_mb: Some(r.memory.peak_rss_mb),
            flavor_identity_pct: Some(r.accuracy.flavor_identity.percent()),
            clean_pct: Some(r.safety.clean_examples.percent()),
            help_pct: Some(r.safety.help_lines.percent()),
        }
    }
}

// ------------------------------------------------------------- pipeline

/// Run the full library pipeline (the exact `mimz compile` path:
/// load → check → transliterate → project → emit) and return the Verilog.
pub fn compile_to_verilog(path: &Path) -> Result<String, String> {
    let files = load(path)?;
    let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    checker::check(&asts).map_err(|d| diags_msg(path, &d))?;
    emit_verilog::transliterate(&mut asts);
    let proj = emit_verilog::Project::from_files(&asts).map_err(|d| diags_msg(path, &d))?;
    emit_verilog::emit(&proj, &asts).map_err(|d| diags_msg(path, &d))
}

pub(super) fn load(path: &Path) -> Result<Vec<project::LoadedFile>, String> {
    project::load_project(path).map_err(|e| match e {
        LoadError::Io(msg) => format!("{}: {msg}", path.display()),
        LoadError::Source { path, diags, .. } => diags_msg(&path, &diags),
    })
}

fn diags_msg(path: &Path, diags: &[mimz::diag::Diag]) -> String {
    let codes: Vec<&str> = diags.iter().map(|d| d.code.unwrap_or("E????")).collect();
    format!("{}: {}", path.display(), codes.join(", "))
}

/// Drop the `// Generated by mimz <version>` banner (same rule as the
/// golden test) so version bumps don't read as accuracy regressions.
pub fn strip_banner(v: &str) -> String {
    let mut lines = v.lines();
    let first = lines.next().unwrap_or("");
    if first.starts_with("// Generated by mimz") {
        let rest: Vec<&str> = lines.collect();
        format!("{}\n", rest.join("\n").trim_start_matches('\n'))
    } else {
        v.replace("\r\n", "\n")
    }
}

pub fn median(xs: &mut [f64]) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).expect("timings are finite"));
    if xs.is_empty() { 0.0 } else { xs[xs.len() / 2] }
}

/// Every `.mimz` under examples/, recursively, sorted (56 today).
pub fn all_example_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("examples/ exists") {
            let path = entry.expect("readable entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "mimz") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&repo().join("examples"), &mut files);
    files.sort();
    files
}

/// Parse the `// expect: Exxxx` header of one fixture (the convention
/// tests/errors.rs enforces). None when the header is missing/garbled.
pub fn expected_code(src: &str) -> Option<String> {
    let first = src.lines().next().unwrap_or("");
    first
        .strip_prefix("//")
        .and_then(|s| s.trim().strip_prefix("expect:"))
        .map(|s| s.trim().to_string())
        .filter(|c| {
            c.starts_with('E') && c.len() == 5 && c[1..].chars().all(|d| d.is_ascii_digit())
        })
}

/// Every fixture under tests/fixtures/errors/, sorted, with its code.
pub fn fixtures() -> Vec<(PathBuf, String)> {
    let dir = repo().join("tests").join("fixtures").join("errors");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("tests/fixtures/errors exists") {
        let path = entry.expect("readable entry").path();
        if path.extension().is_none_or(|e| e != "mimz") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("fixture readable");
        if let Some(code) = expected_code(&src) {
            out.push((path, code));
        }
    }
    out.sort();
    out
}

// ----------------------------------------------------------------- meta

pub fn collect_meta(iterations: usize) -> Meta {
    let cmd_line = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .current_dir(repo())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };
    Meta {
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        git_rev: cmd_line("git", &["rev-parse", "--short", "HEAD"]),
        rustc: cmd_line("rustc", &["--version"]),
        iterations,
        example_files: all_example_files().len(),
        fixture_files: fixtures().len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_percent_handles_zero_and_partial() {
        assert_eq!(
            Rate {
                passed: 0,
                total: 0
            }
            .percent(),
            100.0
        );
        assert_eq!(
            Rate {
                passed: 3,
                total: 4
            }
            .percent(),
            75.0
        );
    }

    #[test]
    fn expect_header_parses_only_the_convention() {
        assert_eq!(expected_code("// expect: E0101\n"), Some("E0101".into()));
        assert_eq!(expected_code("//expect: E0701"), Some("E0701".into()));
        assert_eq!(expected_code("// expect: 0101\n"), None);
        assert_eq!(expected_code("module M {}\n"), None);
    }

    #[test]
    fn banner_strip_matches_the_golden_rule() {
        let v = "// Generated by mimz 0.1.0\n\nmodule M;\n";
        assert_eq!(strip_banner(v), "module M;\n");
        assert_eq!(strip_banner("module M;\n"), "module M;\n");
    }

    #[test]
    fn median_is_the_middle_run() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut []), 0.0);
    }
}
