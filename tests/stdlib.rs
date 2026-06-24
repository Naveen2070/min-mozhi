//! Importable `std.*` library: embedded resolution, trilingual routing,
//! overrides, errors, and regression of plain file-relative imports.

use std::fs;
use std::path::PathBuf;

use mimz::project;
use mimz::project::LoadedFile;

/// `LoadError`/`LoadedFile` intentionally don't implement `Debug` (diagnostics
/// are rendered, not derived-debug-printed) — small helpers stand in for
/// `.unwrap()`/`.expect()` in this test file.
fn expect_ok(r: Result<Vec<LoadedFile>, project::LoadError>, what: &str) -> Vec<LoadedFile> {
    match r {
        Ok(files) => files,
        Err(_) => panic!("expected ok: {what}"),
    }
}
fn expect_err(r: Result<Vec<LoadedFile>, project::LoadError>, what: &str) -> project::LoadError {
    match r {
        Ok(_) => panic!("expected an error: {what}"),
        Err(e) => e,
    }
}

/// A throwaway project dir under the OS temp dir. Removed on drop.
struct TmpProj(PathBuf);
impl TmpProj {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "mimz_std_{tag}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        TmpProj(p)
    }
    fn file(&self, name: &str, body: &str) -> PathBuf {
        let f = self.0.join(name);
        fs::write(&f, body).unwrap();
        f
    }
}
impl Drop for TmpProj {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

#[test]
fn embedded_std_import_resolves_without_filesystem() {
    let p = TmpProj::new("embed");
    // This test only loads/parses (it never runs the checker), so the entry
    // module body is trimmed to whatever parses cleanly; the import line is
    // verbatim and is what's under test.
    let entry = p.file("top.mimz", "import std.fifo\nmodule Top { }\n");
    let files = expect_ok(project::load_project(&entry), "embedded std.fifo loads");
    // entry + the embedded Fifo
    assert!(
        files
            .iter()
            .any(|f| f.path.to_string_lossy().contains("std:fifo"))
    );
    // The entry file must stay files[0] — sim/test elaborate/inspect files[0],
    // so an embedded std module appended ahead of it would silently run the
    // wrong module. The std module is appended after every user file.
    assert_eq!(files[0].path, entry, "entry file must be files[0]");
    assert!(
        files.last().unwrap().path.to_string_lossy().contains("std:fifo"),
        "embedded std module must come after the user files"
    );
}

#[test]
fn tamil_twin_routes_to_twin_source() {
    let p = TmpProj::new("twin");
    // Pure-Tamil importer: Tamil import keyword + namespace + module name
    // (code word-order — no `syntax thamizh` directive needed here; this test
    // only checks that the Tamil module segment routes to the twin source).
    let entry = p.file("mel.mimz", "சேர்க்க நூலகம்.வரிசை\nதொகுதி மேல் { }\n");
    let files = expect_ok(project::load_project(&entry), "twin loads");
    let twin = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("std:fifo"))
        .expect("fifo twin present");
    assert!(
        twin.src.contains("தொகுதி வரிசை"),
        "should be the Tamil twin source"
    );
}

#[test]
fn unknown_std_module_errors_with_available_list() {
    let p = TmpProj::new("unknown");
    let entry = p.file("top.mimz", "import std.nope\nmodule Top { }\n");
    let err = expect_err(project::load_project(&entry), "unknown std module errors");
    match err {
        project::LoadError::Source { diags, .. } => {
            let msg = &diags[0].msg;
            assert!(msg.contains("fifo"), "lists available modules: {msg}");
        }
        _ => panic!("expected a Source diagnostic"),
    }
}

#[test]
fn wrong_std_arity_errors() {
    let p = TmpProj::new("arity");
    let entry = p.file("top.mimz", "import std.fifo.extra\nmodule Top { }\n");
    assert!(project::load_project(&entry).is_err());
}

#[test]
fn plain_relative_import_still_works() {
    let p = TmpProj::new("relimport");
    p.file("helper.mimz", "module Helper { }\n");
    let entry = p.file("top.mimz", "import helper\nmodule Top { }\n");
    let files = expect_ok(
        project::load_project(&entry),
        "relative import still resolves",
    );
    assert!(
        files
            .iter()
            .any(|f| f.path.to_string_lossy().contains("helper"))
    );
}

