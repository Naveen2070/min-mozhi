//! mimz — the Min-Mozhi (மின்மொழி) compiler CLI.
//!
//! A thin shell over the [`mimz`] library crate (`src/lib.rs` holds the
//! crate map): argument parsing, human/JSON rendering of diagnostics,
//! file output, and the LSP server (`lsp.rs` — bin-only so the lib
//! stays async-free) live here — every compiler stage lives in the lib.

// No `unsafe` in the CLI either (see `lib.rs`) — locked by the compiler.
#![forbid(unsafe_code)]

mod lsp;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser as ClapParser, Subcommand};

use mimz::project::{LoadError, LoadedFile};
use mimz::{ast, checker, diag, emit_verilog, lexer, parser, project, sim};

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

/// The `mimz` subcommands. Planned but not yet implemented: `fmt`, `test`,
/// and the full `sim` (docs/plan/); `eval` below is its combinational slice.
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
    /// Explain a diagnostic code in depth (e.g. `mimz explain E0501`)
    Explain {
        /// The diagnostic code to explain (case-insensitive, e.g. E0501)
        code: String,
    },
    /// Reskin a file's keywords into another flavor, and/or convert its word
    /// order between `code` and `thamizh` (spec/04). `--to` alone is lossless
    /// (keyword tokens only, comments/layout preserved); `--order` re-emits
    /// from the AST (canonical layout, comments dropped).
    Translate {
        /// The .mimz file to translate
        file: PathBuf,
        /// Target keyword flavor: english | tanglish | tamil (default: english)
        #[arg(long)]
        to: Option<String>,
        /// Word order: code | thamizh. Omit to keep the source order
        /// (keyword-only, trivia-preserving reskin).
        #[arg(long)]
        order: Option<String>,
        /// Output path (default: print the result to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// (experimental) Evaluate a combinational module's outputs from inputs.
    /// Combinational only — no clocks/regs/instances (that is the Phase 1.5
    /// simulator); a slice of it, for quick checks and the future REPL.
    Eval {
        /// The .mimz file
        file: PathBuf,
        /// Input values, comma-separated: `--in a=3,b=5` (dec/0x/0b)
        #[arg(long = "in", value_name = "NAME=VAL,...")]
        inputs: String,
        /// Parameter overrides, comma-separated: `--param WIDTH=4`
        #[arg(long, default_value = "")]
        param: String,
        /// Which module to evaluate (default: the file's only module)
        #[arg(long)]
        module: Option<String>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Cmd::Check { file, tokens, json } => check(&file, tokens, json),
        Cmd::Compile { file, output, json } => compile(&file, output, json),
        Cmd::Lsp => {
            lsp::run();
            ExitCode::SUCCESS
        }
        Cmd::Explain { code } => explain_code(&code),
        Cmd::Translate {
            file,
            to,
            order,
            output,
        } => translate_file(&file, to.as_deref(), order.as_deref(), output),
        Cmd::Eval {
            file,
            inputs,
            param,
            module,
        } => eval_file(&file, &inputs, &param, module),
    }
}

