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
mod sync_loop_lower;
pub use sync_loop_lower::lower_sync_loop;
mod foreach_lower;
pub use foreach_lower::{
    array_like_len, array_like_len_fn, lower_foreach_fn, lower_foreach_item, lower_foreach_seq,
};

use std::cell::Cell;

use crate::span::Span;

/// One parsed `.mimz` source file: imports first, then top-level items.
#[derive(Clone, Debug)]
pub struct File {
    /// `import` statements, in source order.
    pub imports: Vec<Import>,
    /// Top-level items (modules, enums, tests, ...), in source order.
    pub items: Vec<TopItem>,
}

/// `import lib.adder` — path segments; resolution to a real file is
/// `project::load_project`'s job, not the AST's.
#[derive(Clone, Debug)]
pub struct Import {
    /// Dotted path segments (`lib.adder` → `[lib, adder]`).
    pub path: Vec<Ident>,
    /// Source span of the `import` statement.
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
    /// A file-level `const` declaration.
    Const(ConstDecl),
    /// A `module` declaration.
    Module(Module),
    /// A file-level `enum` declaration.
    Enum(EnumDecl),
    /// A `test` block.
    Test(TestDecl),
    /// A user-defined combinational function (`fn name(params) -> ret { ... }`).
    /// Functions are pure and combinational — no registers, no clocks. The parser
    /// produces this starting in Task 3; no existing checker/emitter/sim path
    /// generates it yet.
    Func(FuncDecl),
    /// A file-level bundle declaration (`bundle Foo { ... }`).
    Bundle(BundleDecl),
    /// An `extern module` declaration (Verilog FFI).
    ExternModule(ExternModule),
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
        /// The condition; must be 1-bit.
        cond: Expr,
        /// Statements to run when `cond` is true.
        then: Vec<FnStmt>,
        /// Statements to run when `cond` is false, if an `else` was written.
        els: Option<Vec<FnStmt>>,
    },
    /// `return expr` — immediately yields `expr` as the function's result;
    /// no later statement or `tail` executes for this control path.
    Return(Expr),
    /// `loop i: lo..hi { ... }` — compile-time unrolling inside a `fn`
    /// body. Unrolls into `hi-lo` copies of `body`, each a fresh scope;
    /// combined with `return` gives first-match-wins search over an
    /// array/mem.
    Loop {
        /// The loop variable, bound to each value in `lo..hi` in turn.
        var: Ident,
        /// Range lower bound (inclusive); must const-evaluate.
        lo: Expr,
        /// Range upper bound (exclusive); must const-evaluate.
        hi: Expr,
        /// Statements unrolled once per iteration.
        body: Vec<FnStmt>,
        /// Source span of the `loop` statement.
        span: Span,
    },
    /// `foreach <var> in <source> { ... }` — statement-level sugar over
    /// bare `loop`, usable inside a `fn` body. See `ForEach`'s doc
    /// comment (the module-item form) for the shared semantics.
    ForEach {
        /// The bound name — an index (Range form) or an element value
        /// (Elements form).
        var: Ident,
        /// Where the values come from.
        source: ForEachSource,
        /// Statements unrolled once per iteration.
        body: Vec<FnStmt>,
        /// Source span of the whole `foreach` statement.
        span: Span,
    },
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
    /// The name text as written.
    pub name: String,
    /// Source span of the name.
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
    /// Leading path segments (`[a, b]` in `a.b.Name`); empty for a bare name.
    pub path: Vec<Ident>,
    /// The final, referenced name.
    pub name: Ident,
    /// Source span covering the whole (possibly dotted) reference.
    pub span: Span,
    /// The file this reference resolved to, filled in once by the checker's
    /// or simulator's name-resolution pass. `None` until resolved.
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
    /// The module name.
    pub name: Ident,
    /// Compile-time parameters (`WIDTH: int = 8`).
    pub params: Vec<Param>,
    /// Body items (ports, declarations, instances, `on` blocks, ...), in
    /// source order.
    pub items: Vec<ModuleItem>,
    /// Source span of the whole `module` declaration.
    pub span: Span,
}

