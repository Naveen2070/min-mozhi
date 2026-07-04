//! Lint passes — style and hygiene warnings, separate from the correctness
//! checker. These never fail the build (all warnings, severity::Warning).
//!
//! Additive and edition-safe: every lint can be suppressed without changing
//! the meaning of the code. New lints never break existing programs.
//!
//! Current rules:
//! - W0002: signal name should be snake_case
//! - W0003: module name should be PascalCase
//! - W0004: signal declared but never used

use crate::ast;
use crate::diag::Diag;
use crate::span::Span;

use std::collections::{HashMap, HashSet};

/// Run all lint passes over a loaded project. Returns zero or more warnings.
/// Every diagnostic has severity::Warning — the build never fails from lint.
pub fn lint(files: &[ast::File]) -> Vec<Diag> {
    let mut diags = Vec::new();
    for (file_idx, file) in files.iter().enumerate() {
        for item in &file.items {
            if let ast::TopItem::Module(module) = item {
                lint_module(&mut diags, file_idx, module);
            }
        }
    }
    diags
}

fn lint_module(diags: &mut Vec<Diag>, file_idx: usize, module: &ast::Module) {
    check_module_name(diags, file_idx, module);
    check_naming(diags, file_idx, module);

    let (spans, referenced) = collect_names(module);
    for (name, span) in &spans {
        if referenced.contains(name) {
            continue;
        }
        diags.push(
            Diag::new(*span, format!("signal `{name}` is declared but never used"))
                .with_code("W0004")
                .with_file(file_idx)
                .with_help(
                    "remove the unused declaration or prefix it with `_` to suppress this warning",
                )
                .as_warning(),
        );
    }
}

/// Check module name follows PascalCase.
fn check_module_name(diags: &mut Vec<Diag>, file_idx: usize, module: &ast::Module) {
    let name = &module.name.name;
    if !is_pascal_case(name) {
        diags.push(
            Diag::new(
                module.name.span,
                format!("module `{name}` should be PascalCase (e.g. `MyModule`)"),
            )
            .with_code("W0003")
            .with_file(file_idx)
            .with_help("use an uppercase first letter and drop underscores, e.g. `MyModule`")
            .as_warning(),
        );
    }
}

/// Check signal names (ports, wires, regs, instances, clocks, resets) follow snake_case.
fn check_naming(diags: &mut Vec<Diag>, file_idx: usize, module: &ast::Module) {
    for item in &module.items {
        match item {
            ast::ModuleItem::Port { name, .. }
            | ast::ModuleItem::Wire { name, .. }
            | ast::ModuleItem::Reg { name, .. }
            | ast::ModuleItem::Clock(name)
            | ast::ModuleItem::Reset { name, .. } => {
                if !is_snake_case(&name.name) {
                    diags.push(
                        Diag::new(
                            name.span,
                            format!("signal `{}` should be snake_case", name.name),
                        )
                        .with_code("W0002")
                        .with_file(file_idx)
                        .with_help(
                            "use lowercase letters, digits, and underscores, e.g. `my_signal`",
                        )
                        .as_warning(),
                    );
                }
            }
            ast::ModuleItem::Inst(inst) => {
                if !is_snake_case(&inst.name.name) {
                    diags.push(
                        Diag::new(
                            inst.name.span,
                            format!("instance `{}` should be snake_case", inst.name.name),
                        )
                        .with_code("W0002")
                        .with_file(file_idx)
                        .with_help(
                            "use lowercase letters, digits, and underscores, e.g. `my_instance`",
                        )
                        .as_warning(),
                    );
                }
            }
            ast::ModuleItem::Mem { name, .. } if !is_snake_case(&name.name) => {
                diags.push(
                    Diag::new(
                        name.span,
                        format!("memory `{}` should be snake_case", name.name),
                    )
                    .with_code("W0002")
                    .with_file(file_idx)
                    .with_help("use lowercase letters, digits, and underscores, e.g. `my_memory`")
                    .as_warning(),
                );
            }
            _ => {}
        }
    }
}

