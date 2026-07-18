//! `mimz check` — lex + parse + all semantic checker passes over a file and its
//! transitive imports. Reports every diagnostic (errors + warnings) across the
//! whole project. With `--tokens`, dumps the token stream instead. With
//! `--watch`, re-runs on filesystem changes.
//!
//! This is the most comprehensive static-analysis command — it exercises every
//! compiler pass through the checker (names, symbols, clocks, widths, drivers).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mimz::{ast, checker, lexer, project};

use super::helpers::{lib_std_dir, ms, project_warnings, resolve_lang};
use crate::Output;

/// The outcome of one [`run_check`] pass.
struct Pass {
    /// Process exit code for a one-shot run.
    code: ExitCode,
    /// Directories `--watch` should watch this run: the entry file's dir plus
    /// one per (transitive) import. Only read by the watcher.
    #[cfg_attr(not(feature = "watch"), allow(dead_code))]
    dirs: Vec<PathBuf>,
    /// Whether the project actually loaded — i.e. `dirs` is the COMPLETE import
    /// set (safe to prune the watch set against) rather than the entry-only
    /// fallback after a load failure. Only read by the watcher.
    #[cfg_attr(not(feature = "watch"), allow(dead_code))]
    loaded: bool,
}

/// `mimz check` — lex + parse + checker passes over the file AND its
/// imports (cross-file names must resolve), reporting all diagnostics.
/// With `--tokens` it stops after the lexer and dumps the token stream
/// instead (the standard way to debug lexer issues). With `--watch` it
/// re-runs on every save (requires the `watch` feature, on by default).
#[allow(clippy::too_many_arguments)]
pub(crate) fn check(
    path: &Path,
    tokens: bool,
    json: bool,
    lang: Option<&str>,
    config_path: Option<&Path>,
    quiet: bool,
    debug: bool,
    watch: bool,
) -> ExitCode {
    if watch {
        #[cfg(feature = "watch")]
        return watch_check(path, tokens, json, lang, config_path, quiet, debug);
        #[cfg(not(feature = "watch"))]
        {
            eprintln!(
                "error: this build has no `--watch` support (rebuild with `--features watch`)"
            );
            return ExitCode::FAILURE;
        }
    }
    run_check(path, tokens, json, lang, config_path, quiet, debug).code
}

