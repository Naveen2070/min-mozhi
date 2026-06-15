//! Pass 2 — compile-time constant evaluation.
//!
//! Evaluates file-level `const` declarations (top to bottom, each may use
//! the ones above it) and provides [`eval`] for every other place a
//! constant is required (`repeat` bounds, parameter defaults). Values are
//! `i128` — wide enough for any width arithmetic; overflow is an error,
//! never a silent wrap (E0202).
//!
//! What does NOT const-evaluate (E0201): signal names, wrapping operators
//! (`+%` needs a bit width), `match`, concat/index/slice, builtins. The
//! error says which and why — this list shrinks as the checker grows.

use std::collections::HashMap;

use crate::ast::{BinOp, Expr, ExprKind, TopItem, UnOp};
use crate::diag::Diag;

use super::Checker;

/// Environment for one evaluation: name -> value. Built from file consts
/// plus (in `names.rs`) module consts and enclosing `repeat` variables.
/// `pub(crate)` so the Verilog emitter can fold the same constants when it
/// unrolls `repeat` (it shares this evaluator rather than reimplementing it).
pub(crate) type Env = HashMap<String, i128>;

impl<'a> Checker<'a> {
    /// Evaluate every file-level `const`, in source order, into
    /// `self.file_consts`. Duplicates are E0004; a const referring to a
    /// LATER const fails naturally as E0201 (evaluation is top to bottom).
    pub(super) fn eval_consts(&mut self) {
        for file in 0..self.files.len() {
            for item in &self.files[file].items {
                let TopItem::Const(c) = item else { continue };
                if self.file_consts[file].contains_key(&c.name.name) {
                    self.err(
                        file,
                        c.name.span,
                        "E0004",
                        format!(
                            "const `{}` is defined more than once in this file",
                            c.name.name
                        ),
                        "rename one of them — consts are file-local, so the names only \
                         need to be unique within this file",
                    );
                    continue;
                }
                match eval(&c.value, &self.file_consts[file]) {
                    Ok(v) => {
                        self.file_consts[file].insert(c.name.name.clone(), v);
                    }
                    Err(d) => self.diags.push(d.with_file(file)),
                }
            }
        }
    }
}

/// Evaluate `e` to a compile-time value, or explain why it is not one.
/// The returned diagnostic carries its code but NOT a file index — the
/// caller stamps that (`.with_file(...)`), since only it knows the file.
// `Diag` is intentionally the error type (it carries the teaching message); this
// is a cold compile-error path, not a hot return, so the large-Err move cost the
// lint warns about is irrelevant here.
#[allow(clippy::result_large_err)]
pub(crate) fn eval(e: &Expr, env: &Env) -> Result<i128, Diag> {
    let not_const = |what: &str, why: &str| {
        Err(
            Diag::new(e.span, format!("{what} is not a compile-time constant"))
                .with_code("E0201")
                .with_help(why.to_string()),
        )
    };
    match &e.kind {
        ExprKind::Int { value, .. } => i128::try_from(*value).map_err(|_| {
            Diag::new(e.span, "constant is too large")
                .with_code("E0202")
                .with_help("compile-time arithmetic works on values up to 2^127 - 1")
        }),
        ExprKind::Bool(b) => Ok(*b as i128),
        ExprKind::Ident(name) => match env.get(name) {
            Some(v) => Ok(*v),
            None => not_const(
                &format!("`{name}`"),
                "only `const` values, literals, and `repeat` variables work here — \
                 consts are evaluated top to bottom, so a const can only use the \
                 ones declared above it",
            ),
        },
        ExprKind::Unary { op, expr } => {
            let v = eval(expr, env)?;
            match op {
                UnOp::Neg => v.checked_neg().ok_or_else(|| overflow(e)),
                UnOp::LogicNot => Ok((v == 0) as i128),
                _ => not_const(
                    "this operator",
                    "bitwise operators need a known bit width, which constants \
                     do not have — use arithmetic and comparisons instead",
                ),
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = eval(lhs, env)?;
            let r = eval(rhs, env)?;
            let arith = |v: Option<i128>| v.ok_or_else(|| overflow(e));
            match op {
                BinOp::Add => arith(l.checked_add(r)),
                BinOp::Sub => arith(l.checked_sub(r)),
                BinOp::Mul => arith(l.checked_mul(r)),
                BinOp::Shl => arith(u32::try_from(r).ok().and_then(|r| l.checked_shl(r))),
                BinOp::Shr => arith(u32::try_from(r).ok().and_then(|r| l.checked_shr(r))),
                BinOp::BitAnd => Ok(l & r),
                BinOp::BitOr => Ok(l | r),
                BinOp::BitXor => Ok(l ^ r),
                BinOp::Eq => Ok((l == r) as i128),
                BinOp::Ne => Ok((l != r) as i128),
                BinOp::Lt => Ok((l < r) as i128),
                BinOp::Le => Ok((l <= r) as i128),
                BinOp::Gt => Ok((l > r) as i128),
                BinOp::Ge => Ok((l >= r) as i128),
                BinOp::LogicAnd => Ok((l != 0 && r != 0) as i128),
                BinOp::LogicOr => Ok((l != 0 || r != 0) as i128),
                BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap => not_const(
                    "a wrapping operator",
                    "`+%`/`-%`/`*%` wrap at a bit width, and compile-time integers \
                     have no width — use plain `+`/`-`/`*` in constants",
                ),
            }
        }
        ExprKind::IfExpr { cond, then, els } => {
            if eval(cond, env)? != 0 {
                eval(then, env)
            } else {
                eval(els, env)
            }
        }
        ExprKind::Field { .. } => not_const(
            "an enum variant or instance port",
            "constants are plain `int`/`bool` values (spec/02 section 1.6)",
        ),
        _ => not_const(
            "this expression",
            "compile-time constants support literals, named consts, arithmetic, \
             comparisons, logic, and `if`/`else` (spec/02 section 1.6)",
        ),
    }
}

fn overflow(e: &Expr) -> Diag {
    Diag::new(e.span, "constant evaluation overflowed")
        .with_code("E0202")
        .with_help("compile-time arithmetic works on signed 128-bit values; this expression left that range")
}
