//! The two compiler-synthesized valid-bundle declarations backing `T?`
//! sugar (design: `docs/superpowers/specs/2026-07-16-valid-bundle-sugar-design.local.md`).
//! Never present in any `.mimz` source; `?`-suffixed types desugar directly
//! to a reference to one of these two names (Task 3's parser type-suffix
//! handling). Lives here — rather than duplicated in both `checker::symbols`
//! and `emit_verilog` — because it's the one place both already share: the
//! checker's `Checker::bundles` and the emitter's `Project::bundles` are
//! both `HashMap<String, Vec<(usize, &'a ast::BundleDecl)>>` over the same
//! `ast::BundleDecl`, and `ast` is `pub` from both call sites, so a single
//! function here backs both tables without needing `checker::symbols` to be
//! made visible to `emit_verilog` (it currently isn't — everything under
//! `checker::` besides the `check` entry point and a couple of test-only
//! items is private to the crate's checker module).
//!
//! **File-index note for callers:** each returned `BundleDecl` is registered
//! under a synthetic file index — pick one strictly greater than every real
//! loaded file's index (i.e. `files.len()`), NOT `usize::MAX`. The checker's
//! `resolve_bundle_fields` (`checker/widths/mod.rs`) does a plain
//! `self.file_consts[bfile]` array index on the bundle's declaring-file
//! index with no bounds check — `usize::MAX` there panics. `files.len()`
//! stays a valid index as long as `Checker::file_consts` is sized
//! `files.len() + 1` (one extra, always-empty slot for this exact case —
//! see `checker::Checker::new`). The emitter's `Project` has no such
//! per-file array, so it doesn't strictly need this, but both call sites use
//! `files.len()` for one consistent convention.

use super::{BundleDecl, Expr, ExprKind, FieldDecl, Ident, Param, ParamTy, Type};
use crate::span::Span;
use std::cell::OnceCell;

thread_local! {
    // Per-thread, not a single global `OnceLock`: `BundleDecl` embeds `Type`,
    // which has a `Named(QualIdent)` variant carrying `Cell<Option<usize>>`
    // (the checker's name-resolution cache) — that `Cell` makes `BundleDecl`
    // (and so `&'static BundleDecl`) `!Sync`/`!Send`, so a shared `static`
    // won't compile, and forcing it with `unsafe impl Sync` would be a real
    // data race across the LSP's multi-threaded tokio runtime if two threads
    // ever annotated the same leaked node concurrently. A `thread_local`
    // sidesteps both: each thread memoizes (and leaks) its own copy once,
    // instead of leaking two fresh `BundleDecl`s on every call.
    static BUILTIN_VALID_BUNDLES: OnceCell<[&'static BundleDecl; 2]> = const { OnceCell::new() };
}

/// Builds `__Valid(N: int = 1) { valid: bit, data: bits[N] }` and
/// `__ValidSigned(N: int = 1) { valid: bit, data: signed[N] }`, leaked once
/// per calling thread to `'static` so a caller can put them in a `&'a
/// BundleDecl` table for any `'a` (they never borrow from a real loaded
/// file). Memoized (see `BUILTIN_VALID_BUNDLES` above) — the one-shot CLI
/// only ever calls this once anyway, but the LSP re-checks on every
/// keystroke and the WASM playground recompiles per run, so without
/// memoization every such call would leak two more `BundleDecl`s (and their
/// heap-allocated `String`s/`Vec`s) forever.
pub fn builtin_valid_bundles() -> [&'static BundleDecl; 2] {
    BUILTIN_VALID_BUNDLES.with(|cell| {
        *cell.get_or_init(|| {
            let synth_span = Span::new(0, 0); // synthetic node — never shown to the user

            let ident = |s: &str| Ident {
                name: s.to_string(),
                span: synth_span,
            };
            let n_param = || Param {
                name: ident("N"),
                ty: ParamTy::Int,
                default: Some(Expr {
                    kind: ExprKind::Int {
                        value: 1,
                        raw: "1".to_string(),
                    },
                    span: synth_span,
                }),
            };
            let n_ident_expr = || Expr {
                kind: ExprKind::Ident("N".to_string()),
                span: synth_span,
            };
            let valid_field = || FieldDecl {
                name: ident("valid"),
                ty: Type::Bit,
                span: synth_span,
            };

            let unsigned = Box::leak(Box::new(BundleDecl {
                name: ident("__Valid"),
                params: vec![n_param()],
                fields: vec![
                    valid_field(),
                    FieldDecl {
                        name: ident("data"),
                        ty: Type::Bits(Box::new(n_ident_expr())),
                        span: synth_span,
                    },
                ],
                span: synth_span,
            }));
            let signed = Box::leak(Box::new(BundleDecl {
                name: ident("__ValidSigned"),
                params: vec![n_param()],
                fields: vec![
                    valid_field(),
                    FieldDecl {
                        name: ident("data"),
                        ty: Type::Signed(Box::new(n_ident_expr())),
                        span: synth_span,
                    },
                ],
                span: synth_span,
            }));
            [unsigned, signed]
        })
    })
}
