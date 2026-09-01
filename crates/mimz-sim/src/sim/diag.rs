//! Thin re-export of `mimz_core::diag`'s `mimz-sim` runtime-code catalog and
//! bridge helpers — moved into `mimz-core` (Phase 2 IR Task 1) alongside
//! `value`/`comb` so those evaluators can construct properly-coded
//! diagnostics across the `Resolver` string boundary without `mimz-core`
//! depending on `mimz-sim`. Kept as a `sim::diag` alias so every existing
//! `crate::sim::diag::...`/`super::diag::...` call site in this crate keeps
//! compiling unchanged.

pub use mimz_core::diag::{ALL_SIM_CODES, bridge_code, diag_from_bridged};
