//! The `speaker` peripheral: plays a bound `bit` output as a tone on the
//! host's default audio output
//! (docs/superpowers/specs/2026-07-08-hw-emulation-speaker-design.local.md).
//!
//! Renders offline: `on_tick` only downsamples and buffers bits in memory
//! (no device I/O, no pacing), and `finish` plays the whole clip back once
//! the sim has finished ticking. A tree-walking interpreter can't sustain
//! a real design's declared clock rate (measured ~1M cycles/sec in release
//! vs. a 50MHz `speed` clause — a ~50x shortfall), so pacing playback to
//! the sim instead of the other way around starves/overruns the audio
//! buffer. Buffering first decouples correct audio from interpreter speed,
//! at the cost of no live sound during a long run.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span as TextSpan;
use ratatui::widgets::{Paragraph, Widget};

use crate::ast::BindArg;

use super::Peripheral;
use mimz_sim::sim::Val;
use mimz_sim::sim::elaborate::Width;
use mimz_sim::sim::value::Bits;

fn bit0(bits: &Bits) -> bool {
    match bits {
        Bits::Small(b) => b & 1 != 0,
        Bits::Wide(limbs) => limbs.first().copied().unwrap_or(0) & 1 != 0,
    }
}

pub(super) fn construct(
    width: Width,
    args: &[BindArg],
    speed_hz: Option<u64>,
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
        initialized: false,
        samples: Vec::new(),
        cycle_counter: 0,
        speed_hz: speed_hz.unwrap_or(50_000_000),
        cycles_per_sample: 1000,
        current_bit: false,
        sample_rate: 0,
    }))
}

struct Speaker {
    /// Live indicator for the dashboard's glyph — set every `on_tick`,
    /// independent of the downsampled `samples` buffer.
    bit: Arc<AtomicBool>,
    /// Whether the device's sample rate has been queried yet (lazily, on
    /// the first `on_tick` — never in a non-live run, which never ticks).
    initialized: bool,
    /// Downsampled bits, one per audio sample, recorded as the sim ticks.
    /// Played back in one shot by `finish`.
    samples: Vec<bool>,
    cycle_counter: u64,
    speed_hz: u64,
    cycles_per_sample: u64,
    current_bit: bool,
    /// The output device's real sample rate, filled in on the first
    /// `on_tick` alongside `cycles_per_sample`.
    sample_rate: u32,
}

impl Speaker {
    fn set_bit(&self, val: &Val) {
        self.bit.store(bit0(&val.bits), Ordering::Relaxed);
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.bit.store(false, Ordering::Relaxed);
    }
}

const AMPLITUDE: f32 = 0.2;

/// The output device's sample rate. Runs on a dedicated thread, NOT the
/// caller's (the sim thread, which also drives the terminal dashboard) —
/// cpal's Windows/WASAPI backend wants its own thread-local COM state, and
/// querying it directly from the sim thread has been observed to hang
/// indefinitely (no error, no progress — just stuck).
fn query_sample_rate() -> Result<u32, String> {
    std::thread::spawn(|| -> Result<u32, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "speaker: no default audio output device".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("speaker: could not read the default output config: {e}"))?;
        Ok(supported.sample_rate())
    })
    .join()
    .map_err(|_| "speaker: audio setup thread panicked".to_string())?
}

/// Play `samples` once at `sample_rate`, blocking until the clip finishes.
/// Same reasoning as `query_sample_rate` for running on a dedicated thread.
fn play_samples(samples: &[bool], sample_rate: u32) -> Result<(), String> {
    let buf = samples.to_vec();
    std::thread::spawn(move || -> Result<(), String> {
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
        let n = buf.len();
        let pos = Arc::new(AtomicUsize::new(0));
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for frame in data.chunks_mut(channels) {
                        let i = pos.fetch_add(1, Ordering::Relaxed);
                        let sample = if buf.get(i).copied().unwrap_or(false) {
                            AMPLITUDE
                        } else {
                            0.0
                        };
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

        // No completion signal from cpal's callback — block for the clip's
        // exact nominal duration instead, which is close enough for a
        // melody clip (a few tens of ms of device buffering at most).
        let duration = std::time::Duration::from_secs_f64(n as f64 / sample_rate.max(1) as f64);
        std::thread::sleep(duration);
        Ok(())
    })
    .join()
    .map_err(|_| "speaker: playback thread panicked".to_string())?
}

impl Peripheral for Speaker {
    fn on_change(&mut self, _val: &Val) {}

    fn on_tick(&mut self, val: &Val) -> Result<(), String> {
        self.current_bit = bit0(&val.bits);
        self.set_bit(val);

        if !self.initialized {
            let sample_rate = query_sample_rate()?;
            // How many simulated cycles elapse per audio sample.
            self.cycles_per_sample = (self.speed_hz / (sample_rate as u64).max(1)).max(1);
            self.sample_rate = sample_rate;
            self.initialized = true;
        }

        self.cycle_counter += 1;
        if self.cycle_counter >= self.cycles_per_sample {
            self.cycle_counter = 0;
            self.samples.push(self.current_bit);
        }

        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        if self.samples.is_empty() {
            return Ok(());
        }
        play_samples(&self.samples, self.sample_rate)
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

    fn new_speaker(bit: Arc<AtomicBool>) -> Speaker {
        Speaker {
            bit,
            initialized: false,
            samples: Vec::new(),
            cycle_counter: 0,
            speed_hz: 50_000_000,
            cycles_per_sample: 1000,
            current_bit: false,
            sample_rate: 0,
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
        let s = new_speaker(Arc::new(AtomicBool::new(false)));
        s.set_bit(&Val::new(1, 1, false));
        assert!(s.bit.load(Ordering::Relaxed));
        s.set_bit(&Val::new(0, 1, false));
        assert!(!s.bit.load(Ordering::Relaxed));
    }

    #[test]
    fn drop_silences_a_held_high_bit() {
        let bit = Arc::new(AtomicBool::new(false));
        let s = new_speaker(Arc::clone(&bit));
        s.set_bit(&Val::new(1, 1, false));
        assert!(bit.load(Ordering::Relaxed));
        drop(s);
        assert!(!bit.load(Ordering::Relaxed));
    }

    #[test]
    fn finish_is_a_no_op_with_no_recorded_samples() {
        // Never touches the audio device when nothing was ever recorded
        // (e.g. a non-live run, where `on_tick` never fires).
        let mut s = new_speaker(Arc::new(AtomicBool::new(false)));
        assert!(s.finish().is_ok());
    }
}
