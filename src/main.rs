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

use mimz::lexer::token::Flavor;
use mimz::project::{LoadError, LoadedFile};
use mimz::{ast, checker, diag, emit_verilog, lexer, morph, parser, project, sim};

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

/// Resolve the `mimz.toml` governing `input` (explicit `--config` wins, else
/// walk up from the file), turning a parse error into a printed message + the
/// failing exit code.
fn resolve_config(input: &Path, explicit: Option<&Path>) -> Result<mimz::config::Config, ExitCode> {
    mimz::config::Config::resolve(input, explicit).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

/// `mimz eval <file> --in a=3,b=5` — interpret a combinational module and print
/// each output. Lexes/parses the file directly (no import resolution — the
/// evaluator is single-module, combinational only) and reports a clear message
/// on anything out of that scope.
fn eval_file(
    path: &Path,
    inputs: &str,
    param: &str,
    module: Option<String>,
    lang: Option<&str>,
) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::Human(flavor);
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
        Err(diags) => return out.one_file(&diags, &src, &path_str),
    };
    // Non-fatal mixed-flavor warning (W0001) — printed, never blocks eval.
    if let Some(w) = morph::flavor_mix_warning(&tokens) {
        eprint!("{}", diag::render_lang(&[w], &src, &path_str, flavor));
    }
    let file = match parser::parse(tokens) {
        Ok(f) => f,
        Err(diags) => return out.one_file(&diags, &src, &path_str),
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

/// `mimz fmt <file>` — normalize the file's keyword flavor in place. Token-based
/// (via [`translate`](mimz::translate)), so comments, layout, identifiers, and
/// numbers are preserved byte-for-byte — only keyword spellings change. The
/// target flavor is `--to` if given, else the file's predominant flavor
/// (`morph::majority_flavor`). With `--strict`, a file that mixes keyword flavors
/// gets a warning first (mixing stays legal — spec/03, the learning path).
fn fmt_file(path: &Path, to: Option<&str>, strict: bool, output: Option<PathBuf>) -> ExitCode {
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
        Err(diags) => return Output::Human(Flavor::English).one_file(&diags, &src, &path_str),
    };
    // Target flavor: explicit `--to`, else the flavor the file mostly uses.
    let target = match to {
        Some(s) => match morph::parse_lang(s) {
            Some(f) => f,
            None => {
                eprintln!("error: unknown flavor `{s}` — expected english, tanglish, or tamil");
                return ExitCode::FAILURE;
            }
        },
        None => morph::majority_flavor(&tokens),
    };
    // `--strict` is the lint mode: it still normalizes (writes the fix), but a
    // mixed-flavor input is also reported and makes the command exit non-zero so
    // CI can flag it. Mixing stays legal under a plain `fmt`.
    let mut mixed = false;
    if strict {
        let used = morph::flavors_used(&tokens);
        if used.len() > 1 {
            mixed = true;
            let names: Vec<&str> = used
                .iter()
                .map(|&f| mimz::translate::flavor_name(f))
                .collect();
            eprintln!(
                "warning: `{path_str}` mixes keyword flavors ({}) — normalizing to {}",
                names.join(", "),
                mimz::translate::flavor_name(target)
            );
        }
    }
    let text = match mimz::translate::translate(&src, target) {
        Ok(t) => t,
        Err(diags) => return Output::Human(Flavor::English).one_file(&diags, &src, &path_str),
    };
    let out_path = output.unwrap_or_else(|| path.to_path_buf());
    // Write atomically: a sibling temp file then rename over the target, so an
    // interrupted or failing write can never truncate the file being formatted
    // (the common case is `fmt` overwriting its own input in place). The temp
    // name carries the PID so concurrent `fmt` runs don't collide.
    let mut tmp = out_path.clone().into_os_string();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    if let Err(e) = std::fs::write(&tmp, &text) {
        eprintln!("error: cannot write `{}`: {e}", tmp.display());
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::rename(&tmp, &out_path) {
        let _ = std::fs::remove_file(&tmp);
        eprintln!("error: cannot write `{}`: {e}", out_path.display());
        return ExitCode::FAILURE;
    }
    println!(
        "formatted {} ({})",
        out_path.display(),
        mimz::translate::flavor_name(target)
    );
    if mixed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
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
    romanize_names: bool,
    names_map: Option<&Path>,
    auto_names_map: bool,
    output: Option<PathBuf>,
) -> ExitCode {
    let to = to.unwrap_or("english");
    let Some(flavor) = mimz::translate::parse_flavor(to) else {
        eprintln!("error: unknown flavor `{to}` — expected english, tanglish, or tamil");
        return ExitCode::FAILURE;
    };
    // `--romanize-names` (Tamil -> Latin) and `--names-map` (Latin -> Tamil) are
    // opposite directions; running both at once is a mistake, not a no-op.
    if romanize_names && names_map.is_some() {
        eprintln!(
            "error: --romanize-names and --names-map are opposite directions — use one (romanize to Latin, or restore Tamil from a map)"
        );
        return ExitCode::FAILURE;
    }
    if romanize_names && order.is_some() {
        eprintln!(
            "warning: --romanize-names applies to the keyword-only reskin; \
             ignored with --order"
        );
    }
    if names_map.is_some() && order.is_some() {
        eprintln!("warning: --names-map applies to the keyword-only reskin; ignored with --order");
    }
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

    // Resolve the name-map for a reverse reskin: an explicit `--names-map` wins;
    // otherwise auto-discover the `<input>.names.json` sidecar (when present and
    // not disabled). Only the plain token reskin restores names — never `--order`
    // (AST path) or a forward `--romanize-names`. The bool records whether it was
    // auto-discovered, so we can tell the user.
    let names_for_restore: Option<(PathBuf, bool)> = if order.is_some() || romanize_names {
        None
    } else if let Some(p) = names_map {
        Some((p.to_path_buf(), false))
    } else if auto_names_map {
        let sidecar = names_sidecar(path);
        sidecar.is_file().then_some((sidecar, true))
    } else {
        None
    };

    // Produce the translated text. `--order` goes through the AST pretty-printer
    // (it can reorder clause heads); `--to` alone uses the trivia-preserving
    // token reskin. On a forward romanize, `captured_map` holds the sidecar.
    let render_err = |diags: &[diag::Diag]| {
        Output::Human(Flavor::English).one_file(diags, &src, &path.display().to_string())
    };
    let mut captured_map: Option<mimz::translate::NameMap> = None;
    let text = match order {
        Some(order) => {
            let tokens = match mimz::lexer::lex(&src) {
                Ok(t) => t,
                Err(diags) => return render_err(&diags),
            };
            match mimz::parser::parse(tokens) {
                Ok(file) => mimz::pretty::pretty_print(&file, flavor, order),
                Err(diags) => return render_err(&diags),
            }
        }
        None if names_for_restore.is_some() => {
            let (map_path, auto) = names_for_restore.as_ref().unwrap();
            let map = match load_name_map(map_path) {
                Ok(m) => m,
                Err(code) => return code,
            };
            if *auto {
                eprintln!(
                    "note: restoring names from {} (auto-discovered; --no-names-map to disable)",
                    map_path.display()
                );
            }
            match mimz::translate::restore_with_map(&src, flavor, &map) {
                Ok(t) => t,
                Err(diags) => return render_err(&diags),
            }
        }
        None if romanize_names => match mimz::translate::romanize_with_map(&src, flavor) {
            Ok((t, map)) => {
                captured_map = Some(map);
                t
            }
            Err(diags) => return render_err(&diags),
        },
        None => match mimz::translate::translate(&src, flavor) {
            Ok(t) => t,
            Err(diags) => return render_err(&diags),
        },
    };

    match output {
        Some(out_path) => {
            if let Err(e) = std::fs::write(&out_path, &text) {
                eprintln!("error: cannot write `{}`: {e}", out_path.display());
                return ExitCode::FAILURE;
            }
            // Forward romanize with names to record: write the sidecar beside the
            // output (`<out>.names.json`) so the run is reversible via --names-map.
            if let Some(map) = captured_map.filter(|m| !m.names.is_empty()) {
                let sidecar = names_sidecar(&out_path);
                let json = serde_json::to_string_pretty(&map).expect("NameMap serializes");
                if let Err(e) = std::fs::write(&sidecar, json) {
                    eprintln!("error: cannot write name map `{}`: {e}", sidecar.display());
                    return ExitCode::FAILURE;
                }
                println!("wrote name map {}", sidecar.display());
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
            if captured_map.is_some_and(|m| !m.names.is_empty()) {
                eprintln!(
                    "note: --romanize-names without -o does not write a name map — the round-trip back to Tamil won't be reversible"
                );
            }
            print!("{text}");
            ExitCode::SUCCESS
        }
    }
}

/// The sidecar path for a translate output: `<out>.names.json` (append, so
/// `foo.mimz` → `foo.mimz.names.json`, never replacing the existing extension).
fn names_sidecar(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_owned();
    name.push(".names.json");
    PathBuf::from(name)
}

/// Read + parse a `--names-map` file into a [`mimz::translate::NameMap`], or print
/// a clean error and return the failing exit code.
fn load_name_map(path: &Path) -> Result<mimz::translate::NameMap, ExitCode> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read name map `{}`: {e}", path.display());
        ExitCode::FAILURE
    })?;
    let map: mimz::translate::NameMap = serde_json::from_str(&text).map_err(|e| {
        eprintln!("error: invalid name map `{}`: {e}", path.display());
        ExitCode::FAILURE
    })?;
    // Honor the format version: reject a map written by a newer/unknown format
    // rather than silently mis-restoring (the `version` field exists for this).
    if map.version != mimz::translate::NameMap::VERSION {
        eprintln!(
            "error: name map `{}` is format version {}, but this mimz understands version {} — regenerate it with --romanize-names",
            path.display(),
            map.version,
            mimz::translate::NameMap::VERSION
        );
        return Err(ExitCode::FAILURE);
    }
    Ok(map)
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
    /// Human carets on stderr, with the error-message language to render in.
    Human(Flavor),
    Json,
}

