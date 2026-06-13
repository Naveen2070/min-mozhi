//! The measurement engine: every benchmark section produces plain
//! serializable data here; rendering (HTML/console) happens elsewhere.
//! Mirrors the corpus conventions of the integration tests
//! (tests/examples.rs, tests/errors.rs, tests/icarus.rs) — the few small
//! private helpers they keep (banner strip, fixture headers, iverilog
//! detection) are re-implemented here, byte-compatible.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use mimz::diag::ALL_CHECKER_CODES;
use mimz::project::LoadError;
use mimz::{ast, checker, emit_verilog, project};

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

fn load(path: &Path) -> Result<Vec<project::LoadedFile>, String> {
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

// ---------------------------------------------------------------- speed

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

pub fn median(xs: &mut [f64]) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).expect("timings are finite"));
    if xs.is_empty() { 0.0 } else { xs[xs.len() / 2] }
}

// --------------------------------------------------------------- memory

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

// ------------------------------------------------------------- accuracy

/// Golden match + flavor identity (always) and the two Icarus layers
/// (when installed and not `--no-icarus`).
pub fn measure_accuracy(run_icarus: bool) -> Accuracy {
    let mut failures = Vec::new();

    // Golden files: english output must match tests/golden/<base>.v.
    let golden_dir = repo().join("tests").join("golden");
    let mut golden = Rate {
        passed: 0,
        total: 0,
    };
    for base in BASE_EXAMPLES {
        golden.total += 1;
        let path = repo()
            .join("examples")
            .join("english")
            .join(format!("{base}.mimz"));
        let golden_path = golden_dir.join(format!("{}.v", base.replace('/', "_")));
        let got = compile_to_verilog(&path).map(|v| strip_banner(&v));
        let want = std::fs::read_to_string(&golden_path).map(|s| s.replace("\r\n", "\n"));
        match (got, want) {
            (Ok(g), Ok(w)) if g == w => golden.passed += 1,
            (Ok(_), Ok(_)) => failures.push(format!("golden mismatch: {base}")),
            (Err(e), _) => failures.push(format!("golden compile failed: {e}")),
            (_, Err(_)) => failures.push(format!("missing golden: {}", golden_path.display())),
        }
    }

    // Flavor byte-identity: 3 comparisons per base, against english.
    let mut flavor_identity = Rate {
        passed: 0,
        total: 0,
    };
    for base in BASE_EXAMPLES {
        let reference = compile_to_verilog(
            &repo()
                .join("examples")
                .join("english")
                .join(format!("{base}.mimz")),
        );
        for flavor in &FLAVORS[1..] {
            flavor_identity.total += 1;
            let v = compile_to_verilog(
                &repo()
                    .join("examples")
                    .join(flavor)
                    .join(format!("{base}.mimz")),
            );
            match (&reference, v) {
                (Ok(r), Ok(v)) if *r == v => flavor_identity.passed += 1,
                (Ok(_), Ok(_)) => failures.push(format!("flavor differs: {flavor}/{base}")),
                _ => failures.push(format!("flavor compile failed: {flavor}/{base}")),
            }
        }
    }

    let (iverilog_syntax, testbenches) = if run_icarus {
        match iverilog_bin() {
            Some(bin) => {
                let (s, t) = run_icarus_layers(&bin, &mut failures);
                (Some(s), Some(t))
            }
            None => (None, None),
        }
    } else {
        (None, None)
    };

    Accuracy {
        golden,
        flavor_identity,
        iverilog_syntax,
        testbenches,
        failures,
    }
}

/// Locate Icarus exactly like tests/icarus.rs: `MIMZ_IVERILOG` (dir or
/// exe) → PATH → the Windows installer default. None = not installed.
pub fn iverilog_bin() -> Option<PathBuf> {
    let exe = |dir: &Path| dir.join(format!("iverilog{}", std::env::consts::EXE_SUFFIX));
    if let Ok(p) = std::env::var("MIMZ_IVERILOG") {
        let p = PathBuf::from(p);
        let dir = if p.is_file() {
            p.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            p
        };
        return exe(&dir).exists().then_some(dir);
    }
    if Command::new("iverilog")
        .arg("-V")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some(PathBuf::new()); // empty = resolve via PATH
    }
    let default = PathBuf::from(r"C:\iverilog\bin");
    if cfg!(windows) && exe(&default).exists() {
        return Some(default);
    }
    None
}

