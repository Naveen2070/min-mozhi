//! Elaboration (Phase 1.5, step B1): turn one AST module plus concrete
//! parameter values into a flat [`Design`] — signals with their widths folded
//! to concrete numbers, registers with their (mandatory, compile-time) reset
//! values folded, the combinational drivers, and the sequential processes.
//! The event-driven kernel (next step) interprets a `Design`; it never walks
//! the AST shape again.
//!
//! Reset is **synthesized**, exactly as the Verilog emitter does it: a `reg`
//! carries a reset value and the module declares `reset rst`, while the `on`
//! block body holds only the non-reset logic. The kernel applies `reset → the
//! folded reset value, else → the on-block result` so its results match the
//! emitted Verilog (the differential oracle).
//!
//! Full structural elaboration, mirroring the Verilog emitter so the flat
//! `Design` matches the emitted hardware: module **instances are flattened**
//! (C2, signals name-prefixed `inst.port` → `inst_port`), **`repeat` is
//! unrolled** (C3, array instances `arr__i`, bit-indexed drives assembled into a
//! Concat), and **enum-typed signals** are encoded by variant index with width
//! `clog2(variants)` (C4, variant reads/patterns → their index). Const/width
//! folding is shared with the combinational evaluator ([`super::comb`]).

use std::collections::{BTreeMap, HashMap, HashSet};

use mimz_core::ast::{
    self, BinOp, Dir, Edge, Expr, ExprKind, FuncDecl, ModuleItem, NamedArg, SeqStmt, UnOp,
};

use super::value::{const_eval, const_eval_wide, pick_module, type_width};

use crate::sim::Diag;

use bundle::{bundle_field_expr, bundle_type_info, flatten_bundle_params_in_func, is_bundle_ty};
use instance::{Flat, flatten_instance};
use module::elaborate_module;
use registry::{
    BundleRegistry, EnumRegistry, ExternRegistry, FuncRegistry, build_bundle_registry,
    build_enum_registry, build_extern_registry, build_func_registry, build_registry,
    resolve_bundle_fields_sim,
};
use rewrite::Rw;

/// Max `repeat` iterations the simulator will unroll — the same crate-root
/// constant the emitter uses, so a design that compiles also elaborates (the
/// simulator is the emitter's differential oracle). See [`mimz_core::REPEAT_BUDGET`].
use mimz_core::REPEAT_BUDGET;

/// Max instance-nesting depth the simulator will flatten. `mimz sim`/`mimz test`
/// run on the parsed AST WITHOUT the checker (which has its own recursion guard),
/// so a recursive/cyclic instantiation (`module A { let u = A() … }`, or A→B→A)
/// would otherwise recurse until the stack overflows and the process aborts. This
/// bound turns that into a clean error — the simulator's analogue of the parser's
/// `MAX_DEPTH` and the emitter's `REPEAT_BUDGET` (see SEC-6 in docs/audit).
///
/// Kept deliberately small: each level is a large `elaborate_module` +
/// `flatten_instance` stack frame, and the bound must fire well within the 1 MB
/// default main-thread stack on Windows. Real hardware nests instances only a few
/// levels deep, so 16 is generous for valid designs while staying crash-safe.
const MAX_INSTANCE_DEPTH: u32 = 16;

/// How the simulator handles an `extern module` instance — a declaration
/// with no body, so nothing here can actually be simulated (Verilog
/// emission is the only backend that models its real behavior). Threaded as
/// a plain function parameter for now; Task 9 wires this to `mimz.toml`/CLI.
/// Every entry point in this crate that doesn't take `mode` explicitly
/// defaults to `Warn` (see [`elaborate`]/[`elaborate_project`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimMode {
    /// An extern instance's output ports read as `Val::unknown` — the design
    /// can still run, just with those signals unconstrained.
    Warn,
    /// An extern instance is a hard `Err` at elaboration time, before any
    /// cycle runs.
    Strict,
}

/// Module registry across all loaded files: every `(file_idx, file, module)`
/// declaring a given name — a multimap, since (spec/02 section 1.5b) a
/// module name is unique only PER FILE; the same name may legally appear in
/// different files (Task 4). Mirrors `emit_verilog::Project::modules`;
/// resolved via [`resolve_module`].
type Registry<'a> = HashMap<String, Vec<(usize, &'a ast::File, &'a ast::Module)>>;

/// A signal's concrete type after width folding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Width {
    /// Bit width, `1..=128`.
    pub bits: u32,
    /// Whether the signal is `signed`.
    pub signed: bool,
}

/// An input, output, or wire with its folded width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signal {
    /// The signal's name.
    pub name: String,
    /// The signal's folded width and signedness.
    pub width: Width,
}

/// A register: its width, its folded compile-time reset value (the kernel
/// masks it to `width`), and the clock whose rising edge updates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reg {
    /// The register's name.
    pub name: String,
    /// The register's folded width and signedness.
    pub width: Width,
    /// The folded compile-time reset value (masked to `width` by the kernel).
    pub reset: mimz_core::checker::consteval::ConstVal,
    /// The clock of the `on` block that assigns this reg (empty if none does,
    /// in which case the reg simply holds its reset value forever).
    pub clock: String,
    /// The edge of the assigning `on` block (`rise`/`fall`). Defaults to `Rise`
    /// for an unassigned reg (it never ticks).
    pub edge: Edge,
}