/// `extern module Name(params) { doc: "...", ports }` — declares the port
/// shape of a real Verilog module living outside Min-Mozhi (Verilog FFI).
/// No body: nothing here is elaborated, checked for drivers, or emitted as
/// logic — `items` is restricted (by the checker, not the parser) to
/// `Port`/`Clock`/`Reset` variants only, with scalar-typed ports only.
/// Deliberately shaped like `Module` minus a body so every existing
/// connection-check/emission function that already walks `.items`
/// generically needs no changes — see `ModuleTarget`.
#[derive(Clone, Debug)]
pub struct ExternModule {
    /// The Min-Mozhi-facing name (what instantiation sites use).
    pub name: Ident,
    /// The real Verilog module's name, if it differs from `name`
    /// (`extern module Pll = "PLL_HARD_IP_v2" { ... }`). `None` means the
    /// real module is literally named `name`.
    pub verilog_name: Option<String>,
    /// Compile-time parameters — identical shape/checking to `Module::params`.
    pub params: Vec<Param>,
    /// Human-readable behavior notes (`doc: "..."`) — there is no body to
    /// document inline, so this is the construct's one place for that
    /// context. Not consumed by any compiler pass today; reserved for
    /// hover/`mimz doc` tooling.
    pub doc: Option<String>,
    /// Port/clock/reset declarations — restricted to `ModuleItem::Port`
    /// (scalar-typed only) / `ModuleItem::Clock` / `ModuleItem::Reset` by
    /// the checker (Task 3); the parser (this task) accepts the same
    /// syntax `Module`'s body does for these three item kinds only.
    pub items: Vec<ModuleItem>,
    /// Source span of the whole `extern module` declaration.
    pub span: Span,
}

/// Resolves an instantiation target to either a real module or an extern
/// declaration — the one type every checker/emitter/simulator resolution
/// call site is retyped to, so connection-checking/emission logic (which
/// only ever reads `.name`/`.params`/`.items`) works identically for both
/// without duplicating a single line per call site.
#[derive(Clone, Copy, Debug)]
pub enum ModuleTarget<'a> {
    /// A real, elaboratable module.
    Real(&'a Module),
    /// An extern declaration — port shape only, no body.
    Extern(&'a ExternModule),
}

impl<'a> ModuleTarget<'a> {
    pub fn name(&self) -> &'a Ident {
        match self {
            ModuleTarget::Real(m) => &m.name,
            ModuleTarget::Extern(e) => &e.name,
        }
    }
    pub fn params(&self) -> &'a [Param] {
        match self {
            ModuleTarget::Real(m) => &m.params,
            ModuleTarget::Extern(e) => &e.params,
        }
    }
    pub fn items(&self) -> &'a [ModuleItem] {
        match self {
            ModuleTarget::Real(m) => &m.items,
            ModuleTarget::Extern(e) => &e.items,
        }
    }
    /// `true` for an extern target — callers use this the few places
    /// behavior must genuinely differ (e.g. "don't try to elaborate a
    /// body", "use `verilog_name` instead of `name` when emitting").
    pub fn is_extern(&self) -> bool {
        matches!(self, ModuleTarget::Extern(_))
    }
}

/// Type of a compile-time parameter or constant. Only `int` and `bool` —
/// hardware types (`bits[N]` etc.) are not compile-time values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamTy {
    /// A compile-time integer parameter or constant.
    Int,
    /// A compile-time boolean parameter or constant.
    Bool,
}

/// One module parameter: `WIDTH: int = 8`. A `None` default means the
/// instantiation must provide a value.
#[derive(Clone, Debug)]
pub struct Param {
    /// The parameter name.
    pub name: Ident,
    /// Compile-time type (`int` or `bool`).
    pub ty: ParamTy,
    /// Default value, if one was written; `None` means instantiation must
    /// supply it.
    pub default: Option<Expr>,
}

/// `const NAME: int = expr` — file-level or module-level compile-time value.
#[derive(Clone, Debug)]
pub struct ConstDecl {
    /// The constant's name.
    pub name: Ident,
    /// Compile-time type (`int` or `bool`).
    pub ty: ParamTy,
    /// The constant's defining expression.
    pub value: Expr,
}

/// `enum State { Red, Green }` — variants encode to the smallest binary
/// width (`clog2(variant count)`); the emitter renders them as localparams.
/// Tagged variants carry typed payload fields; tag-only variants have
/// `fields: vec![]`.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    /// The enum type's name.
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
    /// The bundle type's name.
    pub name: Ident,
    /// Compile-time parameters (same grammar as module params).
    pub params: Vec<Param>,
    /// Field declarations in order.
    pub fields: Vec<FieldDecl>,
    /// Source span of the whole `bundle` declaration.
    pub span: Span,
}

