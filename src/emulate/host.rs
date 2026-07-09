//! The shell crate's `EmulationHost` — the concrete side of the seam
//! `mimz-sim` defines abstractly (`mimz_sim::sim::EmulationHost`). This is
//! the only place ratatui/cpal-backed peripherals meet the simulator: it
//! owns the peripheral registry, the bound peripheral instances, and (when
//! `live`) the `ratatui` [`Dashboard`].
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
    /// Frames drawn so far — the dashboard title's cycle counter.
    ///
    /// ponytail: this counts `frame()` calls, which equals the sim cycle in
    /// `--step` mode (batch size 1) but not in free-running batched mode,
    /// where `frame()` fires once per ~30fps batch of many cycles. The trait
    /// carries no cycle today; thread the real cycle through `frame(cycle)`
    /// if the exact free-run number ever matters.
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
        name: &str,
        width: Width,
        args: &[mimz_core::ast::BindArg],
        speed_hz: Option<u64>,
    ) -> Result<(), String> {
        let entry = self
            .registry
            .get(name)
            .ok_or_else(|| format!("unknown peripheral `{name}`"))?;
        let peripheral = (entry.construct)(width, args, speed_hz)?;
        self.peripherals.push((name.to_string(), peripheral));
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

    fn frame(&mut self) -> Result<bool, String> {
        if !self.live {
            return Ok(false);
        }
        if self.dashboard.is_none() {
            self.dashboard = Some(
                Dashboard::open()
                    .map_err(|e| format!("could not open the emulation dashboard: {e}"))?,
            );
        }
        self.cycle += 1;
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
