//! Byte-offset source spans. Every token and AST node carries one —
//! error quality is a core goal (spec/01 G1), not a feature.

/// A half-open byte range `start..end` into the NFC-normalized source text.
/// Byte offsets (not char offsets) — the diagnostic renderer converts to
/// line/column for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    /// Byte offset of the first byte (inclusive).
    pub start: usize,
    /// Byte offset one past the last byte (exclusive).
    pub end: usize,
}

impl Span {
    /// Builds a span from an explicit `start..end` byte range.
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    /// Smallest span covering both.
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// This span's start, as a 1-based `(line, column)` pair into `src` —
    /// a lightweight, self-contained conversion for callers that need a
    /// human-readable location without going through the full diagnostic
    /// renderer (e.g. a coverage summary line). Counts bytes, not chars,
    /// matching this type's own byte-offset contract; multi-byte UTF-8
    /// sequences never contain `\n`, so this is exact for line-counting
    /// and column-counts bytes within the line (fine for this project's
    /// ASCII-heavy source — a non-ASCII identifier's column is approximate,
    /// same caveat every byte-offset-based tool in this codebase already
    /// carries).
    pub fn line_col(&self, src: &str) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for &b in src.as_bytes().iter().take(self.start) {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_finds_the_first_line() {
        let src = "abc\ndef\n";
        let span = Span::new(1, 2); // the 'b' in "abc"
        assert_eq!(span.line_col(src), (1, 2));
    }

    #[test]
    fn line_col_finds_a_later_line() {
        let src = "abc\ndef\n";
        let span = Span::new(5, 6); // the 'e' in "def"
        assert_eq!(span.line_col(src), (2, 2));
    }
}
