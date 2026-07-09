//! Project-level types shared by the pure pipeline: the loaded-file record
//! diagnostics point into, and rendering diagnostics against it. Loading a
//! project from disk (I/O, `import` resolution) is impure and stays in the
//! root `mimz` crate's `project` module.

use std::path::PathBuf;

use crate::lexer::token::Flavor;
use crate::{ast, diag};

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

/// Render project-level diagnostics, each against the file its `file`
/// index names (the entry file when unset). Single-file passes render
/// with `diag::render` directly; this exists for passes that see the
/// whole project (`Project::from_files`, the emitter), whose spans may
/// point into any loaded file.
pub fn render_diags(diags: &[diag::Diag], files: &[LoadedFile]) -> String {
    render_diags_lang(diags, files, Flavor::English)
}

/// Like [`render_diags`], but renders each message in `flavor` where the
/// localized catalog covers its E-code (Phase 1.8, spec/04 section 5); English
/// otherwise. The CLI passes the file's effective error language here.
pub fn render_diags_lang(diags: &[diag::Diag], files: &[LoadedFile], flavor: Flavor) -> String {
    let mut out = String::new();
    for d in diags {
        let f = &files[d.file.unwrap_or(0).min(files.len() - 1)];
        out.push_str(&diag::render_lang(
            std::slice::from_ref(d),
            &f.src,
            &f.path.display().to_string(),
            flavor,
        ));
    }
    out
}
