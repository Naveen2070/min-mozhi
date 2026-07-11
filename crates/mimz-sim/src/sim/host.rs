//! The seam that inverts today's coupling between the simulator harness and
//! concrete `ratatui`/`cpal` hardware-emulation peripherals
//! (`src/sim/emulate/`). After the full workspace split, `mimz-sim`'s
//! harness talks to peripherals only through [`EmulationHost`] — it has
//! zero knowledge of ratatui/cpal. The shell crate (`mimz::emulate`, Task 7)
//! implements this trait; the harness (Task 4) is rewritten to call it.

use super::elaborate::Width;
use super::value::Val;

/// A `sim{}` block bind argument, e.g. `color: "red"` or `baud: 9600`.
pub use mimz_core::ast::BindArg;

/// Whether a bound peripheral drives values into the simulation (`Input`,
/// e.g. `uart_rx`) or is driven by it (`Output`, e.g. `led`/`speaker`/`uart_tx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The peripheral drives values into the simulation (e.g. `uart_rx`).
    Input,
    /// The simulation drives the peripheral (e.g. `led`/`speaker`/`uart_tx`).
    Output,
}

/// The simulator harness's only view of hardware-emulation peripherals.
/// Implemented by the shell crate (`mimz::emulate`), which owns ratatui/cpal.
/// No type in this trait's signature may come from ratatui or cpal.
pub trait EmulationHost {
    /// Validate + construct a peripheral for a `sim{}` block bind. `port` is
    /// the signal being bound (e.g. `rx`); `peripheral` is the peripheral
    /// type name (e.g. `uart_rx`) — implementations MUST key their internal
    /// peripheral storage by `port`, since every later call (`on_change`,
    /// `on_tick`, `drive`) identifies the peripheral by its bound port, not
    /// its type name (a design/port pair like `bind rx -> uart_rx(...)` has
    /// two different names for the same instance; storing by the wrong one
    /// makes every later dispatch silently find nothing).
    ///
    /// Errors are the same teaching-quality strings the old
    /// `emulate::registry` constructors returned (e.g. "unknown peripheral
    /// 'foo'", direction mismatch messages) — preserve their exact text so
    /// existing harness tests (`sim_block_with_unknown_peripheral_errors`,
    /// etc.) still pass.
    fn bind(
        &mut self,
        port: &str,
        peripheral: &str,
        width: Width,
        args: &[BindArg],
        speed_hz: Option<u64>,
    ) -> Result<(), String>;

    /// Direction for a peripheral name; `None` = unknown name.
    fn direction_of(&self, name: &str) -> Option<Direction>;

    /// Called on every value change for a bound port.
    fn on_change(&mut self, name: &str, val: &Val);

    /// Called once per simulated cycle (drives `uart_tx`/`speaker` playback).
    fn on_tick(&mut self, name: &str, val: &Val) -> Result<(), String>;

    /// Called for input peripherals (e.g. `uart_rx`) to pull a driven value.
    fn drive(&mut self, name: &str) -> Option<u64>;

    /// Dashboard redraw, batched to ~30fps. No-op if `live` is false. `cycle`
    /// is the real simulated cycle count reached so far (not a frame
    /// count) — a free-running batch may cover many cycles per call, so
    /// this is the only way a host can show the true cycle number rather
    /// than its own count of `frame()` calls. Returns `true` if the user
    /// requested quit (e.g. pressed `q` while paused mid-`--step`) — the
    /// harness aborts the test when it sees this. A non-interactive/headless
    /// host always returns `Ok(false)`.
    fn frame(&mut self, cycle: u64) -> Result<bool, String>;

    /// End-of-test cleanup (e.g. flush speaker playback), plus — for a live
    /// host — the final "press Enter to continue, q to quit" dismiss screen.
    /// Returns `true` if the user quit at that dismiss prompt. A
    /// headless host does its cleanup and returns `Ok(false)`.
    fn finish(&mut self) -> Result<bool, String>;
}