/// A memory: an array of `depth` cells, each `width` bits, seeded to the folded
/// `init` value at construction (power-on init). Read combinationally
/// (`m[addr]`) and written on `clock`'s `edge` (`m[addr] <- v`); a memory with
/// no writing `on` block is a read-only ROM holding `init`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mem {
    /// The memory's name.
    pub name: String,
    /// Width and signedness of one cell.
    pub width: Width,
    /// Number of addressable cells.
    pub depth: u128,
    /// The folded compile-time value every cell is seeded to at power-on.
    pub init: mimz_core::checker::consteval::ConstVal,
    /// The clock of the `on` block that writes this memory (empty if none does).
    pub clock: String,
    /// The edge of the writing `on` block (`rise`/`fall`).
    pub edge: Edge,
}

/// One sequential process — the body of an `on rise(clock)` block. The kernel
/// interprets `body` each rising edge of `clock` (after the synthesized reset
/// branch). Registers left unassigned on a path hold their current value.
#[derive(Clone, Debug)]
pub struct Process {
    /// The clock signal whose edge drives this block.
    pub clock: String,
    /// The edge this block triggers on (`on rise`/`on fall`).
    pub edge: Edge,
    /// The block's statements, interpreted in order each triggering edge.
    pub body: Vec<SeqStmt>,
}

/// A fully elaborated single module: a flat signal/process graph with all
/// parameters and widths folded to concrete values.
#[derive(Clone, Debug)]
pub struct Design {
    /// The module this design was elaborated from.
    pub module: String,
    /// Folded compile-time integers (params + consts) — for the const
    /// expressions (indices, slice bounds) the kernel still evaluates.
    pub consts: BTreeMap<String, i128>,
    /// Input ports, with folded widths.
    pub inputs: Vec<Signal>,
    /// Output ports, with folded widths.
    pub outputs: Vec<Signal>,
    /// Internal wires, with folded widths.
    pub wires: Vec<Signal>,
    /// Registers, with folded widths and reset values.
    pub regs: Vec<Reg>,
    /// Memories (RAM/register arrays), seeded at construction.
    pub mems: Vec<Mem>,
    /// Combinational drivers: signal name → driving expression. Covers wire
    /// `init` and `out = expr` drives (outputs and wires only; never regs).
    pub comb: BTreeMap<String, Expr>,
    /// Sequential processes, one per `on` block.
    pub procs: Vec<Process>,
    /// Declared clock signal names.
    pub clocks: Vec<String>,
    /// Declared reset signal names (synchronous, active-high).
    pub resets: Vec<String>,
    /// User-defined combinational functions from ALL project files (D3),
    /// available to the kernel's expression evaluator at runtime (`FnCall`).
    pub funcs: HashMap<String, FuncDecl>,
    /// Names of signals with no driver by design: an extern-module
    /// instance's output ports in `warn` [`SimMode`]. Each is also present
    /// in `wires` (for its width) but deliberately absent from `comb` (there
    /// is no body to derive a driver from) — the kernel resolves a name in
    /// this set straight to `Val::unknown`, bypassing `comb` entirely.
    pub unknown_signals: HashSet<String>,
}

/// Elaborate `module` (or the file's only module when `module` is `None`) into a
/// flat [`Design`]. Single-file entry point: a module that instantiates a
/// sub-module defined in ANOTHER file needs [`elaborate_project`] (so the
/// imported file is available). Handles instances, `repeat`, and enum signals.
pub fn elaborate(
    file: &ast::File,
    module: Option<&str>,
    params: &BTreeMap<String, i128>,
) -> Result<Design, Box<Diag>> {
    elaborate_with_mode(file, module, params, SimMode::Warn)
}

/// Like [`elaborate`], but takes an explicit `mode` for how an `extern
/// module` instance (if any) is handled. See [`SimMode`]; [`elaborate`]
/// defaults to `Warn`.
pub fn elaborate_with_mode(
    file: &ast::File,
    module: Option<&str>,
    params: &BTreeMap<String, i128>,
    mode: SimMode,
) -> Result<Design, Box<Diag>> {
    elaborate_project_with_mode(std::slice::from_ref(file), module, params, mode)
}

/// Elaborate the entry module across a loaded project (`files[0]` is the entry,
/// the rest are its imports — the order the shell crate's `load_project`
/// returns; not linkable here since mimz-sim doesn't depend on it).
/// Instances are **flattened**: each child is elaborated and inlined into the
/// parent with its signals name-prefixed (`inst.port` → wire `inst_port`,
/// matching the Verilog emitter), so the flat [`Design`] the kernel runs is
/// equivalent to the emitted Verilog.
pub fn elaborate_project(
    files: &[ast::File],
    module: Option<&str>,
    params: &BTreeMap<String, i128>,
) -> Result<Design, Box<Diag>> {
    elaborate_project_with_mode(files, module, params, SimMode::Warn)
}

