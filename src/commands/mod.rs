//! The per-command handlers behind the `mimz` subcommands. `main()` parses the
//! CLI and dispatches into one function per command here; the shared `Output`
//! renderer and the `Cli`/`Cmd` clap types stay in `main.rs`. Split out of
//! `main.rs` verbatim — no logic changed, only relocation + visibility glue.

mod check;
mod compile;
mod eject;
mod eval;
mod explain;
mod fmt;
mod helpers;
mod sim;
mod test;
mod translate;

pub(crate) use check::check;
pub(crate) use compile::compile;
pub(crate) use eject::{EjectFlavor, eject_std};
pub(crate) use eval::eval_file;
pub(crate) use explain::explain_code;
pub(crate) use fmt::fmt_file;
pub(crate) use helpers::resolve_config;
pub(crate) use sim::sim_file;
pub(crate) use test::test_file;
pub(crate) use translate::translate_file;
