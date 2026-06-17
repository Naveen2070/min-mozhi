//! `cargo test-summary` — run the whole test suite, then print a per-binary
//! breakdown (lib unit, each bin's unit tests, every `tests/<file>.rs`
//! integration suite, and doctests) and a grand total.
//!
//! A dev helper, invoked via the `.cargo/config.toml` alias
//! (`cargo test-summary [args]`). All args are forwarded to `cargo test`
//! (`--release`, `--test sim`, …), the env is inherited (so `REQUIRE_IVERILOG`
//! works), cargo's own output is streamed live, and the process exits with
//! cargo's status — so it still fails CI on a red test.
//!
//! Cross-platform (std only, no `mimz` deps): one implementation for Windows,
//! macOS, and Linux. It pairs each cargo `Running …` / `Doc-tests …` descriptor
//! (stderr) with the next `test result:` line (stdout); cargo runs test binaries
//! sequentially, so the i-th descriptor matches the i-th result.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;

/// A clean label for a cargo `Running <desc>` / `Doc-tests` descriptor.
fn label(desc: &str) -> String {
    if let Some(src) = desc.strip_prefix("unittests ") {
        let src = src.trim();
        // src/bin/<name>/… → "<name> (bin unit)"
        let mut parts = src.split(['/', '\\']).skip_while(|s| *s != "bin");
        if parts.next().is_some() {
            if let Some(name) = parts.next() {
                return format!("{name} (bin unit)");
            }
        }
        if src.ends_with("main.rs") {
            return "mimz (bin unit)".to_string(); // the default `mimz` binary
        }
        return "lib (unit)".to_string(); // src/lib.rs
    }
    // integration suite: tests/<name>.rs
    let name = desc
        .trim_start_matches("tests/")
        .trim_start_matches("tests\\")
        .strip_suffix(".rs")
        .unwrap_or(desc);
    format!("{name} (integration)")
}

/// The descriptor a stderr line introduces, if any.
fn descriptor(line: &str) -> Option<String> {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("Running ") {
        let d = rest.split(" (").next().unwrap_or(rest).trim();
        return Some(label(d));
    }
    if t.starts_with("Doc-tests") {
        return Some("doctests".to_string());
    }
    None
}

/// `(passed, failed)` from a `test result: ok. N passed; M failed; …` line.
fn parse_result(line: &str) -> Option<(u64, u64)> {
    let t = line.trim_start();
    if !t.starts_with("test result:") {
        return None;
    }
    let toks: Vec<&str> = t.split_whitespace().collect();
    let (mut passed, mut failed) = (0u64, 0u64);
    for w in toks.windows(2) {
        if w[1].starts_with("passed") {
            passed = w[0].parse().unwrap_or(0);
        }
        if w[1].starts_with("failed") {
            failed = w[0].parse().unwrap_or(0);
        }
    }
    Some((passed, failed))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut child = Command::new("cargo")
        .arg("test")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `cargo test` — is cargo on PATH?");

    // Stream stderr live + collect descriptors (the "Running …" lines).
    let stderr = child.stderr.take().expect("piped stderr");
    let stderr_thread = thread::spawn(move || {
        let mut descs = Vec::new();
        let err = std::io::stderr();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = writeln!(err.lock(), "{line}");
            if let Some(d) = descriptor(&line) {
                descs.push(d);
            }
        }
        descs
    });

    // Stream stdout live + collect results.
    let stdout = child.stdout.take().expect("piped stdout");
    let mut results = Vec::new();
    let out = std::io::stdout();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let _ = writeln!(out.lock(), "{line}");
        if let Some(r) = parse_result(&line) {
            results.push(r);
        }
    }

    let descs = stderr_thread.join().unwrap_or_default();
    let status = child.wait().expect("failed to wait on cargo");

    let (mut total, mut total_failed) = (0u64, 0u64);
    println!("\n================ test summary ================");
    for (i, &(passed, failed)) in results.iter().enumerate() {
        let name = descs
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("suite {i}"));
        total += passed;
        total_failed += failed;
        if failed > 0 {
            println!("  {name:<30} {passed:>5} passed   {failed} FAILED");
        } else {
            println!("  {name:<30} {passed:>5} passed");
        }
    }
    println!("  {:<30} {:>5}", "", "-----");
    if total_failed > 0 {
        println!(
            "  {:<30} {total:>5} passed   {total_failed} FAILED",
            "TOTAL"
        );
    } else {
        println!("  {:<30} {total:>5} passed", "TOTAL");
    }
    println!("==============================================");

    std::process::exit(status.code().unwrap_or(1));
}
