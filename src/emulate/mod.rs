//! Native-only peripheral registry for `sim` blocks (`mimz test --emulate`,
//! docs/superpowers/specs/2026-07-07-hw-emulation-led-design.local.md,
//! docs/superpowers/specs/2026-07-08-hw-emulation-uart-design.local.md).
//! Behind the `hw-emulation` Cargo feature — never compiled for wasm32.

pub(crate) mod dashboard;
pub mod host;
mod led;
mod speaker;
mod uart_rx;
mod uart_tx;

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use mimz_sim::sim::Direction;
use mimz_sim::sim::Val;
use mimz_sim::sim::elaborate::Width;

use crate::ast::{BindArg, BindArgValue};

/// Parses a `port: N` bind-config value (`uart_rx`/`uart_tx`'s socket
/// transport) into a valid TCP port number.
pub(super) fn parse_port(value: &BindArgValue, peripheral: &str) -> Result<u16, String> {
    match value {
        BindArgValue::Int(v) if *v > 0 && *v <= u16::MAX as u128 => Ok(*v as u16),
        BindArgValue::Int(v) => Err(format!(
            "`{peripheral}`'s `port` must be between 1 and 65535, got {v}"
        )),
        _ => Err(format!(
            "`{peripheral}`'s `port` must be a number, e.g. `port: 8080`"
        )),
    }
}

/// One bound virtual peripheral. Constructed once per `bind`, then driven
/// (`drive`) and/or observed (`on_tick`, `on_change`) once per cycle or
/// batch — see the design docs' Execution model sections. Object-safe:
/// `render` is the only widget the dashboard needs to draw.
pub trait Peripheral: Send {
    /// Called once per batched frame (~30fps) when the bound port's value
    /// changed. Coarse — fine for a visual indicator (`led`), too coarse
    /// for bit-exact serial decode.
    fn on_change(&mut self, val: &Val);
    /// Called after every individual simulated cycle (not just at
    /// batch-end), with the bound **output** port's current value.
    /// Default no-op — only peripherals needing bit-exact timing
    /// (`uart_tx`) or real hardware that can fail to open (`speaker`)
    /// override this. Wired by the harness's `notify_on_tick`.
    fn on_tick(&mut self, _val: &Val) -> Result<(), String> {
        Ok(())
    }
    /// Called before every individual simulated cycle, for peripherals
    /// bound to an **input** port. Returning `Some(bit)` drives that value
    /// onto the port before the cycle's tick; `None` leaves it unchanged.
    /// Default: drives nothing (only `uart_rx` overrides this). Wired by
    /// the harness's `drive_peripherals`.
    fn drive(&mut self) -> Option<u64> {
        None
    }
    /// Called once after the test body finishes ticking (pass, fail, or
    /// skip alike) — for finalizing work that can't happen incrementally,
    /// like `speaker`'s offline-rendered playback. Default no-op; only
    /// `speaker` overrides it. Wired at the end of `run_test`.
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// Draw this peripheral's row in the dashboard.
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// Validates `args`/`width` and constructs the peripheral, or returns a
/// teaching-quality error message (same tier as a bad `expect`).
/// `speed_hz` is the sim block's declared real-world clock rate, if any —
/// `uart_tx`/`uart_rx` need it to derive `cycles_per_bit` from `baud`;
/// `led` ignores it.
pub type Constructor =
    fn(Width, &[BindArg], Option<u64>) -> Result<Box<dyn Peripheral>, String>;

/// A registered peripheral: which port kind it expects (`Direction` comes
/// from `mimz_sim::sim`, shared with the harness's bind validation), plus
/// how to build one.
pub struct Entry {
    pub direction: Direction,
    pub construct: Constructor,
}

/// Every known peripheral name.
pub fn registry() -> HashMap<&'static str, Entry> {
    let mut m: HashMap<&'static str, Entry> = HashMap::new();
    m.insert(
        "led",
        Entry {
            direction: Direction::Output,
            construct: led::construct,
        },
    );
    m.insert(
        "speaker",
        Entry {
            direction: Direction::Output,
            construct: speaker::construct,
        },
    );
    m.insert(
        "uart_tx",
        Entry {
            direction: Direction::Output,
            construct: uart_tx::construct,
        },
    );
    m.insert(
        "uart_rx",
        Entry {
            direction: Direction::Input,
            construct: uart_rx::construct,
        },
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_is_registered_as_output() {
        assert!(registry().get("speaker").unwrap().direction == Direction::Output);
    }

    #[test]
    fn totally_unknown_peripheral_name_is_not_registered() {
        assert!(!registry().contains_key("microphone"));
    }

    #[test]
    fn led_is_registered_as_output() {
        assert!(registry().get("led").unwrap().direction == Direction::Output);
    }

    #[test]
    fn uart_tx_is_registered_as_output() {
        assert!(registry().get("uart_tx").unwrap().direction == Direction::Output);
    }

    #[test]
    fn uart_rx_is_registered_as_input() {
        assert!(registry().get("uart_rx").unwrap().direction == Direction::Input);
    }
}
