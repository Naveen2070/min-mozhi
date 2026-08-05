//! AST → Min-Mozhi source pretty-printer — the engine behind
//! `mimz translate --order code|thamizh`.
//!
//! Unlike [`crate::translate`] (which re-spells keyword TOKENS and is
//! trivia-preserving), this emits **from the AST**, so it can reorder clause
//! heads between the two word-order profiles (spec/04 section 3). The AST
//! carries no comments and no original layout, so the output is **canonically
//! formatted and drops comments** — it is NOT byte-identical to the input. The
//! correctness contract is semantic: the output compiles to byte-identical
//! Verilog and re-parses to the same AST (`tests/translate.rs`).
//!
//! Keyword spellings come from the same [`TABLE`] the lexer/translate use, so
//! flavor (english/tanglish/tamil) and order (code/thamizh) compose freely.
//!
//! Indentation: most expressions are single-line, but `match` is block-shaped
//! (one arm per line). Expression emitters therefore take an `indent` (the
//! column level of any block they open) so a `match` — even nested in an
//! assignment RHS — lays its arms out correctly.

use crate::ast::*;
use crate::lexer::keywords::TABLE;
use crate::lexer::token::{Flavor, Kw};

/// Which word order to emit. Public mirror of the parser's internal `Profile`
/// (which is `pub(crate)`), so the CLI can request an order without depending
/// on parser internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// English-derived order: `on rise(clk)`, `if c { }`, `match e { }`.
    Code,
    /// SOV/postpositional: `rise(clk) on`, `c if { }`, `e match { }`. Emits a
    /// leading `syntax thamizh` directive so the result re-parses.
    Thamizh,
}

/// Pretty-print a parsed file as Min-Mozhi source in the given keyword `flavor`
/// and word `order`.
pub fn pretty_print(file: &File, flavor: Flavor, order: Order) -> String {
    let mut p = Pretty {
        out: String::new(),
        indent: 0,
        flavor,
        order,
    };
    p.file(file);
    p.out
}

/// Canonical English string for `ty` — used by the names checker's E0808
/// type comparison (Phase 4 of OR-arm binding intersection).
pub(crate) fn type_str(ty: &Type) -> String {
    let p = Pretty {
        out: String::new(),
        indent: 0,
        flavor: crate::lexer::token::Flavor::English,
        order: Order::Code,
    };
    p.ty(ty, 0)
}

/// Canonical English string for `e` — the `Expr`-level counterpart of
/// [`type_str`], for width checker diagnostics that need to render a
/// `NamedArg`'s value (e.g. a valid-bundle's `N` width argument) back to
/// source text.
pub(crate) fn expr_str(e: &Expr) -> String {
    let p = Pretty {
        out: String::new(),
        indent: 0,
        flavor: crate::lexer::token::Flavor::English,
        order: Order::Code,
    };
    p.expr(e, 0)
}

struct Pretty {
    out: String,
    indent: usize,
    flavor: Flavor,
    order: Order,
}

/// Indentation prefix for a given level (2 spaces per level).
fn pad(level: usize) -> String {
    "  ".repeat(level)
}

mod exprs;
mod items;
mod seq;

impl Pretty {
    /// A keyword's spelling in the target flavor.
    fn kw(&self, kw: Kw) -> &'static str {
        TABLE.canonical(kw, self.flavor)
    }

    /// Push a full line at the current indent.
    fn line(&mut self, s: &str) {
        self.out.push_str(&pad(self.indent));
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn blank(&mut self) {
        self.out.push('\n');
    }

    /// Render a string LITERAL VALUE (already lexed from a `.mimz` `TokKind
    /// ::Str` token — `assert`/`cover`'s message/label, `test`'s name, a
    /// `sim { bind }` string arg, `extern module`'s alias/`doc:`) back into
    /// source text. Wraps in plain double quotes — NOT `format!("{s:?}")`
    /// (Rust's `Debug` escaping) — because the lexer's own string grammar
    /// (`lexer/mod.rs`'s `string()`) has no escape-sequence support at all:
    /// it reads raw characters verbatim until an unescaped `"`, so a
    /// successfully-lexed `Str` token is guaranteed (by construction) to
    /// never contain `"` or `\n`, and `\`-escaping it on the way back out
    /// is never necessary — the value round-trips exactly as plain text.
    /// Doing this the `{:?}` way instead is what a real fuzz crash found:
    /// a control byte gets escaped to `\u{4}` on the way out, then the
    /// lexer (which doesn't decode `\u{...}`) reads those 6 characters back
    /// literally on the way in, silently changing the string's content.
    fn quote(&self, s: &str) -> String {
        format!("\"{s}\"")
    }

    // ---------- types / lvalues ----------
}

#[cfg(test)]
mod tests;
