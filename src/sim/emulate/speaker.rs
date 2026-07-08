//! The `speaker` peripheral: plays a bound `bit` output as a tone on the
//! host's default audio output
//! (docs/superpowers/specs/2026-07-08-hw-emulation-speaker-design.local.md).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    }))
}

struct Speaker {
    bit: Arc<AtomicBool>,
}

impl Speaker {
    fn set_bit(&self, val: &Val) {
        self.bit.store(val.bits & 1 != 0, Ordering::Relaxed);
    }
}

impl Peripheral for Speaker {
    fn on_change(&mut self, _val: &Val) {}

    fn on_tick(&mut self, val: &Val) -> Result<(), String> {
        self.set_bit(val);
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
        };
        s.set_bit(&Val::new(1, 1, false));
        assert!(s.bit.load(Ordering::Relaxed));
        s.set_bit(&Val::new(0, 1, false));
        assert!(!s.bit.load(Ordering::Relaxed));
    }
}
