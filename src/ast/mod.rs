//! The ONE shared AST (architecture invariant #1): no keyword-flavor or
//! word-order information survives past the parser. Spans everywhere.
//!
//! Module layout:
//! - `mod.rs`  — files, modules, declarations, sequential/test statements
//! - `expr.rs` — expressions, patterns, operators (re-exported here)

// Several fields are populated by the parser but only consumed by passes
// that land later in Phase 1/1.5 (checker, simulator, translate). Keep the
// contract complete now; drop this allow once those passes exist.
#![allow(dead_code)]

mod expr;
pub use expr::*;

use std::cell::Cell;

use crate::span::Span;

/// One parsed `.mimz` source file: imports first, then top-level items.
#[derive(Clone, Debug)]
pub struct File {
    pub imports: Vec<Import>,
    pub items: Vec<TopItem>,
}

/// `import lib.adder` — path segments; resolution to a real file is
/// `project::load_project`'s job, not the AST's.
#[derive(Clone, Debug)]
pub struct Import {
    pub path: Vec<Ident>,
    pub span: Span,
    /// Which loaded file this import resolved to, filled in by
    /// `project::load_project_with_lib` once the full file list is
    /// assembled (Task 3). `None` for ASTs built without going through
    /// `project.rs` (the in-memory playground, the LSP's own import walk).
    pub resolved_file: Cell<Option<usize>>,
}

/// Anything that may appear at file level (spec/02 section 1).
#[derive(Clone, Debug)]
pub enum TopItem {
    Const(ConstDecl),
    Module(Module),
    Enum(EnumDecl),
    Test(TestDecl),
    /// A user-defined combinational function (`fn name(params) -> ret { ... }`).
    /// Functions are pure and combinational — no registers, no clocks. The parser
    /// produces this starting in Task 3; no existing checker/emitter/sim path
    /// generates it yet.
    Func(FuncDecl),
    /// A file-level bundle declaration (`bundle Foo { ... }`).
    Bundle(BundleDecl),
    /// A top-level item that failed to parse. Produced ONLY by
    /// `parser::parse_recover` (the LSP path); the strict `parser::parse`
    /// pipeline never yields one, so codegen never sees it. The span covers
    /// the skipped source so tooling can locate the hole.
    Error(Span),
}

/// A user-defined combinational function declaration.
///
/// ```text
/// fn add(a: bits[4], b: bits[4]) -> bits[5] {
///     let sum = a + b
///     sum
/// }
/// ```
///
/// - `params` — the function's input parameters (each a hardware `Type`).
/// - `ret`    — the return type (a hardware `Type`).
/// - `stmts`  — statements (`let`, statement-level `if`, `return`) before the tail.
/// - `tail`   — the function's fallthrough value: always present, so every
///   function has a well-defined result on every path even without a
///   `return` firing.
#[derive(Clone, Debug)]
pub struct FuncDecl {
    /// The function name as written in source.
    pub name: Ident,
    /// Input parameters in declaration order.
    pub params: Vec<FnParam>,
    /// Return type.
    pub ret: Type,
    /// Statements in the function body, in order.
    pub stmts: Vec<FnStmt>,
    /// The fallthrough return value if no `return` statement fires.
    pub tail: Expr,
    /// Source span covering the whole declaration.
    pub span: Span,
}

