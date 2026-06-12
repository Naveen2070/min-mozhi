//! mimz — the Min-Mozhi (மின்மொழி) compiler CLI.
//!
//! Phase 1 pipeline (docs/architecture.md):
//! lexer → parser → AST → checker (six passes) → Verilog emitter.
//! Source loading + import resolution live in `project.rs`.
//!
//! Crate map (one module per pipeline stage):
//!
//! | Module          | Role                                                       |
//! | --------------- | ---------------------------------------------------------- |
//! | [`span`]        | Byte-offset source spans carried by every token/AST node   |
//! | [`diag`]        | Teaching diagnostics (stable E-codes) + caret renderer     |
//! | [`lexer`]       | Source text → tokens (trilingual keyword table)            |
//! | [`parser`]      | Tokens → AST (recursive descent, multi-error recovery)     |
//! | [`ast`]         | The one shared AST — flavor- and word-order-blind          |
//! | [`checker`]     | Names, consts, widths, drivers, exhaustiveness, clocks     |
//! | [`emit_verilog`]| AST → Verilog-2005 text                                    |
//! | [`project`]     | File loading, NFC normalization, `import` resolution       |
//!
//! This table is mechanically checked against the `mod` list by
//! `tests/docs_sync.rs` — add a module, add a row (and a docs/code/ page).
//!
//! Generate the API reference with
//! `cargo doc --document-private-items --open` (binary crate, so private
//! items ARE the API).

mod ast;
mod checker;
mod diag;
mod emit_verilog;
mod lexer;
mod parser;
mod project;
mod span;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser as ClapParser, Subcommand};

/// Top-level CLI definition. The `///` docs on [`Cmd`] variants and fields
/// double as the `--help` text (clap derive).
#[derive(ClapParser)]
#[command(
    name = "mimz",
    version,
    about = "Min-Mozhi (மின்மொழி) — the first Tamil-rooted HDL. Reads like Go/TypeScript, safe like Rust."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

/// The `mimz` subcommands. Planned but not yet implemented: `translate`,
/// `fmt`, `test`, `sim` (docs/plan/).
#[derive(Subcommand)]
enum Cmd {
    /// Lex + parse a file and report errors (no output written)
    Check {
        /// The .mimz file to check
        file: PathBuf,
        /// Dump the token stream (debugging)
        #[arg(long)]
        tokens: bool,
    },
    /// Compile a .mimz file (and its imports) to Verilog
    Compile {
        /// The .mimz entry file
        file: PathBuf,
        /// Output path (default: entry file with .v extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Cmd::Check { file, tokens } => check(&file, tokens),
        Cmd::Compile { file, output } => compile(&file, output),
    }
}

/// `mimz check` — lex + parse + checker passes over the file AND its
/// imports (cross-file names must resolve), reporting all diagnostics.
/// With `--tokens` it stops after the lexer and dumps the token stream
/// instead (the standard way to debug lexer issues).
fn check(path: &Path, tokens: bool) -> ExitCode {
    if tokens {
        let src = match project::read_source(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        match lexer::lex(&src) {
            Ok(toks) => {
                for t in &toks {
                    println!("{:?} @ {}..{}", t.kind, t.span.start, t.span.end);
                }
            }
            Err(diags) => {
                eprint!(
                    "{}",
                    diag::render(&diags, &src, &path.display().to_string())
                );
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    if let Err(diags) = checker::check(&asts) {
        eprint!("{}", project::render_diags(&diags, &files));
        return ExitCode::FAILURE;
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
    println!(
        "OK: {} — {modules} module(s), {tests} test(s), {} file(s)",
        path.display(),
        files.len()
    );
    ExitCode::SUCCESS
}

/// `mimz compile` — load the entry file and all transitive imports, build
/// the project symbol table, and emit one Verilog file (default: entry
/// path with `.v` extension).
fn compile(path: &Path, output: Option<PathBuf>) -> ExitCode {
    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();

    if let Err(diags) = checker::check(&asts) {
        eprint!("{}", project::render_diags(&diags, &files));
        return ExitCode::FAILURE;
    }

    // Tamil identifiers become readable ASCII (விளக்கு → villakku) —
    // checked against the original spelling above, emitted as Verilog
    // names below.
    emit_verilog::transliterate(&mut asts);

    let project = match emit_verilog::Project::from_files(&asts) {
        Ok(p) => p,
        Err(diags) => {
            eprint!("{}", project::render_diags(&diags, &files));
            return ExitCode::FAILURE;
        }
    };
    let verilog = match emit_verilog::emit(&project, &asts) {
        Ok(v) => v,
        Err(diags) => {
            eprint!("{}", project::render_diags(&diags, &files));
            return ExitCode::FAILURE;
        }
    };

    let out_path = output.unwrap_or_else(|| {
        let mut p = path.to_path_buf();
        p.set_extension("v");
        p
    });
    if let Err(e) = std::fs::write(&out_path, &verilog) {
        eprintln!("error: cannot write `{}`: {e}", out_path.display());
        return ExitCode::FAILURE;
    }
    println!("compiled {} -> {}", path.display(), out_path.display());
    ExitCode::SUCCESS
}
