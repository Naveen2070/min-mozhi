//! Pass 1 — project symbol tables and per-file duplicate detection.
//!
//! Builds `Checker::modules`, `Checker::enums`, `Checker::bundles`, and
//! `Checker::externs` as multimaps (name -> every declaring file/node) and
//! reports E0001/E0002/E0909/E1301 for a name reused within the SAME file.
//! Reusing a name across different files is legal here — it is resolved
//! (or flagged ambiguous)
//! at use-site by qualifying the reference (spec/02 section 1.5b), which
//! is `names.rs`'s job, not this pass's. `funcs` stays project-wide unique
//! (D-PKG-1) and is unaffected by this pass's per-file relaxation.

use crate::ast::{Builtin, TopItem};

use super::Checker;

impl<'a> Checker<'a> {
    pub(super) fn build_symbols(&mut self) {
        for (file, f) in self.files.iter().enumerate() {
            for item in &f.items {
                match item {
                    TopItem::Module(m) => {
                        let entry = self.modules.entry(m.name.name.clone()).or_default();
                        if entry.iter().any(|&(f, _)| f == file) {
                            self.err(
                                file,
                                m.name.span,
                                "E0001",
                                format!(
                                    "module `{}` is defined more than once in this file",
                                    m.name.name
                                ),
                                "module names are unique within one file — rename one of \
                                 them (a different file may reuse this name; qualify the \
                                 reference with the import path if it becomes ambiguous, \
                                 spec/02 section 1.5b)",
                            );
                        } else {
                            entry.push((file, m));
                        }
                    }
                    TopItem::Enum(e) => {
                        let entry = self.enums.entry(e.name.name.clone()).or_default();
                        if entry.iter().any(|&(f, _)| f == file) {
                            self.err(
                                file,
                                e.name.span,
                                "E0002",
                                format!(
                                    "enum `{}` is defined more than once in this file",
                                    e.name.name
                                ),
                                "file-level enums come into scope with `import`, so their \
                                 names must be unique within one file — rename one of them \
                                 (a different file may reuse this name; qualify the \
                                 reference with the import path if it becomes ambiguous, \
                                 spec/02 section 1.5b)",
                            );
                        } else {
                            entry.push((file, e));
                        }
                    }
                    TopItem::Const(_) | TopItem::Test(_) => {} // consteval.rs / names.rs
                    TopItem::Error(_) => {}                    // parse-recovery placeholder
                    TopItem::ExternModule(em) => {
                        let entry = self.externs.entry(em.name.name.clone()).or_default();
                        if entry.iter().any(|&(f, _)| f == file) {
                            self.err(
                                file,
                                em.name.span,
                                "E1301",
                                format!(
                                    "extern module `{}` is defined more than once in this file",
                                    em.name.name
                                ),
                                "extern module names are unique within one file — rename one \
                                 of them (a different file may reuse this name; qualify the \
                                 reference with the import path if it becomes ambiguous, \
                                 spec/02 section 1.5b)",
                            );
                        } else {
                            entry.push((file, em));
                        }
                    }
                    TopItem::Bundle(b) => {
                        let entry = self.bundles.entry(b.name.name.clone()).or_default();
                        if entry.iter().any(|&(f, _)| f == file) {
                            self.err(
                                file,
                                b.name.span,
                                "E0909",
                                format!(
                                    "bundle `{}` is defined more than once in this file",
                                    b.name.name
                                ),
                                "bundle names are unique within one file — rename one of \
                                 them (a different file may reuse this name; qualify the \
                                 reference with the import path if it becomes ambiguous, \
                                 spec/02 section 1.5b)",
                            );
                        } else {
                            entry.push((file, b));
                        }
                    }
                    TopItem::Func(f) => {
                        let name = &f.name.name;
                        if Builtin::from_name(name.as_str()).is_some() {
                            self.err(
                                file,
                                f.name.span,
                                "E0802",
                                format!("`{name}` is a builtin — choose another function name"),
                                "builtin names are reserved by the language and cannot be \
                                 redefined — pick a different name for your function",
                            );
                        } else if self.funcs.contains_key(name) {
                            self.err(
                                file,
                                f.name.span,
                                "E0801",
                                format!("function `{name}` is defined more than once"),
                                "function names are unique across the whole project — rename one",
                            );
                        } else {
                            self.funcs.insert(name.clone(), (file, f));
                        }
                    }
                }
            }
        }
    }
}