/// Like [`elaborate_project`], but takes an explicit `mode` for how an
/// `extern module` instance (if any) is handled. See [`SimMode`];
/// [`elaborate_project`] defaults to `Warn`.
pub fn elaborate_project_with_mode(
    files: &[ast::File],
    module: Option<&str>,
    params: &BTreeMap<String, i128>,
    mode: SimMode,
) -> Result<Design, Box<Diag>> {
    let reg = build_registry(files);
    let extern_reg = build_extern_registry(files);
    let func_reg = build_func_registry(files);
    let bundle_reg = build_bundle_registry(files);
    let enum_reg = build_enum_registry(files);
    // No source position to point at (there is no file at all) — a
    // defensive, essentially unreachable-in-practice case (every real
    // caller has already loaded at least the entry file).
    let entry = files.first().ok_or_else(|| {
        Box::new(
            Diag::new(
                mimz_core::span::Span { start: 0, end: 0 },
                "no files to elaborate",
            )
            .with_code("S0131"),
        )
    })?;
    let m = pick_module(entry, module)?;
    elaborate_module(
        &reg,
        &extern_reg,
        &func_reg,
        &bundle_reg,
        &enum_reg,
        entry,
        m,
        params,
        0,
        mode,
    )
}

/// A clock/reset connection must be a plain signal name.
fn conn_signal_name(e: &Expr) -> Result<String, Box<Diag>> {
    match &e.kind {
        ExprKind::Ident(n) => Ok(n.clone()),
        _ => Err(Box::new(
            Diag::new(
                e.span,
                "a clock/reset connection must be a plain signal name",
            )
            .with_code("S0133"),
        )),
    }
}

fn ident_expr(name: String, span: mimz_core::span::Span) -> Expr {
    Expr {
        kind: ExprKind::Ident(name),
        span,
    }
}

fn int_expr(v: i128, span: mimz_core::span::Span) -> Expr {
    if v >= 0 {
        return Expr {
            kind: ExprKind::Int {
                value: (v as u128).into(),
                raw: v.to_string(),
            },
            span,
        };
    }
    // Negative: emit `-<magnitude>`. Use `unsigned_abs` (not `-v`) so the one
    // value whose magnitude does not fit `i128` — `i128::MIN`, magnitude 2^127 —
    // is representable in the `u128` literal instead of overflow-panicking the
    // negation. `i128::MIN` is reachable on the unchecked sim path: a child
    // const can evaluate to it via checked arithmetic (e.g. `(-i128::MAX) - 1`),
    // and every flattened const passes through here.
    let mag = v.unsigned_abs();
    Expr {
        kind: ExprKind::Unary {
            op: UnOp::Neg,
            expr: Box::new(Expr {
                kind: ExprKind::Int {
                    value: mag.into(),
                    raw: mag.to_string(),
                },
                span,
            }),
        },
        span,
    }
}

/// Pin `e`'s evaluated width to exactly `width` bits via the `extend`
/// builtin — a plain `ExprKind::Int` literal otherwise evaluates to its own
/// minimal width (`Val::from_int`), not whatever fixed-width slot it needs
/// to fill inside a `Concat`. A no-op at eval time when `e` already
/// evaluates to `width` bits (e.g. an ident naming a same-width signal).
fn extend_to(e: Expr, width: u32, span: mimz_core::span::Span) -> Expr {
    Expr {
        kind: ExprKind::Call {
            func: ast::Builtin::Extend,
            args: vec![e, int_expr(width as i128, span)],
        },
        span,
    }
}

/// `clog2` matching the Verilog emitter and the `clog2` const-builtin: the bit
/// width of an `n`-variant enum encoding (one source of truth, so they agree).
fn clog2(n: usize) -> u32 {
    mimz_core::checker::consteval::clog2_bits(n as u128)
}

/// Does this sequential body assign register `name` on any path (including
/// inside `if`/`else`)?
fn assigns(body: &[SeqStmt], name: &str) -> bool {
    body.iter().any(|s| match s {
        SeqStmt::Assign { lhs, .. } => lhs.base.name == name,
        SeqStmt::If { then, els, .. } => {
            assigns(then, name) || els.as_deref().is_some_and(|e| assigns(e, name))
        }
        SeqStmt::Default { name: n, .. } => n.name == name,
        SeqStmt::Loop { body, .. } => assigns(body, name),
        // Unreachable: every `SeqStmt::ForEach` in an `on`-block body is
        // lowered before `Rw::seq`/`assigns`/`run_seq` ever run — see
        // `elaborate_module`'s `ModuleItem::On` arm.
        SeqStmt::ForEach { .. } => unreachable!(
            "ForEach is lowered before Rw::seq/assigns/run_seq ever run — see elaborate_module's ModuleItem::On arm"
        ),
        SeqStmt::Error(_) => false,
    })
}

mod bundle;
mod instance;
mod module;
mod registry;
mod rewrite;

#[cfg(test)]
mod tests;