/// The directory holding `p` (empty parent ⇒ `.`).
fn dir_of(p: &Path) -> PathBuf {
    p.parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// One check pass. `Pass::code` is FAILURE if any error diagnostic was produced;
/// `Pass::dirs`/`loaded` feed the `--watch` watch set (see [`Pass`]).
fn run_check(
    path: &Path,
    tokens: bool,
    json: bool,
    lang: Option<&str>,
    config_path: Option<&Path>,
    quiet: bool,
    debug: bool,
) -> Pass {
    // Before the project loads we can only watch the entry file's own dir.
    let entry_only = || vec![dir_of(path)];
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => {
            return Pass {
                code,
                dirs: entry_only(),
                loaded: false,
            };
        }
    };
    let out = Output::new(json, flavor);
    if tokens {
        let code = match project::read_source(path) {
            Ok(src) => match lexer::lex(&src) {
                Ok(toks) => {
                    for t in &toks {
                        println!("{:?} @ {}..{}", t.kind, t.span.start, t.span.end);
                    }
                    ExitCode::SUCCESS
                }
                Err(diags) => out.one_file(&diags, &src, &path.display().to_string()),
            },
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
        return Pass {
            code,
            dirs: entry_only(),
            loaded: false,
        };
    }

    if debug {
        eprintln!(
            "debug: loading project starting from entry {}",
            path.display()
        );
    }
    // Phase timing (--debug): `load` fuses lex+parse+import-resolution (the lib
    // does them together); `check` is the six checker passes. For a finer
    // lex-vs-parse-vs-emit split, see the criterion harness (`cargo bench`).
    let t_load = Instant::now();
    let lib_std = match lib_std_dir(path, config_path) {
        Ok(v) => v,
        Err(code) => {
            return Pass {
                code,
                dirs: entry_only(),
                loaded: false,
            };
        }
    };
    let files = match project::load_project_with_lib(path, lib_std.as_deref()) {
        Ok(f) => f,
        Err(e) => {
            return Pass {
                code: out.load_error(&e),
                dirs: entry_only(),
                loaded: false,
            };
        }
    };
    let load_ms = ms(t_load);
    if debug {
        eprintln!("debug: loaded {} project file(s)", files.len());
        for f in &files {
            eprintln!("  - {}", f.path.display());
        }
    }

    // The complete watch set for this run: each loaded file's directory, deduped.
    // The load succeeded, so this is authoritative (the watcher may prune to it).
    let dirs: Vec<PathBuf> = {
        let mut set = std::collections::BTreeSet::new();
        for f in &files {
            set.insert(dir_of(&f.path));
        }
        set.into_iter().collect()
    };

    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    // Non-fatal warnings (W0001 mixed-flavor) ride alongside any checker errors.
    let mut diags = project_warnings(&files);
    let t_check = Instant::now();
    if let Err(errors) = checker::check(&asts) {
        diags.extend(errors);
    }
    let check_ms = ms(t_check);
    if debug {
        eprintln!(
            "debug: timing — load {load_ms:.3}ms, check {check_ms:.3}ms (total {:.3}ms)",
            load_ms + check_ms
        );
    }
    let has_error = diags.iter().any(|d| d.is_error());
    if json {
        // Stable contract: stdout is ALWAYS a JSON array (warnings included, or
        // `[]`). Exit reflects severity.
        return Pass {
            code: out.project(&diags, &files),
            dirs,
            loaded: true,
        };
    }
    if !diags.is_empty() {
        eprint!("{}", project::render_diags_lang(&diags, &files, flavor));
    }
    if has_error {
        return Pass {
            code: ExitCode::FAILURE,
            dirs,
            loaded: true,
        };
    }
    let modules = asts
        .iter()
        .flat_map(|f| &f.items)
        .filter(|i| matches!(i, ast::TopItem::Module(_)))
        .count();
    let tests = asts
        .iter()
        .flat_map(|f| &f.items)
        .filter(|i| matches!(i, ast::TopItem::Test(_)))
        .count();
    if !quiet {
        println!(
            "OK: {} — {modules} module(s), {tests} test(s), {} file(s)",
            path.display(),
            files.len()
        );
    }
    Pass {
        code: ExitCode::SUCCESS,
        dirs,
        loaded: true,
    }
}

/// `--watch`: run once, then re-check on every save until Ctrl-C. Watches the
/// DIRECTORIES (not the files) holding the entry file and every transitive
/// import — so editor atomic-saves (write-temp-then-rename) still fire, and an
/// edit to an imported file in another directory also triggers a recheck. Only
/// reacts to `.mimz` changes. The watch set is reconciled to the project after
/// every run: new import dirs are added, and dirs that are no longer part of
/// the project are dropped (so deleting an import stops watching its dir).
// note: directory granularity — editing an UNRELATED `.mimz` in a watched dir
// also rechecks (harmless, sub-ms). Import dirs are pruned only after a
// successful load; while the entry doesn't parse, the last good watch set is
// kept so the fix-save is never missed. Both are deliberate, not gaps — exact
// per-file matching would miss the fix-save during an import error (rationale
// in docs/log/2026-06-25.md).
#[cfg(feature = "watch")]
fn watch_check(
    path: &Path,
    tokens: bool,
    json: bool,
    lang: Option<&str>,
    config_path: Option<&Path>,
    quiet: bool,
    debug: bool,
) -> ExitCode {
    use std::collections::HashSet;
    use std::sync::mpsc;
    use std::time::Duration;

    use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: cannot start file watcher: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Bring the live watch set in line with this run's project dirs: add the
    // newly-seen ones, and (only when the load succeeded, so the list is
    // complete) unwatch the ones that dropped out of the project.
    let reconcile = |w: &mut RecommendedWatcher, watched: &mut HashSet<PathBuf>, pass: &Pass| {
        let desired: HashSet<PathBuf> = pass.dirs.iter().cloned().collect();
        for dir in &desired {
            if watched.insert(dir.clone()) {
                if let Err(e) = w.watch(dir, RecursiveMode::NonRecursive) {
                    eprintln!("warning: cannot watch `{}`: {e}", dir.display());
                }
            }
        }
        if pass.loaded {
            for dir in watched.difference(&desired).cloned().collect::<Vec<_>>() {
                let _ = w.unwatch(&dir);
                watched.remove(&dir);
            }
        }
    };

    let mut watched: HashSet<PathBuf> = HashSet::new();
    let initial = run_check(path, tokens, json, lang, config_path, quiet, debug);
    reconcile(&mut watcher, &mut watched, &initial);
    if !quiet {
        eprintln!("watching {} dir(s) — press Ctrl-C to stop", watched.len());
    }

    let touches_mimz = |res: &notify::Result<Event>| {
        res.as_ref().is_ok_and(|e| {
            e.paths
                .iter()
                .any(|p| p.extension().is_some_and(|x| x == "mimz"))
        })
    };

    loop {
        let first = match rx.recv() {
            Ok(r) => r,
            Err(_) => return ExitCode::SUCCESS, // watcher dropped
        };
        // Debounce: editors emit a burst per save — drain it, tracking whether
        // any event in the burst actually touched a `.mimz` file.
        let mut hit = touches_mimz(&first);
        while let Ok(r) = rx.recv_timeout(Duration::from_millis(100)) {
            hit |= touches_mimz(&r);
        }
        if !hit {
            continue;
        }
        if !quiet {
            eprintln!("--- rechecking ---");
        }
        // One load per recheck: run_check returns the project dirs it found, so
        // the watch set is reconciled without loading the project a second time.
        let pass = run_check(path, tokens, json, lang, config_path, quiet, debug);
        reconcile(&mut watcher, &mut watched, &pass);
    }
}
