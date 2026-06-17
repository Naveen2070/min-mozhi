//! `match` arm unification, pattern/type agreement (E0409), and the
//! exhaustiveness rules (E0601 missing coverage, E0602 unreachable
//! arms) — the v0.2.3 coverage rulings live here.

use std::collections::HashSet;

use crate::ast::Pattern;
use crate::span::Span;

use super::super::Checker;
use super::{Ty, Wcx, fits_bits, same, show};

impl<'a> Checker<'a> {
    /// `if`/`match` arms must agree. A compile-time arm adapts to a sized
    /// sibling; ALL-compile-time arms have no width to adopt in a
    /// width-free position.
    pub(super) fn unify_arms(
        &mut self,
        cx: &mut Wcx<'a>,
        whole: Span,
        arms: &[(Span, Ty<'a>)],
    ) -> Ty<'a> {
        let mut acc: Option<Ty<'a>> = None;
        for (_, t) in arms {
            if matches!(t, Ty::Unknown) {
                return Ty::Unknown;
            }
            if !matches!(t, Ty::CtInt(_)) {
                match &acc {
                    None => acc = Some(*t),
                    Some(prev) if same(prev, t) => {}
                    Some(prev) => {
                        self.err_args(
                            cx.file,
                            whole,
                            "E0408",
                            format!("the arms disagree: {} vs {}", show(prev), show(t)),
                            "every arm becomes the same wire, so all arms must \
                             have one type and width — `extend`/`trunc` the odd \
                             one out",
                            vec![("first", show(prev)), ("second", show(t))],
                        );
                        return Ty::Unknown;
                    }
                }
            }
        }
        let Some(result) = acc else {
            self.err(
                cx.file,
                whole,
                "E0405",
                "every arm is a bare literal, so this has no width",
                "use it where a width is known (an assignment or connection), \
                 or give one arm a width with `extend(value, N)`",
            );
            return Ty::Unknown;
        };
        // Now fit every compile-time arm against the agreed type.
        for (span, t) in arms {
            if let Ty::CtInt(v) = t {
                self.fit(cx, *span, *v, result);
            }
        }
        result
    }

    /// `match` patterns against the scrutinee's type (E0409), plus the
    /// exhaustiveness rules: every value covered (E0601) and no arm
    /// unreachable (E0602).
    ///
    /// Coverage decisions (spec/02 v0.2.3, dev log 2026-06-12): a match
    /// that names every enum variant is exhaustive WITHOUT `_` (Rust
    /// rule); a `_` arm after full coverage is NOT unreachable — it is
    /// the documented defense against non-enum encodings (bit flips), so
    /// only arms after a `_` arm and duplicate values are E0602.
    /// Exhaustiveness is skipped when a pattern already drew a type
    /// error (anti-cascade: one mistake, one diagnostic).
    pub(super) fn check_patterns(
        &mut self,
        cx: &mut Wcx<'a>,
        scrutinee: Span,
        st: Ty<'a>,
        arms: &'a [crate::ast::Arm],
    ) {
        match st {
            Ty::Unknown => {}
            Ty::Signed(_) => self.err(
                cx.file,
                scrutinee,
                "E0409",
                "cannot `match` on a `signed` value",
                "patterns cannot express negative numbers yet — match on \
                 `unsigned(x)` and compare signs separately",
            ),
            Ty::CtInt(_) => self.err(
                cx.file,
                scrutinee,
                "E0405",
                "`match` needs a sized value to scrutinize",
                "a bare compile-time value has no width — match on a signal, \
                 or decide with `if`/`else` at compile time",
            ),
            Ty::Clock | Ty::Reset => {
                let _ = self.not_data(cx, scrutinee, &st);
            }
            Ty::Bit | Ty::Bits(_) => {
                let n = if let Ty::Bits(n) = st { n } else { 1 };
                let mut bad = false;
                let mut seen: HashSet<u128> = HashSet::new();
                let mut wild = false;
                for arm in arms {
                    if wild {
                        self.unreachable_arm(cx, arm.value.span);
                        continue;
                    }
                    let mut arm_wild = false;
                    for p in &arm.patterns {
                        match p {
                            Pattern::Int { value, raw } => {
                                let v = i128::try_from(*value).unwrap_or(i128::MAX);
                                if !fits_bits(v, n) {
                                    bad = true;
                                    self.err(
                                        cx.file,
                                        arm.value.span,
                                        "E0409",
                                        format!("pattern `{raw}` does not fit in {n} bits"),
                                        format!(
                                            "the matched value is {} — it can never \
                                             equal `{raw}`, so this arm is dead",
                                            show(&st)
                                        ),
                                    );
                                } else if !seen.insert(*value) {
                                    self.covered_already(cx, arm.value.span, raw);
                                }
                            }
                            Pattern::IntMask { width, raw, .. } => {
                                // A don't-care pattern must match the value's
                                // width exactly. It covers a SET of values, so
                                // it earns NO exhaustiveness credit (not added
                                // to `seen`) — a `_` arm or full literal
                                // coverage is still required, and overlap
                                // between masked arms is not diagnosed (first
                                // match wins).
                                if u128::from(*width) != n {
                                    bad = true;
                                    self.err(
                                        cx.file,
                                        arm.value.span,
                                        "E0409",
                                        format!(
                                            "don't-care pattern `{raw}` is {width} bits, \
                                             but the matched value is {}",
                                            show(&st)
                                        ),
                                        "a `0b…?…` pattern must be exactly as wide as the \
                                         value it matches",
                                    );
                                }
                            }
                            Pattern::Bool(b) => {
                                if n != 1 {
                                    bad = true;
                                    self.err(
                                        cx.file,
                                        arm.value.span,
                                        "E0409",
                                        format!(
                                            "`true`/`false` patterns need a `bit`, not {}",
                                            show(&st)
                                        ),
                                        "match multi-bit values against numbers",
                                    );
                                } else if !seen.insert(u128::from(*b)) {
                                    self.covered_already(
                                        cx,
                                        arm.value.span,
                                        if *b { "true" } else { "false" },
                                    );
                                }
                            }
                            Pattern::Variant { enum_name, .. } => {
                                bad = true;
                                self.err(
                                    cx.file,
                                    enum_name.span,
                                    "E0409",
                                    format!("variant pattern on {}", show(&st)),
                                    "enum patterns match enum values — this scrutinee \
                                     is a plain vector, match numbers instead",
                                );
                            }
                            Pattern::Wildcard => arm_wild = true,
                        }
                    }
                    wild |= arm_wild;
                }
                if !bad && !wild {
                    // Every value of `bits[n]` must have an arm. Past
                    // u128 range the arm count alone proves nothing.
                    let total = if n < 128 { 1u128 << n } else { u128::MAX };
                    if (seen.len() as u128) < total {
                        // Smallest uncovered value: after sorting, the
                        // first index whose value differs is the gap.
                        let mut vals: Vec<u128> = seen.iter().copied().collect();
                        vals.sort_unstable();
                        let missing = vals
                            .iter()
                            .enumerate()
                            .find(|&(i, &v)| v != i as u128)
                            .map(|(i, _)| i as u128)
                            .unwrap_or(vals.len() as u128);
                        self.err_args(
                            cx.file,
                            scrutinee,
                            "E0601",
                            format!("`match` does not cover every value of {}", show(&st)),
                            format!(
                                "value `{missing}` has no arm (there may be more) — \
                                 add arms, or end with `_ =>` for the rest"
                            ),
                            vec![("type", show(&st))],
                        );
                    }
                }
            }
            Ty::Enum(en) => {
                let mut bad = false;
                let mut seen: HashSet<&str> = HashSet::new();
                let mut wild = false;
                for arm in arms {
                    if wild {
                        self.unreachable_arm(cx, arm.value.span);
                        continue;
                    }
                    let mut arm_wild = false;
                    for p in &arm.patterns {
                        match p {
                            Pattern::Variant { enum_name, variant } => {
                                if enum_name.name != en.name.name {
                                    bad = true;
                                    self.err(
                                        cx.file,
                                        enum_name.span,
                                        "E0409",
                                        format!(
                                            "pattern is from enum `{}`, but the value is \
                                             enum `{}`",
                                            enum_name.name, en.name.name
                                        ),
                                        format!("use `{}.<variant>` patterns here", en.name.name),
                                    );
                                } else if en.variants.iter().any(|v| v.name == variant.name) {
                                    if !seen.insert(variant.name.as_str()) {
                                        self.covered_already(
                                            cx,
                                            variant.span,
                                            &format!("{}.{}", en.name.name, variant.name),
                                        );
                                    }
                                } else {
                                    // Unknown variant: pass 3 already
                                    // reported E0103 — just disarm coverage.
                                    bad = true;
                                }
                            }
                            Pattern::Int { raw, .. } => {
                                bad = true;
                                self.err(
                                    cx.file,
                                    arm.value.span,
                                    "E0409",
                                    format!("number pattern `{raw}` on enum `{}`", en.name.name),
                                    "an enum's encoding is a compiler detail — match \
                                     its variants by name",
                                );
                            }
                            Pattern::IntMask { raw, .. } => {
                                bad = true;
                                self.err(
                                    cx.file,
                                    arm.value.span,
                                    "E0409",
                                    format!(
                                        "don't-care pattern `{raw}` on enum `{}`",
                                        en.name.name
                                    ),
                                    "an enum's encoding is a compiler detail — match \
                                     its variants by name",
                                );
                            }
                            Pattern::Bool(_) => {
                                bad = true;
                                self.err(
                                    cx.file,
                                    arm.value.span,
                                    "E0409",
                                    format!("`true`/`false` pattern on enum `{}`", en.name.name),
                                    "match the enum's variants by name",
                                );
                            }
                            Pattern::Wildcard => arm_wild = true,
                        }
                    }
                    wild |= arm_wild;
                }
                if !bad && !wild {
                    let missing: Vec<&str> = en
                        .variants
                        .iter()
                        .filter(|v| !seen.contains(v.name.as_str()))
                        .map(|v| v.name.as_str())
                        .collect();
                    if !missing.is_empty() {
                        self.err(
                            cx.file,
                            scrutinee,
                            "E0601",
                            format!(
                                "`match` on enum `{}` is missing `{}`",
                                en.name.name,
                                missing.join("`, `")
                            ),
                            "every variant needs an arm, or end with `_ =>` for \
                             the rest (a `_` arm also catches invalid encodings \
                             after a bit flip)",
                        );
                    }
                }
            }
        }
    }

    /// E0602 — an arm after `_`, in one shared voice.
    fn unreachable_arm(&mut self, cx: &mut Wcx<'a>, span: Span) {
        self.err(
            cx.file,
            span,
            "E0602",
            "this arm is unreachable",
            "a `_` arm above already matches everything — move `_` last, \
             or delete this arm",
        );
    }

    /// E0602 — a duplicate pattern value, in one shared voice.
    fn covered_already(&mut self, cx: &mut Wcx<'a>, span: Span, what: &str) {
        self.err(
            cx.file,
            span,
            "E0602",
            format!("pattern `{what}` is already covered"),
            "an earlier arm matches this value first, so this one can \
             never fire — delete the duplicate",
        );
    }
}