/// A statement inside a `fn` body. Mirrors [`SeqStmt`]'s shape (`on`-block
/// statements) — same idea, different terminal node (`Return` instead of
/// `Assign`/`Default`, since a function produces a value rather than
/// driving a register).
#[derive(Clone, Debug)]
pub enum FnStmt {
    /// `let name = value` — an immutable local binding.
    Let(LocalLet),
    /// Statement-level `if` (distinct from the expression-level `if`, which
    /// lives in [`ExprKind::IfExpr`] and requires `else`). `else` is
    /// OPTIONAL here — a branch that doesn't return just falls through to
    /// the next statement (or ultimately `tail`).
    If {
        cond: Expr,
        then: Vec<FnStmt>,
        els: Option<Vec<FnStmt>>,
    },
    /// `return expr` — immediately yields `expr` as the function's result;
    /// no later statement or `tail` executes for this control path.
    Return(Expr),
    /// A statement that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

/// One input parameter of a user-defined function.
///
/// Distinct from [`Param`] (which is a compile-time `int`/`bool` module
/// parameter). `FnParam.ty` is a hardware [`Type`] (`bit`, `bits[N]`, …).
#[derive(Clone, Debug)]
pub struct FnParam {
    /// Parameter name as written in source.
    pub name: Ident,
    /// Hardware type of this parameter.
    pub ty: Type,
    /// Source span of the parameter declaration.
    pub span: Span,
}

/// A `let` binding inside a function body: `let name = value`.
///
/// `value` is an expression evaluated combinationally when the function is
/// called. The bound name is in scope for subsequent `stmts` and the `tail`.
///
/// `inferred_width` is filled by the checker's width pass so the emitter
/// can declare a sized `reg [W-1:0]` rather than a 32-bit `integer` (R6).
/// Initialized to `None` by the parser; always `Some` after the checker runs.
#[derive(Clone, Debug)]
pub struct LocalLet {
    /// The bound name.
    pub name: Ident,
    /// The defining expression.
    pub value: Expr,
    /// Source span of the `let` statement.
    pub span: Span,
    /// Concrete bit-width inferred by the checker's width pass.
    /// Set via interior mutability so the checker can annotate through a
    /// shared `&FuncDecl` reference. `None` until the checker runs.
    pub inferred_width: Cell<Option<u32>>,
}

/// A name with its source location. Used everywhere a user-written name
/// appears so errors can always point at it.
#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A possibly-namespaced reference: bare `Name` (`path` empty) or
/// `a.b.Name` (`path = [a, b]`). The bare case parses identically to a
/// plain `Ident` — existing single-segment references are unaffected.
/// `resolved_file` is filled in once by the checker's name-resolution pass
/// (spec/02 section 1.5b); later passes read it instead of re-running
/// ambiguity/qualifier resolution — same pattern as `Expr::inferred_width`.
#[derive(Clone, Debug)]
pub struct QualIdent {
    pub path: Vec<Ident>,
    pub name: Ident,
    pub span: Span,
    pub resolved_file: Cell<Option<usize>>,
}

impl QualIdent {
    /// `Name` (bare) or `a.b.Name` (qualified), dot-joined — round-trips
    /// through `pretty.rs`.
    pub fn to_dotted(&self) -> String {
        let mut s = String::new();
        for seg in &self.path {
            s.push_str(&seg.name);
            s.push('.');
        }
        s.push_str(&self.name.name);
        s
    }

    /// True when written with no path segments — the pre-existing form.
    pub fn is_bare(&self) -> bool {
        self.path.is_empty()
    }

    /// The actual qualified-reference disambiguation mechanism (spec/02
    /// section 1.5b, design doc §4.4): match `self.path` against `imports`
    /// — the REFERENCING file's own `import` statements, segment-by-segment
    /// by name — and, on an exact match whose import itself already
    /// resolved to a real file (`Import.resolved_file`, set once by
    /// `project::load_project` per Task 3), cache that file index onto
    /// `self.resolved_file`. No-op when already resolved (idempotent — safe
    /// to call from multiple passes/call sites) or when `self` is bare (bare
    /// references resolve a different way, via ambiguity-checked project-wide
    /// lookup). Shared by both the checker (`checker::names::resolve`) and
    /// the simulator (`sim::elaborate::resolve_module`/`resolve_bundle`),
    /// which run this independently since `mimz sim`/`mimz test` do not run
    /// the checker first.
    pub fn resolve_via_imports(&self, imports: &[Import]) {
        if self.is_bare() || self.resolved_file.get().is_some() {
            return;
        }
        if let Some(target) = imports
            .iter()
            .find(|imp| {
                imp.path.len() == self.path.len()
                    && imp
                        .path
                        .iter()
                        .zip(&self.path)
                        .all(|(a, b)| a.name == b.name)
            })
            .and_then(|imp| imp.resolved_file.get())
        {
            self.resolved_file.set(Some(target));
        }
    }
}

/// `module Name(P: int = 8) { ... }` — the unit of hardware design.
#[derive(Clone, Debug)]
pub struct Module {
    pub name: Ident,
    /// Compile-time parameters (`WIDTH: int = 8`).
    pub params: Vec<Param>,
    pub items: Vec<ModuleItem>,
    pub span: Span,
}

/// Type of a compile-time parameter or constant. Only `int` and `bool` —
/// hardware types (`bits[N]` etc.) are not compile-time values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamTy {
    Int,
    Bool,
}

/// One module parameter: `WIDTH: int = 8`. A `None` default means the
/// instantiation must provide a value.
#[derive(Clone, Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: ParamTy,
    pub default: Option<Expr>,
}

/// `const NAME: int = expr` — file-level or module-level compile-time value.
#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub name: Ident,
    pub ty: ParamTy,
    pub value: Expr,
}

