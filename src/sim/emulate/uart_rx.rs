//! The `uart_rx` peripheral: drives a bound input bit with an 8-N-1
//! encoded byte stream from a literal `source` string
//! (docs/superpowers/specs/2026-07-08-hw-emulation-uart-design.local.md).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span as TextSpan;
use ratatui::widgets::{Paragraph, Widget};

use crate::ast::{BindArg, BindArgValue};

use super::super::elaborate::Width;
use super::super::value::Val;
use super::{Peripheral, parse_port};

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
    let mut port = None;
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
            "port" => {
                if port.is_some() {
                    return Err("`uart_rx` has a duplicate `port` config".to_string());
                }
                port = Some(parse_port(&a.value, "uart_rx")?);
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
    // `port` picks a fixed socket port; `source: "socket"` alone still works,
    // with an OS-assigned ephemeral one. The two can't disagree.
    if let Some(s) = &source {
        if port.is_some() && s != "socket" {
            return Err("`uart_rx`'s `port` only makes sense with a socket source".to_string());
        }
    }
    if port.is_some() || source.as_deref() == Some("socket") {
        return Ok(Box::new(UartRx::new_socket(speed_hz / baud, port)?.0));
    }
    let source = source
        .ok_or_else(|| "`uart_rx` needs a `source` config (e.g. `source: \"hi\"`)".to_string())?;
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
    queue: QueueSource,
    state: State,
}

/// Where `UartRx` pulls its byte stream from. `Shared` is fed by a
/// background thread (see `new_socket`) — `drive()` only ever takes a
/// brief, uncontended lock to peek/pop it, so a peer that never connects
/// leaves the queue permanently empty (idle-high) rather than blocking
/// anything.
enum QueueSource {
    Local(VecDeque<u8>),
    Shared(Arc<Mutex<VecDeque<u8>>>),
}

impl QueueSource {
    fn front(&self) -> Option<u8> {
        match self {
            QueueSource::Local(q) => q.front().copied(),
            QueueSource::Shared(q) => q.lock().unwrap().front().copied(),
        }
    }
    fn pop_front(&mut self) {
        match self {
            QueueSource::Local(q) => {
                q.pop_front();
            }
            QueueSource::Shared(q) => {
                q.lock().unwrap().pop_front();
            }
        }
    }
    fn len(&self) -> usize {
        match self {
            QueueSource::Local(q) => q.len(),
            QueueSource::Shared(q) => q.lock().unwrap().len(),
        }
    }
}

impl UartRx {
    fn new(cycles_per_bit: u64, bytes: Vec<u8>) -> UartRx {
        UartRx {
            cycles_per_bit: cycles_per_bit.max(1),
            queue: QueueSource::Local(bytes.into()),
            state: State::Idle,
        }
    }

    /// Opens a local TCP listener and returns the peripheral plus the port
    /// it's listening on — `port`'s caller-chosen port if given, otherwise
    /// an OS-assigned ephemeral one (printed via `eprintln!` since a
    /// `sim`-block author with no explicit `port` has no other way to
    /// learn it, same as `uart_tx`'s socket target).
    pub(super) fn new_socket(
        cycles_per_bit: u64,
        port: Option<u16>,
    ) -> Result<(UartRx, u16), String> {
        let addr = format!("127.0.0.1:{}", port.unwrap_or(0));
        let listener = std::net::TcpListener::bind(&addr)
            .map_err(|e| format!("`uart_rx` couldn't open a local socket on {addr}: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("`uart_rx` couldn't open a local socket: {e}"))?
            .port();
        eprintln!("uart_rx: listening on 127.0.0.1:{port}");
        let shared = Arc::new(Mutex::new(VecDeque::new()));
        let shared_for_thread = Arc::clone(&shared);
        std::thread::spawn(move || {
            use std::io::Read;
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 256];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 {
                        break; // peer disconnected
                    }
                    shared_for_thread.lock().unwrap().extend(&buf[..n]);
                }
            }
        });
        Ok((
            UartRx {
                cycles_per_bit: cycles_per_bit.max(1),
                queue: QueueSource::Shared(shared),
                state: State::Idle,
            },
            port,
        ))
    }
}

impl Peripheral for UartRx {
    fn on_change(&mut self, _val: &Val) {}

