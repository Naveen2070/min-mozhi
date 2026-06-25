//! `mimz doctor` (alias `mimz env`) — print toolchain & environment info and
//! flag anything that would trip a user up. The default run targets an
//! HDL *user*; `--dev` adds the contributor toolchain (Rust, WASM, test tools).
//!
//! Only `Status::Fail` makes the command exit non-zero. The runtime CLI is
//! entirely in-process (sim/test/eval never shell out), so iverilog / verilator
//! / gtkwave are *optional* cross-check & waveform tools — missing ones are
//! `Warn`, never `Fail`.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use owo_colors::OwoColorize;

use mimz::diag::is_color_enabled;

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Ok,
    Warn,
    Fail,
    Info,
}

impl Status {
    fn symbol(self) -> &'static str {
        match self {
            Status::Ok => "✓",
            Status::Warn => "⚠",
            Status::Fail => "✗",
            Status::Info => "•",
        }
    }
}

/// Print one check line; return `true` if it was a `Fail` (so the caller can OR
/// it into the overall exit status).
fn line(status: Status, name: &str, detail: &str) -> bool {
    let sym = status.symbol();
    let sym = if is_color_enabled() {
        match status {
            Status::Ok => sym.green().bold().to_string(),
            Status::Warn => sym.yellow().bold().to_string(),
            Status::Fail => sym.red().bold().to_string(),
            Status::Info => sym.bright_blue().bold().to_string(),
        }
    } else {
        sym.to_string()
    };
    println!("  {sym} {name:<14} {detail}");
    status == Status::Fail
}

fn heading(title: &str) {
    if is_color_enabled() {
        println!("\n{}", title.bold());
    } else {
        println!("\n{title}");
    }
}

/// Run `cmd args…`; return the first non-empty output line (stdout, then
/// stderr) on success, `None` if the binary is missing or exits non-zero.
fn probe(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let text = if stdout.trim().is_empty() {
        String::from_utf8_lossy(&out.stderr).into_owned()
    } else {
        stdout
    };
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// iverilog via PATH, falling back to the Windows installer's default dir
/// (matches the test/bench harness's detection).
fn probe_iverilog() -> Option<String> {
    probe("iverilog", &["-V"]).or_else(|| {
        cfg!(windows)
            .then(|| probe(r"C:\iverilog\bin\iverilog", &["-V"]))
            .flatten()
    })
}

/// A tool that is nice to have but not required: `Ok` with its version banner,
/// else `Warn` with what it's for and how to get it.
fn optional(name: &str, version: Option<String>, purpose: &str) -> bool {
    match version {
        Some(v) => line(Status::Ok, name, &v),
        None => line(Status::Warn, name, &format!("not found — {purpose}")),
    }
}

pub(crate) fn doctor(dev: bool) -> ExitCode {
    let mut failed = false;

    // ---- Compiler --------------------------------------------------------
    heading("Compiler");
    failed |= line(
        Status::Info,
        "mimz",
        &format!(
            "{} — edition {} ({})",
            mimz::version::COMPILER_VERSION,
            mimz::version::current().tag(),
            mimz::version::current().variant,
        ),
    );
    failed |= line(
        Status::Info,
        "platform",
        &format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
    );
    // End-to-end pipeline smoke test, fully in-memory (lex→parse→check→emit).
    const SMOKE: &str = "module Smoke {\n  in  a: bits[4]\n  in  b: bits[4]\n  out sum: bits[5]\n  sum = a + b\n}\n";
    match mimz::compile_string(SMOKE) {
        Ok(v) if v.contains("module Smoke") => {
            failed |= line(Status::Ok, "pipeline", "in-memory compile OK")
        }
        Ok(_) => failed |= line(Status::Fail, "pipeline", "compiled but output looks wrong"),
        Err(e) => {
            let first = e.lines().next().unwrap_or("compile failed");
            failed |= line(Status::Fail, "pipeline", first);
        }
    }

    // ---- Simulation toolchain (all optional for an HDL user) -------------
    heading("Simulation toolchain (optional)");
    failed |= optional(
        "iverilog",
        probe_iverilog(),
        "Icarus Verilog cross-check (incl. vvp); bleyer.org/icarus or `apt install iverilog`",
    );
    failed |= optional(
        "verilator",
        probe("verilator", &["--version"]),
        "alternative Verilog simulator; veripool.org or `apt install verilator`",
    );
    failed |= optional(
        "gtkwave",
        probe("gtkwave", &["--version"]),
        "view `mimz sim -o out.vcd` waveforms; gtkwave.sourceforge.net",
    );

    // ---- Environment -----------------------------------------------------
    heading("Environment");
    let tmp = std::env::temp_dir();
    let probe_file = tmp.join(format!("mimz_doctor_{}.tmp", std::process::id()));
    match std::fs::write(&probe_file, b"ok") {
        Ok(()) => {
            std::fs::remove_file(&probe_file).ok();
            failed |= line(
                Status::Ok,
                "temp dir",
                &format!("writable ({})", tmp.display()),
            );
        }
        Err(e) => {
            failed |= line(
                Status::Fail,
                "temp dir",
                &format!("{} — {e}", tmp.display()),
            )
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match mimz::config::Config::discover(&cwd) {
        Some(p) => match mimz::config::Config::load(&p) {
            Ok(_) => failed |= line(Status::Ok, "mimz.toml", &format!("valid ({})", p.display())),
            Err(e) => failed |= line(Status::Fail, "mimz.toml", &e),
        },
        None => {
            failed |= line(
                Status::Info,
                "mimz.toml",
                "none in this tree (using built-in defaults)",
            )
        }
    }

    // ---- Developer toolchain (--dev) -------------------------------------
    if dev {
        heading("Developer toolchain");
        failed |= optional("rustc", probe("rustc", &["--version"]), "rustup.rs");
        failed |= optional("cargo", probe("cargo", &["--version"]), "ships with rustc");
        let wasm_ok = Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .ok()
            .is_some_and(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown"));
        failed |= if wasm_ok {
            line(Status::Ok, "wasm32 target", "installed")
        } else {
            line(
                Status::Warn,
                "wasm32 target",
                "missing — `rustup target add wasm32-unknown-unknown` (playground)",
            )
        };
        failed |= optional(
            "wasm-pack",
            probe("wasm-pack", &["--version"]),
            "build the WASM playground; `cargo install wasm-pack`",
        );
        failed |= optional(
            "cargo-nextest",
            probe("cargo", &["nextest", "--version"]),
            "faster test runs; `cargo install cargo-nextest`",
        );
        failed |= optional(
            "node",
            probe("node", &["--version"]),
            "build site/ and the VS Code extension; nodejs.org",
        );
    }

    println!();
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