fn tool(bin: &Path, name: &str) -> Command {
    if bin.as_os_str().is_empty() {
        Command::new(name)
    } else {
        Command::new(bin.join(name))
    }
}

/// Write one compiled example to a unique temp `.v` for the Icarus runs.
fn emit_to_temp(path: &Path) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let v = compile_to_verilog(path)?;
    let name = path.display().to_string().replace(['\\', '/', ':'], "_");
    let out = std::env::temp_dir().join(format!(
        "mimz_bench_{}_{name}.v",
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&out, v).map_err(|e| e.to_string())?;
    Ok(out)
}

fn run_icarus_layers(bin: &Path, failures: &mut Vec<String>) -> (Rate, Rate) {
    // Layer 1: every emitted .v in the corpus passes `iverilog -t null`.
    let mut syntax = Rate {
        passed: 0,
        total: 0,
    };
    for path in all_example_files() {
        syntax.total += 1;
        let ok = emit_to_temp(&path).is_ok_and(|v| {
            tool(bin, "iverilog")
                .args(["-t", "null"])
                .arg(&v)
                .output()
                .is_ok_and(|o| o.status.success())
        });
        if ok {
            syntax.passed += 1;
        } else {
            failures.push(format!("iverilog rejected: {}", path.display()));
        }
    }

    // Layer 2: the self-checking testbenches reach PASS under vvp.
    let mut tbs = Rate {
        passed: 0,
        total: 0,
    };
    for (tb_file, example) in TESTBENCHES {
        tbs.total += 1;
        let tb = repo().join("tests").join("icarus").join(tb_file);
        let tb_module = tb_file.trim_end_matches(".v");
        let design = match emit_to_temp(&repo().join("examples").join(example)) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("testbench design failed: {e}"));
                continue;
            }
        };
        // Per-process path so two bench runs (or two users on a shared host)
        // cannot clobber each other's output or be pre-created via symlink.
        let vvp_out =
            std::env::temp_dir().join(format!("mimz_bench_{}_{tb_module}.vvp", std::process::id()));
        let built = tool(bin, "iverilog")
            .arg("-o")
            .arg(&vvp_out)
            .args(["-s", tb_module])
            .arg(&tb)
            .arg(&design)
            .output()
            .is_ok_and(|o| o.status.success());
        if !built {
            failures.push(format!("iverilog failed on {tb_file}"));
            continue;
        }
        let sim = tool(bin, "vvp").arg(&vvp_out).output();
        let passed = sim.is_ok_and(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            o.status.success() && stdout.contains("PASS") && !stdout.contains("FAIL")
        });
        if passed {
            tbs.passed += 1;
        } else {
            failures.push(format!("testbench FAIL: {tb_module}"));
        }
    }
    (syntax, tbs)
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

// --------------------------------------------------------------- safety

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

/// The safety contract, via the lib (no subprocess): every fixture's
/// diagnostics contain its declared code WITH a help line, and every
/// example checks clean (the false-positive guard).
pub fn measure_safety() -> Safety {
    let mut failures = Vec::new();
    let mut fixture_rate = Rate {
        passed: 0,
        total: 0,
    };
    let mut help_rate = Rate {
        passed: 0,
        total: 0,
    };
    for (path, code) in fixtures() {
        fixture_rate.total += 1;
        help_rate.total += 1;
        let diags = check_diags(&path);
        let hit = diags.iter().find(|d| d.code == Some(code.as_str()));
        match hit {
            Some(d) => {
                fixture_rate.passed += 1;
                if d.help.is_some() {
                    help_rate.passed += 1;
                } else {
                    failures.push(format!(
                        "{code} fired without a help line: {}",
                        file_name(&path)
                    ));
                }
            }
            None => failures.push(format!(
                "fixture expected {code}, got [{}]: {}",
                diags
                    .iter()
                    .map(|d| d.code.unwrap_or("E????"))
                    .collect::<Vec<_>>()
                    .join(", "),
                file_name(&path)
            )),
        }
    }

    let mut clean = Rate {
        passed: 0,
        total: 0,
    };
    for path in all_example_files() {
        clean.total += 1;
        if check_diags(&path).is_empty() {
            clean.passed += 1;
        } else {
            failures.push(format!("false positive on example: {}", path.display()));
        }
    }

    Safety {
        fixtures: fixture_rate,
        help_lines: help_rate,
        clean_examples: clean,
        failures,
    }
}

