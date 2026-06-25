//! mimz — the Min-Mozhi (மின்மொழி) compiler CLI.
//!
//! A thin shell over the [`mimz`] library crate (`src/lib.rs` holds the
//! crate map): argument parsing, human/JSON rendering of diagnostics,
//! file output, and the LSP server (`lsp.rs` — bin-only so the lib
//! stays async-free) live here — every compiler stage lives in the lib.

// No `unsafe` in the CLI either (see `lib.rs`) — locked by the compiler.
#![forbid(unsafe_code)]

mod commands;
#[cfg(feature = "lsp")]
mod lsp;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser as ClapParser, Subcommand};

use mimz::lexer::token::Flavor;
use mimz::project::{LoadError, LoadedFile};
use mimz::{diag, project};

use commands::{
    EjectFlavor, check, compile, completions, doctor, eject_std, eval_file, explain_code, fmt_file,
    init, lint_file, repl, resolve_config, sim_file, test_file, translate_file,
};

/// Compiler for Min-Mozhi (மின்மொழி), a Tamil-rooted HDL.
///
/// Each subcommand has its own `--help` with full details.
/// Use `mimz <command> --help` for detailed documentation.
///
/// EXIT CODES:
///   0  Success
///   1  Generic Failure (compilation errors, check failures, I/O errors, etc.)
#[derive(ClapParser)]
#[command(
    name = "mimz",
    version,
    about = "Min-Mozhi (மின்மொழி) — the first Tamil-rooted HDL. Reads like Go/TypeScript, safe like Rust.",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub(crate) struct Cli {
    /// Path to a `mimz.toml` config (default: walked up from the input file).
    #[arg(short = 'c', long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Control colorized output: always, never, auto (default: auto).
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    color: ColorChoice,

    /// Suppress status banners and non-essential progress output.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Enable verbose progress and debug logging.
    #[arg(short = 'd', long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ColorChoice {
    Always,
    Never,
    Auto,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum CliLang {
    #[value(alias = "en")]
    English,
    #[value(alias = "tl")]
    Tanglish,
    #[value(alias = "ta")]
    Tamil,
}

impl CliLang {
    fn to_str(self) -> &'static str {
        match self {
            CliLang::English => "english",
            CliLang::Tanglish => "tanglish",
            CliLang::Tamil => "tamil",
        }
    }
}