impl Output {
    fn new(json: bool, lang: Flavor) -> Self {
        if json {
            Output::Json
        } else {
            Output::Human(lang)
        }
    }

    /// Report diagnostics that all point into ONE known source. Exits FAILURE
    /// only if some diagnostic is an error — a warning-only set (or empty, on
    /// `--json`) still succeeds.
    fn one_file(self, diags: &[diag::Diag], src: &str, path: &str) -> ExitCode {
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
    fn project(self, diags: &[diag::Diag], files: &[LoadedFile]) -> ExitCode {
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
    fn load_error(self, e: &LoadError) -> ExitCode {
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

/// The error-message language for a command: an explicit `--lang` (validated,
/// erroring with `ExitCode::FAILURE` on an unknown value) else the entry file's
/// predominant keyword flavor (`morph::majority_flavor`). Majority detection is
/// best-effort — the command re-reads and reports any real I/O / lex failure
/// itself, so a file that cannot be read here simply defaults to English.
fn resolve_lang(path: &Path, lang: Option<&str>) -> Result<Flavor, ExitCode> {
    if let Some(s) = lang {
        return morph::parse_lang(s).ok_or_else(|| {
            eprintln!("error: unknown language `{s}` — expected english, tanglish, or tamil");
            ExitCode::FAILURE
        });
    }
    Ok(project::read_source(path)
        .ok()
        .and_then(|src| lexer::lex(&src).ok())
        .map(|toks| morph::majority_flavor(&toks))
        .unwrap_or(Flavor::English))
}

/// Non-fatal warnings for a loaded project (currently just the mixed-flavor
/// lint, W0001), each tagged with its file index. Re-lexes each already-loaded
/// source — cheap, and it lexed clean during `load_project`.
fn project_warnings(files: &[LoadedFile]) -> Vec<diag::Diag> {
    files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            lexer::lex(&f.src)
                .ok()
                .and_then(|toks| morph::flavor_mix_warning(&toks))
                .map(|d| d.with_file(i))
        })
        .collect()
}

/// `mimz check` — lex + parse + checker passes over the file AND its
/// imports (cross-file names must resolve), reporting all diagnostics.
/// With `--tokens` it stops after the lexer and dumps the token stream
/// instead (the standard way to debug lexer issues).
fn check(path: &Path, tokens: bool, json: bool, lang: Option<&str>) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::new(json, flavor);
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
    // Non-fatal warnings (W0001 mixed-flavor) ride alongside any checker errors.
    let mut diags = project_warnings(&files);
    if let Err(errors) = checker::check(&asts) {
        diags.extend(errors);
    }
    let has_error = diags.iter().any(|d| d.is_error());
    if json {
        // Stable contract: stdout is ALWAYS a JSON array (warnings included, or
        // `[]`). Exit reflects severity.
        return out.project(&diags, &files);
    }
    if !diags.is_empty() {
        eprint!("{}", project::render_diags_lang(&diags, &files, flavor));
    }
    if has_error {
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
fn compile(path: &Path, output: Option<PathBuf>, json: bool, lang: Option<&str>) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::new(json, flavor);
    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    // Non-fatal warnings (W0001 mixed-flavor) ride alongside any stage errors,
    // and are surfaced on success too.
    let warnings = project_warnings(&files);
    let report_err = |errors: Vec<diag::Diag>| {
        let mut diags = warnings.clone();
        diags.extend(errors);
        out.project(&diags, &files)
    };

    if let Err(errors) = checker::check(&asts) {
        return report_err(errors);
    }

    // Tamil identifiers become readable ASCII (விளக்கு → villakku) —
    // checked against the original spelling above, emitted as Verilog
    // names below.
    emit_verilog::transliterate(&mut asts);

    let project = match emit_verilog::Project::from_files(&asts) {
        Ok(p) => p,
        Err(errors) => return report_err(errors),
    };
    let verilog = match emit_verilog::emit(&project, &asts) {
        Ok(v) => v,
        Err(errors) => return report_err(errors),
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
    // Success: surface any non-fatal warnings (json → the array, else stderr).
    if json {
        out.project(&warnings, &files);
    } else {
        if !warnings.is_empty() {
            eprint!("{}", project::render_diags_lang(&warnings, &files, flavor));
        }
        println!("compiled {} -> {}", path.display(), out_path.display());
    }
    ExitCode::SUCCESS
}
