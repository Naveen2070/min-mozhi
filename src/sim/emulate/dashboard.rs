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

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
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
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen) {
            // No `Dashboard` (and thus no `Drop`) exists yet to restore the
            // terminal — undo the raw-mode switch ourselves before erroring.
            let _ = disable_raw_mode();
            return Err(e);
        }
        // Discard any input already queued at this point — typically the
        // Enter keystroke that launched `mimz` itself, still sitting in the
        // console's input buffer. Without this, the very first
        // `event::read()` in `wait_for_step`/`wait_for_dismiss` sees that
        // leftover Enter and returns instantly, before the user ever gets
        // a chance to look at the screen.
        while event::poll(std::time::Duration::ZERO)? {
            event::read()?;
        }
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(Dashboard { terminal })
    }

    /// Draw one frame: a title row (`test_name — cycle N`, plus `hint` if
    /// given — e.g. `--step`'s "Enter to advance, q to quit") followed by
    /// one row per bound peripheral (port name, then the peripheral's own
    /// widget).
    pub(in crate::sim) fn draw(
        &mut self,
        test_name: &str,
        cycle: u64,
        peripherals: &[(String, Box<dyn Peripheral>)],
        hint: Option<&str>,
    ) -> io::Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.area();
            let rows = Layout::vertical(
                std::iter::once(Constraint::Length(1))
                    .chain(peripherals.iter().map(|_| Constraint::Length(1)))
                    .collect::<Vec<_>>(),
            )
            .split(area);
            let title = match hint {
                Some(h) => format!("{test_name} — cycle {cycle}  [{h}]"),
                None => format!("{test_name} — cycle {cycle}"),
            };
            frame.render_widget(Paragraph::new(Line::from(title)), rows[0]);
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

    /// Block until the user presses Enter (advance one `--step`) or `q`/Esc
    /// (quit the whole run) — called after each single-cycle `draw` while
    /// stepping. Ignores every other key so a stray keypress can't
    /// accidentally advance or quit.
    pub(in crate::sim) fn wait_for_step(&mut self) -> io::Result<bool> {
        read_continue_or_quit()
    }

    /// Draw a "finished" screen and block until the user presses Enter
    /// (move on to the next test) or `q`/Esc (quit the whole run) — so the
    /// dashboard doesn't vanish the instant a live test's last cycle ticks,
    /// before anyone watching gets to see the final state.
    pub(in crate::sim) fn wait_for_dismiss(
        &mut self,
        test_name: &str,
        cycle: u64,
    ) -> io::Result<bool> {
        self.terminal.draw(|frame| {
            let area = frame.area();
            let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
            frame.render_widget(
                Paragraph::new(Line::from(format!(
                    "{test_name} — finished at cycle {cycle}"
                ))),
                rows[0],
            );
            frame.render_widget(
                Paragraph::new(Line::from("press Enter to continue, q to quit")),
                rows[1],
            );
        })?;
        read_continue_or_quit()
    }
}

/// Blocks for Enter (`Ok(false)`, continue) or `q`/Esc (`Ok(true)`, quit),
/// ignoring every other key/event. Only reacts to `Press` — some terminals
/// report `Release`/`Repeat` too, which would otherwise double-fire on one
/// physical keystroke.
fn read_continue_or_quit() -> io::Result<bool> {
    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Enter => return Ok(false),
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                _ => {}
            }
        }
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
