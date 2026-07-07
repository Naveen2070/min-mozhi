//! One `ratatui` terminal session per `test` block with an active `sim`
//! block (docs/superpowers/specs/2026-07-07-hw-emulation-led-design.local.md
//! — "Dashboard scope: one dashboard per test").
//!
//! Every API here was checked against the vendored source for the versions
//! this workspace actually resolves (`ratatui` 0.29.0, `crossterm` 0.28.1)
//! rather than trusted from memory: `Frame::area` (not the older `size`) at
//! `ratatui-0.29.0/src/terminal/frame.rs:59`, `Frame::buffer_mut` at
//! `frame.rs:226`, `Layout::vertical`/`horizontal` as associated fns at
//! `layout/layout.rs:247,267`, `Constraint::Length`/`Fill` at
//! `layout/constraint.rs:116,194`, `Terminal::draw` at
//! `terminal/terminal.rs:304`, and `enable_raw_mode`/`disable_raw_mode`/
//! `EnterAlternateScreen`/`LeaveAlternateScreen` at `crossterm-0.28.1/src/terminal.rs`.

use std::io::{self, Stdout};

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::Peripheral;

/// A live dashboard for one `test` block's `sim` binding. Opened lazily on
/// the first batch that needs it, restored via `Drop` when the `Run` it
/// lives on is dropped at the end of `run_test`.
///
/// Visibility matches `Peripheral`'s (`pub(in crate::sim)`, i.e. `harness.rs`
/// and below) rather than `pub(crate)` — `draw`'s signature carries a
/// `Peripheral` trait object, and a wider visibility here than that type's
/// own would trip `private_interfaces`.
pub(in crate::sim) struct Dashboard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Dashboard {
    /// Enter raw mode + the alternate screen and open a `ratatui` terminal
    /// on stdout.
    pub(in crate::sim) fn open() -> io::Result<Dashboard> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(Dashboard { terminal })
    }

    /// Draw one frame: a title row (`test_name — cycle N`) followed by one
    /// row per bound peripheral (port name, then the peripheral's own
    /// widget).
    pub(in crate::sim) fn draw(
        &mut self,
        test_name: &str,
        cycle: u64,
        peripherals: &[(String, Box<dyn Peripheral>)],
    ) -> io::Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.area();
            let rows = Layout::vertical(
                std::iter::once(Constraint::Length(1))
                    .chain(peripherals.iter().map(|_| Constraint::Length(1)))
                    .collect::<Vec<_>>(),
            )
            .split(area);
            frame.render_widget(
                Paragraph::new(Line::from(format!("{test_name} — cycle {cycle}"))),
                rows[0],
            );
            for (i, (port, peripheral)) in peripherals.iter().enumerate() {
                let row = rows[i + 1];
                let cols =
                    Layout::horizontal([Constraint::Length(20), Constraint::Fill(1)]).split(row);
                frame.render_widget(Paragraph::new(Line::from(port.clone())), cols[0]);
                let buf = frame.buffer_mut();
                peripheral.render(cols[1], buf);
            }
        })?;
        Ok(())
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        // Best-effort: a failing restore here must not panic mid-unwind
        // (e.g. if the test itself is panicking) — swallow errors.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
