//! Pass: validate `extern module` declarations — port types must be
//! scalar (Verilog FFI non-goal: no bundle/array-typed extern ports, see
//! docs/superpowers/specs/2026-07-15-verilog-ffi-design.local.md).

use crate::ast::{ModuleItem, Type};

use super::Checker;

impl<'a> Checker<'a> {
    pub(super) fn check_extern_modules(&mut self) {
        // Iteration order over a HashMap is not deterministic; sort for
        // stable diagnostic output (same rationale as `funcs.rs`'s
        // `check_func_cycles`/`check_func_unreachable`).
        let mut names: Vec<String> = self.externs.keys().cloned().collect();
        names.sort();
        for name in &names {
            let externs = self.externs[name].clone();
            for &(file, em) in &externs {
                for item in &em.items {
                    if let ModuleItem::Port { name, ty, .. } = item
                        && !is_scalar(ty)
                    {
                        self.err(
                            file,
                            name.span,
                            "E1302",
                            format!(
                                "extern module port `{}` must be a scalar type \
                                 (bit / bits[N] / signed[N])",
                                name.name
                            ),
                            "a real Verilog module's port list is always flat wires — \
                             bundle/array-typed extern ports are not supported (Verilog \
                             FFI v1 restriction)",
                        );
                    }
                }
            }
        }
    }
}

fn is_scalar(ty: &Type) -> bool {
    matches!(ty, Type::Bit | Type::Bits(_) | Type::Signed(_))
}