/// `enum State { Red, Green }` — variants encode to the smallest binary
/// width (`clog2(variant count)`); the emitter renders them as localparams.
/// Tagged variants carry typed payload fields; tag-only variants have
/// `fields: vec![]`.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: Ident,
    /// Variant list — tag-only variants have `fields: vec![]`.
    pub variants: Vec<EnumVariant>,
    /// Span of the whole declaration (from `enum` keyword to closing `}`).
    pub span: Span,
    /// Total wire width computed by the checker's width pass: `tag_w + max_payload_w`.
    /// `None` until the checker runs; always `Some` after. Interior mutability
    /// mirrors [`LocalLet::inferred_width`] so the checker can annotate through
    /// a shared `&EnumDecl` reference.
    pub inferred_total_width: Cell<Option<u32>>,
}

/// One variant inside an `enum` declaration.
///
/// ```text
/// enum Packet { Read(addr: bits[32]), Nop }
///               ^^^^^^^^^^^^^^^^^^^^ ^^^^^
///               tagged variant       tag-only (fields is empty)
/// ```
#[derive(Clone, Debug)]
pub struct EnumVariant {
    /// The variant name.
    pub name: Ident,
    /// Payload fields in declaration order; empty for tag-only variants.
    pub fields: Vec<PayloadField>,
    /// Span covering the variant (name + optional field list).
    pub span: Span,
}

/// One named payload field inside a tagged variant.
///
/// ```text
/// Read(addr: bits[32])
///      ^^^^^^^^^^^^^^
/// ```
///
/// The `name` is documentation-only; bindings in match patterns are
/// positional (design decision D2).
#[derive(Clone, Debug)]
pub struct PayloadField {
    /// Documentation name (not used as a binding in patterns).
    pub name: Ident,
    /// Hardware type; must be a concrete bit-vector (E0807 if not).
    pub ty: Type,
    /// Span covering `name: type`.
    pub span: Span,
}

/// `bundle Name(params) { fields }` — a named group of signals.
/// File-level only (like `enum`); flattened to individual Verilog wires at emit.
#[derive(Clone, Debug)]
pub struct BundleDecl {
    pub name: Ident,
    /// Compile-time parameters (same grammar as module params).
    pub params: Vec<Param>,
    /// Field declarations in order.
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

/// One field in a `bundle` declaration: `valid: bit`.
#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub name: Ident,
    /// Hardware type — must be concrete bit-vector or enum (E0807/E0905).
    pub ty: Type,
    pub span: Span,
}

/// Port direction (`in` / `out`). No `inout` — it is a reserved word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    In,
    Out,
}

