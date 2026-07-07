//! Native-only peripheral registry for `sim` blocks (`mimz test --emulate`,
//! docs/superpowers/specs/2026-07-07-hw-emulation-led-design.local.md).
//! Behind the `hw-emulation` Cargo feature — never compiled for wasm32.

// This skeleton isn't wired into the harness/dashboard yet (Task 5) — drop
// this allow once `sim::harness` calls `registry()`.
#![allow(dead_code)]

pub(crate) mod dashboard;
mod led;

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::elaborate::Width;
use super::value::Val;
use crate::ast::BindArg;

/// One bound virtual peripheral. Constructed once per `bind`, then fed
/// value changes once per batched frame (`led`) — see the design doc's
/// Execution model. Object-safe: `render` is the only widget the
/// dashboard needs to draw.
pub(super) trait Peripheral: Send {
    /// Called when the bound port's value changed this batch.
    fn on_change(&mut self, val: &Val);
    /// Draw this peripheral's row in the dashboard.
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// Validates `args` against `port_width` and constructs the peripheral, or
/// returns a teaching-quality error message (same tier as a bad `expect`).
pub(super) type Constructor = fn(Width, &[BindArg]) -> Result<Box<dyn Peripheral>, String>;

/// Every known peripheral name. `uart`/`speaker` are added here by later
/// specs — the dashboard/batching code never changes to accommodate them.
pub(super) fn registry() -> HashMap<&'static str, Constructor> {
    let mut m: HashMap<&'static str, Constructor> = HashMap::new();
    m.insert("led", led::construct as Constructor);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_peripheral_name_is_not_registered() {
        assert!(!registry().contains_key("speaker"));
        assert!(!registry().contains_key("uart"));
    }

    #[test]
    fn led_is_registered() {
        assert!(registry().contains_key("led"));
    }
}