/// Collect every declared name (mapped to its declaration span) and every
/// referenced name in a module. A name present in `spans` but absent from
/// `referenced` is declared-but-unused — that gap drives the W0004 lint.
///
/// Ports are recorded as both declared *and* referenced: they form the module's
/// public interface, so an unused port is never a warning.
fn collect_names(module: &ast::Module) -> (HashMap<String, Span>, HashSet<String>) {
    let mut spans = HashMap::new();
    let mut referenced = HashSet::new();
    for item in &module.items {
        collect_item(item, &mut spans, &mut referenced);
    }
    (spans, referenced)
}

/// Record the declarations and references contributed by a single module item.
/// `repeat` blocks recurse, so names declared inside an unrolled loop (including
/// nested loops) are tracked the same as top-level ones.
fn collect_item(
    item: &ast::ModuleItem,
    spans: &mut HashMap<String, Span>,
    referenced: &mut HashSet<String>,
) {
    match item {
        ast::ModuleItem::Port { name, .. } => {
            spans.entry(name.name.clone()).or_insert(name.span);
            referenced.insert(name.name.clone());
        }
        ast::ModuleItem::Clock(name)
        | ast::ModuleItem::Reset { name, .. }
        | ast::ModuleItem::Wire { name, .. }
        | ast::ModuleItem::Reg { name, .. }
        | ast::ModuleItem::Mem { name, .. } => {
            spans.entry(name.name.clone()).or_insert(name.span);
        }
        ast::ModuleItem::Const(c) => {
            spans.entry(c.name.name.clone()).or_insert(c.name.span);
        }
        ast::ModuleItem::Enum(e) => {
            spans.entry(e.name.name.clone()).or_insert(e.name.span);
        }
        ast::ModuleItem::Inst(inst) => {
            spans
                .entry(inst.name.name.clone())
                .or_insert(inst.name.span);
            referenced.insert(inst.module.name.name.clone());
            for conn in &inst.conns {
                collect_expr_names(&conn.signal, referenced);
            }
            for arg in &inst.args {
                collect_expr_names(&arg.value, referenced);
            }
        }
        ast::ModuleItem::Drive { lhs, rhs } => {
            referenced.insert(lhs.base.name.clone());
            collect_expr_names(rhs, referenced);
        }
        ast::ModuleItem::On(on) => {
            referenced.insert(on.clock.name.clone());
            for stmt in &on.body {
                collect_seq_names(stmt, referenced);
            }
        }
        ast::ModuleItem::Repeat(repeat) => {
            referenced.insert(repeat.var.name.clone());
            collect_expr_names(&repeat.lo, referenced);
            collect_expr_names(&repeat.hi, referenced);
            for inner in &repeat.items {
                collect_item(inner, spans, referenced);
            }
        }
        ast::ModuleItem::ConstIf {
            cond, then, els, ..
        } => {
            collect_expr_names(cond, referenced);
            for inner in then {
                collect_item(inner, spans, referenced);
            }
            if let Some(el) = els {
                for inner in el {
                    collect_item(inner, spans, referenced);
                }
            }
        }
        ast::ModuleItem::Error(_) => {}
        ast::ModuleItem::BundleDestructure { expr, .. } => {
            collect_expr_names(expr, referenced);
        }
    }
}

