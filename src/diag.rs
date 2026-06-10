//! Teaching diagnostics: every error says WHAT is wrong and, where possible,
//! HOW to fix it (spec/01 G1). Rendered with source excerpt and carets.
//! (English catalog first; Tanglish/Tamil catalogs land with Phase 1.8.)

use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Diag {
    pub span: Span,
    pub msg: String,
    pub help: Option<String>,
}

impl Diag {
    pub fn new(span: Span, msg: impl Into<String>) -> Self {
        Diag {
            span,
            msg: msg.into(),
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Render diagnostics against the (NFC-normalized) source they refer to.
pub fn render(diags: &[Diag], src: &str, path: &str) -> String {
    let mut out = String::new();
    for d in diags {
        let (line_no, col, line_text, line_start) = locate(src, d.span.start);
        out.push_str(&format!("error: {}\n", d.msg));
        out.push_str(&format!("  --> {path}:{line_no}:{col}\n"));
        out.push_str("   |\n");
        out.push_str(&format!("{line_no:>3}| {line_text}\n"));
        // Caret underline: from span start to span end, clamped to the line.
        let in_line_start = d.span.start - line_start;
        let len = (d.span.end.saturating_sub(d.span.start)).max(1);
        let len = len.min(line_text.len().saturating_sub(in_line_start).max(1));
        let pad = line_text[..in_line_start.min(line_text.len())]
            .chars()
            .count();
        out.push_str(&format!("   | {}{}\n", " ".repeat(pad), "^".repeat(len)));
        if let Some(h) = &d.help {
            out.push_str(&format!("   = help: {h}\n"));
        }
        out.push('\n');
    }
    out
}

/// (1-based line, 1-based column, line text, byte offset of line start)
fn locate(src: &str, offset: usize) -> (usize, usize, String, usize) {
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
