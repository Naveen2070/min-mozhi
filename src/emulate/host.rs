//! The shell crate's `EmulationHost` — the concrete side of the seam
//! `mimz-sim` defines abstractly (`mimz_sim::sim::EmulationHost`). This is
//! the only place ratatui/cpal-backed peripherals meet the simulator: it
//! owns the peripheral registry, the bound peripheral instances, and (when
//! `live`) the `ratatui` `Dashboard` (crate-private — no public link target).
//!
//! Constructed unconditionally by `commands/test.rs` (Task 8) on every
//! `mimz test` run — bind validation must fire even without `--emulate`, so
//! a headless run passes `live: false` and this host quietly no-ops every
//! draw/pause.

use std::collections::HashMap;

use mimz_sim::sim::elaborate::Width;
use mimz_sim::sim::{Direction, EmulationHost, Val};

use super::dashboard::Dashboard;
use super::{Entry, Peripheral, registry};

/// Shell-side `EmulationHost`: registry + bound peripherals + optional live
/// dashboard, scoped to one `test` block.
pub struct EmulateHost {
    registry: HashMap<&'static str, Entry>,
    peripherals: Vec<(String, Box<dyn Peripheral>)>,
    dashboard: Option<Dashboard>,
    /// Whether ticks pace/redraw in real time (caller decided from
    /// `--emulate` + a real terminal). `false` = headless: every draw/pause
    /// below short-circuits.
    live: bool,
    /// `--step` AND `live` — pause after each single-cycle frame for a
    /// keypress. Drives the dashboard `hint` and the `wait_for_step` pause.
    stepping: bool,
    /// The quoted test name, for the dashboard title / dismiss screen.
    test_name: String,
    /// The real simulated cycle count as of the last `frame()` call — the
    /// dashboard title's cycle counter. Set from `frame`'s `cycle` argument,
    /// so it's accurate in free-running batched mode too (where `frame()`
    /// fires once per ~30fps batch of many cycles, not once per cycle).
    cycle: u64,
    /// Set once the user pressed `q` at a `--step` pause, so `finish` skips
    /// the redundant final dismiss screen.
    quit: bool,
}

impl EmulateHost {
    /// `test_name` titles the dashboard; `live` gates all real-time
    /// behavior; `step` (only meaningful when `live`) forces single-cycle
    /// pauses.
    pub fn new(test_name: String, live: bool, step: bool) -> Self {
        Self {
            registry: registry(),
            peripherals: Vec::new(),
            dashboard: None,
            live,
            stepping: step && live,
            test_name,
            cycle: 0,
            quit: false,
        }
    }
}

impl EmulationHost for EmulateHost {
    fn bind(
        &mut self,
        port: &str,
        peripheral: &str,
        width: Width,
        args: &[mimz_core::ast::BindArg],
        speed_hz: Option<u64>,
    ) -> Result<(), String> {
        let entry = self
            .registry
            .get(peripheral)
            .ok_or_else(|| format!("unknown peripheral `{peripheral}`"))?;
        let instance = (entry.construct)(width, args, speed_hz)?;
        self.peripherals.push((port.to_string(), instance));
        Ok(())
    }

    fn direction_of(&self, name: &str) -> Option<Direction> {
        self.registry.get(name).map(|e| e.direction)
    }

    fn on_change(&mut self, name: &str, val: &Val) {
        if let Some((_, p)) = self.peripherals.iter_mut().find(|(n, _)| n == name) {
            p.on_change(val);
        }
    }

    fn on_tick(&mut self, name: &str, val: &Val) -> Result<(), String> {
        if let Some((_, p)) = self.peripherals.iter_mut().find(|(n, _)| n == name) {
            p.on_tick(val)?;
        }
        Ok(())
    }

    fn drive(&mut self, name: &str) -> Option<u64> {
        self.peripherals
            .iter_mut()
            .find(|(n, _)| n == name)
            .and_then(|(_, p)| p.drive())
    }

    fn frame(&mut self, cycle: u64) -> Result<bool, String> {
        if !self.live {
            return Ok(false);
        }
        if self.dashboard.is_none() {
            self.dashboard = Some(
                Dashboard::open()
                    .map_err(|e| format!("could not open the emulation dashboard: {e}"))?,
            );
        }
        self.cycle = cycle;
        let hint = self.stepping.then_some("step: Enter to advance, q to quit");
        let dashboard = self.dashboard.as_mut().expect("just opened above");
        dashboard
            .draw(&self.test_name, self.cycle, &self.peripherals, hint)
            .map_err(|e| format!("dashboard draw failed: {e}"))?;
        if self.stepping {
            let quit = dashboard
                .wait_for_step()
                .map_err(|e| format!("dashboard input failed: {e}"))?;
            self.quit |= quit;
            return Ok(quit);
        }
        Ok(false)
    }

    fn finish(&mut self) -> Result<bool, String> {
        // Flush deferred work (speaker's offline-rendered playback) first —
        // always, even on a quit, so audio isn't left half-written.
        for (_, p) in &mut self.peripherals {
            p.finish()?;
        }
        // A dashboard only ever opened for a live run — hold it on the final
        // frame until the user dismisses (Enter) or quits (q), unless they
        // already quit mid-step.
        if !self.quit {
            if let Some(dashboard) = &mut self.dashboard {
                return dashboard
                    .wait_for_dismiss(&self.test_name, self.cycle)
                    .map_err(|e| format!("dashboard input failed: {e}"));
            }
        }
        Ok(self.quit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimz_core::ast::{BindArg, BindArgValue, Ident};
    use mimz_core::span::Span;

    fn arg(name: &str, value: BindArgValue) -> BindArg {
        BindArg {
            name: Ident {
                name: name.to_string(),
                span: Span::default(),
            },
            value,
            span: Span::default(),
        }
    }

    /// `bind rx -> uart_rx(...)` gives the peripheral instance a different
    /// name (`rx`, the port) than the registry key used to construct it
    /// (`uart_rx`, the peripheral type) — but every later call
    /// (`on_change`/`on_tick`/`drive`) identifies it by the PORT name only.
    /// Storage keyed by peripheral name instead of port name means those
    /// calls silently find nothing whenever port != peripheral name (this
    /// is exactly the `showcase/english/uart_echo.mimz` failure: `rx` never
    /// gets driven, stays 0 forever).
    #[test]
    fn drive_dispatches_by_port_name_not_peripheral_name() {
        let mut host = EmulateHost::new("t".to_string(), false, false);
        let width = Width {
            bits: 1,
            signed: false,
        };
        let args = [
            arg("baud", BindArgValue::Int(1)),
            arg("source", BindArgValue::Str("Z".to_string())),
        ];
        host.bind("custom_rx_port", "uart_rx", width, &args, Some(4))
            .expect("binds");

        // uart_rx's drive() always returns Some(_) (idle-high or a framed
        // bit) — it never legitimately returns None — so if every call
        // returns None, the peripheral was never actually reached.
        let saw_a_value = (0..8).any(|_| host.drive("custom_rx_port").is_some());
        assert!(
            saw_a_value,
            "drive() by port name returned None every time — peripheral \
             storage must be keyed by port, not peripheral, name"
        );

        // Guard against silently reverting to peripheral-name keying.
        assert!(
            host.drive("uart_rx").is_none(),
            "peripheral-name key should not resolve anything — port name \
             is the only valid key"
        );
    }
}