#[test]
fn lib_std_override_wins_over_embedded() {
    let p = TmpProj::new("override");
    // A local std/ dir with a sentinel fifo that is NOT the embedded one.
    fs::create_dir_all(p.0.join("vendorstd")).unwrap();
    fs::write(
        p.0.join("vendorstd").join("fifo.mimz"),
        "module SentinelFifo { }\n",
    )
    .unwrap();
    let entry = p.file("top.mimz", "import std.fifo\nmodule Top { }\n");

    // Embedded (None) -> real Fifo.
    let embedded = expect_ok(project::load_project(&entry), "embedded fifo loads");
    assert!(embedded.iter().any(|f| f.src.contains("module Fifo")));

    // Override -> the sentinel from the dir wins.
    let dir = p.0.join("vendorstd");
    let overridden = expect_ok(
        project::load_project_with_lib(&entry, Some(&dir)),
        "overridden fifo loads",
    );
    assert!(overridden.iter().any(|f| f.src.contains("SentinelFifo")));
    assert!(!overridden.iter().any(|f| f.src.contains("module Fifo")));
}

#[test]
fn lib_std_override_filename_matches_eject_for_twin_spellings() {
    // The override dir is what `mimz eject std` produced: twins are named by
    // their romanization (`varisai.mimz`), never the Tamil-script name. An
    // `import std.வரிசை` (twin Tamil name) and `import std.varisai` (roman)
    // both resolve to that one ejected file — the filename keys on the
    // resolved variant, not the raw written alias.
    let p = TmpProj::new("override_twin");
    let dir = p.0.join("vendorstd");
    mimz::stdlib::eject_to(&dir, true, false).expect("eject tamil twins");

    for (tag, import) in [
        ("tamil-name", "சேர்க்க நூலகம்.வரிசை\nதொகுதி மேல் { }\n"),
        ("roman", "import std.varisai\nmodule Top { }\n"),
    ] {
        let entry = p.file(&format!("top_{tag}.mimz"), import);
        let files = expect_ok(
            project::load_project_with_lib(&entry, Some(&dir)),
            &format!("twin override resolves ({tag})"),
        );
        // The ejected twin (`தொகுதி வரிசை`) is loaded, not the embedded canonical.
        assert!(
            files.iter().any(|f| f.src.contains("தொகுதி வரிசை")),
            "ejected twin must resolve for {tag}"
        );
        assert!(files.iter().any(|f| f.path.ends_with("varisai.mimz")));
    }
}

#[test]
fn eject_writes_english_modules() {
    let p = TmpProj::new("eject_en");
    let dir = p.0.join("out");
    let written = mimz::stdlib::eject_to(&dir, false, false).expect("eject ok");
    assert_eq!(written.len(), 5);
    let fifo = fs::read_to_string(dir.join("fifo.mimz")).unwrap();
    assert!(fifo.contains("module Fifo"));
}

#[test]
fn eject_tamil_writes_twins() {
    let p = TmpProj::new("eject_ta");
    let dir = p.0.join("out");
    mimz::stdlib::eject_to(&dir, true, false).expect("eject ok");
    let v = fs::read_to_string(dir.join("varisai.mimz")).unwrap();
    assert!(v.contains("தொகுதி வரிசை"));
}

#[test]
fn eject_refuses_overwrite_without_force() {
    let p = TmpProj::new("eject_force");
    let dir = p.0.join("out");
    mimz::stdlib::eject_to(&dir, false, false).expect("first eject ok");
    let err = mimz::stdlib::eject_to(&dir, false, false);
    assert!(err.is_err(), "second eject without force must fail");
    mimz::stdlib::eject_to(&dir, false, true).expect("force overwrite ok");
}

#[test]
fn eject_is_all_or_nothing_on_partial_conflict() {
    // One pre-existing target must abort the whole eject before any other
    // file is written — no half-vendored directory.
    let p = TmpProj::new("eject_partial");
    let dir = p.0.join("out");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("pwm.mimz"), "// sentinel\n").unwrap();

    let err = mimz::stdlib::eject_to(&dir, false, false);
    assert!(err.is_err(), "partial conflict must fail");

    // The pre-existing file is untouched and none of the others were created.
    assert_eq!(
        fs::read_to_string(dir.join("pwm.mimz")).unwrap(),
        "// sentinel\n",
        "conflicting file must not be overwritten"
    );
    for other in ["debouncer", "fifo", "seg7", "uart_tx"] {
        assert!(
            !dir.join(format!("{other}.mimz")).exists(),
            "{other}.mimz must not be written when eject aborts"
        );
    }
}
