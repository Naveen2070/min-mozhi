//! mimz — the Min-Mozhi (மின்மொழி) compiler CLI.
//!
//! Phase 1 pipeline (docs/architecture.md):
//! lexer → parser → AST → checker (WIP) → Verilog emitter.
//! Source loading + import resolution live in `project.rs`.

mod ast;
mod diag;
mod emit_verilog;
mod lexer;
mod parser;
mod project;
mod span;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser as ClapParser, Subcommand};

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

    match project::parse_file(path) {
        Ok(loaded) => {
            let modules = loaded
                .ast
                .items
                .iter()
                .filter(|i| matches!(i, ast::TopItem::Module(_)))
                .count();
            let tests = loaded
                .ast
                .items
                .iter()
                .filter(|i| matches!(i, ast::TopItem::Test(_)))
                .count();
            println!(
                "OK: {} — {modules} module(s), {tests} test(s)",
                loaded.path.display()
            );
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn compile(path: &Path, output: Option<PathBuf>) -> ExitCode {
    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();

    let project = match emit_verilog::Project::from_files(&asts) {
        Ok(p) => p,
        Err(diags) => {
            eprint!(
                "{}",
                diag::render(&diags, &files[0].src, &files[0].path.display().to_string())
            );
            return ExitCode::FAILURE;
        }
    };
    let verilog = match emit_verilog::emit(&project, &asts) {
        Ok(v) => v,
        Err(diags) => {
            eprint!(
                "{}",
                diag::render(&diags, &files[0].src, &files[0].path.display().to_string())
            );
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