/// CLI subcommands.
///
/// Use `mimz <command> --help` for full details, including all flags.
#[derive(Subcommand)]
enum Cmd {
    /// Scaffold a new project in ./<name>/.
    ///
    /// Generates a documented `mimz.toml` and a starter `<name>.mimz` (a
    /// counter module plus a passing inline `test` block), so `mimz test` and
    /// `mimz compile` work right away. Refuses to overwrite a non-empty dir.
    Init {
        /// Project name — becomes the new directory and the `.mimz` file name.
        name: String,
    },
    /// Check a .mimz file for errors.
    ///
    /// Lex + parse + check a file and report errors (no output written).
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
        #[arg(short = 'l', long)]
        lang: Option<CliLang>,
        /// Re-check on every save until Ctrl-C (watches the file's directory).
        #[arg(long)]
        watch: bool,
    },
    /// Compile a .mimz file to Verilog.
    ///
    /// Compile a .mimz file (and its imports) to Verilog.
    Compile {
        /// The .mimz entry file
        file: PathBuf,
        /// Output path (default: entry file with .v extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Emit a standalone Verilog testbench (_tb.v) from inline test blocks
        #[arg(long)]
        emit_testbench: bool,
        /// Print diagnostics as a JSON array on stdout (tool consumers)
        #[arg(long)]
        json: bool,
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses). JSON output stays English.
        #[arg(short = 'l', long)]
        lang: Option<CliLang>,
    },
    /// Normalize a file's keyword flavor in place.
    ///
    /// Lossless — comments and layout are preserved; only keyword spellings
    /// change. Default target is the flavor the file predominantly uses;
    /// `--to` overrides. (Word-order reformatting is `translate --order`,
    /// which is not lossless.)
    #[command(alias = "format")]
    Fmt {
        /// The .mimz file to format
        file: PathBuf,
        /// Target flavor: english | tanglish | tamil (default: the file's majority)
        #[arg(long)]
        to: Option<CliLang>,
        /// Warn when the file mixes keyword flavors (mixing stays legal)
        #[arg(long)]
        strict: bool,
        /// Write here instead of overwriting the input file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run the language server over stdio.
    ///
    /// Diagnostics-only v0; editors launch this — not for interactive use.
    #[cfg(feature = "lsp")]
    Lsp,
    /// Explain a diagnostic code in depth, or list all codes with `--list`.
    ///
    /// Explain a diagnostic code in depth (e.g. `mimz explain E0501`).
    /// `mimz explain --list` prints every code with its one-line summary.
    Explain {
        /// The diagnostic code to explain (case-insensitive, e.g. E0501)
        code: Option<String>,
        /// List all diagnostic codes with one-line summaries
        #[arg(long)]
        list: bool,
    },
    /// Report toolchain & environment health.
    ///
    /// Prints mimz version/edition, platform, an in-memory compile smoke test,
    /// and which optional simulators (iverilog, verilator, gtkwave) are present.
    /// Exits non-zero only on a real problem (broken pipeline, unwritable temp
    /// dir, invalid `mimz.toml`) — missing optional tools are warnings.
    /// `--dev` adds the contributor toolchain (Rust, WASM target, test tools).
    #[command(alias = "env")]
    Doctor {
        /// Also check the compiler-development toolchain (Rust, wasm-pack, …).
        #[arg(long)]
        dev: bool,
    },
    /// Generate a shell tab-completion script.
    ///
    /// Prints the script for the given shell to stdout, e.g.
    /// `mimz completions bash > /etc/bash_completion.d/mimz` or
    /// `mimz completions powershell >> $PROFILE`.
    Completions {
        /// Target shell: bash | zsh | fish | powershell | elvish
        shell: clap_complete::Shell,
    },
    /// Write the embedded standard library to a directory for vendoring
    /// (then set `mimz.toml [lib] std = "<dir>"`).
    Eject {
        /// What to eject (only `std` is supported)
        #[arg(value_parser = ["std"])]
        what: String,
        /// Destination directory (created if missing)
        #[arg(long, default_value = "std")]
        to: PathBuf,
        /// Eject the pure-Tamil twins instead of the English canonical
        #[arg(long)]
        flavor: Option<String>,
        /// Overwrite existing files
        #[arg(long)]
        force: bool,
    },
    /// Reskin a file's keywords into another flavor.
    ///
    /// Also converts word order between `code` and `thamizh` (spec/04).
    /// `--to` alone is lossless (keyword tokens only, comments/layout
    /// preserved); `--order` re-emits from the AST (canonical layout,
    /// comments dropped).
    Translate {
        /// The .mimz file to translate
        file: PathBuf,
        /// Target keyword flavor: english | tanglish | tamil (default: english)
        #[arg(long)]
        to: Option<CliLang>,
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
    /// (experimental) Evaluate a combinational module.
    ///
    /// No clocks/regs/instances (that is the Phase 1.5 simulator);
    /// a slice of it, for quick checks and the future REPL.
    #[command(alias = "evaluate")]
    Eval {
        /// The .mimz file
        file: PathBuf,
        /// Input values, comma-separated: `-i a=3,b=5` (dec/0x/0b)
        #[arg(short = 'i', long = "in", value_name = "NAME=VAL,...")]
        inputs: String,
        /// Parameter overrides, comma-separated: `--param WIDTH=4`
        #[arg(long, default_value = "")]
        param: String,
        /// Which module to evaluate (default: the file's only module)
        #[arg(long)]
        module: Option<String>,
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses)
        #[arg(short = 'l', long)]
        lang: Option<CliLang>,
    },
    /// (experimental) Simulate a clocked module.
    ///
    /// Runs a default stimulus (reset asserted the first cycle, inputs held,
    /// the clock toggled) and writes a VCD waveform (`-o`) and/or a per-cycle
    /// console trace (`--trace`). Single-module for now; use `mimz eval` for
    /// a combinational module.
    #[command(alias = "simulate")]
    Sim {
        /// The .mimz file
        file: PathBuf,
        /// Write a VCD waveform here (open in GTKWave). Omit to skip the VCD.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Number of clock cycles to run
        #[arg(short = 'n', long, default_value_t = 16, value_parser = clap::value_parser!(u64).range(1..=mimz::sim::run::MAX_SIM_CYCLES))]
        cycles: u64,
        /// Which clock to drive (default: the module's only clock)
        #[arg(long)]
        clock: Option<String>,
        /// Input values, comma-separated, held for the run: `-i a=3,b=5`
        #[arg(short = 'i', long = "in", default_value = "")]
        inputs: String,
        /// Parameter overrides, comma-separated: `--param WIDTH=4`
        #[arg(long, default_value = "")]
        param: String,
        /// Combinational only: sweep inputs over value lists, one frame per
        /// combination: `--sweep a=0|1|2,b=3|4` (entries `,`, values `|`)
        #[arg(long, default_value = "")]
        sweep: String,
        /// Which module to simulate (default: the file's only module)
        #[arg(long)]
        module: Option<String>,
        /// Per-cycle console trace: `--trace` (every-cycle table) or
        /// `--trace=changes` (on-change, `$monitor`-style)
        #[arg(long, value_name = "STYLE", num_args = 0..=1, default_missing_value = "table")]
        trace: Option<String>,
        /// Trace ALL signals (clocks/resets/wires too), not just inputs/outputs/regs
        #[arg(long)]
        verbose: bool,
        /// Trace only these comma-separated signals
        #[arg(long)]
        signals: Option<String>,
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses)
        #[arg(short = 'l', long)]
        lang: Option<CliLang>,
    },
    /// Run the file's test blocks and report pass/fail.
    ///
    /// A failing `expect` prints a teaching-quality message (the expression,
    /// the cycle, each side's value); the command exits non-zero if any test
    /// fails. Add `--trace` for a per-cycle console trace.
    Test {
        /// The .mimz file
        file: PathBuf,
        /// Run only tests whose name contains this substring
        #[arg(short = 'f', long)]
        filter: Option<String>,
        /// Per-cycle console trace: `--trace` (every-cycle table) or
        /// `--trace=changes` (on-change, `$monitor`-style)
        #[arg(long, value_name = "STYLE", num_args = 0..=1, default_missing_value = "table")]
        trace: Option<String>,
        /// Trace ALL signals (clocks/resets/wires too), not just inputs/outputs/regs
        #[arg(long)]
        verbose: bool,
        /// Trace only these comma-separated signals
        #[arg(long)]
        signals: Option<String>,
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses)
        #[arg(short = 'l', long)]
        lang: Option<CliLang>,
    },
    /// (experimental) Interactive REPL for a combinational module.
    ///
    /// Parses a .mimz file once, then reads input bindings from stdin.
    /// Each line is evaluated immediately and outputs are printed.
    ///
    ///   mimz> a=3, b=5
    ///   sum = 8  (bits[9])
    ///
    /// Commands: `:quit` / `:q` to exit, `:help` for help.
    Repl {
        /// The .mimz file
        file: PathBuf,
        /// Parameter overrides, comma-separated: `--param WIDTH=4`
        #[arg(long, default_value = "")]
        param: String,
        /// Which module to evaluate (default: the file's only module)
        #[arg(long)]
        module: Option<String>,
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses)
        #[arg(short = 'l', long)]
        lang: Option<CliLang>,
    },
    /// Lint a .mimz file for style and hygiene warnings.
    ///
    /// Separate from `check` (which is correctness). Lint checks for things
    /// like unused signals and naming convention violations. All diagnostics
    /// are warnings — `mimz lint` never fails the build.
    Lint {
        /// The .mimz file to lint
        file: PathBuf,
        /// Print diagnostics as a JSON array on stdout (tool consumers)
        #[arg(long)]
        json: bool,
        /// Error-message language: english | tanglish | tamil (default: the
        /// flavor the file predominantly uses). JSON output stays English.
        #[arg(short = 'l', long)]
        lang: Option<CliLang>,
    },
}

