//! End-to-end tests for `mimz.toml` config defaults and name-map
//! auto-discovery — driven through the real binary (the merge + discovery only
//! matter at the CLI layer; the parser/precedence units live in `src/config.rs`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A fresh temp dir unique to this process + call (tests run in parallel).
fn work_dir(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mimz_cfg_it_{tag}_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn stdout_of(out: std::process::Output) -> String {
    assert!(
        out.status.success(),
        "command failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A reverse translate auto-loads the `<input>.names.json` sidecar with no flag,
/// restoring the original Tamil names.
#[test]
fn auto_name_map_restores_without_a_flag() {
    let dir = work_dir("auto");
    let romanized = dir.join("k.mimz");
    // Forward: romanize + write the sidecar.
    let fwd = mimz()
        .arg("translate")
        .arg(repo().join("examples/tamil-pure/kanakki.mimz"))
        .args(["--to", "tanglish", "--romanize-names", "-o"])
        .arg(&romanized)
        .output()
        .unwrap();
    assert!(fwd.status.success());
    assert!(
        dir.join("k.mimz.names.json").is_file(),
        "forward run must write the sidecar"
    );

    // Reverse: no --names-map; the sidecar is discovered automatically.
    let out = stdout_of(
        mimz()
            .arg("translate")
            .arg(&romanized)
            .args(["--to", "tamil"])
            .output()
            .unwrap(),
    );
    assert!(
        out.contains("தொகுதி கணக்கி"),
        "auto-discovered map should restore Tamil names, got:\n{out}"
    );
    fs::remove_dir_all(&dir).ok();
}

/// `--no-names-map` opts out of auto-discovery: the romanized Latin names stay.
#[test]
fn no_names_map_keeps_latin_names() {
    let dir = work_dir("noauto");
    let romanized = dir.join("k.mimz");
    mimz()
        .arg("translate")
        .arg(repo().join("examples/tamil-pure/kanakki.mimz"))
        .args(["--to", "tanglish", "--romanize-names", "-o"])
        .arg(&romanized)
        .output()
        .unwrap();

    let out = stdout_of(
        mimz()
            .arg("translate")
            .arg(&romanized)
            .args(["--to", "tamil", "--no-names-map"])
            .output()
            .unwrap(),
    );
    // Check the module declaration, not the whole file: the header COMMENT
    // mentions `கணக்கி → kannakki` verbatim, so a blanket Tamil-absence check
    // would wrongly trip. `kannakki(` is the romanized decl; `கணக்கி(` would be
    // the restored one.
    assert!(
        out.contains("kannakki("),
        "Latin name should remain in the decl:\n{out}"
    );
    assert!(
        !out.contains("கணக்கி("),
        "no restoration should happen:\n{out}"
    );
    fs::remove_dir_all(&dir).ok();
}

/// A `mimz.toml` (found by walking up from the file) supplies the default
/// `--to`, and an explicit `--to` on the command line overrides it.
#[test]
fn config_default_flavor_is_overridden_by_the_cli() {
    let dir = work_dir("prec");
    fs::write(dir.join("mimz.toml"), "[translate]\nto = \"tamil\"\n").unwrap();
    let sub = dir.join("sub");
    fs::create_dir_all(&sub).unwrap();
    let file = sub.join("c.mimz");
    fs::copy(repo().join("examples/english/counter.mimz"), &file).unwrap();

    // No --to: the config default (tamil) applies.
    let from_config = stdout_of(mimz().arg("translate").arg(&file).output().unwrap());
    assert!(
        from_config.contains("தொகுதி"),
        "config to=tamil should drive the keyword flavor:\n{from_config}"
    );

    // Explicit --to english wins over the config.
    let from_cli = stdout_of(
        mimz()
            .arg("translate")
            .arg(&file)
            .args(["--to", "english"])
            .output()
            .unwrap(),
    );
    assert!(
        from_cli.contains("module") && !from_cli.contains("தொகுதி"),
        "CLI --to must override the config:\n{from_cli}"
    );
    fs::remove_dir_all(&dir).ok();
}

/// A malformed `mimz.toml` is a clean error, not a panic.
#[test]
fn malformed_config_is_a_clean_error() {
    let dir = work_dir("bad");
    fs::write(dir.join("mimz.toml"), "[translate]\nto = \n").unwrap();
    let file = dir.join("c.mimz");
    fs::copy(repo().join("examples/english/counter.mimz"), &file).unwrap();

    let out = mimz().arg("translate").arg(&file).output().unwrap();
    assert!(!out.status.success(), "a broken config must fail the run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid config"),
        "error should name the bad config:\n{stderr}"
    );
    fs::remove_dir_all(&dir).ok();
}
