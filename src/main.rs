//! mimz — the Min-Mozhi (மின்மொழி) compiler CLI.
//!
//! A thin shell over the [`mimz`] library crate (`src/lib.rs` holds the
//! crate map): argument parsing, human/JSON rendering of diagnostics,
//! file output, and the LSP server (`lsp.rs` — bin-only so the lib
//! stays async-free) live here — every compiler stage lives in the lib.

mod lsp;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser as ClapParser, Subcommand};

use mimz::project::{LoadError, LoadedFile};
use mimz::{ast, checker, diag, emit_verilog, lexer, project};

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
    /// Lex + parse + check a file and report errors (no output written)
    Check {
        /// The .mimz file to check
        file: PathBuf,
        /// Dump the token stream (debugging)
        #[arg(long)]
        tokens: bool,
        /// Print diagnostics as a JSON array on stdout (tool consumers)
        #[arg(long)]
        json: bool,
    },
    /// Compile a .mimz file (and its imports) to Verilog
    Compile {
        /// The .mimz entry file
        file: PathBuf,
        /// Output path (default: entry file with .v extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print diagnostics as a JSON array on stdout (tool consumers)
        #[arg(long)]
        json: bool,
    },
    /// Run the language server over stdio (diagnostics-only v0;
    /// editors launch this — not for interactive use)
    Lsp,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Cmd::Check { file, tokens, json } => check(&file, tokens, json),
        Cmd::Compile { file, output, json } => compile(&file, output, json),
        Cmd::Lsp => {
            lsp::run();
            ExitCode::SUCCESS
        }
    }
}

/// How diagnostics leave the process: rendered carets on stderr (human),
/// or one JSON array on stdout (`--json`, for editors and wrappers —
/// schema in docs/code/06-diagnostics.md).
#[derive(Clone, Copy)]
enum Output {
    Human,
    Json,
}

impl Output {
    fn new(json: bool) -> Self {
        if json { Output::Json } else { Output::Human }
    }

    /// Report diagnostics that all point into ONE known source.
    fn one_file(self, diags: &[diag::Diag], src: &str, path: &str) -> ExitCode {
        match self {
            Output::Human => eprint!("{}", diag::render(diags, src, path)),
            Output::Json => {
                let json: Vec<diag::JsonDiag> = diags
                    .iter()
                    .map(|d| diag::JsonDiag::new(d, path, src))
                    .collect();
                println!("{}", serde_json::to_string(&json).expect("diag serializes"));
            }
        }
        ExitCode::FAILURE
    }

    /// Report project-wide diagnostics (each carries a file index).
    fn project(self, diags: &[diag::Diag], files: &[LoadedFile]) -> ExitCode {
        match self {
            Output::Human => eprint!("{}", project::render_diags(diags, files)),
            Output::Json => {
                let json: Vec<diag::JsonDiag> = diags
                    .iter()
                    .map(|d| {
                        let f = &files[d.file.unwrap_or(0).min(files.len() - 1)];
                        diag::JsonDiag::new(d, &f.path.display().to_string(), &f.src)
                    })
                    .collect();
                println!("{}", serde_json::to_string(&json).expect("diag serializes"));
            }
        }
        ExitCode::FAILURE
    }

    /// Report a load failure (I/O, or lexer/parser/import diagnostics).
    fn load_error(self, e: &LoadError) -> ExitCode {
        match e {
            LoadError::Io(msg) => {
                match self {
                    Output::Human => eprintln!("error: {msg}"),
                    Output::Json => {
                        println!("{}", serde_json::json!([{ "code": null, "message": msg }]))
                    }
                }
                ExitCode::FAILURE
            }
            LoadError::Source { path, src, diags } => {
                self.one_file(diags, src, &path.display().to_string())
            }
        }
    }
}

/// `mimz check` — lex + parse + checker passes over the file AND its
/// imports (cross-file names must resolve), reporting all diagnostics.
/// With `--tokens` it stops after the lexer and dumps the token stream
/// instead (the standard way to debug lexer issues).
fn check(path: &Path, tokens: bool, json: bool) -> ExitCode {
    let out = Output::new(json);
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
            Err(diags) => return out.one_file(&diags, &src, &path.display().to_string()),
        }
        return ExitCode::SUCCESS;
    }

    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    if let Err(diags) = checker::check(&asts) {
        return out.project(&diags, &files);
    }
    if json {
        println!("[]"); // stable contract: stdout is ALWAYS a JSON array
        return ExitCode::SUCCESS;
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
fn compile(path: &Path, output: Option<PathBuf>, json: bool) -> ExitCode {
    let out = Output::new(json);
    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();

    if let Err(diags) = checker::check(&asts) {
        return out.project(&diags, &files);
    }

    // Tamil identifiers become readable ASCII (விளக்கு → villakku) —
    // checked against the original spelling above, emitted as Verilog
    // names below.
    emit_verilog::transliterate(&mut asts);

    let project = match emit_verilog::Project::from_files(&asts) {
        Ok(p) => p,
        Err(diags) => return out.project(&diags, &files),
    };
    let verilog = match emit_verilog::emit(&project, &asts) {
        Ok(v) => v,
        Err(diags) => return out.project(&diags, &files),
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
    if json {
        println!("[]");
    } else {
        println!("compiled {} -> {}", path.display(), out_path.display());
    }
    ExitCode::SUCCESS
}
