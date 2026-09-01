use super::*;
use crate::diag::Diag;

pub(super) fn build_registry(files: &[ast::File]) -> Registry<'_> {
    let mut reg: Registry<'_> = HashMap::new();
    for (file_idx, f) in files.iter().enumerate() {
        for it in &f.items {
            if let ast::TopItem::Module(m) = it {
                reg.entry(m.name.name.clone())
                    .or_default()
                    .push((file_idx, f, m));
            }
        }
    }
    reg
}

/// Resolve an `Inst`'s target module reference against `reg`. Mirrors
/// `emit_verilog::Project::resolve_module`'s bare/qualified logic — but,
/// unlike the emitter (which only ever runs after the checker has already
/// rejected an ambiguous bare reference as E0110), `mimz sim`/`mimz test`
/// elaborate the raw parse tree directly (see the module doc comment): an
/// ambiguous reference is a real, reachable outcome here, so it gets its
/// own error instead of emit_verilog's "unreachable in practice, checker
/// already rejected it" `None`.
///
/// `Box<Diag>` (not a bare `Diag`): `Diag` is 128+ bytes (spans, an owned
/// `msg: String`, `Vec<(&str, String)>` args), so `Result<T, Diag>` trips
/// clippy's `result_large_err` on every one of these hot, frequently-`?`'d
/// resolution functions — `Diag` itself is fine staying an owned value for
/// the checker's own `Vec<Diag>` accumulation style, but a `?`-propagated
/// single-error return type wants a pointer-sized `Err`.
pub(super) fn resolve_module<'a>(
    reg: &Registry<'a>,
    imports: &[ast::Import],
    q: &ast::QualIdent,
) -> Result<(&'a ast::File, &'a ast::Module), Box<Diag>> {
    // BUG-26 (fixed): this function's only caller, `resolve_target`, always
    // checks `reg.contains_key` before ever calling here — a miss can never
    // reach this point, so there is no meaningful diagnostic to construct
    // for it (the retired `S0101` code covered exactly this dead arm).
    let candidates = reg
        .get(&q.name.name)
        .expect("resolve_target already confirmed this name is in `reg`");
    if q.is_bare() {
        match candidates.as_slice() {
            [(_, f, m)] => Ok((f, m)),
            [] => unreachable!("empty Vec is never inserted"),
            _ => Err(Box::new(
                Diag::new(
                    q.span,
                    format!(
                        "uses module `{}`, which is ambiguous — declared in {} different \
                         files; qualify with the import path to pick one (e.g. `a.b.{}`)",
                        q.name.name,
                        candidates.len(),
                        q.name.name
                    ),
                )
                .with_code("S0102"),
            )),
        }
    } else {
        // `mimz sim`/`mimz test` never run the checker (module doc comment),
        // so — unlike `emit_verilog`, which can rely on the checker having
        // already populated `q.resolved_file` — this match against the
        // referencing file's own `import` statements must be computed here
        // too. Mirrors `checker::names::resolve`'s identical step.
        q.resolve_via_imports(imports);
        let target = q.resolved_file.get().ok_or_else(|| {
            Box::new(
                Diag::new(
                    q.span,
                    format!(
                        "the path in `{}` doesn't match any `import` in this file",
                        q.to_dotted()
                    ),
                )
                .with_code("S0103"),
            )
        })?;
        candidates
            .iter()
            .find(|&&(f, _, _)| f == target)
            .map(|&(_, f, m)| (f, m))
            .ok_or_else(|| {
                Box::new(
                    Diag::new(q.span, format!("uses unknown module `{}`", q.name.name))
                        .with_code("S0104"),
                )
            })
    }
}

/// Extern-module registry across all loaded files: every `(file_idx, decl)`
/// declaring a given name — mirrors [`Registry`]. Resolved (alongside
/// `Registry`) via [`resolve_target`].
pub(super) type ExternRegistry<'a> = HashMap<String, Vec<(usize, &'a ast::ExternModule)>>;

pub(super) fn build_extern_registry(files: &[ast::File]) -> ExternRegistry<'_> {
    let mut reg: ExternRegistry<'_> = HashMap::new();
    for (file_idx, f) in files.iter().enumerate() {
        for item in &f.items {
            if let ast::TopItem::ExternModule(em) = item {
                reg.entry(em.name.name.clone())
                    .or_default()
                    .push((file_idx, em));
            }
        }
    }
    reg
}

