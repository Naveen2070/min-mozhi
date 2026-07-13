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
    /// A `module Name { ... }` declaration.
    Module,
    /// A module parameter (compile-time `int`/`bool`).
    Param,
    /// A port; `dir` says input or output.
    Port {
        /// Input or output.
        dir: Dir,
    },
    /// A `clock` declaration.
    Clock,
    /// A `reset` declaration.
    Reset,
    /// A `wire` declaration.
    Wire,
    /// A `reg` declaration.
    Reg,
    /// A `mem` declaration.
    Mem,
    /// A file- or module-level `const`.
    Const,
    /// An `enum` type declaration.
    Enum,
    /// One variant of an `enum`.
    EnumVariant,
    /// A `let name = Module(...) { ... }` instance.
    Inst,
}

/// One named definition, with where it lives and its hover render.
#[derive(Clone, Debug)]
pub struct Symbol {
    /// The symbol's name, as written in source.
    pub name: String,
    /// What kind of definition this is.
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
    /// Every definition found across all `files`, in discovery order.
    pub symbols: Vec<Symbol>,
}

/// Render a hardware type back to source-like text for hover.
fn type_str(t: &Type) -> String {
    match t {
        Type::Bit => "bit".to_string(),
        Type::Bits(e) => format!("bits[{}]", expr_str(e)),
        Type::Signed(e) => format!("signed[{}]", expr_str(e)),
        Type::Named(id) => id.to_dotted(),
        Type::Bundle { name, args } => {
            if args.is_empty() {
                name.to_dotted()
            } else {
                format!("{}(…)", name.to_dotted())
            }
        }
        Type::Array { elem, len } => format!("{}[{}]", type_str(elem), expr_str(len)),
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
                TopItem::Func(_) => {} // fn declarations not yet indexed for LSP
                TopItem::Bundle(_) => {} // bundle declarations not yet indexed for LSP
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
            name: v.name.name.clone(),
            kind: SymKind::EnumVariant,
            file_idx,
            span: v.span,
            render: format!("{}.{} — enum variant", e.name.name, v.name.name),
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
                    inst.name.name,
                    inst.module.to_dotted()
                ),
                module_idx,
            }),
            ModuleItem::Repeat(r) => push_module_items(out, &r.items, file_idx, module_idx),
            ModuleItem::ForEach(fe) => push_module_items(out, &fe.items, file_idx, module_idx),
            ModuleItem::ConstIf { then, els, .. } => {
                push_module_items(out, then, file_idx, module_idx);
                if let Some(el) = els {
                    push_module_items(out, el, file_idx, module_idx);
                }
            }
            ModuleItem::On(_)
            | ModuleItem::Drive { .. }
            | ModuleItem::Error(_)
            | ModuleItem::SyncLoop(_) => {} // body is Vec<SeqStmt>, not more ModuleItems
            ModuleItem::BundleDestructure { .. } => {} // not yet indexed for LSP
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

/// A name written somewhere in a file (a definition's name OR a use site),
/// with its span and the module it sits in (for scope-priority lookup).
struct Ref {
    name: String,
    span: Span,
    module_idx: Option<usize>,
}

/// Resolve the cursor at `offset` in file `file_idx` to a definition.
/// Finds the smallest name span covering the cursor, then looks that name up
/// in the index: enclosing module first, then file-level, then any module
/// (cross-file). Returns an index into `index.symbols`.
pub fn resolve_at(
    index: &SymbolIndex,
    files: &[LoadedFile],
    file_idx: usize,
    offset: usize,
) -> Option<usize> {
    let refs = collect_refs(index, files, file_idx);
    // Smallest span that covers the offset wins (handles nested names).
    let hit = refs
        .iter()
        .filter(|r| r.span.start <= offset && offset < r.span.end)
        .min_by_key(|r| r.span.end - r.span.start)?;

    // Scope priority: same module → same module any file (test blocks point
    // their body refs at a possibly-cross-file module-under-test) → file-level
    // → any definition.
    let same_module = index.symbols.iter().position(|s| {
        s.name == hit.name && s.file_idx == file_idx && s.module_idx == hit.module_idx
    });
    let same_module_any_file = || {
        hit.module_idx.and_then(|_| {
            index
                .symbols
                .iter()
                .position(|s| s.name == hit.name && s.module_idx == hit.module_idx)
        })
    };
    let file_level = || {
        index
            .symbols
            .iter()
            .position(|s| s.name == hit.name && s.file_idx == file_idx && s.module_idx.is_none())
    };
    let anywhere = || index.symbols.iter().position(|s| s.name == hit.name);
    same_module
        .or_else(same_module_any_file)
        .or_else(file_level)
        .or_else(anywhere)
}

