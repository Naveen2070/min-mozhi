//! Stub — `uart_tx` decode is implemented in Task 3
//! (docs/superpowers/specs/2026-07-08-hw-emulation-uart-design.local.md).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ast::BindArg;

use super::super::elaborate::Width;
use super::super::value::Val;
use super::Peripheral;

pub(super) fn construct(
    _width: Width,
    _args: &[BindArg],
    _speed_hz: Option<u64>,
) -> Result<Box<dyn Peripheral>, String> {
    Err("`uart_tx` is not implemented yet".to_string())
}

// ponytail: never constructed until Task 3 replaces this stub with a real
// peripheral — allow(dead_code) beats a fake caller just to appease clippy.
#[allow(dead_code)]
struct Stub;
impl Peripheral for Stub {
    fn on_change(&mut self, _val: &Val) {}
    fn render(&self, _area: Rect, _buf: &mut Buffer) {}
}