/// One field in a `bundle` declaration: `valid: bit`.
#[derive(Clone, Debug)]
pub struct FieldDecl {
    /// The field's name.
    pub name: Ident,
    /// Hardware type — must be concrete bit-vector or enum (E0807/E0905).
    pub ty: Type,
    /// Source span of `name: type`.
    pub span: Span,
}

/// Port direction (`in` / `out`). No `inout` — it is a reserved word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// `in name: type` — a module input.
    In,
    /// `out name: type` — a module output.
    Out,
}

/// Anything that may appear in a module body. Declaration order is free —
/// the emitter regroups (ports, declarations, instances, assigns, always).
#[derive(Clone, Debug)]
pub enum ModuleItem {
    /// `in name: type` / `out name: type`.
    Port {
        /// Input or output.
        dir: Dir,
        /// The port name.
        name: Ident,
        /// Hardware type of the port.
        ty: Type,
    },
    /// `clock clk` — clocks are a dedicated type, never plain bits (safety
    /// rule: clock-domain typing).
    Clock(Ident),
    /// `reset rst` (synchronous, active-high) or `async reset rst`
    /// (asynchronous). `is_async` widens every always-block that uses this
    /// reset to `@(… or posedge rst)`; polarity stays active-high (v0.2).
    Reset {
        /// The reset signal's name.
        name: Ident,
        /// `true` for `async reset`, `false` for a synchronous reset.
        is_async: bool,
    },
    /// `wire name: type = expr` — declared and driven in one statement;
    /// an undriven wire cannot be written.
    Wire {
        /// The wire's name.
        name: Ident,
        /// Hardware type of the wire.
        ty: Type,
        /// The driving expression.
        init: Expr,
    },
    /// `reg name: type = reset_value` — the reset value is mandatory
    /// (safety rule: no uninitialized state).
    Reg {
        /// The register's name.
        name: Ident,
        /// Hardware type of the register.
        ty: Type,
        /// Value the register takes on reset.
        reset: Expr,
    },
    /// `mem name: element_type[DEPTH] = init` — an addressable memory of
    /// `DEPTH` elements. Read combinationally (`m[addr]`), written on a clock
    /// (`m[addr] <- v` inside `on`). The init value is mandatory and seeds
    /// every cell at power-on (safety rule: no uninitialized state); `depth`
    /// must const-evaluate to a positive width.
    Mem {
        /// The memory's name.
        name: Ident,
        /// Element type of each cell.
        ty: Type,
        /// Number of elements; must const-evaluate to a positive width.
        depth: Expr,
        /// Value every cell is seeded with at power-on.
        init: Expr,
    },
    /// A module-level `const` declaration.
    Const(ConstDecl),
    /// A module-level `enum` declaration.
    Enum(EnumDecl),
    /// Child-module instantiation (`let u = Adder(...) { ... }`).
    Inst(Inst),
    /// Sequential block (`on rise(clk) { ... }`).
    On(OnBlock),
    /// Combinational drive of an output port or a slice: `count = value`.
    Drive {
        /// The signal, bit, or slice being driven.
        lhs: LValue,
        /// The driving expression.
        rhs: Expr,
    },
    /// Compile-time generation (`repeat i: 0..8 { ... }`).
    Repeat(Repeat),
    /// `foreach <var> in <source> { ... }` — module-item-level sugar over
    /// `repeat`; see `ForEach`'s doc comment.
    ForEach(ForEach),
    /// `sync loop <name> on rise(clk) (var: lo..hi) -> result: ty = init { ... }`
    /// — cycle-iterating loop; see `SyncLoop` doc comment.
    SyncLoop(Box<SyncLoop>),
    /// `const if (COND) { items } [else { items }]` — compile-time conditional
    /// module-body items. The losing branch is completely discarded before
    /// name resolution, type checking, and codegen (D-CONSTIF-4).
    ConstIf {
        /// The compile-time condition; must const-evaluate to a bool.
        cond: Expr,
        /// Items kept when `cond` is true.
        then: Vec<ModuleItem>,
        /// Items kept when `cond` is false, if an `else` was written.
        els: Option<Vec<ModuleItem>>,
        /// Source span of the whole `const if`.
        span: Span,
    },
    /// `let { field, ... } = expr` — bind bundle fields as local wires.
    /// Module-body only (not in `on` blocks or `fn` bodies).
    BundleDestructure {
        /// Fields to bind; partial destructure allowed.
        bindings: Vec<Ident>,
        /// The bundle-typed expression being destructured.
        expr: Expr,
        /// Source span of the whole destructuring statement.
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
    /// The compile-time loop variable, bound in `items` for each iteration.
    pub var: Ident,
    /// Range is half-open: `lo..hi` runs `lo, lo+1, …, hi-1`.
    pub lo: Expr,
    /// Range upper bound (exclusive); must const-evaluate.
    pub hi: Expr,
    /// Module items unrolled once per iteration.
    pub items: Vec<ModuleItem>,
    /// Source span of the whole `repeat` block.
    pub span: Span,
}

/// `sync loop <name> on rise(clk) (var: lo..hi) -> result: ty = init { body }`
/// — a module-item-level cycle-iterating loop. Lowers (see
/// `ast::sync_loop_lower::lower_sync_loop`) to synthesized `Port`/`Reg`/`On`/
/// `Drive` items: a counter + running/done state machine spanning
/// `hi - lo` clock cycles, NOT elaboration-time unrolling (contrast
/// `Repeat`/`SeqStmt::Loop`). Bounds must const-evaluate, same requirement
/// as `Repeat`'s `lo`/`hi`.
#[derive(Clone, Debug)]
pub struct SyncLoop {
    /// Instance name — namespaces the four generated signals
    /// (`<name>_start`/`_done`/`_result`/`_running`).
    pub name: Ident,
    /// The clock driving the underlying counter/state machine.
    pub clock: Ident,
    /// Which edge of `clock` advances the loop.
    pub edge: Edge,
    /// The loop variable, bound to the live counter value each cycle
    /// inside `body` (a runtime signal, unlike `Repeat`'s compile-time var).
    pub var: Ident,
    /// Range lower bound (inclusive); must const-evaluate.
    pub lo: Expr,
    /// Range upper bound (exclusive); must const-evaluate.
    pub hi: Expr,
    /// The accumulator's name as written (e.g. `result` in
    /// `-> result: bits[8] = 0`) — used inside `body` via `<-`.
    pub result_name: Ident,
    /// Hardware type of the accumulator.
    pub result_ty: Type,
    /// The accumulator's value before the loop starts.
    pub result_init: Expr,
    /// Statements run each cycle while the loop is active.
    pub body: Vec<SeqStmt>,
    /// Source span of the whole `sync loop` declaration.
    pub span: Span,
}

/// Where a `foreach` loop pulls its values from.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ForEachSource {
    /// `foreach i in lo..hi` — same range shape as `repeat`/bare `loop`.
    Range {
        /// Range lower bound (inclusive); must const-evaluate.
        lo: Expr,
        /// Range upper bound (exclusive); must const-evaluate.
        hi: Expr,
    },
    /// `foreach x in arr` — binds each element of an array/`mem`-typed
    /// identifier by value. `arr` must be a bare identifier naming an
    /// array- or `mem`-typed `Port`/`Wire`/`Reg`/`Mem` declared in the
    /// enclosing module (checker-enforced, E0417) — arbitrary expressions
    /// are not resolved for length in v1.
    Elements(Ident),
}