/// Anything that may appear in a module body. Declaration order is free —
/// the emitter regroups (ports, declarations, instances, assigns, always).
#[derive(Clone, Debug)]
pub enum ModuleItem {
    /// `in name: type` / `out name: type`.
    Port {
        dir: Dir,
        name: Ident,
        ty: Type,
    },
    /// `clock clk` — clocks are a dedicated type, never plain bits (safety
    /// rule: clock-domain typing).
    Clock(Ident),
    /// `reset rst` (synchronous, active-high) or `async reset rst`
    /// (asynchronous). `is_async` widens every always-block that uses this
    /// reset to `@(… or posedge rst)`; polarity stays active-high (v0.2).
    Reset {
        name: Ident,
        is_async: bool,
    },
    /// `wire name: type = expr` — declared and driven in one statement;
    /// an undriven wire cannot be written.
    Wire {
        name: Ident,
        ty: Type,
        init: Expr,
    },
    /// `reg name: type = reset_value` — the reset value is mandatory
    /// (safety rule: no uninitialized state).
    Reg {
        name: Ident,
        ty: Type,
        reset: Expr,
    },
    /// `mem name: element_type[DEPTH] = init` — an addressable memory of
    /// `DEPTH` elements. Read combinationally (`m[addr]`), written on a clock
    /// (`m[addr] <- v` inside `on`). The init value is mandatory and seeds
    /// every cell at power-on (safety rule: no uninitialized state); `depth`
    /// must const-evaluate to a positive width.
    Mem {
        name: Ident,
        ty: Type,
        depth: Expr,
        init: Expr,
    },
    Const(ConstDecl),
    Enum(EnumDecl),
    /// Child-module instantiation (`let u = Adder(...) { ... }`).
    Inst(Inst),
    /// Sequential block (`on rise(clk) { ... }`).
    On(OnBlock),
    /// Combinational drive of an output port or a slice: `count = value`.
    Drive {
        lhs: LValue,
        rhs: Expr,
    },
    /// Compile-time generation (`repeat i: 0..8 { ... }`).
    Repeat(Repeat),
    /// `const if (COND) { items } [else { items }]` — compile-time conditional
    /// module-body items. The losing branch is completely discarded before
    /// name resolution, type checking, and codegen (D-CONSTIF-4).
    ConstIf {
        cond: Expr,
        then: Vec<ModuleItem>,
        els: Option<Vec<ModuleItem>>,
        span: Span,
    },
    /// `let { field, ... } = expr` — bind bundle fields as local wires.
    /// Module-body only (not in `on` blocks or `fn` bodies).
    BundleDestructure {
        /// Fields to bind; partial destructure allowed.
        bindings: Vec<Ident>,
        /// The bundle-typed expression being destructured.
        expr: Expr,
        span: Span,
    },
    /// A module-body item that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

/// `repeat i: lo..hi { ... }` — compile-time unrolling, NOT a runtime loop.
/// Bounds must const-evaluate; unrolling happens in the checker pass
/// (Phase 1 work item 4), so the emitter currently rejects it cleanly.
#[derive(Clone, Debug)]
pub struct Repeat {
    pub var: Ident,
    /// Range is half-open: `lo..hi` runs `lo, lo+1, …, hi-1`.
    pub lo: Expr,
    pub hi: Expr,
    pub items: Vec<ModuleItem>,
    pub span: Span,
}

/// `let name = Module(param: value) { port: signal, ... }`.
/// Child outputs are read as `name.port`; the emitter auto-wires them.
#[derive(Clone, Debug)]
pub struct Inst {
    pub name: Ident,
    /// `let name[i] = ...` inside `repeat`.
    pub index: Option<Expr>,
    /// The module being instantiated (resolved by name, project-wide).
    pub module: QualIdent,
    /// Compile-time parameter overrides.
    pub args: Vec<NamedArg>,
    /// Input/clock/reset connections. Same-named clock/reset connect
    /// implicitly when omitted; inputs never do.
    pub conns: Vec<Conn>,
    pub span: Span,
}

/// `name: value` — one parameter binding in an instantiation or test header.
#[derive(Clone, Debug)]
pub struct NamedArg {
    pub name: Ident,
    pub value: Expr,
}

/// `port: signal` — one port connection in an instantiation.
#[derive(Clone, Debug)]
pub struct Conn {
    pub port: Ident,
    pub signal: Expr,
}

/// Which clock edge a sequential block (and its registers) triggers on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// `on rise(clk)` — Verilog `posedge`.
    Rise,
    /// `on fall(clk)` — Verilog `negedge`.
    Fall,
}

/// `on rise(clk) { ... }` / `on fall(clk) { ... }` — everything inside updates
/// registers with `<-` on the chosen `edge` of `clock`.
#[derive(Clone, Debug)]
pub struct OnBlock {
    pub clock: Ident,
    pub edge: Edge,
    pub body: Vec<SeqStmt>,
    pub span: Span,
}

