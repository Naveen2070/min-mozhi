use super::*;

impl<'a> Checker<'a> {
    /// Resolve a bundle's fields to `(name, Ty)` pairs under the given args.
    /// `bfile_hint` is the bundle type's own `QualIdent::resolved_file`
    /// (set by names.rs pass 3) — the exact candidate when it names one,
    /// else the sole candidate (bare-and-unambiguous; a `None` hint here
    /// only ever means an already-reported ambiguous/unknown reference).
    /// Returns `None` and emits E0906 if a required param has no value.
    pub(super) fn resolve_bundle_fields(
        &mut self,
        cx: &Wcx<'a>,
        bname: &str,
        bfile_hint: Option<usize>,
        bargs: &[NamedArg],
        span: Span,
    ) -> Option<Vec<(String, Ty<'a>)>> {
        let candidates = self.bundles.get(bname)?;
        let &(bfile, bdecl) = candidates
            .iter()
            .find(|&&(f, _)| Some(f) == bfile_hint)
            .or_else(|| candidates.first())?;
        let mut benv = self.file_consts[bfile].clone();
        for param in &bdecl.params {
            let arg = bargs.iter().find(|a| a.name.name == param.name.name);
            if let Some(a) = arg {
                if let Ok(v) = consteval::eval(&a.value, &cx.env) {
                    benv.insert(param.name.name.clone(), v);
                }
            } else if let Some(def) = &param.default {
                match consteval::eval(def, &benv) {
                    Ok(v) => {
                        benv.insert(param.name.name.clone(), v);
                    }
                    Err(d) => {
                        self.diags.push(d.with_file(bfile));
                        return None;
                    }
                }
            } else {
                self.err(
                    bfile,
                    span,
                    "E0906",
                    format!("bundle `{bname}` param `{}` has no value", param.name.name),
                    "provide the value: `BundleName(PARAM: value)`",
                );
                return None;
            }
        }
        // Field types must be self-contained, so the outer scope is
        // deliberately excluded here, not carried over by omission.
        let mut tmp = Wcx {
            file: bfile,
            sc: Rc::new(Scope {
                names: HashMap::new(),
            }),
            env: benv,
            sigs: HashMap::new(),
        };
        let fields = bdecl
            .fields
            .iter()
            .map(|f| (f.name.name.clone(), self.resolve_ty(&mut tmp, &f.ty)))
            .collect();
        Some(fields)
    }

    /// Structurally compares two bundle types: every field `required`
    /// declares must exist in `provided` with an exactly-matching type.
    /// Both arguments MUST be `Ty::Bundle` — callers guard this before
    /// calling. Resolves each side's field list via `resolve_bundle_fields`
    /// (which may itself emit E0906 for an unresolvable bundle param); if
    /// either side fails to resolve, this returns `Compatible` rather than
    /// reporting a second diagnostic for the same root cause — the same
    /// "don't pile on" convention `Ty::Unknown` already follows elsewhere
    /// in this pass.
    pub(super) fn bundle_shape_match(
        &mut self,
        cx: &Wcx<'a>,
        required: Ty<'a>,
        provided: Ty<'a>,
        span: Span,
    ) -> BundleShapeMatch {
        let (
            Ty::Bundle {
                name: rname,
                bfile_hint: rhint,
                args: rargs,
            },
            Ty::Bundle {
                name: pname,
                bfile_hint: phint,
                args: pargs,
            },
        ) = (required, provided)
        else {
            unreachable!("bundle_shape_match called with a non-Bundle Ty");
        };
        let Some(rfields) = self.resolve_bundle_fields(cx, rname, rhint, rargs, span) else {
            return BundleShapeMatch::Compatible;
        };
        let Some(pfields) = self.resolve_bundle_fields(cx, pname, phint, pargs, span) else {
            return BundleShapeMatch::Compatible;
        };
        for (fname, fty) in &rfields {
            match pfields.iter().find(|(n, _)| n == fname) {
                None => return BundleShapeMatch::MissingField(fname.clone()),
                Some((_, pty)) => {
                    if !same(fty, pty) {
                        return BundleShapeMatch::FieldTypeMismatch {
                            field: fname.clone(),
                            expected: show(fty),
                            got: show(pty),
                        };
                    }
                }
            }
        }
        BundleShapeMatch::Compatible
    }

    /// Field-by-field check of a bundle literal against its declared type.
    /// Emits E0901 (missing field) and E0902 (unknown field), then checks
    /// each supplied field value's width against the declared field type.
    pub(super) fn check_bundle_lit(
        &mut self,
        cx: &mut Wcx<'a>,
        bname: &str,
        bfile_hint: Option<usize>,
        bargs: &[NamedArg],
        inits: &'a [FieldInit],
        span: Span,
    ) {
        let fields = match self.resolve_bundle_fields(cx, bname, bfile_hint, bargs, span) {
            Some(f) => f,
            None => {
                // Bundle lookup failed; recurse anyway to surface inner errors.
                for init in inits {
                    let _ = self.infer_ty(cx, &init.value);
                }
                return;
            }
        };
        // E0902: literal provides a field that the bundle doesn't declare.
        for init in inits {
            if !fields.iter().any(|(n, _)| *n == init.name.name) {
                self.err(
                    cx.file,
                    init.name.span,
                    "E0902",
                    format!("bundle `{bname}` has no field `{}`", init.name.name),
                    "check the bundle declaration for the correct field names",
                );
            }
        }
        // E0901: bundle declares a field the literal omits; type-check present fields.
        for (fname, fty) in &fields {
            if let Some(init) = inits.iter().find(|i| i.name.name == *fname) {
                self.check_expr(cx, &init.value, fty.clone());
            } else {
                self.err(
                    cx.file,
                    span,
                    "E0901",
                    format!("bundle literal missing field `{fname}`"),
                    format!("add `{fname}: <expr>` to the literal"),
                );
            }
        }
    }
}
