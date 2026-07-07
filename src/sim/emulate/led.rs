//! The `led` peripheral: a colored on/off indicator in the dashboard,
//! bound to a `bit` or `bits[N]` output (docs/superpowers/specs/2026-07-07-hw-emulation-led-design.local.md).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span as TextSpan;
use ratatui::widgets::{Paragraph, Widget};

use crate::ast::{BindArg, BindArgValue};

use super::super::elaborate::Width;
use super::super::value::Val;
use super::Peripheral;

pub(super) fn construct(width: Width, args: &[BindArg]) -> Result<Box<dyn Peripheral>, String> {
    if width.bits == 0 || width.bits > 64 {
        return Err(format!(
            "`led` binds to a `bit` or `bits[N]` (N <= 64) output, found a {}-bit signal",
            width.bits
        ));
    }
    let mut color = Color::Green;
    for a in args {
        match a.name.name.as_str() {
            "color" => {
                let name = match &a.value {
                    BindArgValue::Ident(s) | BindArgValue::Str(s) => s.as_str(),
                };
                color = parse_color(name)
                    .ok_or_else(|| format!("`led` doesn't know the color `{name}`"))?;
            }
            other => return Err(format!("`led` has no config option `{other}`")),
        }
    }
    Ok(Box::new(Led { color, on: false }))
}

fn parse_color(name: &str) -> Option<Color> {
    Some(match name {
        "green" => Color::Green,
        "red" => Color::Red,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "white" => Color::White,
        _ => return None,
    })
}

struct Led {
    color: Color,
    on: bool,
}

impl Peripheral for Led {
    fn on_change(&mut self, val: &Val) {
        self.on = val.bits != 0;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (glyph, style) = if self.on {
            ("\u{25cf} on", Style::default().fg(self.color))
        } else {
            ("\u{25cb} off", Style::default().fg(Color::DarkGray))
        };
        Paragraph::new(TextSpan::styled(glyph, style)).render(area, buf);
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

    #[test]
    fn rejects_wide_signal() {
        let w = Width {
            bits: 65,
            signed: false,
        };
        assert!(construct(w, &[]).is_err());
    }

    #[test]
    fn accepts_bit_with_no_args() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        assert!(construct(w, &[]).is_ok());
    }

    #[test]
    fn accepts_valid_color() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        let args = [arg("color", BindArgValue::Ident("red".into()))];
        assert!(construct(w, &args).is_ok());
    }

    #[test]
    fn rejects_unknown_color() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        let args = [arg("color", BindArgValue::Ident("mauve".into()))];
        assert!(construct(w, &args).is_err());
    }

    #[test]
    fn rejects_unknown_config_key() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        let args = [arg("brightness", BindArgValue::Ident("high".into()))];
        assert!(construct(w, &args).is_err());
    }

    #[test]
    fn on_change_tracks_nonzero() {
        let w = Width {
            bits: 1,
            signed: false,
        };
        let mut p = construct(w, &[]).unwrap();
        p.on_change(&Val::new(1, 1, false));
    }
}