/// Every name (definition or use) written in `file_idx`, with its module.
fn collect_refs(index: &SymbolIndex, files: &[LoadedFile], file_idx: usize) -> Vec<Ref> {
    let mut refs = Vec::new();
    for s in &index.symbols {
        if s.file_idx == file_idx {
            refs.push(Ref {
                name: s.name.clone(),
                span: s.span,
                module_idx: s.module_idx,
            });
        }
    }
    let f = &files[file_idx];
    for item in &f.ast.items {
        match item {
            TopItem::Module(m) => {
                // The module's symbol position == its module_idx for its members.
                let mod_pos = index.symbols.iter().position(|s| {
                    s.file_idx == file_idx
                        && s.kind == SymKind::Module
                        && s.span.start == m.name.span.start
                });
                for mi in &m.items {
                    collect_item_refs(mi, mod_pos, &mut refs);
                }
            }
            TopItem::Const(c) => collect_expr_refs(&c.value, None, &mut refs),
            TopItem::Test(t) => collect_test_refs(t, index, &mut refs),
            TopItem::Enum(_) | TopItem::Error(_) => {}
            TopItem::Func(_) => {} // fn body refs not yet tracked for LSP go-to-def
            TopItem::Bundle(_) => {} // bundle body refs not yet tracked for LSP go-to-def
        }
    }
    refs
}

/// References inside a `test "..." for M(...) { ... }` block. The
/// module-under-test name is a use (resolves to its module def). Body names
/// are scoped to that module via its symbol position, so a driven input or an
/// `expect`-ed signal resolves to the right port — even cross-file (the
/// `same_module_any_file` tier in `resolve_at` handles the cross-file case).
fn collect_test_refs(t: &TestDecl, index: &SymbolIndex, refs: &mut Vec<Ref>) {
    refs.push(Ref {
        name: t.module.name.name.clone(),
        span: t.module.span,
        module_idx: None,
    });
    let mut_pos = index
        .symbols
        .iter()
        .position(|s| s.kind == SymKind::Module && s.name == t.module.name.name);
    for a in &t.args {
        collect_expr_refs(&a.value, mut_pos, refs);
    }
    for s in &t.body {
        collect_test_stmt_refs(s, mut_pos, refs);
    }
}

fn collect_test_stmt_refs(s: &TestStmt, module_idx: Option<usize>, refs: &mut Vec<Ref>) {
    match s {
        TestStmt::Tick { clock, count } => {
            refs.push(Ref {
                name: clock.name.clone(),
                span: clock.span,
                module_idx,
            });
            if let Some(c) = count {
                collect_expr_refs(c, module_idx, refs);
            }
        }
        TestStmt::Expect(e) => collect_expr_refs(e, module_idx, refs),
        TestStmt::Drive { name, value } => {
            refs.push(Ref {
                name: name.name.clone(),
                span: name.span,
                module_idx,
            });
            collect_expr_refs(value, module_idx, refs);
        }
        TestStmt::If { cond, then, els } => {
            collect_expr_refs(cond, module_idx, refs);
            for s in then {
                collect_test_stmt_refs(s, module_idx, refs);
            }
            for s in els.iter().flatten() {
                collect_test_stmt_refs(s, module_idx, refs);
            }
        }
        TestStmt::Sim(sim) => {
            if let Some(speed) = &sim.speed {
                collect_expr_refs(speed, module_idx, refs);
            }
            for b in &sim.binds {
                refs.push(Ref {
                    name: b.port.name.clone(),
                    span: b.port.span,
                    module_idx,
                });
            }
        }
        TestStmt::Error(_) => {}
    }
}

