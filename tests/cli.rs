//! CLI-surface tests for the two new commands that carry real logic of their
//! own: `doctor` (status aggregation + exit code + in-memory pipeline smoke
//! test) and `check --watch` (initial run + watch-mode entry). The other
//! subcommands are either covered by their own files (`check`, `compile`,
//! `fmt`, `translate`, `eval`, `sim`, `test`, `lsp`) or are thin passthroughs
//! with nothing of ours to break (`explain` → lib catalog, already tested;
//! `completions` → generated entirely by clap_complete), so they are left be.

use std::process::Command;

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

// ---- doctor / env -------------------------------------------------------

/// `mimz doctor` runs the in-memory pipeline smoke test, reports the standard
/// sections, and exits 0 (optional tools missing are warnings, not failures;
/// there is no root `mimz.toml`, and the temp dir is writable on any sane CI).
#[test]
fn doctor_reports_sections_and_pipeline_ok() {
    let out = mimz().arg("doctor").output().unwrap();
    assert!(
        out.status.success(),
        "doctor should exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("Compiler"), "missing Compiler section: {s}");
    assert!(
        s.contains("in-memory compile OK"),
        "pipeline smoke test should pass: {s}"
    );
    assert!(
        s.contains("Environment"),
        "missing Environment section: {s}"
    );
}

/// `--dev` adds the contributor toolchain section (still exits 0 — missing dev
/// tools are warnings).
#[test]
fn doctor_dev_adds_developer_section() {
    let out = mimz().args(["doctor", "--dev"]).output().unwrap();
    assert!(out.status.success(), "doctor --dev should exit 0");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("Developer toolchain"),
        "missing Developer toolchain section: {s}"
    );
}

/// `mimz env` is the documented alias for `mimz doctor`.
#[test]
fn env_is_an_alias_for_doctor() {
    let out = mimz().arg("env").output().unwrap();
    assert!(out.status.success(), "env alias should exit 0");
    assert!(String::from_utf8_lossy(&out.stdout).contains("Compiler"));
}

// ---- init ---------------------------------------------------------------

/// `mimz init <name>` scaffolds `<name>/mimz.toml` + `<name>/<name>.mimz`, and
/// the starter design must pass its own inline `test` out of the box — this is
/// the contract that keeps the scaffold valid as the language evolves.
#[test]
fn init_scaffolds_a_project_that_passes_its_own_test() {
    let base = std::env::temp_dir().join(format!("mimz_init_{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let name = "demo_proj";

    let out = mimz()
        .current_dir(&base)
        .args(["init", name])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let proj = base.join(name);
    assert!(proj.join("mimz.toml").is_file(), "mimz.toml not created");
    let design = proj.join(format!("{name}.mimz"));
    assert!(design.is_file(), "starter .mimz not created");

    let t = mimz().args(["test"]).arg(&design).output().unwrap();
    assert!(
        t.status.success(),
        "generated project should pass its test:\n{}\n{}",
        String::from_utf8_lossy(&t.stdout),
        String::from_utf8_lossy(&t.stderr)
    );

    std::fs::remove_dir_all(&base).ok();
}

/// `init` must not clobber an existing non-empty directory.
#[test]
fn init_refuses_to_clobber_a_non_empty_dir() {
    let base = std::env::temp_dir().join(format!("mimz_init_clobber_{}", std::process::id()));
    let proj = base.join("taken");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("keep.txt"), b"existing").unwrap();

    let out = mimz()
        .current_dir(&base)
        .args(["init", "taken"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "init should refuse a non-empty dir");
    assert!(
        proj.join("keep.txt").is_file(),
        "existing file must be untouched"
    );

    std::fs::remove_dir_all(&base).ok();
}

// ---- check --watch ------------------------------------------------------

/// `check --watch` runs the initial check and enters watch mode (announcing the
/// watch set), then blocks. We can't drive filesystem events deterministically
/// in a unit test, so this just asserts startup: the initial `OK` and the
/// `watching …` banner appear, then we kill it. Gated on the `watch` feature so
/// a `--no-default-features` build (which prints a "no watch support" error
/// instead) doesn't fail here.
#[cfg(feature = "watch")]
#[test]
fn watch_starts_and_enters_watch_mode() {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Duration;

    let dir = std::env::temp_dir().join(format!("mimz_cli_watch_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("w.mimz");
    std::fs::write(&f, "module Top {\n  out led: bits[1]\n  led = 0\n}\n").unwrap();

    let mut child = mimz()
        .arg("check")
        .arg(&f)
        .arg("--watch")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // The initial check + "watching" banner are printed immediately, before the
    // event loop blocks — 800ms is plenty even on a loaded CI box.
    std::thread::sleep(Duration::from_millis(800));
    child.kill().unwrap();
    let _ = child.wait();

    let mut out = String::new();
    let mut err = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();

    assert!(
        out.contains("OK"),
        "initial check should report OK; stdout={out}"
    );
    assert!(
        err.contains("watching"),
        "should announce watch mode; stderr={err}"
    );
}
