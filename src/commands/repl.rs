//! `mimz repl <file> [--module M] [--param P=8]` — interactive combinational
//! evaluator. Parses the file once, then reads input bindings from stdin.
//! Supports `:quit` / `:q` to exit and `:help` for usage.

use std::path::Path;
use std::process::ExitCode;

use mimz::{diag, lexer, morph, parser, project, sim};

use super::helpers::{parse_bindings, parse_u128, resolve_lang};
use crate::Output;

/// `mimz repl <file> [--module M] [--param P=8]` — interactive combinational
/// evaluator. Parses the file once, then reads input bindings from stdin:
///
/// ```text
/// mimz> a=3, b=5
/// sum = 8  (bits[9])
/// ```
///
/// Commands: `:quit` / `:q` / Ctrl-D to exit, `:help` for help.
pub(crate) fn repl(
    path: &Path,
    param: &str,
    module: Option<String>,
    lang: Option<&str>,
    debug: bool,
) -> ExitCode {
    if debug {
        eprintln!("debug: starting REPL for file {}", path.display());
    }
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
    if let Some(w) = morph::flavor_mix_warning(&tokens) {
        eprint!("{}", diag::render_lang(&[w], &src, &path_str, flavor));
    }
    let file = match parser::parse(tokens) {
        Ok(f) => f,
        Err(diags) => return out.one_file(&diags, &src, &path_str),
    };

    // Parse initial parameters.
    let mut params = match parse_bindings(param, |s| parse_u128(s).map(|v| v as i128)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Print banner.
    let mod_name = module
        .as_deref()
        .or_else(|| {
            file.items.iter().find_map(|it| match it {
                mimz::ast::TopItem::Module(m) => Some(m.name.name.as_str()),
                _ => None,
            })
        })
        .unwrap_or("<unknown>");
    println!("Min-Mozhi REPL  —  module `{mod_name}`  (Ctrl-C or :quit to exit)");
    println!();

    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut stdout = std::io::stdout();

    loop {
        print!("mimz> ");
        let _ = stdout.flush();
        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        // Built-in commands.
        if line.starts_with(':') {
            match line.as_str() {
                ":quit" | ":q" => break,
                ":help" => {
                    println!("  a=3, b=5        evaluate with these input values");
                    println!("  :param P=8       set parameter P to 8 for this session");
                    println!("  :quit  :q        exit the REPL");
                    println!("  :help            this help");
                }
                cmd if cmd.starts_with(":param ") => {
                    let rest = cmd.trim_start_matches(":param ").trim();
                    match parse_bindings(rest, |s| parse_u128(s).map(|v| v as i128)) {
                        Ok(bindings) => {
                            params.extend(bindings);
                            println!("  param {rest}");
                        }
                        Err(e) => eprintln!("error: {e}"),
                    }
                }
                other => eprintln!("unknown command `{other}` — try :help"),
            }
            continue;
        }

        // Evaluate with these input bindings.
        match parse_bindings(&line, parse_u128) {
            Ok(inputs) => match sim::comb::eval_outputs(
                std::slice::from_ref(&file),
                module.as_deref(),
                &inputs,
                &params,
            ) {
                Ok(outputs) => {
                    for o in outputs {
                        let kind = if o.signed { "signed" } else { "bits" };
                        println!("  {} = {}  ({kind}[{}])", o.name, o.value, o.width);
                    }
                }
                Err(e) => eprintln!("error: {e}"),
            },
            Err(e) => eprintln!("error: {e}"),
        }
    }

    ExitCode::SUCCESS
}
