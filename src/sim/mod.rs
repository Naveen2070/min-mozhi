//! The simulator (Phase 1.5) — today only its combinational SLICE exists.
//!
//! [`comb`] evaluates a clockless module's outputs from a set of input values:
//! no clock, no registers, no instances, no event kernel. The full
//! event-driven engine, VCD output, and `test` execution are Phase 1.5 proper
//! (docs/plan/phase-1.5-simulator.md).
//!
//! This slice exists now as a deliberate down-payment: it is the engine the
//! 8.5 hardware REPL and the WASM playground ride on, so it lives in the lib
//! and stays callable on a single module / single expression. `mimz eval` is
//! its (experimental) CLI surface.
//!
//! The full engine is being built in steps (Phase 1.5): `value` holds the
//! shared 2-state value model + expression evaluator (a `Resolver` trait both
//! evaluators implement); [`elaborate`] flattens an AST module into a
//! [`elaborate::Design`] (signals, registers, combinational drivers, sequential
//! processes) with widths and reset values folded; [`kernel`] interprets a
//! `Design` over clock cycles with an event-driven two-phase commit and emits
//! the per-cycle snapshot the VCD writer and console tracer will consume.

mod value;

pub mod comb;
pub mod elaborate;
pub mod kernel;
pub mod run;
pub mod trace;
pub mod vcd;
