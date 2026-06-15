//! mimz — the Min-Mozhi (மின்மொழி) compiler CLI.
//!
//! A thin shell over the [`mimz`] library crate (`src/lib.rs` holds the
//! crate map): argument parsing, human/JSON rendering of diagnostics,
//! file output, and the LSP server (`lsp.rs` — bin-only so the lib
//! stays async-free) live here — every compiler stage lives in the lib.

// No `unsafe` in the CLI either (see `lib.rs`) — locked by the compiler.
#![forbid(unsafe_code)]

mod commands;
mod lsp;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser as ClapParser, Subcommand};

use mimz::lexer::token::Flavor;
use mimz::project::{LoadError, LoadedFile};
use mimz::{diag, project};

use commands::{check, compile, eval_file, explain_code, fmt_file, resolve_config, translate_file};

/// Top-level CLI definition. The `///` docs on [`Cmd`] variants and fields
/// double as the `--help` text (clap derive).
#[derive(ClapParser)]
#[command(
    name = "mimz",
    version,
    about = "Min-Mozhi (மின்மொழி) — the first Tamil-rooted HDL. Reads like Go/TypeScript, safe like Rust."
)]
struct Cli {
    /// Path to a `mimz.toml` config. Default: discovered by walking up from the
    /// input file (CLI flags always override config values).
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,
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
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses). JSON output stays English.
        #[arg(long)]
        lang: Option<String>,
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
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses). JSON output stays English.
        #[arg(long)]
        lang: Option<String>,
    },
    /// Normalize a file's keyword flavor in place (lossless — comments and
    /// layout are preserved; only keyword spellings change). Default target is
    /// the flavor the file predominantly uses; `--to` overrides. (Word-order
    /// reformatting is `translate --order`, which is not lossless.)
    Fmt {
        /// The .mimz file to format
        file: PathBuf,
        /// Target flavor: english | tanglish | tamil (default: the file's majority)
        #[arg(long)]
        to: Option<String>,
        /// Warn when the file mixes keyword flavors (mixing stays legal)
        #[arg(long)]
        strict: bool,
        /// Write here instead of overwriting the input file
        #[arg(short, long)]
        output: Option<PathBuf>,
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
        /// Romanize Tamil identifiers to readable Latin (கணக்கி -> kannakki),
        /// the same scheme the Verilog emitter uses. One-way on its own, but with
        /// `-o` a `<out>.names.json` sidecar is written so `--names-map` can
        /// restore the exact Tamil names. Applies to the keyword-only reskin
        /// (no `--order`).
        #[arg(long)]
        romanize_names: bool,
        /// Restore original Tamil identifiers from a name-map. Default: the
        /// `<input>.names.json` sidecar is auto-loaded when present; this flag
        /// overrides the path.
        #[arg(long, value_name = "FILE")]
        names_map: Option<PathBuf>,
        /// Do not auto-load the `<input>.names.json` sidecar (keep romanized
        /// Latin names as-is).
        #[arg(long)]
        no_names_map: bool,
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
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses)
        #[arg(long)]
        lang: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config_path = cli.config;
    match cli.command {
        Cmd::Check {
            file,
            tokens,
            json,
            lang,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang = lang.or(cfg.lang);
            check(&file, tokens, json, lang.as_deref())
        }
        Cmd::Compile {
            file,
            output,
            json,
            lang,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang = lang.or(cfg.lang);
            compile(&file, output, json, lang.as_deref())
        }
        Cmd::Fmt {
            file,
            to,
            strict,
            output,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let to = to.or(cfg.fmt.to);
            let strict = strict || cfg.fmt.strict.unwrap_or(false);
            fmt_file(&file, to.as_deref(), strict, output)
        }
        Cmd::Lsp => {
            lsp::run();
            ExitCode::SUCCESS
        }
        Cmd::Explain { code } => explain_code(&code),
        Cmd::Translate {
            file,
            to,
            order,
            romanize_names,
            names_map,
            no_names_map,
            output,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let to = to.or(cfg.translate.to);
            let order = order.or(cfg.translate.order);
            let romanize_names = romanize_names || cfg.translate.romanize_names.unwrap_or(false);
            // Auto name-map discovery is on unless `--no-names-map` or the config
            // turns it off (an unrecognized value warns and falls back to auto).
            let auto_names_map = if no_names_map {
                false
            } else {
                match cfg.translate.names_map.as_deref() {
                    Some("off") => false,
                    Some("auto") | None => true,
                    Some(other) => {
                        eprintln!(
                            "warning: [translate] names_map = \"{other}\" is not recognized — use \"auto\" or \"off\"; assuming \"auto\""
                        );
                        true
                    }
                }
            };
            translate_file(
                &file,
                to.as_deref(),
                order.as_deref(),
                romanize_names,
                names_map.as_deref(),
                auto_names_map,
                output,
            )
        }
        Cmd::Eval {
            file,
            inputs,
            param,
            module,
            lang,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang = lang.or(cfg.lang);
            eval_file(&file, &inputs, &param, module, lang.as_deref())
        }
    }
}

/// How diagnostics leave the process: rendered carets on stderr (human),
/// or one JSON array on stdout (`--json`, for editors and wrappers —
/// schema in docs/code/06-diagnostics.md).
#[derive(Clone, Copy)]
pub(crate) enum Output {
    /// Human carets on stderr, with the error-message language to render in.
    Human(Flavor),
    Json,
}

impl Output {
    pub(crate) fn new(json: bool, lang: Flavor) -> Self {
        if json {
            Output::Json
        } else {
            Output::Human(lang)
        }
    }

    /// Report diagnostics that all point into ONE known source. Exits FAILURE
    /// only if some diagnostic is an error — a warning-only set (or empty, on
    /// `--json`) still succeeds.
    pub(crate) fn one_file(self, diags: &[diag::Diag], src: &str, path: &str) -> ExitCode {
        match self {
            Output::Human(flavor) => eprint!("{}", diag::render_lang(diags, src, path, flavor)),
            Output::Json => {
                let json: Vec<diag::JsonDiag> = diags
                    .iter()
                    .map(|d| diag::JsonDiag::new(d, path, src))
                    .collect();
                println!("{}", serde_json::to_string(&json).expect("diag serializes"));
            }
        }
        Self::exit_for(diags)
    }

    /// Report project-wide diagnostics (each carries a file index). Exits
    /// FAILURE only if some diagnostic is an error (warnings are non-fatal).
    pub(crate) fn project(self, diags: &[diag::Diag], files: &[LoadedFile]) -> ExitCode {
        match self {
            Output::Human(flavor) => {
                eprint!("{}", project::render_diags_lang(diags, files, flavor))
            }
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
        Self::exit_for(diags)
    }

    /// FAILURE if any diagnostic is an error; SUCCESS otherwise (warnings-only
    /// or empty).
    fn exit_for(diags: &[diag::Diag]) -> ExitCode {
        if diags.iter().any(|d| d.is_error()) {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }

    /// Report a load failure (I/O, or lexer/parser/import diagnostics).
    pub(crate) fn load_error(self, e: &LoadError) -> ExitCode {
        match e {
            LoadError::Io(msg) => {
                match self {
                    Output::Human(_) => eprintln!("error: {msg}"),
                    Output::Json => {
                        println!(
                            "{}",
                            serde_json::json!([{ "severity": "error", "code": null, "message": msg }])
                        )
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