/// `mimz eval <file> --in a=3,b=5` — interpret a combinational module and print
/// each output. Lexes/parses the file directly (no import resolution — the
/// evaluator is single-module, combinational only) and reports a clear message
/// on anything out of that scope.
fn eval_file(path: &Path, inputs: &str, param: &str, module: Option<String>) -> ExitCode {
    let src = match project::read_source(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let path_str = path.display().to_string();
    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(diags) => return Output::Human.one_file(&diags, &src, &path_str),
    };
    let file = match parser::parse(tokens) {
        Ok(f) => f,
        Err(diags) => return Output::Human.one_file(&diags, &src, &path_str),
    };
    let inputs = match parse_bindings(inputs, parse_u128) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let params = match parse_bindings(param, |s| parse_u128(s).map(|v| v as i128)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    match sim::comb::eval_outputs(&file, module.as_deref(), &inputs, &params) {
        Ok(outputs) => {
            for o in outputs {
                let kind = if o.signed { "signed" } else { "bits" };
                println!("{} = {}  ({kind}[{}])", o.name, o.value, o.width);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Parse `name=val,name=val` into a map, applying `val_parser` to each value.
/// An empty string is an empty map.
fn parse_bindings<T>(
    s: &str,
    val_parser: impl Fn(&str) -> Result<T, String>,
) -> Result<BTreeMap<String, T>, String> {
    let mut map = BTreeMap::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (name, val) = part
            .split_once('=')
            .ok_or_else(|| format!("expected `name=value`, got `{part}`"))?;
        map.insert(name.trim().to_string(), val_parser(val.trim())?);
    }
    Ok(map)
}

/// Parse a `u128` literal in decimal, `0x` hex, or `0b` binary.
fn parse_u128(s: &str) -> Result<u128, String> {
    let parsed = if let Some(hex) = s.strip_prefix("0x") {
        u128::from_str_radix(hex, 16)
    } else if let Some(bin) = s.strip_prefix("0b") {
        u128::from_str_radix(bin, 2)
    } else {
        s.parse::<u128>()
    };
    parsed.map_err(|_| format!("`{s}` is not a number (use decimal, 0x.., or 0b..)"))
}

/// `mimz translate <file> [--to <flavor>] [--order code|thamizh]` — re-emit the
/// file in another keyword flavor and/or word order (default: stdout).
///
/// `--to` defaults to english. With no `--order`, this is the lossless
/// keyword-only reskin (comments/layout preserved). With `--order`, it parses
/// to the AST and pretty-prints in the requested order — canonical layout,
/// comments dropped (spec/04: trivia-preservation is the `--to` path).
fn translate_file(
    path: &Path,
    to: Option<&str>,
    order: Option<&str>,
    output: Option<PathBuf>,
) -> ExitCode {
    let to = to.unwrap_or("english");
    let Some(flavor) = mimz::translate::parse_flavor(to) else {
        eprintln!("error: unknown flavor `{to}` — expected english, tanglish, or tamil");
        return ExitCode::FAILURE;
    };
    let order = match order {
        None => None,
        Some("code") => Some(mimz::pretty::Order::Code),
        Some("thamizh") => Some(mimz::pretty::Order::Thamizh),
        Some(other) => {
            eprintln!("error: unknown order `{other}` — expected code or thamizh");
            return ExitCode::FAILURE;
        }
    };
    let src = match project::read_source(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Produce the translated text. `--order` goes through the AST pretty-printer
    // (it can reorder clause heads); `--to` alone uses the trivia-preserving
    // token reskin.
    let text = match order {
        Some(order) => {
            let tokens = match mimz::lexer::lex(&src) {
                Ok(t) => t,
                Err(diags) => {
                    return Output::Human.one_file(&diags, &src, &path.display().to_string());
                }
            };
            match mimz::parser::parse(tokens) {
                Ok(file) => mimz::pretty::pretty_print(&file, flavor, order),
                Err(diags) => {
                    return Output::Human.one_file(&diags, &src, &path.display().to_string());
                }
            }
        }
        None => match mimz::translate::translate(&src, flavor) {
            Ok(t) => t,
            Err(diags) => {
                return Output::Human.one_file(&diags, &src, &path.display().to_string());
            }
        },
    };

    match output {
        Some(out_path) => {
            if let Err(e) = std::fs::write(&out_path, &text) {
                eprintln!("error: cannot write `{}`: {e}", out_path.display());
                return ExitCode::FAILURE;
            }
            println!(
                "translated {} -> {} ({})",
                path.display(),
                out_path.display(),
                mimz::translate::flavor_name(flavor)
            );
            ExitCode::SUCCESS
        }
        None => {
            print!("{text}");
            ExitCode::SUCCESS
        }
    }
}

/// `mimz explain <CODE>` — print the long-form teaching text for a diagnostic
/// code on stdout, or a friendly "unknown code" message (listing the valid
/// ones) on stderr. Backed by the lib so editors/WASM share the same catalog.
fn explain_code(code: &str) -> ExitCode {
    match mimz::explain::explain(code) {
        Some(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("error: no explanation for `{code}` — is it a real diagnostic code?");
            let codes: Vec<&str> = mimz::explain::codes().collect();
            eprintln!("known codes: {}", codes.join(", "));
            ExitCode::FAILURE
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
