//! The `uart_tx` peripheral: decodes 8-N-1 serial off a bound output bit
//! to a scrolling text log
//! (docs/superpowers/specs/2026-07-08-hw-emulation-uart-design.local.md).

use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span as TextSpan;
use ratatui::widgets::{Paragraph, Widget};

use crate::ast::{BindArg, BindArgValue};

use super::super::elaborate::Width;
use super::super::value::Val;
use super::Peripheral;

/// How many trailing decoded characters `render` shows.
const LOG_CAP: usize = 32;

pub(super) fn construct(
    width: Width,
    args: &[BindArg],
    speed_hz: Option<u64>,
) -> Result<Box<dyn Peripheral>, String> {
    if width.bits != 1 {
        return Err(format!(
            "`uart_tx` binds to a single `bit` output, found a {}-bit signal",
            width.bits
        ));
    }
    let mut baud = None;
    for a in args {
        match a.name.name.as_str() {
            "baud" => {
                if baud.is_some() {
                    return Err("`uart_tx` has a duplicate `baud` config".to_string());
                }
                baud = Some(match &a.value {
                    BindArgValue::Int(v) => *v as u64,
                    _ => {
                        return Err(
                            "`uart_tx`'s `baud` must be a number, e.g. `baud: 9600`".to_string()
                        );
                    }
                });
            }
            other => return Err(format!("`uart_tx` has no config option `{other}`")),
        }
    }
    let baud =
        baud.ok_or_else(|| "`uart_tx` needs a `baud` config (e.g. `baud: 9600`)".to_string())?;
    let speed_hz = speed_hz.ok_or_else(|| {
        "`uart_tx` needs the sim block's `speed` clause to derive bit timing from `baud`"
            .to_string()
    })?;
    if baud == 0 || speed_hz / baud == 0 {
        return Err(format!(
            "`uart_tx`'s baud ({baud}) is faster than the sim speed ({speed_hz} Hz) — no cycles left per bit"
        ));
    }
    Ok(Box::new(UartTx::new(speed_hz / baud)))
}

#[derive(Clone, Copy)]
enum Phase {
    Start,
    Data(u8),
    Stop,
}

#[derive(Clone, Copy)]
enum State {
    Idle,
    Framing {
        phase: Phase,
        cycle_in_phase: u64,
        byte: u8,
    },
}

pub(super) struct UartTx {
    cycles_per_bit: u64,
    state: State,
    log: VecDeque<char>,
}

impl UartTx {
    fn new(cycles_per_bit: u64) -> UartTx {
        UartTx {
            cycles_per_bit: cycles_per_bit.max(1),
            state: State::Idle,
            log: VecDeque::new(),
        }
    }

    fn push_char(&mut self, c: char) {
        self.log.push_back(c);
        if self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }

    fn push_note(&mut self, s: &str) {
        for c in s.chars() {
            self.push_char(c);
        }
    }

    #[cfg(test)]
    pub(super) fn log_text(&self) -> String {
        self.log.iter().collect()
    }
}

impl Peripheral for UartTx {
    fn on_change(&mut self, _val: &Val) {}

    fn on_tick(&mut self, val: &Val) {
        let bit = (val.bits & 1) as u8;
        if matches!(self.state, State::Idle) {
            if bit != 0 {
                return; // still idle-high
            }
            self.state = State::Framing {
                phase: Phase::Start,
                cycle_in_phase: 0,
                byte: 0,
            };
        }
        let State::Framing {
            phase,
            cycle_in_phase,
            byte,
        } = self.state
        else {
            unreachable!()
        };
        let mut byte = byte;
        let midpoint = self.cycles_per_bit / 2;
        if cycle_in_phase == midpoint {
            match phase {
                Phase::Start => {
                    if bit != 0 {
                        self.state = State::Idle; // false start / glitch
                        return;
                    }
                }
                Phase::Data(i) => byte |= bit << i,
                Phase::Stop => {
                    if bit == 1 {
                        self.push_char(byte as char);
                    } else {
                        self.push_note("<framing error>");
                    }
                }
            }
        }
        let next_cycle = cycle_in_phase + 1;
        self.state = if next_cycle >= self.cycles_per_bit {
            match phase {
                Phase::Start => State::Framing {
                    phase: Phase::Data(0),
                    cycle_in_phase: 0,
                    byte,
                },
                Phase::Data(i) if i < 7 => State::Framing {
                    phase: Phase::Data(i + 1),
                    cycle_in_phase: 0,
                    byte,
                },
                Phase::Data(_) => State::Framing {
                    phase: Phase::Stop,
                    cycle_in_phase: 0,
                    byte,
                },
                Phase::Stop => State::Idle,
            }
        } else {
            State::Framing {
                phase,
                cycle_in_phase: next_cycle,
                byte,
            }
        };
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let text: String = self.log.iter().collect();
        Paragraph::new(TextSpan::raw(text)).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Ident;
    use crate::span::Span;

    fn arg(name: &str, value: BindArgValue) -> BindArg {
        BindArg {
            name: Ident {
                name: name.into(),
                span: Span::new(0, 0),
            },
            value,
            span: Span::new(0, 0),
        }
    }

    fn feed(tx: &mut UartTx, bit: u8, cycles: u64) {
        for _ in 0..cycles {
            tx.on_tick(&Val::new(bit as u128, 1, false));
        }
    }

    #[test]
    fn decodes_a_byte_at_small_cycles_per_bit() {
        let mut tx = UartTx::new(4); // cycles_per_bit = 4
        feed(&mut tx, 1, 4); // idle
        feed(&mut tx, 0, 4); // start bit
        // 0x41 ('A') LSB-first: 1,0,0,0,0,0,1,0
        for bit in [1, 0, 0, 0, 0, 0, 1, 0] {
            feed(&mut tx, bit, 4);
        }
        feed(&mut tx, 1, 4); // stop bit
        assert!(tx.log_text().contains('A'), "got: {:?}", tx.log_text());
    }

    #[test]
    fn framing_error_on_bad_stop_bit_is_logged() {
        let mut tx = UartTx::new(4);
        feed(&mut tx, 1, 4);
        feed(&mut tx, 0, 4); // start
        for _ in 0..8 {
            feed(&mut tx, 0, 4); // 8 zero data bits
        }
        feed(&mut tx, 0, 4); // BAD stop bit (should be 1)
        assert!(
            tx.log_text().contains("framing error"),
            "got: {:?}",
            tx.log_text()
        );
    }

    #[test]
    fn rejects_wide_signal() {
        let w = Width {
            bits: 8,
            signed: false,
        };
        assert!(construct(w, &[arg("baud", BindArgValue::Int(9600))], Some(115_200)).is_err());
    }

    #[test]
    fn rejects_missing_baud() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        assert!(construct(w, &[], Some(115_200)).is_err());
    }

    #[test]
    fn rejects_missing_speed() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        assert!(construct(w, &[arg("baud", BindArgValue::Int(9600))], None).is_err());
    }

    #[test]
    fn rejects_baud_faster_than_speed() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        assert!(construct(w, &[arg("baud", BindArgValue::Int(9600))], Some(1000)).is_err());
    }
}
