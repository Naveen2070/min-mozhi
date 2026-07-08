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
    let mut target_socket = false;
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
            "target" => {
                let name = match &a.value {
                    BindArgValue::Ident(s) | BindArgValue::Str(s) => s.as_str(),
                    BindArgValue::Int(_) => {
                        return Err(
                            "`uart_tx`'s `target` must be `terminal` or `socket`".to_string()
                        );
                    }
                };
                target_socket = match name {
                    "terminal" => false,
                    "socket" => true,
                    other => {
                        return Err(format!(
                            "`uart_tx` doesn't know the target `{other}` (expected `terminal` or `socket`)"
                        ));
                    }
                };
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
    if target_socket {
        let (tx, port) = UartTx::new_socket_target(speed_hz / baud);
        eprintln!("uart_tx: listening on 127.0.0.1:{port}");
        Ok(Box::new(tx))
    } else {
        Ok(Box::new(UartTx::new(speed_hz / baud)))
    }
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

pub(super) enum Target {
    Terminal,
    /// Receives the accepted `TcpStream` once a peer connects (accept
    /// runs on a background thread so the sim loop never blocks on it
    /// waiting for a connection to even arrive). `push_char` waits on this
    /// at most once per `UartTx` instance — see `gave_up_on_socket`.
    Socket(std::sync::mpsc::Receiver<std::net::TcpStream>),
}

pub(super) struct UartTx {
    cycles_per_bit: u64,
    state: State,
    log: VecDeque<char>,
    target: Target,
    stream: Option<std::net::TcpStream>,
    /// Set after one failed wait for a socket connection, so a `target:
    /// "socket"` peripheral whose peer never connects pays the bounded
    /// wait exactly once (on the first decoded byte) rather than once per
    /// byte forever — without this, every subsequent byte would re-block
    /// the shared per-cycle tick loop for the full timeout.
    gave_up_on_socket: bool,
}

impl UartTx {
    fn new(cycles_per_bit: u64) -> UartTx {
        UartTx::new_with_target(cycles_per_bit, Target::Terminal)
    }

    fn new_with_target(cycles_per_bit: u64, target: Target) -> UartTx {
        UartTx {
            cycles_per_bit: cycles_per_bit.max(1),
            state: State::Idle,
            log: VecDeque::new(),
            target,
            stream: None,
            gave_up_on_socket: false,
        }
    }

    /// Opens a local TCP listener and returns the peripheral plus the
    /// port it's listening on (tests connect to this port directly; a
    /// real `sim`-block author never sees it — `construct` prints it via
    /// `eprintln!`, same as `uart_rx`'s socket source).
    pub(super) fn new_socket_target(cycles_per_bit: u64) -> (UartTx, u16) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (send, recv) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let _ = send.send(stream);
            }
        });
        (
            UartTx::new_with_target(cycles_per_bit, Target::Socket(recv)),
            port,
        )
    }

    fn push_char(&mut self, c: char) {
        self.log.push_back(c);
        if self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
        if self.stream.is_none() && !self.gave_up_on_socket {
            if let Target::Socket(rx) = &self.target {
                // Bounded wait, attempted EXACTLY ONCE ever (guarded by
                // `gave_up_on_socket`, not just `stream.is_none()`): a
                // `try_recv()` here can race ahead of the background accept
                // thread, which needs an OS scheduling tick to complete the
                // handshake and forward the stream — on a freshly-opened
                // listener that tick may not have happened yet by the time
                // the first byte is ready to send. A short `recv_timeout`
                // absorbs that race. Without the one-shot guard, a `target:
                // "socket"` peripheral whose peer never connects would
                // retry this wait on every single decoded byte, stalling
                // the shared per-cycle tick loop for a full second each
                // time — indefinitely. One missed connection window means
                // this peripheral falls back to log-only for the rest of
                // the run, matching the "accept one connection" scope.
                match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                    Ok(s) => self.stream = Some(s),
                    Err(_) => self.gave_up_on_socket = true,
                }
            }
        }
        if let Some(stream) = &mut self.stream {
            use std::io::Write;
            let _ = stream.write_all(&[c as u8]); // best-effort: a dropped peer just misses bytes
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

    #[test]
    fn socket_target_streams_decoded_bytes() {
        use std::io::Read;
        use std::net::TcpStream;

        let (mut tx, port) = UartTx::new_socket_target(4);
        let accepted = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let mut buf = [0u8; 1];
            stream.read_exact(&mut buf).unwrap();
            buf[0]
        });
        feed(&mut tx, 1, 4); // idle
        feed(&mut tx, 0, 4); // start
        for bit in [1u8, 0, 0, 0, 0, 0, 1, 0] {
            // 'A'
            feed(&mut tx, bit, 4);
        }
        feed(&mut tx, 1, 4); // stop
        let byte = accepted.join().unwrap();
        assert_eq!(byte, b'A');
    }

    #[test]
    fn socket_target_with_no_client_falls_back_to_log_without_repeated_stalls() {
        // Regression test: without the `gave_up_on_socket` one-shot guard,
        // a socket target whose peer never connects would re-attempt the
        // bounded `recv_timeout` wait on every decoded byte, stalling the
        // shared per-cycle tick loop for a full second each time. Decoding
        // two bytes here would take >=2s if that guard regressed; it
        // should take a small fraction of a second once the first wait
        // gives up and every later byte skips straight to the log.
        let (mut tx, _port) = UartTx::new_socket_target(4);
        let started = std::time::Instant::now();
        for byte in [0x41u8, 0x42u8] {
            // 'A' then 'B', LSB-first
            feed(&mut tx, 1, 4); // idle
            feed(&mut tx, 0, 4); // start
            for i in 0..8 {
                feed(&mut tx, (byte >> i) & 1, 4);
            }
            feed(&mut tx, 1, 4); // stop
        }
        assert!(
            started.elapsed() < std::time::Duration::from_millis(1500),
            "decoding two bytes took {:?} — the one-shot guard likely regressed \
             into re-waiting on every byte",
            started.elapsed()
        );
        assert_eq!(tx.log_text(), "AB");
    }
}
