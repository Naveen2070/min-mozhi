//! Editor analysis over the AST: a symbol index plus offset→definition
//! resolution and completion candidates. PURE and async-free — the binary
//! `src/lsp.rs` adapts these to tower-lsp types, and the WASM playground can
//! reuse them later. All offsets are BYTES; UTF-16 conversion is the LSP
//! adapter's job.

use crate::ast::*;
use crate::project::LoadedFile;
use crate::span::Span;
use std::path::PathBuf;

/// What a [`Symbol`] is. Drives the hover label and completion item kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymKind {
    Module,
    Param,
    Port { dir: Dir },
    Clock,
    Reset,
    Wire,
    Reg,
    Mem,
    Const,
    Enum,
    EnumVariant,
    Inst,
}

/// One named definition, with where it lives and its hover render.
#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymKind,
    /// Index into [`SymbolIndex::files`].
    pub file_idx: usize,
    /// The defining identifier's span (byte offsets into that file's src).
    pub span: Span,
    /// Hover text, English in v1 (e.g. `out y: bits[8] — output port`).
    pub render: String,
    /// Enclosing module's index in [`SymbolIndex::symbols`]; `None` = file-level.
    pub module_idx: Option<usize>,
}

/// The project-wide definition table for one analysis run.
pub struct SymbolIndex {
    /// `(path, src)` per `file_idx`, for URIs and offset math.
    pub files: Vec<(PathBuf, String)>,
    pub symbols: Vec<Symbol>,
}

/// Render a hardware type back to source-like text for hover.
fn type_str(t: &Type) -> String {
    match t {
        Type::Bit => "bit".to_string(),
        Type::Bits(e) => format!("bits[{}]", expr_str(e)),
        Type::Signed(e) => format!("signed[{}]", expr_str(e)),
        Type::Named(id) => id.name.clone(),
    }
}

/// A minimal expression renderer for type widths in hover text (names and
/// integer literals cover the common `bits[WIDTH]` / `bits[8]` cases).
fn expr_str(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Ident(s) => s.clone(),
        ExprKind::Int { raw, .. } => raw.clone(),
        _ => "…".to_string(),
    }
}

/// Collect every definition across all files into one index.
pub fn build_index(files: &[LoadedFile]) -> SymbolIndex {
    let mut symbols = Vec::new();
    let file_meta = files
        .iter()
        .map(|f| (f.path.clone(), f.src.clone()))
        .collect();

    for (file_idx, f) in files.iter().enumerate() {
        for item in &f.ast.items {
            match item {
                TopItem::Const(c) => push_const(&mut symbols, c, file_idx, None),
                TopItem::Enum(e) => push_enum(&mut symbols, e, file_idx, None),
                TopItem::Module(m) => {
                    let module_pos = symbols.len();
                    symbols.push(Symbol {
                        name: m.name.name.clone(),
                        kind: SymKind::Module,
                        file_idx,
                        span: m.name.span,
                        render: format!("module {}", m.name.name),
                        module_idx: None,
                    });
                    let parent = Some(module_pos);
                    for p in &m.params {
                        symbols.push(Symbol {
                            name: p.name.name.clone(),
                            kind: SymKind::Param,
                            file_idx,
                            span: p.name.span,
                            render: format!("{}: int — parameter", p.name.name),
                            module_idx: parent,
                        });
                    }
                    push_module_items(&mut symbols, &m.items, file_idx, parent);
                }
                TopItem::Test(_) | TopItem::Error(_) => {}
            }
        }
    }

    SymbolIndex {
        files: file_meta,
        symbols,
    }
}

fn push_const(out: &mut Vec<Symbol>, c: &ConstDecl, file_idx: usize, module_idx: Option<usize>) {
    out.push(Symbol {
        name: c.name.name.clone(),
        kind: SymKind::Const,
        file_idx,
        span: c.name.span,
        render: format!("const {} — compile-time value", c.name.name),
        module_idx,
    });
}

fn push_enum(out: &mut Vec<Symbol>, e: &EnumDecl, file_idx: usize, module_idx: Option<usize>) {
    out.push(Symbol {
        name: e.name.name.clone(),
        kind: SymKind::Enum,
        file_idx,
        span: e.name.span,
        render: format!("enum {}", e.name.name),
        module_idx,
    });
    for v in &e.variants {
        out.push(Symbol {
            name: v.name.clone(),
            kind: SymKind::EnumVariant,
            file_idx,
            span: v.span,
            render: format!("{}.{} — enum variant", e.name.name, v.name),
            module_idx,
        });
    }
}