fn collect_item_refs(item: &ModuleItem, module_idx: Option<usize>, refs: &mut Vec<Ref>) {
    match item {
        ModuleItem::Wire { init, .. } => collect_expr_refs(init, module_idx, refs),
        ModuleItem::Reg { reset, .. } => collect_expr_refs(reset, module_idx, refs),
        ModuleItem::Mem { depth, init, .. } => {
            collect_expr_refs(depth, module_idx, refs);
            collect_expr_refs(init, module_idx, refs);
        }
        ModuleItem::Drive { lhs, rhs } => {
            refs.push(Ref {
                name: lhs.base.name.clone(),
                span: lhs.base.span,
                module_idx,
            });
            collect_expr_refs(rhs, module_idx, refs);
        }
        ModuleItem::Inst(inst) => {
            // The instantiated module name is a cross-file reference.
            refs.push(Ref {
                name: inst.module.name.name.clone(),
                span: inst.module.span,
                module_idx,
            });
            for c in &inst.conns {
                collect_expr_refs(&c.signal, module_idx, refs);
            }
            for a in &inst.args {
                collect_expr_refs(&a.value, module_idx, refs);
            }
        }
        ModuleItem::On(b) => {
            for s in &b.body {
                collect_seq_refs(s, module_idx, refs);
            }
        }
        ModuleItem::Repeat(r) => {
            for mi in &r.items {
                collect_item_refs(mi, module_idx, refs);
            }
        }
        ModuleItem::ForEach(fe) => {
            match &fe.source {
                ForEachSource::Range { lo, hi } => {
                    collect_expr_refs(lo, module_idx, refs);
                    collect_expr_refs(hi, module_idx, refs);
                }
                ForEachSource::Elements(id) => {
                    refs.push(Ref {
                        name: id.name.clone(),
                        span: id.span,
                        module_idx,
                    });
                }
            }
            for mi in &fe.items {
                collect_item_refs(mi, module_idx, refs);
            }
        }
        ModuleItem::SyncLoop(sl) => {
            for s in &sl.body {
                collect_seq_refs(s, module_idx, refs);
            }
        }
        ModuleItem::ConstIf { then, els, .. } => {
            for mi in then {
                collect_item_refs(mi, module_idx, refs);
            }
            if let Some(el) = els {
                for mi in el {
                    collect_item_refs(mi, module_idx, refs);
                }
            }
        }
        ModuleItem::Port { .. }
        | ModuleItem::Clock(_)
        | ModuleItem::Reset { .. }
        | ModuleItem::Const(_)
        | ModuleItem::Enum(_)
        | ModuleItem::Error(_) => {}
        ModuleItem::BundleDestructure { expr, .. } => {
            collect_expr_refs(expr, module_idx, refs);
        }
    }
}

fn collect_seq_refs(s: &SeqStmt, module_idx: Option<usize>, refs: &mut Vec<Ref>) {
    match s {
        SeqStmt::Assign { lhs, rhs } => {
            refs.push(Ref {
                name: lhs.base.name.clone(),
                span: lhs.base.span,
                module_idx,
            });
            collect_expr_refs(rhs, module_idx, refs);
        }
        SeqStmt::If { cond, then, els } => {
            collect_expr_refs(cond, module_idx, refs);
            for s in then {
                collect_seq_refs(s, module_idx, refs);
            }
            for s in els.iter().flatten() {
                collect_seq_refs(s, module_idx, refs);
            }
        }
        SeqStmt::Default { name, val, .. } => {
            refs.push(Ref {
                name: name.name.clone(),
                span: name.span,
                module_idx,
            });
            collect_expr_refs(val, module_idx, refs);
        }
        SeqStmt::Loop { lo, hi, body, .. } => {
            collect_expr_refs(lo, module_idx, refs);
            collect_expr_refs(hi, module_idx, refs);
            for s in body {
                collect_seq_refs(s, module_idx, refs);
            }
        }
        SeqStmt::ForEach { source, body, .. } => {
            match source {
                ForEachSource::Range { lo, hi } => {
                    collect_expr_refs(lo, module_idx, refs);
                    collect_expr_refs(hi, module_idx, refs);
                }
                ForEachSource::Elements(id) => {
                    refs.push(Ref {
                        name: id.name.clone(),
                        span: id.span,
                        module_idx,
                    });
                }
            }
            for s in body {
                collect_seq_refs(s, module_idx, refs);
            }
        }
        SeqStmt::Error(_) => {}
    }
}