/// Collect identifier names from an expression tree.
fn collect_expr_names(expr: &ast::Expr, names: &mut HashSet<String>) {
    match &expr.kind {
        ast::ExprKind::Ident(name) => {
            names.insert(name.clone());
        }
        ast::ExprKind::Field { base, field } => {
            collect_expr_names(base, names);
            names.insert(field.name.clone());
        }
        ast::ExprKind::Unary { expr: inner, .. } => {
            collect_expr_names(inner, names);
        }
        ast::ExprKind::Binary { lhs, rhs, .. } => {
            collect_expr_names(lhs, names);
            collect_expr_names(rhs, names);
        }
        ast::ExprKind::IfExpr { cond, then, els } => {
            collect_expr_names(cond, names);
            collect_expr_names(then, names);
            collect_expr_names(els, names);
        }
        ast::ExprKind::Match { scrutinee, arms } => {
            collect_expr_names(scrutinee, names);
            for arm in arms {
                collect_expr_names(&arm.value, names);
            }
        }
        ast::ExprKind::Concat(exprs) => {
            for e in exprs {
                collect_expr_names(e, names);
            }
        }
        ast::ExprKind::Replicate { parts, .. } => {
            for e in parts {
                collect_expr_names(e, names);
            }
        }
        ast::ExprKind::Index { base, index } => {
            collect_expr_names(base, names);
            collect_expr_names(index, names);
        }
        ast::ExprKind::Slice { base, hi, lo } => {
            collect_expr_names(base, names);
            collect_expr_names(hi, names);
            collect_expr_names(lo, names);
        }
        ast::ExprKind::Call { args, .. } => {
            for a in args {
                collect_expr_names(a, names);
            }
        }
        ast::ExprKind::Int { .. } | ast::ExprKind::Bool(_) => {}
        ast::ExprKind::FnCall { args, .. } => {
            for a in args {
                collect_expr_names(a, names);
            }
        }
        ast::ExprKind::BundleLit(inits) => {
            for fi in inits {
                collect_expr_names(&fi.value, names);
            }
        }
        ast::ExprKind::ArrayLit(elems) => {
            for e in elems {
                collect_expr_names(e, names);
            }
        }
    }
}

/// Collect identifier names from a sequential statement.
fn collect_seq_names(stmt: &ast::SeqStmt, names: &mut HashSet<String>) {
    match stmt {
        ast::SeqStmt::Assign { lhs, rhs } => {
            names.insert(lhs.base.name.clone());
            collect_expr_names(rhs, names);
        }
        ast::SeqStmt::If { cond, then, els } => {
            collect_expr_names(cond, names);
            for s in then {
                collect_seq_names(s, names);
            }
            if let Some(els) = els {
                for s in els {
                    collect_seq_names(s, names);
                }
            }
        }
        ast::SeqStmt::Default { name, val, .. } => {
            names.insert(name.name.clone());
            collect_expr_names(val, names);
        }
        ast::SeqStmt::Error(_) => {}
    }
}

/// Check if a name follows snake_case: `^[a-z_][a-z0-9_]*$`
fn is_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() && bytes[0] != b'_' {
        return false;
    }
    for &b in &bytes[1..] {
        if !b.is_ascii_lowercase() && !b.is_ascii_digit() && b != b'_' {
            return false;
        }
    }
    true
}

/// Check if a name follows PascalCase: `^[A-Z][a-zA-Z0-9]*$`
fn is_pascal_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    for &b in &bytes[1..] {
        if !b.is_ascii_uppercase() && !b.is_ascii_lowercase() && !b.is_ascii_digit() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_accepts_valid_names() {
        assert!(is_snake_case("a"));
        assert!(is_snake_case("my_signal"));
        assert!(is_snake_case("count_1"));
        assert!(is_snake_case("_"));
        assert!(is_snake_case("a_b_c"));
    }

    #[test]
    fn snake_case_rejects_bad_names() {
        assert!(!is_snake_case("A"));
        assert!(!is_snake_case("MySignal"));
        assert!(!is_snake_case("mySignal"));
        assert!(!is_snake_case("1abc"));
        assert!(!is_snake_case("signal-name"));
    }

    #[test]
    fn pascal_case_accepts_valid_names() {
        assert!(is_pascal_case("A"));
        assert!(is_pascal_case("MyModule"));
        assert!(is_pascal_case("Adder8"));
        assert!(is_pascal_case("ABCD"));
    }

    #[test]
    fn pascal_case_rejects_bad_names() {
        assert!(!is_pascal_case("myModule"));
        assert!(!is_pascal_case("my_module"));
        assert!(!is_pascal_case("1Module"));
        assert!(!is_pascal_case(""));
    }

    #[test]
    fn lint_empty_file_produces_no_warnings() {
        let files = vec![ast::File {
            imports: vec![],
            items: vec![],
        }];
        let diags = lint(&files);
        assert!(diags.is_empty());
    }
}