/// A statement inside an `on` block. Registers may be left unassigned on
/// some paths (they hold their value) — unlike wires, no `else` is forced.
#[derive(Clone, Debug)]
pub enum SeqStmt {
    /// `lhs <- rhs` — the only register assignment.
    Assign { lhs: LValue, rhs: Expr },
    /// Statement-level `if` (distinct from the expression-level `if`,
    /// which lives in [`ExprKind::IfExpr`] and requires `else`).
    If {
        cond: Expr,
        then: Vec<SeqStmt>,
        els: Option<Vec<SeqStmt>>,
    },
    /// `default name <- expr` — priority-lowest register assignment.
    /// Emitter MUST emit these nodes FIRST within the always-block body
    /// so conditional `<-` assignments override them (D-DEFAULT-3).
    Default { name: Ident, val: Expr, span: Span },
    /// A sequential statement that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

/// Assignment target: a signal, one bit of it, or a slice.
#[derive(Clone, Debug)]
pub struct LValue {
    pub base: Ident,
    /// `[i]` or `[hi:lo]`.
    pub index: Option<(Expr, Option<Expr>)>,
    pub span: Span,
}

/// A hardware type. Widths are expressions (often parameter names like
/// `WIDTH`), not numbers — const evaluation is the checker's job.
#[derive(Clone, Debug)]
pub enum Type {
    /// `bit` — a single wire.
    Bit,
    /// `bits[N]` — unsigned N-bit vector.
    Bits(Box<Expr>),
    /// `signed[N]` — two's-complement N-bit vector; never mixes with
    /// `bits` without an explicit cast (spec/02 section 1.7).
    Signed(Box<Expr>),
    /// Enum type by name.
    Named(QualIdent),
    /// Parametric bundle type: `MemBus(WIDTH: 32)` or plain `Handshake`.
    /// `args` is empty for bundles with no params.
    /// note: nominal-only today; structural subtyping adds one field-list
    /// comparison (2.9); first-class IR bundle (post-Phase 2) promotes
    /// BundleType to a Type variant in IR
    Bundle {
        name: QualIdent,
        args: Vec<NamedArg>,
    },
    /// `<elem>[N]` — a fixed-size, immutable array value. `elem` is
    /// restricted to `Bit`/`Bits`/`Signed` (checker-enforced, E0411,
    /// matching `mem`'s own element-type restriction). `len` is a
    /// compile-time constant (checker-enforced, E0412, matching `mem`'s
    /// `DEPTH` and `repeat`'s bound). An array is never a real Verilog
    /// array — the emitter and simulator each lower it to N independent
    /// scalars (see `docs/superpowers/specs/2026-07-04-array-typed-fn-params-design.local.md`).
    Array { elem: Box<Type>, len: Box<Expr> },
}

/// `test "name" for Module(args) { ... }` — runs on the Phase 1.5
/// simulator; parsed and validated today so test files are not a dead end.
#[derive(Clone, Debug)]
pub struct TestDecl {
    /// The quoted human-readable test name.
    pub name: String,
    /// The module under test.
    pub module: QualIdent,
    /// Parameter values for this test run.
    pub args: Vec<NamedArg>,
    pub body: Vec<TestStmt>,
    pub span: Span,
}

/// A statement inside a `test` block (spec/02 section 1.10).
#[derive(Clone, Debug)]
pub enum TestStmt {
    /// `tick(clk)` / `tick(clk, n)` — advance n clock cycles (default 1).
    Tick { clock: Ident, count: Option<Expr> },
    /// `expect expr` — assert the expression is true now.
    Expect(Expr),
    /// `name = value` — drive an input of the module under test.
    Drive { name: Ident, value: Expr },
    If {
        cond: Expr,
        then: Vec<TestStmt>,
        els: Option<Vec<TestStmt>>,
    },
    /// A test statement that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    #[test]
    fn bundle_decl_node_constructs() {
        let span = Span::new(0, 0);
        let b = BundleDecl {
            name: Ident {
                name: "MemBus".into(),
                span,
            },
            params: vec![],
            fields: vec![FieldDecl {
                name: Ident {
                    name: "valid".into(),
                    span,
                },
                ty: Type::Bit,
                span,
            }],
            span,
        };
        let _item = TopItem::Bundle(b);
        let _destr = ModuleItem::BundleDestructure {
            bindings: vec![Ident {
                name: "valid".into(),
                span,
            }],
            expr: Expr {
                kind: ExprKind::Ident("bus".into()),
                span,
            },
            span,
        };
        let _ty = Type::Bundle {
            name: QualIdent {
                path: vec![],
                name: Ident {
                    name: "MemBus".into(),
                    span,
                },
                span,
                resolved_file: Cell::new(None),
            },
            args: vec![],
        };
        let _lit = ExprKind::BundleLit(vec![FieldInit {
            name: Ident {
                name: "valid".into(),
                span,
            },
            value: Expr {
                kind: ExprKind::Bool(true),
                span,
            },
            span,
        }]);
    }

    #[test]
    fn func_decl_node_constructs() {
        let sp = Span::new(0, 0);
        let _ = TopItem::Func(FuncDecl {
            name: Ident {
                name: "f".into(),
                span: sp,
            },
            params: vec![],
            ret: Type::Bit,
            stmts: vec![],
            tail: Expr {
                kind: ExprKind::Bool(true),
                span: sp,
            },
            span: sp,
        });
    }

    #[test]
    fn array_type_constructs() {
        let sp = Span::new(0, 0);
        let _ty = Type::Array {
            elem: Box::new(Type::Bits(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 8,
                    raw: "8".into(),
                },
                span: sp,
            }))),
            len: Box::new(Expr {
                kind: ExprKind::Int {
                    value: 4,
                    raw: "4".into(),
                },
                span: sp,
            }),
        };
        let _lit = ExprKind::ArrayLit(vec![
            Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp,
            },
            Expr {
                kind: ExprKind::Int {
                    value: 1,
                    raw: "1".into(),
                },
                span: sp,
            },
        ]);
    }
}