/// `foreach <var> in <source> { ... }` — module-item-level sugar over
/// `repeat`. Preserved through parse/pretty/lint/translit for fidelity;
/// the checker validates this node directly for the Elements-form source
/// check (E0417), then delegates width/driver/clock checking to the
/// lowered `Repeat` form (see `ast::foreach_lower::lower_foreach_item`).
/// Emit/sim never see this node — only the lowered `Repeat`.
#[derive(Clone, Debug)]
pub struct ForEach {
    /// The bound name — an index (Range form) or an element value
    /// (Elements form).
    pub var: Ident,
    /// Where the values come from.
    pub source: ForEachSource,
    /// Module items unrolled once per iteration.
    pub items: Vec<ModuleItem>,
    /// Source span of the whole `foreach` block.
    pub span: Span,
}

/// `let name = Module(param: value) { port: signal, ... }`.
/// Child outputs are read as `name.port`; the emitter auto-wires them.
#[derive(Clone, Debug)]
pub struct Inst {
    /// The instance name (`let name = ...`).
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
    /// Source span of the whole instantiation.
    pub span: Span,
}

/// `name: value` — one parameter binding in an instantiation or test header.
#[derive(Clone, Debug)]
pub struct NamedArg {
    /// The parameter name.
    pub name: Ident,
    /// The bound value.
    pub value: Expr,
}

