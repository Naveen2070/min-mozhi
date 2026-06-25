//! Golden-match accuracy measurement: verify that each example file compiles to
//! the expected Verilog byte-for-byte (the reference `.v.golden` files), and
//! that all keyword-flavor variants produce identical output. Optionally runs
//! Icarus Verilog as a cross-check when installed.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{
    Accuracy, BASE_EXAMPLES, FLAVORS, Rate, TESTBENCHES, all_example_files, compile_to_verilog,
    repo, strip_banner,
};

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
