//! Pass: validate `extern module` declarations — port types must be
//! scalar (`bit`/`bits[N]`/`signed[N]`, E1302), and an `= "alias"` must be a
//! legal Verilog identifier (E1303). A real Verilog module's port
//! list is always a flat list of wires, so bundle/array-typed extern
//! ports are out of scope for v1.

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
                if let (Some(alias), Some(span)) = (&em.verilog_name, em.verilog_name_span)
                    && !is_verilog_identifier(alias)
                {
                    self.err(
                        file,
                        span,
                        "E1303",
                        format!("extern module alias `{alias}` is not a Verilog identifier"),
                        "the alias is the real Verilog module's name: letters, digits, `_` and \
                         `$`, not starting with a digit, and not a Verilog keyword \
                         (e.g. `= \"PLL_HARD_IP_v2\"`)",
                    );
                }
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

/// `[A-Za-z_][A-Za-z0-9_$]*`, not a Verilog-2005 keyword.
fn is_verilog_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        && !crate::backend::VERILOG_KEYWORDS.contains(&s)
}

fn is_scalar(ty: &Type) -> bool {
    matches!(ty, Type::Bit | Type::Bits(_) | Type::Signed(_))
}