fn main() -> ExitCode {
    // `--version` prints the uname-style block (variant on top, both axes),
    // handled here so the variant is truly the top line — clap's `long_version`
    // would prepend the binary name. `-V` keeps clap's short crate version.
    if std::env::args().skip(1).any(|a| a == "--version") {
        println!("{}", mimz::version::version_block());
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();

    // Check color choice
    let use_color = match cli.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            use std::io::IsTerminal;
            std::io::stderr().is_terminal()
        }
    };
    mimz::diag::set_color_enabled(use_color);

    let config_path = cli.config;
    let quiet = cli.quiet;
    let debug = cli.debug;

    match cli.command {
        Cmd::Init { name } => init(&name, quiet),
        Cmd::Check {
            file,
            tokens,
            json,
            lang,
            watch,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang_str = lang.map(|l| l.to_str().to_string()).or(cfg.lang);
            check(
                &file,
                tokens,
                json,
                lang_str.as_deref(),
                config_path.as_deref(),
                quiet,
                debug,
                watch,
            )
        }
        Cmd::Compile {
            file,
            output,
            emit_testbench,
            json,
            lang,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang_str = lang.map(|l| l.to_str().to_string()).or(cfg.lang);
            let emit_testbench = emit_testbench || cfg.compile.emit_testbench.unwrap_or(false);
            compile(
                &file,
                output,
                emit_testbench,
                json,
                lang_str.as_deref(),
                config_path.as_deref(),
                quiet,
                debug,
            )
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
            let to_str = to.map(|l| l.to_str().to_string()).or(cfg.fmt.to);
            let strict = strict || cfg.fmt.strict.unwrap_or(false);
            fmt_file(&file, to_str.as_deref(), strict, output, quiet, debug)
        }
        #[cfg(feature = "lsp")]
        Cmd::Lsp => {
            lsp::run();
            ExitCode::SUCCESS
        }
        Cmd::Explain { code, list } => explain_code(code.as_deref(), list),
        Cmd::Doctor { dev } => doctor(dev),
        Cmd::Repl {
            file,
            param,
            module,
            lang,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang_str = lang.map(|l| l.to_str().to_string()).or(cfg.lang);
            repl(&file, &param, module, lang_str.as_deref(), debug)
        }
        Cmd::Lint { file, json, lang } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang_str = lang.map(|l| l.to_str().to_string()).or(cfg.lang);
            lint_file(&file, json, lang_str.as_deref(), quiet, debug)
        }
        Cmd::Completions { shell } => completions(shell),
        Cmd::Eject {
            what: _,
            to,
            flavor,
            force,
        } => {
            let f = match flavor.as_deref() {
                None | Some("english") => EjectFlavor::English,
                Some("tamil") => EjectFlavor::Tamil,
                Some(other) => {
                    eprintln!("error: unknown --flavor `{other}` (english | tamil)");
                    return ExitCode::FAILURE;
                }
            };
            eject_std(&to, f, force)
        }
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
            let to_str = to.map(|l| l.to_str().to_string()).or(cfg.translate.to);
            let order = order.or(cfg.translate.order);
            let romanize_names = romanize_names || cfg.translate.romanize_names.unwrap_or(false);
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
                to_str.as_deref(),
                order.as_deref(),
                romanize_names,
                names_map.as_deref(),
                auto_names_map,
                output,
                quiet,
                debug,
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
            let lang_str = lang.map(|l| l.to_str().to_string()).or(cfg.lang);
            eval_file(
                &file,
                &inputs,
                &param,
                module,
                lang_str.as_deref(),
                quiet,
                debug,
            )
        }
        Cmd::Sim {
            file,
            output,
            cycles,
            clock,
            inputs,
            param,
            sweep,
            module,
            trace,
            verbose,
            signals,
            lang,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang_str = lang.map(|l| l.to_str().to_string()).or(cfg.lang);
            sim_file(
                &file,
                output,
                cycles,
                clock,
                &inputs,
                &param,
                &sweep,
                module,
                trace,
                verbose,
                signals,
                lang_str.as_deref(),
                config_path.as_deref(),
                quiet,
                debug,
            )
        }
        Cmd::Test {
            file,
            filter,
            trace,
            verbose,
            signals,
            lang,
        } => {
            let cfg = match resolve_config(&file, config_path.as_deref()) {
                Ok(c) => c,
                Err(code) => return code,
            };
            let lang_str = lang.map(|l| l.to_str().to_string()).or(cfg.lang);
            test_file(
                &file,
                filter,
                trace,
                verbose,
                signals,
                lang_str.as_deref(),
                config_path.as_deref(),
                quiet,
                debug,
            )
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