/// Resolve an `Inst`'s target against both real modules and extern
/// declarations — mirrors `emit_verilog::Project::resolve_target_with_file`'s
/// modules-then-externs fallback (real modules win on a same-name clash,
/// same defensive tie-break). A real module's declaring `&File` is returned
/// alongside it, since `flatten_instance` needs it to recursively elaborate
/// the child (see its `elaborate_module(reg, func_reg, bundle_reg, enum_reg,
/// cfile, cm, ...)` call). An extern declaration has no body to elaborate, so no
/// file is needed for it — confirmed by reading how `resolve_module`'s
/// `&'a ast::File` half is actually consumed at that call site: only the
/// `ModuleTarget::Real` case ever reads it.
pub(super) fn resolve_target<'a>(
    reg: &Registry<'a>,
    extern_reg: &ExternRegistry<'a>,
    imports: &[ast::Import],
    q: &ast::QualIdent,
) -> Result<(Option<&'a ast::File>, ast::ModuleTarget<'a>), Box<Diag>> {
    if reg.contains_key(&q.name.name) {
        let (f, m) = resolve_module(reg, imports, q)?;
        return Ok((Some(f), ast::ModuleTarget::Real(m)));
    }
    let candidates = extern_reg.get(&q.name.name).ok_or_else(|| {
        Box::new(
            Diag::new(
                q.span,
                format!(
                    "uses unknown module `{}` — is the file that defines it imported?",
                    q.name.name
                ),
            )
            .with_code("S0105"),
        )
    })?;
    if q.is_bare() {
        match candidates.as_slice() {
            [(_, em)] => Ok((None, ast::ModuleTarget::Extern(em))),
            [] => unreachable!("empty Vec is never inserted"),
            _ => Err(Box::new(
                Diag::new(
                    q.span,
                    format!(
                        "uses extern module `{}`, which is ambiguous — declared in {} \
                         different files; qualify with the import path to pick one",
                        q.name.name,
                        candidates.len()
                    ),
                )
                .with_code("S0102"),
            )),
        }
    } else {
        q.resolve_via_imports(imports);
        let target_file = q.resolved_file.get().ok_or_else(|| {
            Box::new(
                Diag::new(
                    q.span,
                    format!(
                        "the path in `{}` doesn't match any `import` in this file",
                        q.to_dotted()
                    ),
                )
                .with_code("S0103"),
            )
        })?;
        candidates
            .iter()
            .find(|&&(f, _)| f == target_file)
            .map(|&(_, em)| (None, ast::ModuleTarget::Extern(em)))
            .ok_or_else(|| {
                Box::new(
                    Diag::new(
                        q.span,
                        format!("uses unknown extern module `{}`", q.name.name),
                    )
                    .with_code("S0104"),
                )
            })
    }
}

/// Bundle registry across all loaded files: every `(file_idx, decl)`
/// declaring a given name — a multimap, mirroring [`Registry`] (bundles
/// have the same per-file-unique, project-wide-reusable scoping as modules,
/// unlike enums — see spec/02 section 1.5b). Used by the elaboration pass
/// to flatten bundle-typed ports/wires to N scalar signals
/// `signame_fieldname`; resolved via [`resolve_bundle`].
pub(super) type BundleRegistry<'a> = HashMap<String, Vec<(usize, &'a ast::BundleDecl)>>;

pub(super) fn build_bundle_registry(files: &[ast::File]) -> BundleRegistry<'_> {
    let mut reg: BundleRegistry<'_> = HashMap::new();
    for (file_idx, f) in files.iter().enumerate() {
        for it in &f.items {
            if let ast::TopItem::Bundle(b) = it {
                reg.entry(b.name.name.clone())
                    .or_default()
                    .push((file_idx, b));
            }
        }
    }
    // Mirror the checker's/emitter's synthesized `__Valid`/`__ValidSigned`
    // builtin bundles (`ast::builtin_valid_bundles`) so a `bit?`/`bits[N]?`/
    // `signed[N]?`-typed signal resolves here too — the sim elaborates the
    // raw parsed AST without the checker pass that normally registers these
    // (see this module's doc comment), so without this they'd be an
    // "unknown bundle `__Valid`" error the moment `?`-sugar reaches a wire.
    // `files.len()` — one past every real file index — matches the
    // checker/emitter convention (see `builtin_valid_bundles`'s doc comment).
    let builtin_file = files.len();
    for decl in ast::builtin_valid_bundles() {
        reg.entry(decl.name.name.clone())
            .or_default()
            .push((builtin_file, decl));
    }
    reg
}

