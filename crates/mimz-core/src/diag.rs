//! Teaching diagnostics: every error says WHAT is wrong and, where possible,
//! HOW to fix it (spec/01 G1). Rendered with source excerpt and carets.
//! (English catalog first; Tanglish/Tamil catalogs land with Phase 1.8.)

use crate::lexer::token::Flavor;
use crate::span::Span;

/// Every stable checker error code (catalog: docs/code/11-checker.md).
/// THE machine-readable list — `tests/errors.rs` guards it against the
/// human catalog and demands an end-to-end fixture per code, and
/// `mimz-bench` measures fixture coverage against it. Append-only by
/// the E-code stability contract (codes are never renumbered).
pub const ALL_CHECKER_CODES: [&str; 67] = [
    "E0001", "E0002", "E0003", "E0004", "E0101", "E0102", "E0103", "E0104", "E0105", "E0106",
    "E0107", "E0108", "E0109", "E0110", "E0111", "E0201", "E0202", "E0301", "E0302", "E0303",
    "E0401", "E0402", "E0403", "E0404", "E0405", "E0406", "E0407", "E0408", "E0409", "E0410",
    "E0411", "E0412", "E0413", "E0414", "E0415", "E0416", "E0417", "E0501", "E0502", "E0503",
    "E0504", "E0505", "E0601", "E0602", "E0701", "E0801", "E0802", "E0803", "E0804", "E0805",
    "E0806", "E0807", "E0808", "E0809", "E0810", "E0811", "E0812", "E0813", "E0901", "E0902",
    "E0903", "E0906", "E0907", "E0909", "E0910", "E1301", "E1302",
];

/// How loud a diagnostic is. `Error` fails the build; `Warning` is advisory —
/// it is printed but the command still succeeds (exit 0) and still produces
/// output. Almost every `Diag` is an `Error`; warnings are opt-in via
/// [`Diag::as_warning`] (e.g. the mixed-flavor lint, W0001).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Fails the build; exit code reflects it.
    Error,
    /// Advisory only — printed, but the command still exits 0.
    Warning,
}

/// One compiler diagnostic. Diagnostics are plain values — passes collect
/// `Vec<Diag>` and keep going (multi-error reporting), they never panic
/// or print directly. Rendering happens once, in [`render`].
#[derive(Clone, Debug)]
pub struct Diag {
    /// Where in the source the problem is (drives the caret underline).
    pub span: Span,
    /// WHAT is wrong — one sentence, names the construct.
    pub msg: String,
    /// HOW to fix it — the teaching line, ideally with a spec reference.
    pub help: Option<String>,
    /// Which project file the span points into (index into the loaded
    /// file list). `None` in single-file passes (lexer, parser), where
    /// the caller already knows the file. Project-wide passes
    /// (`Project::from_files`, the checker, the emitter) MUST set this —
    /// see `project::render_diags`.
    pub file: Option<usize>,
    /// Stable code (`E0101` error, `W0001` warning), rendered as
    /// `error[E0101]: ...` / `warning[W0001]: ...`. Catalog lives in
    /// docs/code/11-checker.md + 06-diagnostics.md. Checker errors always
    /// carry one; lexer/parser errors will be retrofitted (Phase 1).
    pub code: Option<&'static str>,
    /// Error (fails the build) or Warning (advisory; exit 0). Defaults to
    /// `Error` in [`Diag::new`]; flip with [`Diag::as_warning`].
    pub severity: Severity,
    /// Structured interpolation args for the localized catalog, `(token, value)`
    /// — e.g. `("expected", "bits[8]")`. The English `msg` already bakes these
    /// in via `format!`; this carries the SAME values to `morph::fill` so a
    /// localized template can interpolate `{expected}` etc. Empty for most
    /// diagnostics. The `--json` and English paths ignore it.
    pub args: Vec<(&'static str, String)>,
}

impl Diag {
    /// A plain `Error`-severity diagnostic with no help text, code, or file
    /// index yet — attach those with the builder methods below.
    pub fn new(span: Span, msg: impl Into<String>) -> Self {
        Diag {
            span,
            msg: msg.into(),
            help: None,
            file: None,
            code: None,
            severity: Severity::Error,
            args: Vec::new(),
        }
    }

    /// Builder-style: attach a `(token, value)` interpolation arg for the
    /// localized catalog (the localizer fills `{token}`; see `morph::fill`).
    pub fn with_arg(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.args.push((key, value.into()));
        self
    }

    /// Builder-style: attach the `= help:` line.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Builder-style: record which project file the span points into.
    pub fn with_file(mut self, file: usize) -> Self {
        self.file = Some(file);
        self
    }

    /// Builder-style: attach the stable error code.
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    /// Builder-style: mark this diagnostic as a non-fatal warning (advisory;
    /// the command still succeeds and still produces output).
    pub fn as_warning(mut self) -> Self {
        self.severity = Severity::Warning;
        self
    }

