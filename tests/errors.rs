//! End-to-end error validation: run the REAL `mimz` binary on intentionally
//! broken `.mimz` fixtures and confirm it (a) exits non-zero and (b) prints
//! the correct stable error code. The checker unit tests
//! (`src/checker/tests.rs`) prove the checker FUNCTION rejects bad code; this
//! proves the CLI SURFACES that rejection to a user, error code and all.
//!
//! Each fixture under `tests/fixtures/errors/` is parse-clean (so the checker
//! runs and produces the TARGET error, not a parse error) and declares its
//! expected code in a header comment: `// expect: E0401`. See the README in
//! that folder for the convention.

use std::path::PathBuf;
use std::process::Command;

/// Every stable checker error code (docs/code/11-checker.md). The corpus must
/// exercise each one end-to-end; `error_corpus_covers_every_checker_code`
/// fails if any is missing a fixture, so a new code cannot ship without one.
const ALL_CHECKER_CODES: [&str; 36] = [
    "E0001", "E0002", "E0003", "E0004", "E0101", "E0102", "E0103", "E0104", "E0105", "E0106",
    "E0107", "E0108", "E0109", "E0201", "E0202", "E0301", "E0302", "E0303", "E0401", "E0402",
    "E0403", "E0404", "E0405", "E0406", "E0407", "E0408", "E0409", "E0410", "E0501", "E0502",
    "E0503", "E0504", "E0505", "E0601", "E0602", "E0701",
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("errors")
}

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Every `.mimz` fixture, sorted, as (path, expected-code) — the code read
/// from the `// expect: Exxxx` header. Panics on a missing/garbled header so
/// the convention is enforced.
fn fixtures() -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(fixtures_dir()).expect("fixtures/errors exists") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "mimz") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let first = src.lines().next().unwrap_or("");
        let code = first
            .strip_prefix("//")
            .and_then(|s| s.trim().strip_prefix("expect:"))
            .map(|s| s.trim().to_string())
            .filter(|c| {
                c.starts_with('E') && c.len() == 5 && c[1..].chars().all(|d| d.is_ascii_digit())
            })
            .unwrap_or_else(|| {
                panic!(
                    "{}: first line must be `// expect: Exxxx`, found {first:?}",
                    path.display()
                )
            });
        out.push((path, code));
    }
    out.sort();
    out
}

/// The core contract: every fixture, run through `mimz check`, must FAIL with
/// its declared code printed to stderr.
#[test]
fn every_error_fixture_reports_its_code() {
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= 60,
        "expected the full error corpus (~67), found {}",
        fixtures.len()
    );
    for (path, code) in fixtures {
        let out = mimz().arg("check").arg(&path).output().unwrap();
        assert!(
            !out.status.success(),
            "{} declares {code} but `mimz check` SUCCEEDED — the fixture is not broken",
            path.display()
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("error[{code}]")),
            "{} should report {code}, but stderr was:\n{stderr}",
            path.display()
        );
        // The teaching contract end-to-end: every checker error carries a
        // help line (`Checker::err` makes it structurally mandatory; this
        // proves the CLI actually PRINTS it).
        assert!(
            stderr.contains("help:"),
            "{} reported {code} without a help line:\n{stderr}",
            path.display()
        );
    }
}

/// The `--json` wire contract (docs/code/06): stdout is ALWAYS one JSON
/// array — diagnostics with code/path/line/col on failure, empty on
/// success — so editors and the future npm/PyPI wrappers never parse
/// human text. Exercises a checker error, a LEXER error (the E10xx
/// retrofit through the real CLI), and a clean run.
#[test]
fn json_flag_emits_machine_readable_diagnostics() {
    // Checker error: E0101 fixture.
    let fixture = fixtures_dir().join("e0101_unknown_name.mimz");
    let out = mimz()
        .arg("check")
        .arg(&fixture)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");
    let arr = v.as_array().expect("a JSON array");
    assert_eq!(arr[0]["code"], "E0101");
    assert!(
        arr[0]["path"]
            .as_str()
            .unwrap()
            .contains("e0101_unknown_name"),
        "path field resolves the file"
    );
    assert!(arr[0]["line"].as_u64().unwrap() >= 1);
    assert!(
        arr[0]["help"].as_str().is_some(),
        "the teaching line rides along"
    );

    // Lexer error: division, straight through load_project's error path.
    let div = std::env::temp_dir().join("mimz_json_div.mimz");
    std::fs::write(
        &div,
        "module M {\n  in a: bit\n  out y: bit\n  y = a / a\n}\n",
    )
    .unwrap();
    let out = mimz()
        .arg("check")
        .arg(&div)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v[0]["code"], "E1006");

    // Clean run: an empty array and success.
    let ok = mimz()
        .arg("check")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("english")
                .join("counter.mimz"),
        )
        .arg("--json")
        .output()
        .unwrap();
    assert!(ok.status.success());
    assert_eq!(String::from_utf8_lossy(&ok.stdout).trim(), "[]");
}

/// `ALL_CHECKER_CODES` must mirror the human catalog in
/// docs/code/11-checker.md — same docs-sync idea as `tests/docs_sync.rs`:
/// a code added to the catalog without a fixture (or vice versa) fails by
/// name. Rows whose meaning says "reserved" are tombstones/placeholders
/// and are exempt.
#[test]
fn checker_code_list_matches_the_catalog() {
    let doc = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs")
            .join("code")
            .join("11-checker.md"),
    )
    .expect("docs/code/11-checker.md exists");
    let mut catalog: Vec<&str> = Vec::new();
    for line in doc.lines() {
        // Catalog rows look like `| E0101 | meaning... | fix... |`.
        let Some(rest) = line.strip_prefix("| E") else {
            continue;
        };
        let Some((digits, meaning)) = rest.split_once(' ') else {
            continue;
        };
        if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if meaning.contains("reserved") {
            continue;
        }
        catalog.push(&line[2..7]);
    }
    assert_eq!(
        catalog,
        ALL_CHECKER_CODES.to_vec(),
        "ALL_CHECKER_CODES (tests/errors.rs) and the catalog table \
         (docs/code/11-checker.md) disagree — update whichever is stale"
    );
}

/// Completeness guard: every stable checker code has at least one end-to-end
/// fixture. A new E-code cannot land without one (docs-sync spirit).
#[test]
fn error_corpus_covers_every_checker_code() {
    let covered: std::collections::HashSet<String> =
        fixtures().into_iter().map(|(_, c)| c).collect();
    let missing: Vec<&str> = ALL_CHECKER_CODES
        .iter()
        .copied()
        .filter(|c| !covered.contains(*c))
        .collect();
    assert!(
        missing.is_empty(),
        "these checker codes have no end-to-end error fixture: {missing:?}\n\
         add one under tests/fixtures/errors/ (see the README there)"
    );
}