fn collect_expr_refs(e: &Expr, module_idx: Option<usize>, refs: &mut Vec<Ref>) {
    match &e.kind {
        ExprKind::Ident(name) => {
            refs.push(Ref {
                name: name.clone(),
                span: e.span,
                module_idx,
            });
        }
        ExprKind::Field { base, field } => {
            collect_expr_refs(base, module_idx, refs);
            // `field` (port/variant after `.`) is left unresolved in v1.
            let _ = field;
        }
        ExprKind::Unary { expr, .. } => collect_expr_refs(expr, module_idx, refs),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_refs(lhs, module_idx, refs);
            collect_expr_refs(rhs, module_idx, refs);
        }
        ExprKind::IfExpr { cond, then, els } => {
            collect_expr_refs(cond, module_idx, refs);
            collect_expr_refs(then, module_idx, refs);
            collect_expr_refs(els, module_idx, refs);
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_expr_refs(scrutinee, module_idx, refs);
            for a in arms {
                collect_expr_refs(&a.value, module_idx, refs);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                collect_expr_refs(p, module_idx, refs);
            }
        }
        ExprKind::Replicate { count, parts } => {
            collect_expr_refs(count, module_idx, refs);
            for p in parts {
                collect_expr_refs(p, module_idx, refs);
            }
        }
        ExprKind::Index { base, index } => {
            collect_expr_refs(base, module_idx, refs);
            collect_expr_refs(index, module_idx, refs);
        }
        ExprKind::Slice { base, hi, lo } => {
            collect_expr_refs(base, module_idx, refs);
            collect_expr_refs(hi, module_idx, refs);
            collect_expr_refs(lo, module_idx, refs);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                collect_expr_refs(a, module_idx, refs);
            }
        }
        ExprKind::Int { .. } | ExprKind::Bool(_) => {}
        ExprKind::FnCall { args, .. } => {
            for a in args {
                collect_expr_refs(a, module_idx, refs);
            }
        }
        ExprKind::BundleLit(inits) => {
            for fi in inits {
                collect_expr_refs(&fi.value, module_idx, refs);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for e in elems {
                collect_expr_refs(e, module_idx, refs);
            }
        }
    }
}

/// A completion suggestion: the text to insert and what it is.
pub struct Candidate {
    /// The text to insert.
    pub label: String,
    /// What kind of completion this is (drives the editor's icon/sort).
    pub kind: CandKind,
}

/// What a [`Candidate`] completes to.
pub enum CandKind {
    /// A language keyword, in the file's majority flavor.
    Keyword,
    /// An in-scope identifier, carrying its definition kind.
    Symbol(SymKind),
}

/// Completion candidates at `offset`: in-scope identifiers (enclosing module's
/// members + file-level consts/enums + every module name) plus keywords in the
/// file's majority flavor. Prefix filtering is left to the editor.
pub fn completions(
    index: &SymbolIndex,
    files: &[LoadedFile],
    file_idx: usize,
    offset: usize,
) -> Vec<Candidate> {
    let mut out = Vec::new();

    // Which module (if any) does the cursor sit inside?
    let enclosing = enclosing_module(index, files, file_idx, offset);

    for s in &index.symbols {
        let in_scope = match s.module_idx {
            // Members of the enclosing module.
            Some(m) => Some(m) == enclosing && s.file_idx == file_idx,
            // File-level: consts/enums of this file, or any module name.
            None => {
                s.kind == SymKind::Module
                    || (s.file_idx == file_idx
                        && matches!(
                            s.kind,
                            SymKind::Const | SymKind::Enum | SymKind::EnumVariant
                        ))
            }
        };
        if in_scope {
            out.push(Candidate {
                label: s.name.clone(),
                kind: CandKind::Symbol(s.kind.clone()),
            });
        }
    }

    // Majority-flavor keywords.
    let src = &index.files[file_idx].1;
    let flavor = crate::lexer::lex(src)
        .ok()
        .map(|toks| crate::morph::majority_flavor(&toks))
        .unwrap_or(crate::lexer::token::Flavor::English);
    for kw in crate::lexer::keywords::TABLE.canonical_spellings(flavor) {
        out.push(Candidate {
            label: kw.to_string(),
            kind: CandKind::Keyword,
        });
    }

    out
}