    fn drive(&mut self) -> Option<u64> {
        if matches!(self.state, State::Idle) {
            match self.queue.front() {
                Some(byte) => {
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
        // The start bit holds one cycle longer than every other phase: a
        // receiver's `Idle` state detects it (`rx == 0`) on a cycle that
        // doesn't itself count toward the bit timer — only the *next*
        // cycle starts that countdown — so the total time `rx` must stay
        // low for a valid start bit is `cycles_per_bit + 1`, not
        // `cycles_per_bit`. Every other phase transitions within the same
        // counted rhythm and needs no such adjustment. Matches this
        // codebase's own literal-drive convention (see
        // showcase/*/uart_echo.mimz's `tick(clk, CLKS_PER_BIT + 1)` after
        // setting the start bit).
        let phase_len = match phase {
            Phase::Start => self.cycles_per_bit + 1,
            Phase::Data(_) | Phase::Stop => self.cycles_per_bit,
        };
        let next_cycle = cycle_in_phase + 1;
        self.state = if next_cycle >= phase_len {
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
        // The start bit holds 5 cycles (cycles_per_bit + 1), one longer
        // than every other phase — see the comment in `drive` for why.
        let bits = drain(&mut rx, 5 + 4 * 8 + 4); // start + 8 data + stop
        let mut expected = vec![0u64; 5]; // start bit, held 5 cycles
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
    fn accepts_socket_source_via_construct() {
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
        assert!(construct(w, &args, Some(115_200)).is_ok());
    }

    fn cfg_arg(name: &str, value: BindArgValue) -> BindArg {
        use crate::ast::Ident;
        use crate::span::Span;
        BindArg {
            name: Ident {
                name: name.into(),
                span: Span::new(0, 0),
            },
            value,
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn port_alone_implies_socket_transport() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        // Probe the OS for a free port rather than hardcoding one, to avoid
        // both a privileged-port bind failure (<1024) and a CI collision.
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let free_port = probe.local_addr().unwrap().port();
        drop(probe);
        let args = [
            cfg_arg("baud", BindArgValue::Int(9600)),
            cfg_arg("port", BindArgValue::Int(free_port as u128)),
        ];
        assert!(construct(w, &args, Some(115_200)).is_ok());
    }

    #[test]
    fn port_conflicts_with_a_non_socket_source() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        let args = [
            cfg_arg("baud", BindArgValue::Int(9600)),
            cfg_arg("source", BindArgValue::Str("hi".into())),
            cfg_arg("port", BindArgValue::Int(8080)),
        ];
        assert!(construct(w, &args, Some(115_200)).is_err());
    }

    #[test]
    fn rejects_port_out_of_range() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        let args = [
            cfg_arg("baud", BindArgValue::Int(9600)),
            cfg_arg("port", BindArgValue::Int(70_000)),
        ];
        assert!(construct(w, &args, Some(115_200)).is_err());
    }

    #[test]
    fn new_socket_binds_the_requested_port() {
        // Ask the OS for a free port, release it, then request that exact
        // port from `new_socket` — the same tiny "will it still be free"
        // race any fixed-port test accepts.
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let want = probe.local_addr().unwrap().port();
        drop(probe);
        let (_rx, got) = UartRx::new_socket(4, Some(want)).unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn socket_source_feeds_the_queue() {
        use std::io::Write;
        use std::net::TcpStream;

        let (mut rx, port) = UartRx::new_socket(4, None).unwrap();
        let sent = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"A").unwrap();
        });
        sent.join().unwrap();
        // Poll drive() until the byte has arrived and been queued — the
        // accept + read happen on a background thread, so the first few
        // drive() calls may still see an empty queue (idle-high).
        let mut bits = Vec::new();
        for _ in 0..200 {
            let bit = rx.drive().unwrap();
            bits.push(bit);
            if bits.len() >= 4 + 4 * 8 + 4 && bits.contains(&0) {
                break;
            }
        }
        assert!(bits.contains(&0), "never saw a start bit: {bits:?}");
    }

    #[test]
    fn socket_source_with_no_client_idles_high_without_blocking() {
        // Regression check for a Task-5-style unbounded wait: this source
        // never blocks drive() on a connection at all (front()/pop_front()
        // only take a brief, uncontended lock on the shared queue), so a
        // peer that never connects should behave exactly like an empty
        // literal source — idle-high, immediately, every call.
        let (mut rx, _port) = UartRx::new_socket(4, None).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..1000 {
            assert_eq!(rx.drive(), Some(1));
        }
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "1000 drive() calls with no client took {:?} — drive() may be blocking \
             on the connection somewhere",
            started.elapsed()
        );
    }
}
