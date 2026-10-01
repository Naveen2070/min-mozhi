//! CLI-surface tests for the commands that carry real logic of their own:
//! `doctor` (status aggregation + exit code + in-memory pipeline smoke test),
//! `check --watch` (initial run + watch-mode entry) and `ir` (IR pipeline,
//! flags, classified failure reports). The other
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

// ---- ir -----------------------------------------------------------------

fn repo(rel: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// The library pipeline `mimz ir` must match: load, check, elaborate,
/// lower, optimize, print.
fn library_ir(rel: &str) -> String {
    let Ok(files) = mimz::project::load_project(&repo(rel)) else {
        panic!("loads {rel}")
    };
    let asts: Vec<mimz::ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    mimz::checker::check(&asts).expect("checks");
    let design = mimz::sim::elaborate::elaborate_project(&asts, None, &Default::default())
        .expect("elaborates");
    let mut m = mimz::ir::lower(&design);
    mimz::ir::opt::optimize(&mut m);
    mimz::ir::print_line::print(&m)
}

fn ir(args: &[&str]) -> std::process::Output {
    mimz().arg("ir").args(args).output().unwrap()
}

#[test]
fn ir_prints_the_optimized_line_form() {
    let out = ir(&[repo("examples/english/adder.mimz").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        library_ir("examples/english/adder.mimz")
    );
}

#[test]
fn ir_no_opt_keeps_what_the_optimizer_removes() {
    let path = repo("tests/fixtures/ir_cli/foldable.mimz");
    let path = path.to_str().unwrap();
    let opt = String::from_utf8_lossy(&ir(&[path]).stdout).into_owned();
    let raw = String::from_utf8_lossy(&ir(&[path, "--no-opt"]).stdout).into_owned();
    assert!(raw.contains("$mux"), "{raw}");
    assert!(!opt.contains("$mux"), "{opt}");
}

#[test]
fn ir_sexpr_prints_the_s_expression_form() {
    let out = ir(&[
        repo("examples/english/adder.mimz").to_str().unwrap(),
        "--sexpr",
    ]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .trim_start()
            .starts_with('(')
    );
}

#[test]
fn ir_module_picks_the_top_of_a_two_module_file() {
    let path = repo("examples/english/alu.mimz");
    let without = ir(&[path.to_str().unwrap()]);
    assert!(!without.status.success(), "two modules, no top named");
    let with = ir(&[path.to_str().unwrap(), "--module", "Top"]);
    assert!(
        with.status.success(),
        "{}",
        String::from_utf8_lossy(&with.stderr)
    );
    assert!(String::from_utf8_lossy(&with.stdout).starts_with("module Top"));
}

#[test]
fn ir_output_writes_the_file() {
    let dest = std::env::temp_dir().join(format!("mimz-ir-{}.ir", std::process::id()));
    let out = ir(&[
        repo("examples/english/adder.mimz").to_str().unwrap(),
        "-o",
        dest.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "IR goes to the file, not stdout");
    let text = std::fs::read_to_string(&dest).unwrap();
    std::fs::remove_file(&dest).ok();
    assert_eq!(text, library_ir("examples/english/adder.mimz"));
}

#[test]
fn ir_output_to_an_unwritable_path_is_a_clean_error() {
    let dest = repo("tests/fixtures/ir_cli/no-such-dir/out.ir");
    let out = ir(&[
        repo("examples/english/adder.mimz").to_str().unwrap(),
        "-o",
        dest.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("error: cannot write"));
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn ir_limitation_is_a_clean_error_with_an_underline() {
    let out = ir(&[repo("tests/fixtures/ir_cli/limitation.mimz")
        .to_str()
        .unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("IR lowering does not support this design yet"),
        "{err}"
    );
    assert!(err.contains("IR limitation"), "{err}");
    assert!(
        err.contains("wire h: Handshake"),
        "source line shown: {err}"
    );
    assert!(err.contains("not implemented"), "{err}");
    assert!(err.contains("lower.rs"), "panic site: {err}");
    assert!(err.contains("full text with -d"), "long message cut: {err}");
    assert!(!err.contains("panicked at"), "default hook silenced: {err}");
}

#[test]
fn ir_internal_error_names_the_bug_class() {
    let path = repo("tests/fixtures/ir_cli/internal.mimz");
    let out = ir(&[path.to_str().unwrap(), "--module", "Top"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("internal compiler error in IR lowering"),
        "{err}"
    );
    assert!(err.contains("compiler bug"), "{err}");
    assert!(err.contains("s__-1_y"), "{err}");
    assert!(err.contains("o[i] ="), "source line shown: {err}");
}

#[test]
fn ir_debug_adds_a_backtrace() {
    let path = repo("tests/fixtures/ir_cli/internal.mimz");
    let out = mimz()
        .args(["-d", "ir", path.to_str().unwrap(), "--module", "Top"])
        .output()
        .unwrap();
    assert!(stderr(&out).contains("backtrace:"), "{}", stderr(&out));
}

#[test]
fn ir_panic_flag_crashes_on_an_internal_error() {
    let path = repo("tests/fixtures/ir_cli/internal.mimz");
    let out = ir(&[path.to_str().unwrap(), "--module", "Top", "--panic"]);
    assert_eq!(out.status.code(), Some(101), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("s__-1_y"),
        "report printed before the crash"
    );
}

#[test]
fn ir_panic_flag_keeps_a_limitation_clean() {
    let path = repo("tests/fixtures/ir_cli/limitation.mimz");
    let out = ir(&[path.to_str().unwrap(), "--panic"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        !stderr(&out).contains("backtrace:"),
        "a limitation stays a clean error: {}",
        stderr(&out)
    );
}

#[test]
fn ir_extern_design_prints_no_simulation_warning() {
    let out = ir(&[repo("tests/fixtures/extern/pll.mimz").to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains("in simulation"),
        "mimz ir does not simulate: {}",
        stderr(&out)
    );
}
