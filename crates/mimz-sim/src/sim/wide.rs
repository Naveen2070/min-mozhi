//! Thin re-export of `mimz_core::wide` — the limb-arithmetic
//! implementation moved into `mimz-core` (BUG-13 layer 2) so the
//! lexer/AST/checker can share it too. Kept as a `sim::wide` alias so
//! every existing `crate::sim::wide::...`/`super::wide::...` call site in
//! this crate keeps compiling unchanged.

pub use mimz_core::wide::*;
