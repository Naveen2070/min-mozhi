//! Source loading and project assembly: reading + NFC-normalizing files,
//! running lexer/parser, and resolving `import` declarations
//! (file-relative, `.mimz` extension, cycle-safe — spec/02 section 1.5).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use unicode_normalization::UnicodeNormalization;

use crate::{ast, diag, lexer, parser};

/// One successfully lexed + parsed source file. The original source text is
/// kept alongside the AST because diagnostics render spans against it.
pub struct LoadedFile {
    /// Path as given on the command line / resolved from an `import`.
    pub path: PathBuf,
    /// NFC-normalized source text (what all spans refer to).
    pub src: String,
    /// The parsed file.
    pub ast: ast::File,
}

/// Read + NFC-normalize a source file (spec/02 section 2: lexing is defined over
/// NFC-normalized text so Tamil combining marks compare consistently).
pub fn read_source(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s.nfc().collect()),
        Err(e) => Err(format!("cannot read `{}`: {e}", path.display())),
    }
}

/// Lex + parse one file, printing rendered diagnostics on failure.
pub fn parse_file(path: &Path) -> Result<LoadedFile, ExitCode> {
    let src = match read_source(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return Err(ExitCode::FAILURE);
        }
    };
    let display = path.display().to_string();
    let toks = match lexer::lex(&src) {
        Ok(t) => t,
        Err(diags) => {
            eprint!("{}", diag::render(&diags, &src, &display));
            return Err(ExitCode::FAILURE);
        }
    };
    let ast = match parser::parse(toks) {
        Ok(f) => f,
        Err(diags) => {
            eprint!("{}", diag::render(&diags, &src, &display));
            return Err(ExitCode::FAILURE);
        }
    };
    Ok(LoadedFile {
        path: path.to_path_buf(),
        src,
        ast,
    })
}

/// Render project-level diagnostics, each against the file its `file`
/// index names (the entry file when unset). Single-file passes render
/// with `diag::render` directly; this exists for passes that see the
/// whole project (`Project::from_files`, the emitter), whose spans may
/// point into any loaded file.
pub fn render_diags(diags: &[diag::Diag], files: &[LoadedFile]) -> String {
    let mut out = String::new();
    for d in diags {
        let f = &files[d.file.unwrap_or(0).min(files.len() - 1)];
        out.push_str(&diag::render(
            std::slice::from_ref(d),
            &f.src,
            &f.path.display().to_string(),
        ));
    }
    out
}

/// Resolve `import` declarations transitively from the entry file.
/// Dots become path separators (`import lib.adder` → `lib/adder.mimz`,
/// relative to the importing file); duplicates and cycles are handled by
/// the canonicalized visited set. The entry file is always `files[0]`.
pub fn load_project(entry: &Path) -> Result<Vec<LoadedFile>, ExitCode> {
    let mut files: Vec<LoadedFile> = Vec::new();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: Vec<PathBuf> = vec![entry.to_path_buf()];

    while let Some(path) = queue.pop() {
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !visited.insert(canon) {
            continue;
        }
        let loaded = parse_file(&path)?;
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for import in &loaded.ast.imports {
            let mut p = dir.clone();
            for seg in &import.path {
                p.push(&seg.name);
            }
            p.set_extension("mimz");
            if !p.exists() {
                let diags = [diag::Diag::new(
                    import.span,
                    format!("imported file `{}` does not exist", p.display()),
                )
                .with_help(
                    "`import name` loads `name.mimz` relative to the importing file (spec/02 section 1.5)",
                )];
                eprint!(
                    "{}",
                    diag::render(&diags, &loaded.src, &loaded.path.display().to_string())
                );
                return Err(ExitCode::FAILURE);
            }
            queue.push(p);
        }
        files.push(loaded);
    }
    Ok(files)
}
