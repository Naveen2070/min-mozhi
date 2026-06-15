//! `mimz fmt` — the in-place keyword-flavor normalizer. It is the lossless,
//! comment-preserving token reskin (the `translate` path), so these tests assert
//! the workflow contracts: normalization to one flavor, idempotency, comment
//! preservation, `--to` override, and the `--strict` mixed-flavor lint.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

fn temp_mimz(src: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "mimz_fmt_{}.mimz",
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&p, src).unwrap();
    p
}

/// Run `mimz fmt <file> [args]`; return (success, stdout, stderr).
fn run_fmt(path: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let out = mimz().arg("fmt").arg(path).args(args).output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `module` (english) + `veliyeedu` (tanglish `out`) — a deliberately mixed file
/// with a comment and an identifier to prove they survive.
const MIXED: &str = "module M {  // keep me\n  in a: bit\n  veliyeedu y: bit\n  y = a\n}\n";

#[test]
fn normalizes_to_majority_and_is_idempotent() {
    let path = temp_mimz(MIXED);
    let (ok, _, _) = run_fmt(&path, &[]);
    assert!(ok, "plain fmt succeeds");
    let once = fs::read_to_string(&path).unwrap();
    // Majority flavor is english (module + in vs the single tanglish `veliyeedu`).
    assert!(
        once.contains("out y: bit"),
        "veliyeedu normalized to out:\n{once}"
    );
    assert!(!once.contains("veliyeedu"), "no tanglish keyword survives");
    // Comment + identifiers preserved (lossless token reskin).
    assert!(once.contains("// keep me"), "comment preserved");
    // Idempotent: a second run changes nothing.
    run_fmt(&path, &[]);
    assert_eq!(
        once,
        fs::read_to_string(&path).unwrap(),
        "fmt not idempotent"
    );
}

#[test]
fn to_flag_forces_the_target_flavor() {
    let path = temp_mimz(MIXED);
    let (ok, _, _) = run_fmt(&path, &["--to", "tamil"]);
    assert!(ok);
    let out = fs::read_to_string(&path).unwrap();
    assert!(out.contains("தொகுதி M"), "module → Tamil:\n{out}");
    assert!(out.contains("// keep me"), "comment preserved under --to");
}

#[test]
fn strict_warns_and_fails_on_mixed_but_still_fixes() {
    let path = temp_mimz(MIXED);
    let (ok, _, stderr) = run_fmt(&path, &["--strict"]);
    assert!(!ok, "--strict exits non-zero on a mixed file");
    assert!(
        stderr.contains("mixes keyword flavors"),
        "strict warns about the mix:\n{stderr}"
    );
    // It still normalized the file (the fix is applied).
    assert!(fs::read_to_string(&path).unwrap().contains("out y: bit"));
}

#[test]
fn strict_is_clean_on_a_single_flavor_file() {
    let path = temp_mimz("module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n");
    let (ok, _, stderr) = run_fmt(&path, &["--strict"]);
    assert!(ok, "single-flavor file passes --strict");
    assert!(!stderr.contains("mixes"), "no mix warning expected");
}

#[test]
fn a_keyword_free_file_is_left_intact() {
    // No keywords → majority is English; the token reskin changes nothing.
    let src = "// just a comment, no code\n";
    let path = temp_mimz(src);
    let (ok, _, _) = run_fmt(&path, &[]);
    assert!(ok);
    assert_eq!(fs::read_to_string(&path).unwrap(), src, "nothing to change");
}

#[test]
fn a_non_lexing_file_is_a_clean_error() {
    // `/` is a hard lexer error (division is rejected) — fmt must report it, not
    // panic, and must not clobber the file.
    let src = "module M {\n  wire w: bit = a / b\n}\n";
    let path = temp_mimz(src);
    let (ok, _, err) = run_fmt(&path, &[]);
    assert!(!ok, "must fail on a lex error");
    assert!(err.contains("error"), "reports the diagnostic: {err}");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        src,
        "input not clobbered"
    );
}

#[test]
fn output_flag_leaves_the_input_untouched() {
    let path = temp_mimz(MIXED);
    let dest = temp_mimz(""); // reuse the unique-name helper for a dest path
    let (ok, _, _) = run_fmt(&path, &["--to", "tamil", "-o", dest.to_str().unwrap()]);
    assert!(ok);
    assert_eq!(fs::read_to_string(&path).unwrap(), MIXED, "input untouched");
    assert!(fs::read_to_string(&dest).unwrap().contains("தொகுதி"));
}

#[test]
fn output_to_the_input_path_round_trips() {
    // `-o <input>` is the explicit form of an in-place format; the atomic
    // temp-file + rename must produce exactly what a plain `fmt` does, with no
    // leftover temp file in the directory.
    let path = temp_mimz(MIXED);
    let (ok, _, _) = run_fmt(&path, &["--to", "tamil", "-o", path.to_str().unwrap()]);
    assert!(ok, "fmt -o <input> succeeds");
    let out = fs::read_to_string(&path).unwrap();
    assert!(out.contains("தொகுதி M"), "module → Tamil in place:\n{out}");
    assert!(out.contains("// keep me"), "comment preserved");
    // No stray `<input>.<pid>.tmp` sibling left behind.
    let dir = path.parent().unwrap();
    let stem = path.file_name().unwrap().to_string_lossy().into_owned();
    let leftover = fs::read_dir(dir).unwrap().any(|e| {
        let n = e.unwrap().file_name().to_string_lossy().into_owned();
        n.starts_with(&stem) && n.ends_with(".tmp")
    });
    assert!(
        !leftover,
        "temp file should be renamed away, not left behind"
    );
}

#[test]
fn unknown_to_flavor_is_a_clean_error() {
    let src = "module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n";
    let path = temp_mimz(src);
    let (ok, _, err) = run_fmt(&path, &["--to", "klingon"]);
    assert!(!ok, "an unknown --to value exits non-zero");
    assert!(
        err.contains("unknown flavor"),
        "reports the bad flavor: {err}"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        src,
        "input not clobbered on a bad --to"
    );
}