/// The symbol index of the module whose body span contains `offset`, if any.
fn enclosing_module(
    index: &SymbolIndex,
    files: &[LoadedFile],
    file_idx: usize,
    offset: usize,
) -> Option<usize> {
    let f = &files[file_idx];
    for item in &f.ast.items {
        if let TopItem::Module(m) = item
            && m.span.start <= offset
            && offset < m.span.end
        {
            return index.symbols.iter().position(|s| {
                s.file_idx == file_idx
                    && s.kind == SymKind::Module
                    && s.span.start == m.name.span.start
            });
        }
    }
    None
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

    fn offset_of(src: &str, needle: &str) -> usize {
        src.find(needle).expect("needle present")
    }

    #[test]
    fn resolve_at_use_returns_definition() {
        let src = "module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n";
        let files = loaded(src);
        let idx = build_index(&files);
        // Cursor on the USE of `a` in `y = a` resolves to the port `a` decl.
        let use_off = src.rfind('a').unwrap();
        let sym = resolve_at(&idx, &files, 0, use_off).expect("resolves");
        assert_eq!(idx.symbols[sym].name, "a");
        assert!(matches!(idx.symbols[sym].kind, SymKind::Port { .. }));
        // And the resolved span is the DECLARATION, not the use site.
        assert_eq!(idx.symbols[sym].span.start, offset_of(src, "a: bit"));
    }

    #[test]
    fn resolve_at_works_on_partial_tree() {
        // A broken line between two good ports: parse_recover keeps the ports.
        // (Plan used `@@@`, but the lexer rejects `@`; a bare `let` lexes fine
        // yet fails to parse, producing the same ModuleItem::Error recovery.)
        let src = "module M {\n  in a: bit\n  let\n  out y: bit\n  y = a\n}\n";
        let files = loaded(src);
        let idx = build_index(&files);
        let use_off = src.rfind('a').unwrap();
        let sym = resolve_at(&idx, &files, 0, use_off).expect("resolves around Error node");
        assert_eq!(idx.symbols[sym].name, "a");
    }

    #[test]
    fn resolve_at_inside_test_block() {
        let src = "module Adder {\n  in a: bit\n  out sum: bit\n  sum = a\n}\n\
                   test \"works\" for Adder {\n  a = true\n  expect sum\n}\n";
        let files = loaded(src);
        let idx = build_index(&files);
        // go-to-def on the module-under-test name in the `for Adder` header.
        let m_off = src.find("for Adder").unwrap() + "for ".len();
        let sym = resolve_at(&idx, &files, 0, m_off).expect("module-under-test resolves");
        assert_eq!(idx.symbols[sym].name, "Adder");
        assert!(matches!(idx.symbols[sym].kind, SymKind::Module));
        // a driven input `a` in the test body resolves to the port `a`.
        let a_off = src.find("a = true").unwrap();
        let sym = resolve_at(&idx, &files, 0, a_off).expect("driven input resolves");
        assert_eq!(idx.symbols[sym].name, "a");
        assert!(matches!(idx.symbols[sym].kind, SymKind::Port { .. }));
        // `expect sum` resolves the output port.
        let y_off = src.find("expect sum").unwrap() + "expect ".len();
        let sym = resolve_at(&idx, &files, 0, y_off).expect("expected signal resolves");
        assert_eq!(idx.symbols[sym].name, "sum");
    }

    #[test]
    fn resolve_at_cross_file_instance() {
        use std::path::PathBuf;
        let lib = "module Adder {\n  in a: bit\n  out s: bit\n  s = a\n}\n";
        // Plan wrote `Adder { a: x }`, but the grammar requires the param paren
        // list even when empty: `Adder() { ... }`. Without it the instance is a
        // parse Error and no module ref is produced.
        let top =
            "module Top {\n  in x: bit\n  out z: bit\n  let u = Adder() { a: x }\n  z = u.s\n}\n";
        let mk = |path: &str, src: &str| {
            let toks = lexer::lex(src).unwrap();
            let (ast, _) = parser::parse_recover(toks);
            LoadedFile {
                path: PathBuf::from(path),
                src: src.to_string(),
                ast,
            }
        };
        let files = vec![mk("top.mimz", top), mk("adder.mimz", lib)];
        let idx = build_index(&files);
        // Cursor on `Adder` in the instantiation resolves into adder.mimz.
        let off = top.find("Adder").unwrap();
        let sym = resolve_at(&idx, &files, 0, off).expect("module ref resolves");
        assert_eq!(idx.symbols[sym].name, "Adder");
        assert_eq!(idx.symbols[sym].file_idx, 1);
    }

    #[test]
    fn completions_include_scope_idents_and_majority_keywords() {
        let src = "module M {\n  in abc: bit\n  out y: bit\n  y = \n}\n";
        let files = loaded(src);
        let idx = build_index(&files);
        let at = src.find("y = ").unwrap() + 4; // just after `y = `
        let cands = completions(&idx, &files, 0, at);
        let labels: Vec<&str> = cands.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"abc"), "in-scope port missing: {labels:?}");
        assert!(labels.contains(&"module"), "English keyword missing");
        assert!(matches!(
            cands.iter().find(|c| c.label == "abc").unwrap().kind,
            CandKind::Symbol(SymKind::Port { .. })
        ));
    }

    #[test]
    fn completions_exclude_other_flavor_keywords() {
        // A Tamil-flavored file: keyword completion offers Tamil, not English.
        let src = "தொகுதி M {\n  உள்ளீடு a: bit\n  வெளியீடு y: bit\n  y = \n}\n";
        let files = loaded(src);
        let idx = build_index(&files);
        let at = src.find("y = ").unwrap() + 4;
        let labels: Vec<String> = completions(&idx, &files, 0, at)
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert!(
            labels.iter().any(|l| l == "தொகுதி"),
            "Tamil keyword missing"
        );
        assert!(
            !labels.iter().any(|l| l == "module"),
            "English keyword leaked in"
        );
    }
}
