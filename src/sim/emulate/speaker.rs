//! The `speaker` peripheral: plays a bound `bit` output as a tone on the
//! host's default audio output
//! (docs/superpowers/specs/2026-07-08-hw-emulation-speaker-design.local.md).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span as TextSpan;
use ratatui::widgets::{Paragraph, Widget};

use crate::ast::BindArg;

use super::super::elaborate::Width;
use super::super::value::Val;
use super::Peripheral;

pub(super) fn construct(
    width: Width,
    args: &[BindArg],
    _speed_hz: Option<u64>,
) -> Result<Box<dyn Peripheral>, String> {
    if width.bits != 1 {
        return Err(format!(
            "`speaker` binds to a single `bit` output, found a {}-bit signal",
            width.bits
        ));
    }
    if let Some(a) = args.first() {
        return Err(format!("`speaker` has no config option `{}`", a.name.name));
    }
    Ok(Box::new(Speaker {
        bit: Arc::new(AtomicBool::new(false)),
        stream_started: false,
    }))
}

struct Speaker {
    bit: Arc<AtomicBool>,
    /// `true` once the background audio thread has been spawned. The
    /// `cpal::Stream` itself never appears on this struct — `cpal::Stream`
    /// is `!Send` on every platform (see the Amendment above), so it must
    /// be created AND held on the same thread that plays it, never moved.
    stream_started: bool,
}

impl Speaker {
    fn set_bit(&self, val: &Val) {
        self.bit.store(val.bits & 1 != 0, Ordering::Relaxed);
    }
}

/// Fixed output amplitude — full-scale (`1.0`) clips/distorts on most
/// devices; this is a moderate, always-audible level. No `volume` config
/// this spec (see the design doc's non-goals).
const AMPLITUDE: f32 = 0.2;

/// Opens the host's default audio output device and starts a stream whose
/// callback zero-order-holds `bit`'s current value as a square wave. Only
/// f32 output is supported — anything else is a named error, not a panic.
/// MUST be called on the thread that will hold the returned `Stream` for
/// its lifetime — `cpal::Stream` is `!Send`.
fn open_stream(bit: Arc<AtomicBool>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "speaker: no default audio output device".to_string())?;
    let supported = device
        .default_output_config()
        .map_err(|e| format!("speaker: could not read the default output config: {e}"))?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels.max(1) as usize;
    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let sample = if bit.load(Ordering::Relaxed) {
                    AMPLITUDE
                } else {
                    0.0
                };
                for frame in data.chunks_mut(channels) {
                    for s in frame {
                        *s = sample;
                    }
                }
            },
            |e| eprintln!("speaker: stream error: {e}"),
            None,
        ),
        other => {
            return Err(format!(
                "speaker: unsupported audio sample format {other:?} (only f32 is supported)"
            ));
        }
    }
    .map_err(|e| format!("speaker: could not build the audio stream: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("speaker: could not start the audio stream: {e}"))?;
    Ok(stream)
}

impl Peripheral for Speaker {
    fn on_change(&mut self, _val: &Val) {}

    fn on_tick(&mut self, val: &Val) -> Result<(), String> {
        self.set_bit(val);
        if !self.stream_started {
            let bit = Arc::clone(&self.bit);
            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            thread::spawn(move || match open_stream(bit) {
                Ok(_stream) => {
                    // Report success, then park forever so `_stream`
                    // (this thread's local, never returned) stays alive
                    // for the process's remaining lifetime — cpal's
                    // callback keeps running as long as it isn't dropped.
                    // Accepted debt, same shape as uart_tx/uart_rx's
                    // parked accept threads (Spec 2): never joined or
                    // signaled to stop, harmless for a CLI-lifetime
                    // peripheral (.superpowers/sdd/spec2-summary.md).
                    let _ = tx.send(Ok(()));
                    loop {
                        thread::park();
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            });
            // Block for the open-or-fail result so a bad device still
            // surfaces as a synchronous `Err` from THIS call, matching
            // the original hard-error contract.
            match rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(
                        "speaker: audio thread exited before opening the device".to_string()
                    );
                }
            }
            self.stream_started = true;
        }
        Ok(())
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (glyph, style) = if self.bit.load(Ordering::Relaxed) {
            ("\u{266a} on", Style::default().fg(Color::Cyan))
        } else {
            ("\u{266a} off", Style::default().fg(Color::DarkGray))
        };
        Paragraph::new(TextSpan::styled(glyph, style)).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BindArgValue, Ident};
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

    #[test]
    fn rejects_non_single_bit_signal() {
        let w = Width {
            bits: 8,
            signed: false,
        };
        assert!(construct(w, &[], None).is_err());
    }

    #[test]
    fn accepts_single_bit_with_no_args() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        assert!(construct(w, &[], None).is_ok());
    }

    #[test]
    fn rejects_any_config_arg() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        let args = [arg("volume", BindArgValue::Int(50))];
        assert!(construct(w, &args, None).is_err());
    }

    #[test]
    fn set_bit_tracks_the_value() {
        let s = Speaker {
            bit: Arc::new(AtomicBool::new(false)),
            stream_started: false,
        };
        s.set_bit(&Val::new(1, 1, false));
        assert!(s.bit.load(Ordering::Relaxed));
        s.set_bit(&Val::new(0, 1, false));
        assert!(!s.bit.load(Ordering::Relaxed));
    }
}
