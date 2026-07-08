//! The `uart_rx` peripheral: drives a bound input bit with an 8-N-1
//! encoded byte stream from a literal `source` string
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

pub(super) fn construct(
    width: Width,
    args: &[BindArg],
    speed_hz: Option<u64>,
) -> Result<Box<dyn Peripheral>, String> {
    if width.bits != 1 {
        return Err(format!(
            "`uart_rx` binds to a single `bit` input, found a {}-bit signal",
            width.bits
        ));
    }
    let mut baud = None;
    let mut source = None;
    for a in args {
        match a.name.name.as_str() {
            "baud" => {
                if baud.is_some() {
                    return Err("`uart_rx` has a duplicate `baud` config".to_string());
                }
                baud = Some(match &a.value {
                    BindArgValue::Int(v) => *v as u64,
                    _ => {
                        return Err(
                            "`uart_rx`'s `baud` must be a number, e.g. `baud: 9600`".to_string()
                        );
                    }
                });
            }
            "source" => {
                if source.is_some() {
                    return Err("`uart_rx` has a duplicate `source` config".to_string());
                }
                source = Some(match &a.value {
                    BindArgValue::Str(s) | BindArgValue::Ident(s) => s.clone(),
                    BindArgValue::Int(_) => {
                        return Err(
                            "`uart_rx`'s `source` must be text, e.g. `source: \"hi\"`".to_string()
                        );
                    }
                });
            }
            other => return Err(format!("`uart_rx` has no config option `{other}`")),
        }
    }
    let baud =
        baud.ok_or_else(|| "`uart_rx` needs a `baud` config (e.g. `baud: 9600`)".to_string())?;
    let speed_hz = speed_hz.ok_or_else(|| {
        "`uart_rx` needs the sim block's `speed` clause to derive bit timing from `baud`"
            .to_string()
    })?;
    if baud == 0 || speed_hz / baud == 0 {
        return Err(format!(
            "`uart_rx`'s baud ({baud}) is faster than the sim speed ({speed_hz} Hz) — no cycles left per bit"
        ));
    }
    let source = source
        .ok_or_else(|| "`uart_rx` needs a `source` config (e.g. `source: \"hi\"`)".to_string())?;
    if source == "socket" {
        return Err("`uart_rx`'s `source: \"socket\"` isn't supported yet".to_string());
    }
    Ok(Box::new(UartRx::new(speed_hz / baud, source.into_bytes())))
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

pub(super) struct UartRx {
    cycles_per_bit: u64,
    queue: VecDeque<u8>,
    state: State,
}

impl UartRx {
    fn new(cycles_per_bit: u64, bytes: Vec<u8>) -> UartRx {
        UartRx {
            cycles_per_bit: cycles_per_bit.max(1),
            queue: bytes.into(),
            state: State::Idle,
        }
    }
}

impl Peripheral for UartRx {
    fn on_change(&mut self, _val: &Val) {}

    fn drive(&mut self) -> Option<u64> {
        if matches!(self.state, State::Idle) {
            match self.queue.front() {
                Some(&byte) => {
                    self.state = State::Framing {
                        phase: Phase::Start,
                        cycle_in_phase: 0,
                        byte,
                    };
                }
                None => return Some(1), // idle-high, nothing queued
            }
        }
        let State::Framing {
            phase,
            cycle_in_phase,
            byte,
        } = self.state
        else {
            unreachable!()
        };
        let bit = match phase {
            Phase::Start => 0,
            Phase::Data(i) => (byte >> i) & 1,
            Phase::Stop => 1,
        };
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
                Phase::Stop => {
                    self.queue.pop_front();
                    State::Idle
                }
            }
        } else {
            State::Framing {
                phase,
                cycle_in_phase: next_cycle,
                byte,
            }
        };
        Some(bit as u64)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let text = format!("{} byte(s) queued", self.queue.len());
        Paragraph::new(TextSpan::raw(text)).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(rx: &mut UartRx, cycles: u64) -> Vec<u64> {
        (0..cycles).map(|_| rx.drive().unwrap()).collect()
    }

    #[test]
    fn encodes_a_literal_source_with_8n1_framing() {
        let mut rx = UartRx::new(4, "A".as_bytes().to_vec()); // cycles_per_bit = 4
        // idle-high while queue is non-empty is only true before drive() is
        // ever called; the very first drive() call starts the start bit.
        let bits = drain(&mut rx, 4 + 4 * 8 + 4); // start + 8 data + stop
        let mut expected = vec![0u64; 4]; // start bit, held 4 cycles
        // 0x41 ('A') LSB-first: 1,0,0,0,0,0,1,0
        for bit in [1u64, 0, 0, 0, 0, 0, 1, 0] {
            expected.extend(std::iter::repeat_n(bit, 4));
        }
        expected.extend(std::iter::repeat_n(1u64, 4)); // stop bit
        assert_eq!(bits, expected);
    }

    #[test]
    fn idles_high_once_the_queue_drains() {
        let mut rx = UartRx::new(1, Vec::new());
        assert_eq!(rx.drive(), Some(1));
        assert_eq!(rx.drive(), Some(1));
    }

    #[test]
    fn rejects_wide_signal() {
        let w = Width {
            bits: 8,
            signed: false,
        };
        assert!(construct(w, &[], Some(115_200)).is_err());
    }

    #[test]
    fn rejects_socket_source_for_now() {
        use crate::ast::Ident;
        use crate::span::Span;
        let w = Width {
            bits: 1,
            signed: false,
        };
        let args = [
            BindArg {
                name: Ident {
                    name: "baud".into(),
                    span: Span::new(0, 0),
                },
                value: BindArgValue::Int(9600),
                span: Span::new(0, 0),
            },
            BindArg {
                name: Ident {
                    name: "source".into(),
                    span: Span::new(0, 0),
                },
                value: BindArgValue::Str("socket".into()),
                span: Span::new(0, 0),
            },
        ];
        assert!(construct(w, &args, Some(115_200)).is_err());
    }
}