/// Resolve a bundle-typed reference against `bundles`. Mirrors
/// [`resolve_module`] — same ambiguous-bare-reference-is-a-real-error
/// reasoning applies (the sim has no checker pass gating this).
pub(super) fn resolve_bundle<'a>(
    bundles: &BundleRegistry<'a>,
    imports: &[ast::Import],
    q: &ast::QualIdent,
) -> Result<&'a ast::BundleDecl, Box<Diag>> {
    let candidates = bundles.get(&q.name.name).ok_or_else(|| {
        Box::new(Diag::new(q.span, format!("unknown bundle `{}`", q.name.name)).with_code("S0106"))
    })?;
    if q.is_bare() {
        match candidates.as_slice() {
            [(_, only)] => Ok(*only),
            [] => unreachable!("empty Vec is never inserted"),
            _ => Err(Box::new(
                Diag::new(
                    q.span,
                    format!(
                        "bundle `{}` is ambiguous — declared in {} different files; \
                         qualify with the import path to pick one (e.g. `a.b.{}`)",
                        q.name.name,
                        candidates.len(),
                        q.name.name
                    ),
                )
                .with_code("S0102"),
            )),
        }
    } else {
        // Same "no checker pass gating this" reasoning as `resolve_module`.
        q.resolve_via_imports(imports);
        let target = q.resolved_file.get().ok_or_else(|| {
            Box::new(
                Diag::new(
                    q.span,
                    format!(
                        "the path in `{}` doesn't match any `import` in this file",
                        q.to_dotted()
                    ),
                )
                .with_code("S0103"),
            )
        })?;
        candidates
            .iter()
            .find(|&&(f, _)| f == target)
            .map(|&(_, b)| b)
            .ok_or_else(|| {
                Box::new(
                    Diag::new(q.span, format!("unknown bundle `{}`", q.name.name))
                        .with_code("S0104"),
                )
            })
    }
}

/// Resolve a bundle type to `(field_name, Width)` pairs, substituting any
/// bundle parameters from `args` and folding width expressions against `consts`.
/// Mirrors the emitter's `resolve_bundle_fields` but returns concrete `Width`s
/// for the sim rather than AST `Type`s for code generation.
pub(super) fn resolve_bundle_fields_sim(
    bundles: &BundleRegistry<'_>,
    imports: &[ast::Import],
    bname: &ast::QualIdent,
    args: &[NamedArg],
    consts: &BTreeMap<String, i128>,
) -> Result<Vec<(String, Width)>, Box<Diag>> {
    let bdecl = resolve_bundle(bundles, imports, bname)?;
    // Build a merged const env: module consts + bundle param defaults + call-site overrides.
    let mut merged = consts.clone();
    for p in &bdecl.params {
        if let Some(default) = &p.default
            && let Ok(v) = const_eval(default, &merged)
        {
            merged.insert(p.name.name.clone(), v);
        }
    }
    for a in args {
        if let Ok(v) = const_eval(&a.value, &merged) {
            merged.insert(a.name.name.clone(), v);
        }
    }
    let bname_str = &bname.name.name;
    bdecl
        .fields
        .iter()
        .map(|f| {
            let (bits, signed) = type_width(&f.ty, &merged, f.span).map_err(|mut e| {
                e.msg = format!("bundle `{bname_str}` field `{}`: {}", f.name.name, e.msg);
                e
            })?;
            Ok((f.name.name.clone(), Width { bits, signed }))
        })
        .collect()
}

/// File-level enum registry across all loaded files: name → AST declaration.
/// Enums declared at file scope (`enum Name { ... }`, spec/02 §1.5b) are
/// visible to every module in that file, same as bundles/modules — mirrors
/// [`build_func_registry`]. `elaborate_module` merges this with any enum
/// declared *inside* the module body (`ModuleItem::Enum`, still supported),
/// module-local taking priority on a name clash. Not a full per-file
/// multimap like [`BundleRegistry`]/the checker's own enum table (no
/// `a.b.Name` qualifier resolution here) — a checker-clean program (gated
/// before every sim path, A2) never reaches sim with a genuine cross-file
/// enum-name ambiguity, so last-declaration-wins on a name collision here is
/// unreachable in practice, not a silent-miscompute risk.
pub(super) type EnumRegistry<'a> = HashMap<String, &'a ast::EnumDecl>;

pub(super) fn build_enum_registry(files: &[ast::File]) -> EnumRegistry<'_> {
    let mut reg = HashMap::new();
    for f in files {
        for it in &f.items {
            if let ast::TopItem::Enum(e) = it {
                reg.insert(e.name.name.clone(), e);
            }
        }
    }
    reg
}

/// Function registry across all loaded files: name → AST declaration. Functions
/// are project-wide (D3 — a fn declared in any imported file is callable from
/// any module in any file), so the simulator collects them from the whole set.
pub(super) type FuncRegistry<'a> = HashMap<String, &'a FuncDecl>;

pub(super) fn build_func_registry(files: &[ast::File]) -> FuncRegistry<'_> {
    let mut reg = HashMap::new();
    for f in files {
        for it in &f.items {
            if let ast::TopItem::Func(func) = it {
                reg.insert(func.name.name.clone(), func);
            }
        }
    }
    reg
}
