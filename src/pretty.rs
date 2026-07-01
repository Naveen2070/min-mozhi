//! AST → Min-Mozhi source pretty-printer — the engine behind
//! `mimz translate --order code|thamizh`.
//!
//! Unlike [`crate::translate`] (which re-spells keyword TOKENS and is
//! trivia-preserving), this emits **from the AST**, so it can reorder clause
//! heads between the two word-order profiles (spec/04 section 3). The AST
//! carries no comments and no original layout, so the output is **canonically
//! formatted and drops comments** — it is NOT byte-identical to the input. The
//! correctness contract is semantic: the output compiles to byte-identical
//! Verilog and re-parses to the same AST (`tests/translate.rs`).
//!
//! Keyword spellings come from the same [`TABLE`] the lexer/translate use, so
//! flavor (english/tanglish/tamil) and order (code/thamizh) compose freely.
//!
//! Indentation: most expressions are single-line, but `match` is block-shaped
//! (one arm per line). Expression emitters therefore take an `indent` (the
//! column level of any block they open) so a `match` — even nested in an
//! assignment RHS — lays its arms out correctly.

use crate::ast::*;
use crate::lexer::keywords::TABLE;
use crate::lexer::token::{Flavor, Kw};

/// Which word order to emit. Public mirror of the parser's internal `Profile`
/// (which is `pub(crate)`), so the CLI can request an order without depending
/// on parser internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// English-derived order: `on rise(clk)`, `if c { }`, `match e { }`.
    Code,
    /// SOV/postpositional: `rise(clk) on`, `c if { }`, `e match { }`. Emits a
    /// leading `syntax thamizh` directive so the result re-parses.
    Thamizh,
}

/// Pretty-print a parsed file as Min-Mozhi source in the given keyword `flavor`
/// and word `order`.
pub fn pretty_print(file: &File, flavor: Flavor, order: Order) -> String {
    let mut p = Pretty {
        out: String::new(),
        indent: 0,
        flavor,
        order,
    };
    p.file(file);
    p.out
}

/// Canonical English string for `ty` — used by the names checker's E0808
/// type comparison (Phase 4 of OR-arm binding intersection).
pub(crate) fn type_str(ty: &Type) -> String {
    let p = Pretty {
        out: String::new(),
        indent: 0,
        flavor: crate::lexer::token::Flavor::English,
        order: Order::Code,
    };
    p.ty(ty, 0)
}

struct Pretty {
    out: String,
    indent: usize,
    flavor: Flavor,
    order: Order,
}

/// Indentation prefix for a given level (2 spaces per level).
fn pad(level: usize) -> String {
    "  ".repeat(level)
}