fn push_module_items(
    out: &mut Vec<Symbol>,
    items: &[ModuleItem],
    file_idx: usize,
    module_idx: Option<usize>,
) {
    for item in items {
        match item {
            ModuleItem::Port { dir, name, ty } => {
                let word = match dir {
                    Dir::In => "in",
                    Dir::Out => "out",
                };
                let dirword = match dir {
                    Dir::In => "input",
                    Dir::Out => "output",
                };
                out.push(Symbol {
                    name: name.name.clone(),
                    kind: SymKind::Port { dir: *dir },
                    file_idx,
                    span: name.span,
                    render: format!("{word} {}: {} — {dirword} port", name.name, type_str(ty)),
                    module_idx,
                });
            }
            ModuleItem::Clock(id) => {
                out.push(simple(id, SymKind::Clock, "clock", file_idx, module_idx))
            }
            ModuleItem::Reset { name, .. } => {
                out.push(simple(name, SymKind::Reset, "reset", file_idx, module_idx))
            }
            ModuleItem::Wire { name, ty, .. } => out.push(Symbol {
                name: name.name.clone(),
                kind: SymKind::Wire,
                file_idx,
                span: name.span,
                render: format!("wire {}: {}", name.name, type_str(ty)),
                module_idx,
            }),
            ModuleItem::Reg { name, ty, .. } => out.push(Symbol {
                name: name.name.clone(),
                kind: SymKind::Reg,
                file_idx,
                span: name.span,
                render: format!("reg {}: {}", name.name, type_str(ty)),
                module_idx,
            }),
            ModuleItem::Mem { name, ty, .. } => out.push(Symbol {
                name: name.name.clone(),
                kind: SymKind::Mem,
                file_idx,
                span: name.span,
                render: format!("mem {}: {}[]", name.name, type_str(ty)),
                module_idx,
            }),
            ModuleItem::Const(c) => push_const(out, c, file_idx, module_idx),
            ModuleItem::Enum(e) => push_enum(out, e, file_idx, module_idx),
            ModuleItem::Inst(inst) => out.push(Symbol {
                name: inst.name.name.clone(),
                kind: SymKind::Inst,
                file_idx,
                span: inst.name.span,
                render: format!(
                    "let {} = {}(…) — instance",
                    inst.name.name, inst.module.name
                ),
                module_idx,
            }),
            ModuleItem::Repeat(r) => push_module_items(out, &r.items, file_idx, module_idx),
            ModuleItem::On(_) | ModuleItem::Drive { .. } | ModuleItem::Error(_) => {}
        }
    }
}

fn simple(
    id: &Ident,
    kind: SymKind,
    word: &str,
    file_idx: usize,
    module_idx: Option<usize>,
) -> Symbol {
    Symbol {
        name: id.name.clone(),
        kind,
        file_idx,
        span: id.span,
        render: format!("{word} {}", id.name),
        module_idx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser, project::LoadedFile};
    use std::path::PathBuf;

    fn loaded(src: &str) -> Vec<LoadedFile> {
        let toks = lexer::lex(src).expect("lex");
        let (ast, _diags) = parser::parse_recover(toks);
        vec![LoadedFile {
            path: PathBuf::from("m.mimz"),
            src: src.to_string(),
            ast,
        }]
    }

    #[test]
    fn index_collects_each_definition_kind() {
        let src = "const N: int = 4\n\
                   enum State { Red, Green }\n\
                   module M(W: int = 8) {\n\
                     in a: bits[8]\n\
                     out y: bit\n\
                     clock clk\n\
                     reg r: bit = false\n\
                     y = a[0]\n\
                   }\n";
        let idx = build_index(&loaded(src));
        let names: Vec<&str> = idx.symbols.iter().map(|s| s.name.as_str()).collect();
        for want in ["N", "State", "Red", "Green", "M", "W", "a", "y", "clk", "r"] {
            assert!(
                names.contains(&want),
                "missing symbol `{want}` in {names:?}"
            );
        }
        // The port `y` carries a render string usable as hover text.
        let y = idx.symbols.iter().find(|s| s.name == "y").unwrap();
        assert!(matches!(y.kind, SymKind::Port { .. }));
        assert!(y.render.contains("out y: bit"), "got render {:?}", y.render);
    }
}