/// All diagnostics for one file: load errors (lexer/parser) or checker
/// errors — empty means it checks clean.
fn check_diags(path: &Path) -> Vec<mimz::diag::Diag> {
    match project::load_project(path) {
        Ok(files) => {
            let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
            checker::check(&asts).err().unwrap_or_default()
        }
        Err(LoadError::Source { diags, .. }) => diags,
        Err(LoadError::Io(_)) => Vec::new(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

// ------------------------------------------------------------- coverage

/// Corpus completeness (always) + cargo-llvm-cov line coverage (when the
/// tool is installed and not `--no-cov`).
pub fn measure_coverage(run_llvm_cov: bool) -> Coverage {
    let covered: std::collections::HashSet<String> =
        fixtures().into_iter().map(|(_, c)| c).collect();
    let corpus = CorpusCoverage {
        codes_with_fixture: Rate {
            passed: ALL_CHECKER_CODES
                .iter()
                .filter(|c| covered.contains(**c))
                .count(),
            total: ALL_CHECKER_CODES.len(),
        },
        examples_with_golden: Rate {
            passed: BASE_EXAMPLES
                .iter()
                .filter(|b| {
                    repo()
                        .join("tests")
                        .join("golden")
                        .join(format!("{}.v", b.replace('/', "_")))
                        .exists()
                })
                .count(),
            total: BASE_EXAMPLES.len(),
        },
        examples_with_testbench: Rate {
            passed: TESTBENCHES.len(),
            total: BASE_EXAMPLES.len(),
        },
        flavor_completeness: Rate {
            passed: BASE_EXAMPLES
                .iter()
                .flat_map(|b| FLAVORS.iter().map(move |f| (b, f)))
                .filter(|(b, f)| {
                    repo()
                        .join("examples")
                        .join(f)
                        .join(format!("{b}.mimz"))
                        .exists()
                })
                .count(),
            total: BASE_EXAMPLES.len() * FLAVORS.len(),
        },
    };

    let (llvm, llvm_note) = if !run_llvm_cov {
        (None, Some("skipped (--no-cov)".to_string()))
    } else {
        match run_cargo_llvm_cov() {
            Ok(l) => (Some(l), None),
            Err(note) => (None, Some(note)),
        }
    };

    Coverage {
        corpus,
        llvm,
        llvm_note,
    }
}

/// Shell out to `cargo llvm-cov --json --summary-only` (runs the whole
/// instrumented test suite — minutes, not seconds) and pull the totals.
fn run_cargo_llvm_cov() -> Result<LlvmCov, String> {
    let probe = Command::new("cargo")
        .args(["llvm-cov", "--version"])
        .output();
    if !probe.is_ok_and(|o| o.status.success()) {
        return Err(
            "cargo-llvm-cov not installed — `cargo install cargo-llvm-cov` \
             (plus `rustup component add llvm-tools-preview`)"
                .to_string(),
        );
    }
    let out = Command::new("cargo")
        .args(["llvm-cov", "--json", "--summary-only"])
        .current_dir(repo())
        .output()
        .map_err(|e| format!("cargo llvm-cov failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo llvm-cov failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("cargo llvm-cov output was not JSON: {e}"))?;
    let totals = &v["data"][0]["totals"];
    let pct = |k: &str| totals[k]["percent"].as_f64().unwrap_or(0.0);
    Ok(LlvmCov {
        line_percent: pct("lines"),
        function_percent: pct("functions"),
        region_percent: pct("regions"),
    })
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