impl Pretty {
    /// A keyword's spelling in the target flavor.
    fn kw(&self, kw: Kw) -> &'static str {
        TABLE.canonical(kw, self.flavor)
    }

    /// Push a full line at the current indent.
    fn line(&mut self, s: &str) {
        self.out.push_str(&pad(self.indent));
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn blank(&mut self) {
        self.out.push('\n');
    }

    // ---------- file / items ----------

    fn file(&mut self, f: &File) {
        if self.order == Order::Thamizh {
            // `syntax thamizh` (in the target flavor) so the output re-parses
            // under the thamizh-order profile.
            let s = format!("{} {}", self.kw(Kw::Syntax), self.kw(Kw::Thamizh));
            self.line(&s);
            self.blank();
        }
        for imp in &f.imports {
            let path = imp
                .path
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join(".");
            let s = format!("{} {path}", self.kw(Kw::Import));
            self.line(&s);
        }
        if !f.imports.is_empty() && !f.items.is_empty() {
            self.blank();
        }
        for (i, item) in f.items.iter().enumerate() {
            if i > 0 {
                self.blank();
            }
            self.top_item(item);
        }
    }

    fn top_item(&mut self, item: &TopItem) {
        match item {
            TopItem::Const(c) => self.const_decl(c),
            TopItem::Module(m) => self.module(m),
            TopItem::Enum(e) => self.enum_decl(e),
            TopItem::Test(t) => self.test_decl(t),
            TopItem::Func(f) => self.func_decl(f),
            // Unreachable: pretty-printing runs on a strict-parsed tree, which
            // never carries an `Error` placeholder.
            TopItem::Error(_) => {}
            TopItem::Bundle(b) => self.bundle_decl(b),
        }
    }

    fn const_decl(&mut self, c: &ConstDecl) {
        let s = format!(
            "{} {}: {} = {}",
            self.kw(Kw::Const),
            c.name.name,
            param_ty(c.ty),
            self.expr(&c.value, self.indent)
        );
        self.line(&s);
    }

    fn func_decl(&mut self, f: &FuncDecl) {
        let params = f
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name.name, self.ty(&p.ty, self.indent)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = self.ty(&f.ret, self.indent);
        let head = format!("{} {}({params}) -> {ret} {{", self.kw(Kw::Fn), f.name.name);
        self.line(&head);
        self.indent += 1;
        for local in &f.locals {
            let v = self.expr(&local.value, self.indent);
            let s = format!("{} {} = {v}", self.kw(Kw::Let), local.name.name);
            self.line(&s);
        }
        let body = self.expr(&f.body, self.indent);
        self.line(&body);
        self.indent -= 1;
        self.line("}");
    }

    fn enum_decl(&mut self, e: &EnumDecl) {
        let ind = self.indent;
        let variants = e
            .variants
            .iter()
            .map(|v| {
                if v.fields.is_empty() {
                    v.name.name.clone()
                } else {
                    let fields = v
                        .fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name.name, self.ty(&f.ty, ind)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}({fields})", v.name.name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let s = format!("{} {} {{ {variants} }}", self.kw(Kw::Enum), e.name.name);
        self.line(&s);
    }

    fn bundle_decl(&mut self, b: &BundleDecl) {
        let params = if b.params.is_empty() {
            String::new()
        } else {
            let ps = b
                .params
                .iter()
                .map(|p| self.param(p))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({ps})")
        };
        let head = format!("{} {}{params} {{", self.kw(Kw::Bundle), b.name.name);
        self.line(&head);
        self.indent += 1;
        for f in &b.fields {
            let s = format!("{}: {}", f.name.name, self.ty(&f.ty, self.indent));
            self.line(&s);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn module(&mut self, m: &Module) {
        let params = if m.params.is_empty() {
            String::new()
        } else {
            let ps = m
                .params
                .iter()
                .map(|p| self.param(p))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({ps})")
        };
        let head = format!("{} {}{params} {{", self.kw(Kw::Module), m.name.name);
        self.line(&head);
        self.indent += 1;
        for it in &m.items {
            self.module_item(it);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn param(&self, p: &Param) -> String {
        let base = format!("{}: {}", p.name.name, param_ty(p.ty));
        match &p.default {
            Some(d) => format!("{base} = {}", self.expr(d, self.indent)),
            None => base,
        }
    }

    fn module_item(&mut self, it: &ModuleItem) {
        let ind = self.indent;
        match it {
            ModuleItem::Port { dir, name, ty } => {
                let kw = match dir {
                    Dir::In => self.kw(Kw::In),
                    Dir::Out => self.kw(Kw::Out),
                };
                let s = format!("{kw} {}: {}", name.name, self.ty(ty, ind));
                self.line(&s);
            }
            ModuleItem::Clock(c) => {
                let s = format!("{} {}", self.kw(Kw::Clock), c.name);
                self.line(&s);
            }
            ModuleItem::Reset { name: r, is_async } => {
                let s = if *is_async {
                    format!("{} {} {}", self.kw(Kw::Async), self.kw(Kw::Reset), r.name)
                } else {
                    format!("{} {}", self.kw(Kw::Reset), r.name)
                };
                self.line(&s);
            }
            ModuleItem::Wire { name, ty, init } => {
                let s = format!(
                    "{} {}: {} = {}",
                    self.kw(Kw::Wire),
                    name.name,
                    self.ty(ty, ind),
                    self.expr(init, ind)
                );
                self.line(&s);
            }
            ModuleItem::Reg { name, ty, reset } => {
                let s = format!(
                    "{} {}: {} = {}",
                    self.kw(Kw::Reg),
                    name.name,
                    self.ty(ty, ind),
                    self.expr(reset, ind)
                );
                self.line(&s);
            }
            ModuleItem::Mem {
                name,
                ty,
                depth,
                init,
            } => {
                let s = format!(
                    "{} {}: {}[{}] = {}",
                    self.kw(Kw::Mem),
                    name.name,
                    self.ty(ty, ind),
                    self.expr(depth, ind),
                    self.expr(init, ind)
                );
                self.line(&s);
            }
            ModuleItem::Const(c) => self.const_decl(c),
            ModuleItem::Enum(e) => self.enum_decl(e),
            ModuleItem::Inst(inst) => self.inst(inst),
            ModuleItem::On(on) => self.on_block(on),
            ModuleItem::Drive { lhs, rhs } => {
                let s = format!("{} = {}", self.lvalue(lhs, ind), self.expr(rhs, ind));
                self.line(&s);
            }
            ModuleItem::Repeat(r) => self.repeat(r),
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                let ind = self.indent;
                let head = format!(
                    "{} {} ({}) {{",
                    self.kw(Kw::Const),
                    self.kw(Kw::If),
                    self.expr(cond, ind)
                );
                self.line(&head);
                self.indent += 1;
                for it in then {
                    self.module_item(it);
                }
                self.indent -= 1;
                if let Some(el) = els {
                    self.line(&format!("}} {} {{", self.kw(Kw::Else)));
                    self.indent += 1;
                    for it in el {
                        self.module_item(it);
                    }
                    self.indent -= 1;
                }
                self.line("}");
            }
            ModuleItem::Error(_) => {} // unreachable on a strict-parsed tree
            ModuleItem::BundleDestructure { bindings, expr, .. } => {
                let bs = bindings
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let rhs = self.expr(expr, ind);
                let s = format!("{} {{ {bs} }} = {rhs}", self.kw(Kw::Let));
                self.line(&s);
            }
        }
    }

    fn inst(&mut self, inst: &Inst) {
        let ind = self.indent;
        let idx = match &inst.index {
            Some(e) => format!("[{}]", self.expr(e, ind)),
            None => String::new(),
        };
        // The parameter list is always parenthesized — `Module()` when empty —
        // because the parser requires `(` after the module name.
        let a = inst
            .args
            .iter()
            .map(|na| format!("{}: {}", na.name.name, self.expr(&na.value, ind)))
            .collect::<Vec<_>>()
            .join(", ");
        let args = format!("({a})");
        let conns = inst
            .conns
            .iter()
            .map(|c| format!("{}: {}", c.port.name, self.expr(&c.signal, ind)))
            .collect::<Vec<_>>()
            .join(", ");
        let conns = if conns.is_empty() {
            " {}".to_string()
        } else {
            format!(" {{ {conns} }}")
        };
        let s = format!(
            "{} {}{idx} = {}{args}{conns}",
            self.kw(Kw::Let),
            inst.name.name,
            inst.module.name
        );
        self.line(&s);
    }

    fn repeat(&mut self, r: &Repeat) {
        let ind = self.indent;
        let head = format!(
            "{} {}: {}..{} {{",
            self.kw(Kw::Repeat),
            r.var.name,
            self.expr(&r.lo, ind),
            self.expr(&r.hi, ind)
        );
        self.line(&head);
        self.indent += 1;
        for it in &r.items {
            self.module_item(it);
        }
        self.indent -= 1;
        self.line("}");
    }

    // ---------- on-block + sequential statements (ORDER-SENSITIVE) ----------

    fn on_block(&mut self, on: &OnBlock) {
        let on_kw = self.kw(Kw::On);
        let edge = self.kw(match on.edge {
            crate::ast::Edge::Rise => Kw::Rise,
            crate::ast::Edge::Fall => Kw::Fall,
        });
        let head = match self.order {
            // code-order:  on rise(clk) {  /  on fall(clk) {
            Order::Code => format!("{on_kw} {edge}({}) {{", on.clock.name),
            // thamizh-order:  rise(clk) on {  /  fall(clk) on {
            Order::Thamizh => format!("{edge}({}) {on_kw} {{", on.clock.name),
        };
        self.line(&head);
        self.indent += 1;
        for st in &on.body {
            self.seq_stmt(st);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn seq_stmt(&mut self, st: &SeqStmt) {
        let ind = self.indent;
        match st {
            SeqStmt::Assign { lhs, rhs } => {
                let s = format!("{} <- {}", self.lvalue(lhs, ind), self.expr(rhs, ind));
                self.line(&s);
            }
            SeqStmt::If { cond, then, els } => {
                let if_kw = self.kw(Kw::If);
                let cond = self.operand(cond, ind);
                let head = match self.order {
                    Order::Code => format!("{if_kw} {cond} {{"),
                    Order::Thamizh => format!("{cond} {if_kw} {{"),
                };
                self.line(&head);
                self.indent += 1;
                for s in then {
                    self.seq_stmt(s);
                }
                self.indent -= 1;
                match els {
                    None => self.line("}"),
                    Some(else_body) => {
                        let s = format!("}} {} {{", self.kw(Kw::Else));
                        self.line(&s);
                        self.indent += 1;
                        for s in else_body {
                            self.seq_stmt(s);
                        }
                        self.indent -= 1;
                        self.line("}");
                    }
                }
            }
            SeqStmt::Default { name, val, .. } => {
                let kw = self.kw(Kw::Default);
                let v = self.expr(val, ind);
                self.line(&format!("{kw} {} <- {v}", name.name));
            }
            SeqStmt::Error(_) => {} // unreachable on a strict-parsed tree
        }
    }

    // ---------- tests (test HEADER + test `if` stay code-order; the test-form
    // flip is deferred to Phase 1.5, so they are not reorderable) ----------

    fn test_decl(&mut self, t: &TestDecl) {
        let test_kw = self.kw(Kw::Test);
        let for_kw = self.kw(Kw::For);
        let module = &t.module.name;
        let args = self.named_args(&t.args);
        let head = match self.order {
            // code-order:    test "name" for M(args) {
            Order::Code => format!("{test_kw} {:?} {for_kw} {module}{args} {{", t.name),
            // thamizh-order: M(args) kaaga "name" sodhanai {
            Order::Thamizh => format!("{module}{args} {for_kw} {:?} {test_kw} {{", t.name),
        };
        self.line(&head);
        self.indent += 1;
        for st in &t.body {
            self.test_stmt(st);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn named_args(&self, args: &[NamedArg]) -> String {
        if args.is_empty() {
            return String::new();
        }
        let a = args
            .iter()
            .map(|na| format!("{}: {}", na.name.name, self.expr(&na.value, self.indent)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("({a})")
    }

    fn test_stmt(&mut self, st: &TestStmt) {
        let ind = self.indent;
        match st {
            TestStmt::Tick { clock, count } => {
                let s = match count {
                    Some(c) => format!(
                        "{}({}, {})",
                        self.kw(Kw::Tick),
                        clock.name,
                        self.expr(c, ind)
                    ),
                    None => format!("{}({})", self.kw(Kw::Tick), clock.name),
                };
                self.line(&s);
            }
            TestStmt::Expect(e) => {
                let s = format!("{} {}", self.kw(Kw::Expect), self.expr(e, ind));
                self.line(&s);
            }
            TestStmt::Drive { name, value } => {
                let s = format!("{} = {}", name.name, self.expr(value, ind));
                self.line(&s);
            }
            TestStmt::If { cond, then, els } => {
                // Always code-order — the parser only flips `on`-block `if`.
                let head = format!("{} {} {{", self.kw(Kw::If), self.operand(cond, ind));
                self.line(&head);
                self.indent += 1;
                for s in then {
                    self.test_stmt(s);
                }
                self.indent -= 1;
                match els {
                    None => self.line("}"),
                    Some(else_body) => {
                        let s = format!("}} {} {{", self.kw(Kw::Else));
                        self.line(&s);
                        self.indent += 1;
                        for s in else_body {
                            self.test_stmt(s);
                        }
                        self.indent -= 1;
                        self.line("}");
                    }
                }
            }
            TestStmt::Error(_) => {} // unreachable on a strict-parsed tree
        }
    }

    // ---------- types / lvalues ----------

    fn ty(&self, t: &Type, ind: usize) -> String {
        match t {
            Type::Bit => "bit".to_string(),
            Type::Bits(e) => format!("bits[{}]", self.expr(e, ind)),
            Type::Signed(e) => format!("signed[{}]", self.expr(e, ind)),
            Type::Named(id) => id.name.clone(),
            Type::Bundle { name, args } => {
                if args.is_empty() {
                    name.name.clone()
                } else {
                    let a = args
                        .iter()
                        .map(|a| format!("{}: {}", a.name.name, self.expr(&a.value, ind)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}({a})", name.name)
                }
            }
        }
    }

    fn lvalue(&self, lv: &LValue, ind: usize) -> String {
        match &lv.index {
            None => lv.base.name.clone(),
            Some((i, None)) => format!("{}[{}]", lv.base.name, self.expr(i, ind)),
            Some((hi, Some(lo))) => {
                format!(
                    "{}[{}:{}]",
                    lv.base.name,
                    self.expr(hi, ind),
                    self.expr(lo, ind)
                )
            }
        }
    }

    // ---------- expressions ----------

    /// An operand in a precedence-sensitive position (binary/unary operand,
    /// `if` condition, `match` scrutinee). Parenthesize anything that is not
    /// atomic so the tree re-parses identically; atoms/postfix bind tightest
    /// and need no parens.
    fn operand(&self, e: &Expr, ind: usize) -> String {
        match e.kind {
            ExprKind::Binary { .. } | ExprKind::IfExpr { .. } | ExprKind::Match { .. } => {
                format!("({})", self.expr(e, ind))
            }
            _ => self.expr(e, ind),
        }
    }

    /// Emit an expression. `ind` is the column level for any block this
    /// expression opens (only `match` uses it — its arms go one per line at
    /// `ind + 1`, closing brace at `ind`).
    fn expr(&self, e: &Expr, ind: usize) -> String {
        match &e.kind {
            ExprKind::Int { raw, .. } => raw.clone(),
            ExprKind::Bool(b) => self.kw(if *b { Kw::True } else { Kw::False }).to_string(),
            ExprKind::Ident(name) => name.clone(),
            ExprKind::Field { base, field } => {
                format!("{}.{}", self.operand(base, ind), field.name)
            }
            ExprKind::Unary { op, expr } => format!("{}{}", un_op(*op), self.operand(expr, ind)),
            ExprKind::Binary { op, lhs, rhs } => {
                format!(
                    "{} {} {}",
                    self.operand(lhs, ind),
                    bin_op(*op),
                    self.operand(rhs, ind)
                )
            }
            ExprKind::IfExpr { cond, then, els } => {
                let if_kw = self.kw(Kw::If);
                let else_kw = self.kw(Kw::Else);
                let cond = self.operand(cond, ind);
                let then = self.expr(then, ind);
                let els = self.expr(els, ind);
                match self.order {
                    Order::Code => format!("{if_kw} {cond} {{ {then} }} {else_kw} {{ {els} }}"),
                    Order::Thamizh => {
                        format!("{cond} {if_kw} {{ {then} }} {else_kw} {{ {els} }}")
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                let match_kw = self.kw(Kw::Match);
                let scrut = self.operand(scrutinee, ind);
                // One arm per line (the parser separates arms by newlines, not
                // commas), indented one level deeper than the opening line.
                let inner = pad(ind + 1);
                let close = pad(ind);
                let arms_src: String = arms
                    .iter()
                    .map(|a| format!("{inner}{}\n", self.arm(a, ind + 1)))
                    .collect();
                let body = format!("{{\n{arms_src}{close}}}");
                match self.order {
                    Order::Code => format!("{match_kw} {scrut} {body}"),
                    Order::Thamizh => format!("{scrut} {match_kw} {body}"),
                }
            }
            ExprKind::Concat(parts) => {
                let p = parts
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{p}}}")
            }
            ExprKind::Replicate { count, parts } => {
                let p = parts
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}{{{p}}}}}", self.expr(count, ind))
            }
            ExprKind::Index { base, index } => {
                format!("{}[{}]", self.operand(base, ind), self.expr(index, ind))
            }
            ExprKind::Slice { base, hi, lo } => {
                format!(
                    "{}[{}:{}]",
                    self.operand(base, ind),
                    self.expr(hi, ind),
                    self.expr(lo, ind)
                )
            }
            ExprKind::Call { func, args } => {
                let a = args
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({a})", builtin(*func))
            }
            ExprKind::FnCall { name, args } => {
                let a = args
                    .iter()
                    .map(|e| self.expr(e, ind))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({a})", name.name)
            }
            ExprKind::BundleLit(inits) => {
                let fields = inits
                    .iter()
                    .map(|fi| format!("{}: {}", fi.name.name, self.expr(&fi.value, ind)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {fields} }}")
            }
        }
    }

    fn arm(&self, a: &Arm, ind: usize) -> String {
        let pats = a
            .patterns
            .iter()
            .map(pattern)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{pats} => {}", self.expr(&a.value, ind))
    }
}

fn param_ty(t: ParamTy) -> &'static str {
    match t {
        ParamTy::Int => "int",
        ParamTy::Bool => "bool",
    }
}

fn pattern(p: &Pattern) -> String {
    match p {
        Pattern::Int { raw, .. } => raw.clone(),
        Pattern::IntMask { raw, .. } => raw.clone(),
        Pattern::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Pattern::Variant {
            enum_name,
            variant,
            bindings,
        } => {
            if bindings.is_empty() {
                format!("{}.{}", enum_name.name, variant.name)
            } else {
                let bs = bindings
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}.{}({bs})", enum_name.name, variant.name)
            }
        }
        Pattern::Wildcard => "_".to_string(),
    }
}

fn un_op(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::BitNot => "~",
        UnOp::LogicNot => "!",
        UnOp::RedAnd => "&",
        UnOp::RedOr => "|",
        UnOp::RedXor => "^",
    }
}

fn bin_op(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::AddWrap => "+%",
        BinOp::SubWrap => "-%",
        BinOp::MulWrap => "*%",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::LogicAnd => "&&",
        BinOp::LogicOr => "||",
    }
}

fn builtin(b: Builtin) -> &'static str {
    match b {
        Builtin::Extend => "extend",
        Builtin::Trunc => "trunc",
        Builtin::SignedCast => "signed",
        Builtin::UnsignedCast => "unsigned",
        Builtin::Min => "min",
        Builtin::Max => "max",
        Builtin::Abs => "abs",
        Builtin::Nand => "nand",
        Builtin::Nor => "nor",
        Builtin::Xnor => "xnor",
        Builtin::Clog2 => "clog2",
    }
}
