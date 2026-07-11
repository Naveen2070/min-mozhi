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
}
