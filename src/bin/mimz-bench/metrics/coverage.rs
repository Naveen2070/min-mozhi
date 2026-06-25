//! Coverage metrics: measure what fraction of the language spec and diagnostic
//! codes are exercised by the test suite (corpus coverage), plus optional
//! LLVM source-based line coverage via `cargo-llvm-cov`.

use std::process::Command;

use mimz::diag::ALL_CHECKER_CODES;

use super::{
    BASE_EXAMPLES, CorpusCoverage, Coverage, FLAVORS, LlvmCov, Rate, TESTBENCHES, fixtures, repo,
};

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