/// `port: signal` — one port connection in an instantiation.
#[derive(Clone, Debug)]
pub struct Conn {
    /// The port being connected on the child module.
    pub port: Ident,
    /// The signal (in the parent module) driving/read from it.
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
    /// The triggering clock signal.
    pub clock: Ident,
    /// Which edge of `clock` triggers this block.
    pub edge: Edge,
    /// Statements run on each trigger.
    pub body: Vec<SeqStmt>,
    /// Source span of the whole `on` block.
    pub span: Span,
}

/// A statement inside an `on` block. Registers may be left unassigned on
/// some paths (they hold their value) — unlike wires, no `else` is forced.
#[derive(Clone, Debug)]
pub enum SeqStmt {
    /// `lhs <- rhs` — the only register assignment.
    Assign {
        /// The register, bit, or slice being assigned.
        lhs: LValue,
        /// The value assigned on the triggering edge.
        rhs: Expr,
    },
    /// Statement-level `if` (distinct from the expression-level `if`,
    /// which lives in [`ExprKind::IfExpr`] and requires `else`).
    If {
        /// The condition; must be 1-bit.
        cond: Expr,
        /// Statements to run when `cond` is true.
        then: Vec<SeqStmt>,
        /// Statements to run when `cond` is false, if an `else` was written.
        els: Option<Vec<SeqStmt>>,
    },
    /// `default name <- expr` — priority-lowest register assignment.
    /// Emitter MUST emit these nodes FIRST within the always-block body
    /// so conditional `<-` assignments override them (D-DEFAULT-3).
    Default {
        /// The register being defaulted.
        name: Ident,
        /// The default value.
        val: Expr,
        /// Source span of the `default` statement.
        span: Span,
    },
    /// `loop i: lo..hi { ... }` — compile-time unrolling inside an `on`
    /// block, same model as `repeat` but usable in a clocked context
    /// (`repeat` itself stays item-level only). NOT a runtime loop —
    /// unrolls into `hi-lo` copies of `body` at elaboration time.
    Loop {
        /// The compile-time loop variable, bound in `body` for each iteration.
        var: Ident,
        /// Range lower bound (inclusive); must const-evaluate.
        lo: Expr,
        /// Range upper bound (exclusive); must const-evaluate.
        hi: Expr,
        /// Statements unrolled once per iteration.
        body: Vec<SeqStmt>,
        /// Source span of the whole `loop` statement.
        span: Span,
    },
    /// `foreach <var> in <source> { ... }` — statement-level sugar over
    /// bare `loop`, usable inside an `on` block. See `ForEach`'s doc
    /// comment (the module-item form) for the shared semantics; this
    /// variant is inline (matching `Loop`'s shape) rather than wrapping
    /// the `ForEach` struct, since `body`'s element type differs
    /// per context.
    ForEach {
        /// The bound name — an index (Range form) or an element value
        /// (Elements form).
        var: Ident,
        /// Where the values come from.
        source: ForEachSource,
        /// Statements unrolled once per iteration.
        body: Vec<SeqStmt>,
        /// Source span of the whole `foreach` statement.
        span: Span,
    },
    /// A sequential statement that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

/// Assignment target: a signal, one bit of it, or a slice.
#[derive(Clone, Debug)]
pub struct LValue {
    /// The signal being assigned.
    pub base: Ident,
    /// `[i]` or `[hi:lo]`.
    pub index: Option<(Expr, Option<Expr>)>,
    /// Source span of the whole assignment target.
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
        /// The bundle type's name.
        name: QualIdent,
        /// Compile-time parameter overrides (empty for parameterless bundles).
        args: Vec<NamedArg>,
    },
    /// `<elem>[N]` — a fixed-size, immutable array value. `elem` is
    /// restricted to `Bit`/`Bits`/`Signed` (checker-enforced, E0411,
    /// matching `mem`'s own element-type restriction). `len` is a
    /// compile-time constant (checker-enforced, E0412, matching `mem`'s
    /// `DEPTH` and `repeat`'s bound). An array is never a real Verilog
    /// array — the emitter and simulator each lower it to N independent
    /// scalars (see `docs/superpowers/specs/2026-07-04-array-typed-fn-params-design.local.md`).
    Array {
        /// Element type; restricted to `Bit`/`Bits`/`Signed`.
        elem: Box<Type>,
        /// Number of elements; must const-evaluate.
        len: Box<Expr>,
    },
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
    /// Statements run in order (drives, ticks, expects, ...).
    pub body: Vec<TestStmt>,
    /// Source span of the whole `test` block.
    pub span: Span,
}

