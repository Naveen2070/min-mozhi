//! The seam that inverts today's coupling between the simulator harness and
//! concrete `ratatui`/`cpal` hardware-emulation peripherals
//! (`src/sim/emulate/`). After the full workspace split, `mimz-sim`'s
//! harness talks to peripherals only through [`EmulationHost`] — it has
//! zero knowledge of ratatui/cpal. The shell crate (`mimz::emulate`, Task 7)
//! implements this trait; the harness (Task 4) is rewritten to call it.

// TODO(task-4): `elaborate.rs`/`value.rs` haven't moved into `mimz-sim` yet
// (that's Task 4), so `Width`/`Val` don't exist here. Restore these once
// they land, and un-comment the `width`/`val` parameter types below.
// use super::elaborate::Width;
// use super::value::Val;

/// A `sim{}` block bind argument, e.g. `color: "red"` or `baud: 9600`.
pub use mimz_core::ast::BindArg;

/// Whether a bound peripheral drives values into the simulation (`Input`,
/// e.g. `uart_rx`) or is driven by it (`Output`, e.g. `led`/`speaker`/`uart_tx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

/// The simulator harness's only view of hardware-emulation peripherals.
/// Implemented by the shell crate (`mimz::emulate`), which owns ratatui/cpal.
/// No type in this trait's signature may come from ratatui or cpal.
pub trait EmulationHost {
    /// Validate + construct a peripheral for a `sim{}` block bind. Errors
    /// are the same teaching-quality strings the old `emulate::registry`
    /// constructors returned (e.g. "unknown peripheral 'foo'", direction
    /// mismatch messages) — preserve their exact text so existing harness
    /// tests (`sim_block_with_unknown_peripheral_errors`, etc.) still pass.
    fn bind(
        &mut self,
        name: &str,
        // TODO(task-4): restore `width: Width` once `elaborate::Width`
        // moves into mimz-sim.
        args: &[BindArg],
        speed_hz: Option<u64>,
    ) -> Result<(), String>;

    /// Direction for a peripheral name; `None` = unknown name.
    fn direction_of(&self, name: &str) -> Option<Direction>;

    /// Called on every value change for a bound port.
    // TODO(task-4): restore `val: &super::value::Val` once `value::Val`
    // moves into mimz-sim.
    fn on_change(&mut self, name: &str);

    /// Called once per simulated cycle (drives `uart_tx`/`speaker` playback).
    // TODO(task-4): restore `val: &super::value::Val` once `value::Val`
    // moves into mimz-sim.
    fn on_tick(&mut self, name: &str) -> Result<(), String>;

    /// Called for input peripherals (e.g. `uart_rx`) to pull a driven value.
    fn drive(&mut self, name: &str) -> Option<u64>;

    /// Dashboard redraw, batched to ~30fps. No-op if `live` is false.
    fn frame(&mut self) -> Result<(), String>;

    /// End-of-test cleanup (e.g. flush speaker playback).
    fn finish(&mut self) -> Result<(), String>;
}
