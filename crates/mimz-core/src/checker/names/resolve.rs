use super::*;

impl<'a> Checker<'a> {
    /// Enum lookup: module scope first, then file-level enums project-wide.
    pub(in crate::checker) fn lookup_enum(
        &self,
        sc: &Scope<'a>,
        name: &str,
    ) -> Option<&'a EnumDecl> {
        if let Some(Bind::Enum(e)) = sc.names.get(name).copied() {
            return Some(e);
        }
        self.enums
            .get(name)
            .and_then(|v| v.first())
            .map(|&(_, e)| e)
    }

    /// Resolve a possibly-namespaced reference against the caller's
    /// already-looked-up candidate bucket (`table.get(&q.name.name).cloned()`
    /// — cloning just the one bucket, not the whole project-wide multimap,
    /// sidesteps the borrow conflict between holding a `&self.modules`
    /// borrow and calling `self.err`/`unknown` below, which need `&mut
    /// self`). `unknown` is called (and its diagnostic emitted) when there
    /// are 0 candidates — same behavior/codes as before this feature.
    /// Returns `None` on 0, ambiguous-bare, or unmatched-qualifier; `Some` on
    /// exactly 1 candidate or a qualifier that matches exactly one.
    pub(super) fn resolve<'b, T>(
        &mut self,
        file: usize,
        candidates: Option<Vec<(usize, &'b T)>>,
        q: &'b crate::ast::QualIdent,
        unknown: impl FnOnce(&mut Self),
    ) -> Option<&'b T> {
        let Some(candidates) = candidates else {
            unknown(self);
            return None;
        };
        if q.is_bare() {
            match candidates.as_slice() {
                [] => unreachable!(
                    "empty Vec is never inserted — symbols.rs always pushes at least one"
                ),
                [(f, only)] => {
                    q.resolved_file.set(Some(*f));
                    Some(*only)
                }
                _ => {
                    let files: Vec<String> = candidates
                        .iter()
                        .map(|&(f, _)| format!("file {f}"))
                        .collect();
                    self.err(
                        file,
                        q.span,
                        "E0110",
                        format!(
                            "`{}` is ambiguous — declared in {} different files",
                            q.name.name,
                            candidates.len()
                        ),
                        format!(
                            "qualify with the import path to pick one, e.g. `a.b.{}` \
                             (candidates: {})",
                            q.name.name,
                            files.join(", ")
                        ),
                    );
                    None
                }
            }
        } else {
            // The actual disambiguation mechanism (spec/02 section 1.5b,
            // design doc §4.4): match this reference's `.path` against THIS
            // file's own `import` statements, caching the target file onto
            // `q.resolved_file` (a `Cell`) so every later pass that reads
            // the same Cell — `drivers.rs`, `widths/*.rs`, and
            // `emit_verilog::Project` (which runs on these SAME `ast::File`/
            // `QualIdent` instances after the checker, per
            // `commands/compile.rs`) — gets the answer for free.
            q.resolve_via_imports(&self.files[file].imports);
            let Some(target_file) = q.resolved_file.get() else {
                self.err(
                    file,
                    q.span,
                    "E0111",
                    format!(
                        "the path in `{}` doesn't match any `import` in this file",
                        q.to_dotted()
                    ),
                    "check the import path segments, or drop the qualifier if \
                     the bare name is unambiguous",
                );
                return None;
            };
            match candidates.iter().find(|&&(f, _)| f == target_file) {
                Some(&(_, t)) => Some(t),
                None => {
                    self.err(
                        file,
                        q.span,
                        "E0111",
                        format!(
                            "the file imported as `{}` doesn't declare `{}`",
                            q.path
                                .iter()
                                .map(|s| s.name.as_str())
                                .collect::<Vec<_>>()
                                .join("."),
                            q.name.name
                        ),
                        "the import resolves to a real file, but that file has no \
                         declaration by this name — check the spelling, or declare \
                         it there",
                    );
                    None
                }
            }
        }
    }
}