/// A statement inside a `test` block (spec/02 section 1.10).
#[derive(Clone, Debug)]
pub enum TestStmt {
    /// `tick(clk)` / `tick(clk, n)` — advance n clock cycles (default 1).
    Tick {
        /// The clock signal to advance.
        clock: Ident,
        /// Number of cycles to advance; defaults to 1 when omitted.
        count: Option<Expr>,
    },
    /// `expect expr` — assert the expression is true now.
    Expect(Expr),
    /// `name = value` — drive an input of the module under test.
    Drive {
        /// The input port being driven.
        name: Ident,
        /// The value to drive it with.
        value: Expr,
    },
    /// Statement-level `if` inside a `test` block.
    If {
        /// The condition; must be 1-bit.
        cond: Expr,
        /// Statements to run when `cond` is true.
        then: Vec<TestStmt>,
        /// Statements to run when `cond` is false, if an `else` was written.
        els: Option<Vec<TestStmt>>,
    },
    /// `sim { ... }` — see [`SimBlock`].
    Sim(SimBlock),
    /// A test statement that failed to parse. Produced ONLY by
    /// `parser::parse_recover`; see [`TopItem::Error`]. The span covers the
    /// skipped source.
    Error(Span),
}

/// `sim { speed mhz(50)  bind audio -> speaker(...) }` inside a `test`
/// block. Simulation-only (docs/superpowers/specs/2026-07-07-hw-emulation-led-design.local.md).
#[derive(Clone, Debug)]
pub struct SimBlock {
    /// The declared real-world clock rate in Hz, already desugared from
    /// `hz(n)`/`khz(n)`/`mhz(n)` to a plain multiplication expr. `None` if
    /// the `speed` clause was omitted (run as fast as possible).
    pub speed: Option<Expr>,
    /// Peripheral bindings, in source order.
    pub binds: Vec<Bind>,
    /// Source span of the whole `sim` block.
    pub span: Span,
}

/// `bind <port> -> <peripheral>(args)`.
#[derive(Clone, Debug)]
pub struct Bind {
    /// The module port being bound.
    pub port: Ident,
    /// The peripheral kind (e.g. `led`, `speaker`).
    pub peripheral: Ident,
    /// Peripheral configuration values.
    pub args: Vec<BindArg>,
    /// Source span of the whole `bind` statement.
    pub span: Span,
}

/// One `name: value` inside a `bind(...)` peripheral config. Not
/// `NamedArg`/`Expr` — the language has no string-literal expression, so
/// `led(color: "green")` needs its own tiny value shape, not a detour
/// through `self.expr()`.
#[derive(Clone, Debug)]
pub struct BindArg {
    /// The config key.
    pub name: Ident,
    /// The config value.
    pub value: BindArgValue,
    /// Source span of the `name: value` pair.
    pub span: Span,
}

/// The value shape for a [`BindArg`] — a bare identifier, string, or integer,
/// since peripheral config has no room for full [`Expr`]s.
#[derive(Clone, Debug)]
pub enum BindArgValue {
    /// A bare word (e.g. `color: green`).
    Ident(String),
    /// A quoted string (e.g. `color: "green"`).
    Str(String),
    /// An integer literal.
    Int(u128),
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

    #[test]
    fn sync_loop_node_constructs() {
        let sp = Span::new(0, 0);
        let id = |n: &str| Ident {
            name: n.into(),
            span: sp,
        };
        let int = |v: u128| Expr {
            kind: ExprKind::Int {
                value: v,
                raw: v.to_string(),
            },
            span: sp,
        };
        let _item = ModuleItem::SyncLoop(Box::new(SyncLoop {
            name: id("find_first"),
            clock: id("clk"),
            edge: Edge::Rise,
            var: id("i"),
            lo: int(0),
            hi: int(8),
            result_name: id("result"),
            result_ty: Type::Bit,
            result_init: int(0),
            body: vec![],
            span: sp,
        }));
    }
}
