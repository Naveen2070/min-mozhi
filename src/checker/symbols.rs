//! Pass 1 — project symbol tables and project-wide duplicate detection.
//!
//! Builds `Checker::modules` and `Checker::enums` (both project-wide by
//! name) and reports E0001/E0002 duplicates. The per-module scope is
//! built later, in `names.rs` — this pass only handles names that must
//! be unique across the whole project (spec/02 section 1.5).

use crate::ast::{Builtin, TopItem};

use super::Checker;

impl<'a> Checker<'a> {
    pub(super) fn build_symbols(&mut self) {
        for (file, f) in self.files.iter().enumerate() {
            for item in &f.items {
                match item {
                    TopItem::Module(m) => {
                        if self.modules.contains_key(&m.name.name) {
                            self.err(
                                file,
                                m.name.span,
                                "E0001",
                                format!("module `{}` is defined more than once", m.name.name),
                                "module names are unique across the whole project \
                                 (spec/02 section 1.5) — rename one of them",
                            );
                        } else {
                            self.modules.insert(m.name.name.clone(), (file, m));
                        }
                    }
                    TopItem::Enum(e) => {
                        if self.enums.contains_key(&e.name.name) {
                            self.err(
                                file,
                                e.name.span,
                                "E0002",
                                format!("enum `{}` is defined more than once", e.name.name),
                                "file-level enums come into scope with `import`, so their \
                                 names must be unique across the project — rename one",
                            );
                        } else {
                            self.enums.insert(e.name.name.clone(), (file, e));
                        }
                    }
                    TopItem::Const(_) | TopItem::Test(_) => {} // consteval.rs / names.rs
                    TopItem::Error(_) => {}                    // parse-recovery placeholder
                    TopItem::Bundle(_) => {}                   // checker stub (T5)
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