    /// Whether this diagnostic should fail the build.
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Render diagnostics against the (NFC-normalized) source they refer to.
/// rustc-style shape:
///
/// ```text
/// error: register `value` has no reset value
///   --> examples/english/counter.mimz:7:3
///    |
///   7|   reg value: bits[WIDTH]
///    |   ^^^
///    = help: every reg declares its reset value ...
/// ```
///
/// Messages render in English. For another error `flavor` see [`render_lang`].
pub fn render(diags: &[Diag], src: &str, path: &str) -> String {
    render_lang(diags, src, path, Flavor::English)
}

use std::sync::atomic::{AtomicBool, Ordering};

static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable colorized diagnostics output globally.
pub fn set_color_enabled(enabled: bool) {
    COLOR_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check if colorized diagnostics output is globally enabled.
pub fn is_color_enabled() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

/// Like [`render`], but emits each message in `flavor` when the localized
/// catalog covers its E-code (`morph::localized_msg`); otherwise the English
/// `msg` is used verbatim. The caret/location/help layout is identical — only
/// the WHAT line is localized (Phase 1.8, spec/04 section 5).
pub fn render_lang(diags: &[Diag], src: &str, path: &str, flavor: Flavor) -> String {
    use owo_colors::OwoColorize;
    let mut out = String::new();
    let use_color = is_color_enabled();
    for d in diags {
        let (line_no, col, line_text, line_start) = locate(src, d.span.start);
        let msg = crate::morph::localized_msg(d, src, flavor).unwrap_or_else(|| d.msg.clone());

        let label = match d.severity {
            Severity::Error => {
                if use_color {
                    "error".red().bold().to_string()
                } else {
                    "error".to_string()
                }
            }
            Severity::Warning => {
                if use_color {
                    "warning".yellow().bold().to_string()
                } else {
                    "warning".to_string()
                }
            }
        };

        let msg_styled = if use_color {
            msg.bold().to_string()
        } else {
            msg.clone()
        };

        match d.code {
            Some(c) => {
                let code_styled = if use_color {
                    format!("[{c}]").bold().to_string()
                } else {
                    format!("[{c}]")
                };
                let code_colored = match d.severity {
                    Severity::Error => {
                        if use_color {
                            code_styled.red().to_string()
                        } else {
                            code_styled
                        }
                    }
                    Severity::Warning => {
                        if use_color {
                            code_styled.yellow().to_string()
                        } else {
                            code_styled
                        }
                    }
                };
                out.push_str(&format!("{label}{code_colored}: {msg_styled}\n"));
            }
            None => out.push_str(&format!("{label}: {msg_styled}\n")),
        }

        let arrow = if use_color {
            "-->".bright_blue().bold().to_string()
        } else {
            "-->".to_string()
        };
        out.push_str(&format!("  {arrow} {path}:{line_no}:{col}\n"));

        let pipe = if use_color {
            "|".bright_blue().bold().to_string()
        } else {
            "|".to_string()
        };
        out.push_str(&format!("   {pipe}\n"));

        let line_no_styled = if use_color {
            format!("{line_no:>3}").bright_blue().bold().to_string()
        } else {
            format!("{line_no:>3}")
        };
        out.push_str(&format!("{line_no_styled} {pipe} {line_text}\n"));

        // Caret underline: from span start to span end, clamped to the line.
        let in_line_start = d.span.start - line_start;
        let len = (d.span.end.saturating_sub(d.span.start)).max(1);
        let len = len.min(line_text.len().saturating_sub(in_line_start).max(1));
        let pad = line_text[..in_line_start.min(line_text.len())]
            .chars()
            .count();

        let carets = "^".repeat(len);
        let carets_styled = match d.severity {
            Severity::Error => {
                if use_color {
                    carets.red().bold().to_string()
                } else {
                    carets
                }
            }
            Severity::Warning => {
                if use_color {
                    carets.yellow().bold().to_string()
                } else {
                    carets
                }
            }
        };
        out.push_str(&format!("   {pipe} {}{}\n", " ".repeat(pad), carets_styled));

        if let Some(h) = &d.help {
            let help_label = if use_color {
                "= help:".bright_blue().bold().to_string()
            } else {
                "= help:".to_string()
            };
            out.push_str(&format!("   {help_label} {h}\n"));
        }
        out.push('\n');
    }
    out
}

/// One diagnostic in the `--json` wire format (docs/code/06): the stable
/// machine-readable contract for editors and the npm/PyPI wrappers.
/// Positions are 1-based line/column (columns count CHARS, matching the
/// human renderer); the byte span is included for exact tooling.
#[derive(serde::Serialize)]
pub struct JsonDiag {
    /// `"error"` or `"warning"` — the diagnostic's severity.
    pub severity: &'static str,
    /// Stable code (`"E0101"`/`"W0001"`), or `null` for pre-code diagnostics.
    pub code: Option<&'static str>,
    /// WHAT is wrong.
    pub message: String,
    /// HOW to fix it (the teaching line), when present.
    pub help: Option<String>,
    /// The file the span points into.
    pub path: String,
    /// 1-based line of the span start.
    pub line: usize,
    /// 1-based character column of the span start.
    pub col: usize,
    /// Byte offsets `[start, end)` into the NFC-normalized source.
    pub span: (usize, usize),
}

impl JsonDiag {
    /// Resolve a [`Diag`] against the source it points into.
    pub fn new(d: &Diag, path: &str, src: &str) -> Self {
        let (line, col, _, _) = locate(src, d.span.start);
        JsonDiag {
            severity: match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            code: d.code,
            message: d.msg.clone(),
            help: d.help.clone(),
            path: path.to_string(),
            line,
            col,
            span: (d.span.start, d.span.end),
        }
    }
}

/// (1-based line, 1-based column, line text, byte offset of line start)
pub(crate) fn locate(src: &str, offset: usize) -> (usize, usize, String, usize) {
    let offset = offset.min(src.len());
    let mut line_no = 1usize;
    let mut line_start = 0usize;
    for (i, b) in src.bytes().enumerate() {
        if i >= offset {
            break;
        }
        if b == b'\n' {
            line_no += 1;
            line_start = i + 1;
        }
    }
    let line_end = src[line_start..]
        .find('\n')
        .map(|i| line_start + i)
        .unwrap_or(src.len());
    let line_text = src[line_start..line_end].trim_end_matches('\r').to_string();
    let col = src[line_start..offset].chars().count() + 1;
    (line_no, col, line_text, line_start)
}
