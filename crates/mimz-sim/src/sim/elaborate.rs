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
    self, BinOp, Dir, Edge, Expr, ExprKind, FuncDecl, ModuleItem, NamedArg, Pattern, SeqStmt, UnOp,
};

use super::value::{const_eval, pick_module, type_width};

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

fn build_registry(files: &[ast::File]) -> Registry<'_> {
    let mut reg: Registry<'_> = HashMap::new();
    for (file_idx, f) in files.iter().enumerate() {
        for it in &f.items {
            if let ast::TopItem::Module(m) = it {
                reg.entry(m.name.name.clone())
                    .or_default()
                    .push((file_idx, f, m));
            }
        }
    }
    reg
}

/// Resolve an `Inst`'s target module reference against `reg`. Mirrors
/// `emit_verilog::Project::resolve_module`'s bare/qualified logic — but,
/// unlike the emitter (which only ever runs after the checker has already
/// rejected an ambiguous bare reference as E0110), `mimz sim`/`mimz test`
/// elaborate the raw parse tree directly (see the module doc comment): an
/// ambiguous reference is a real, reachable outcome here, so it gets its
/// own error instead of emit_verilog's "unreachable in practice, checker
/// already rejected it" `None`.
fn resolve_module<'a>(
    reg: &Registry<'a>,
    imports: &[ast::Import],
    q: &ast::QualIdent,
) -> Result<(&'a ast::File, &'a ast::Module), String> {
    let candidates = reg.get(&q.name.name).ok_or_else(|| {
        format!(
            "uses unknown module `{}` — is the file that defines it imported?",
            q.name.name
        )
    })?;
    if q.is_bare() {
        match candidates.as_slice() {
            [(_, f, m)] => Ok((f, m)),
            [] => unreachable!("empty Vec is never inserted"),
            _ => Err(format!(
                "uses module `{}`, which is ambiguous — declared in {} different \
                 files; qualify with the import path to pick one (e.g. `a.b.{}`)",
                q.name.name,
                candidates.len(),
                q.name.name
            )),
        }
    } else {
        // `mimz sim`/`mimz test` never run the checker (module doc comment),
        // so — unlike `emit_verilog`, which can rely on the checker having
        // already populated `q.resolved_file` — this match against the
        // referencing file's own `import` statements must be computed here
        // too. Mirrors `checker::names::resolve`'s identical step.
        q.resolve_via_imports(imports);
        let target = q.resolved_file.get().ok_or_else(|| {
            format!(
                "the path in `{}` doesn't match any `import` in this file",
                q.to_dotted()
            )
        })?;
        candidates
            .iter()
            .find(|&&(f, _, _)| f == target)
            .map(|&(_, f, m)| (f, m))
            .ok_or_else(|| format!("uses unknown module `{}`", q.name.name))
    }
}

/// Extern-module registry across all loaded files: every `(file_idx, decl)`
/// declaring a given name — mirrors [`Registry`]. Resolved (alongside
/// `Registry`) via [`resolve_target`].
type ExternRegistry<'a> = HashMap<String, Vec<(usize, &'a ast::ExternModule)>>;

fn build_extern_registry(files: &[ast::File]) -> ExternRegistry<'_> {
    let mut reg: ExternRegistry<'_> = HashMap::new();
    for (file_idx, f) in files.iter().enumerate() {
        for item in &f.items {
            if let ast::TopItem::ExternModule(em) = item {
                reg.entry(em.name.name.clone())
                    .or_default()
                    .push((file_idx, em));
            }
        }
    }
    reg
}

/// Resolve an `Inst`'s target against both real modules and extern
/// declarations — mirrors `emit_verilog::Project::resolve_target_with_file`'s
/// modules-then-externs fallback (real modules win on a same-name clash,
/// same defensive tie-break). A real module's declaring `&File` is returned
/// alongside it, since `flatten_instance` needs it to recursively elaborate
/// the child (see its `elaborate_module(reg, func_reg, bundle_reg, cfile,
/// cm, ...)` call). An extern declaration has no body to elaborate, so no
/// file is needed for it — confirmed by reading how `resolve_module`'s
/// `&'a ast::File` half is actually consumed at that call site: only the
/// `ModuleTarget::Real` case ever reads it.
fn resolve_target<'a>(
    reg: &Registry<'a>,
    extern_reg: &ExternRegistry<'a>,
    imports: &[ast::Import],
    q: &ast::QualIdent,
) -> Result<(Option<&'a ast::File>, ast::ModuleTarget<'a>), String> {
    if reg.contains_key(&q.name.name) {
        let (f, m) = resolve_module(reg, imports, q)?;
        return Ok((Some(f), ast::ModuleTarget::Real(m)));
    }
    let candidates = extern_reg.get(&q.name.name).ok_or_else(|| {
        format!(
            "uses unknown module `{}` — is the file that defines it imported?",
            q.name.name
        )
    })?;
    if q.is_bare() {
        match candidates.as_slice() {
            [(_, em)] => Ok((None, ast::ModuleTarget::Extern(em))),
            [] => unreachable!("empty Vec is never inserted"),
            _ => Err(format!(
                "uses extern module `{}`, which is ambiguous — declared in {} \
                 different files; qualify with the import path to pick one",
                q.name.name,
                candidates.len()
            )),
        }
    } else {
        q.resolve_via_imports(imports);
        let target_file = q.resolved_file.get().ok_or_else(|| {
            format!(
                "the path in `{}` doesn't match any `import` in this file",
                q.to_dotted()
            )
        })?;
        candidates
            .iter()
            .find(|&&(f, _)| f == target_file)
            .map(|&(_, em)| (None, ast::ModuleTarget::Extern(em)))
            .ok_or_else(|| format!("uses unknown extern module `{}`", q.name.name))
    }
}

/// Bundle registry across all loaded files: every `(file_idx, decl)`
/// declaring a given name — a multimap, mirroring [`Registry`] (bundles
/// have the same per-file-unique, project-wide-reusable scoping as modules,
/// unlike enums — see spec/02 section 1.5b). Used by the elaboration pass
/// to flatten bundle-typed ports/wires to N scalar signals
/// `signame_fieldname`; resolved via [`resolve_bundle`].
type BundleRegistry<'a> = HashMap<String, Vec<(usize, &'a ast::BundleDecl)>>;

fn build_bundle_registry(files: &[ast::File]) -> BundleRegistry<'_> {
    let mut reg: BundleRegistry<'_> = HashMap::new();
    for (file_idx, f) in files.iter().enumerate() {
        for it in &f.items {
            if let ast::TopItem::Bundle(b) = it {
                reg.entry(b.name.name.clone())
                    .or_default()
                    .push((file_idx, b));
            }
        }
    }
    // Mirror the checker's/emitter's synthesized `__Valid`/`__ValidSigned`
    // builtin bundles (`ast::builtin_valid_bundles`) so a `bit?`/`bits[N]?`/
    // `signed[N]?`-typed signal resolves here too — the sim elaborates the
    // raw parsed AST without the checker pass that normally registers these
    // (see this module's doc comment), so without this they'd be an
    // "unknown bundle `__Valid`" error the moment `?`-sugar reaches a wire.
    // `files.len()` — one past every real file index — matches the
    // checker/emitter convention (see `builtin_valid_bundles`'s doc comment).
    let builtin_file = files.len();
    for decl in ast::builtin_valid_bundles() {
        reg.entry(decl.name.name.clone())
            .or_default()
            .push((builtin_file, decl));
    }
    reg
}

/// Resolve a bundle-typed reference against `bundles`. Mirrors
/// [`resolve_module`] — same ambiguous-bare-reference-is-a-real-error
/// reasoning applies (the sim has no checker pass gating this).
fn resolve_bundle<'a>(
    bundles: &BundleRegistry<'a>,
    imports: &[ast::Import],
    q: &ast::QualIdent,
) -> Result<&'a ast::BundleDecl, String> {
    let candidates = bundles
        .get(&q.name.name)
        .ok_or_else(|| format!("unknown bundle `{}`", q.name.name))?;
    if q.is_bare() {
        match candidates.as_slice() {
            [(_, only)] => Ok(*only),
            [] => unreachable!("empty Vec is never inserted"),
            _ => Err(format!(
                "bundle `{}` is ambiguous — declared in {} different files; \
                 qualify with the import path to pick one (e.g. `a.b.{}`)",
                q.name.name,
                candidates.len(),
                q.name.name
            )),
        }
    } else {
        // Same "no checker pass gating this" reasoning as `resolve_module`.
        q.resolve_via_imports(imports);
        let target = q.resolved_file.get().ok_or_else(|| {
            format!(
                "the path in `{}` doesn't match any `import` in this file",
                q.to_dotted()
            )
        })?;
        candidates
            .iter()
            .find(|&&(f, _)| f == target)
            .map(|&(_, b)| b)
            .ok_or_else(|| format!("unknown bundle `{}`", q.name.name))
    }
}

/// Resolve a bundle type to `(field_name, Width)` pairs, substituting any
/// bundle parameters from `args` and folding width expressions against `consts`.
/// Mirrors the emitter's `resolve_bundle_fields` but returns concrete `Width`s
/// for the sim rather than AST `Type`s for code generation.
fn resolve_bundle_fields_sim(
    bundles: &BundleRegistry<'_>,
    imports: &[ast::Import],
    bname: &ast::QualIdent,
    args: &[NamedArg],
    consts: &BTreeMap<String, i128>,
) -> Result<Vec<(String, Width)>, String> {
    let bdecl = resolve_bundle(bundles, imports, bname)?;
    // Build a merged const env: module consts + bundle param defaults + call-site overrides.
    let mut merged = consts.clone();
    for p in &bdecl.params {
        if let Some(default) = &p.default
            && let Ok(v) = const_eval(default, &merged)
        {
            merged.insert(p.name.name.clone(), v);
        }
    }
    for a in args {
        if let Ok(v) = const_eval(&a.value, &merged) {
            merged.insert(a.name.name.clone(), v);
        }
    }
    let bname = &bname.name.name;
    bdecl
        .fields
        .iter()
        .map(|f| {
            let (bits, signed) = type_width(&f.ty, &merged)
                .map_err(|e| format!("bundle `{bname}` field `{}`: {e}", f.name.name))?;
            Ok((f.name.name.clone(), Width { bits, signed }))
        })
        .collect()
}

/// Function registry across all loaded files: name → AST declaration. Functions
/// are project-wide (D3 — a fn declared in any imported file is callable from
/// any module in any file), so the simulator collects them from the whole set.
type FuncRegistry<'a> = HashMap<String, &'a FuncDecl>;

fn build_func_registry(files: &[ast::File]) -> FuncRegistry<'_> {
    let mut reg = HashMap::new();
    for f in files {
        for it in &f.items {
            if let ast::TopItem::Func(func) = it {
                reg.insert(func.name.name.clone(), func);
            }
        }
    }
    reg
}

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
    pub reset: i128,
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
    pub init: i128,
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
) -> Result<Design, String> {
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
) -> Result<Design, String> {
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
) -> Result<Design, String> {
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
) -> Result<Design, String> {
    let reg = build_registry(files);
    let extern_reg = build_extern_registry(files);
    let func_reg = build_func_registry(files);
    let bundle_reg = build_bundle_registry(files);
    let entry = files.first().ok_or("no files to elaborate")?;
    let m = pick_module(entry, module)?;
    elaborate_module(
        &reg,
        &extern_reg,
        &func_reg,
        &bundle_reg,
        entry,
        m,
        params,
        0,
        mode,
    )
}

/// Elaborate one module (`m`, defined in `file`) under concrete `params`,
/// resolving any instantiated children through `reg`. User-defined functions
/// from ALL project files are supplied via `func_reg` (D3: functions are
/// project-wide) so a fn declared in an imported file is available here.
#[allow(clippy::too_many_arguments)]
fn elaborate_module(
    reg: &Registry,
    extern_reg: &ExternRegistry,
    func_reg: &FuncRegistry<'_>,
    bundle_reg: &BundleRegistry<'_>,
    file: &ast::File,
    m: &ast::Module,
    params: &BTreeMap<String, i128>,
    depth: u32,
    mode: SimMode,
) -> Result<Design, String> {
    // Guard against recursive/cyclic instantiation (the checker would catch it,
    // but `mimz sim`/`test` skip the checker) — bound the stack, fail cleanly.
    if depth > MAX_INSTANCE_DEPTH {
        return Err(format!(
            "instance nesting exceeds {MAX_INSTANCE_DEPTH} levels in `{}` — \
             a module likely instantiates itself (directly or in a cycle)",
            m.name.name
        ));
    }
    // Compile-time integer environment: params (override or default), then
    // file-level and module-level consts (same order as `comb::eval_outputs`).
    let mut consts: BTreeMap<String, i128> = BTreeMap::new();
    for p in &m.params {
        let v = match params.get(&p.name.name) {
            Some(v) => *v,
            None => match &p.default {
                Some(d) => const_eval(d, &consts)?,
                None => {
                    return Err(format!(
                        "parameter `{}` has no default — provide a value for it",
                        p.name.name
                    ));
                }
            },
        };
        consts.insert(p.name.name.clone(), v);
    }
    for item in &file.items {
        if let ast::TopItem::Const(c) = item {
            let v = const_eval(&c.value, &consts)?;
            consts.insert(c.name.name.clone(), v);
        }
    }
    for it in &m.items {
        if let ModuleItem::Const(c) = it {
            let v = const_eval(&c.value, &consts)?;
            consts.insert(c.name.name.clone(), v);
        }
    }

    // User-defined functions from ALL project files (D3: functions are
    // project-wide) — collected from `func_reg` so the kernel's expression
    // evaluator can call any fn regardless of which imported file defines it.
    let funcs: HashMap<String, FuncDecl> = func_reg
        .values()
        .map(|f| (f.name.name.clone(), (*f).clone()))
        .collect();

    // Module-level enums: name → full decl. The total wire width
    // (`inferred_total_width`) is set by the checker; falls back to `clog2(count)`
    // for tag-only enums when the checker has not run (e.g. bare sim tests).
    let enums: HashMap<String, &ast::EnumDecl> = m
        .items
        .iter()
        .filter_map(|it| match it {
            ModuleItem::Enum(e) => Some((e.name.name.clone(), e)),
            _ => None,
        })
        .collect();

    // Instance names (top-level AND inside `repeat`), so `inst.port` and the
    // array form `arr[i].port` rewrite to their flat wire names.
    let mut insts: HashSet<String> = HashSet::new();
    collect_inst_names(&m.items, &mut insts);

    // Folded width of a type — an enum type resolves to its total wire width.
    // Uses `inferred_total_width` when the checker has run; falls back to
    // `clog2(variant count)` for tag-only enums without the checker.
    let width_of = |ty: &ast::Type, ints: &BTreeMap<String, i128>| -> Result<Width, String> {
        if let ast::Type::Named(n) = ty {
            let e = enums
                .get(&n.name.name)
                .ok_or_else(|| format!("unknown enum type `{}`", n.name.name))?;
            // note: fallback only correct for tag-only enums (max_payload_w=0)
            let bits = e.inferred_total_width.get().unwrap_or_else(|| {
                debug_assert!(
                    e.variants.iter().all(|v| v.fields.is_empty()),
                    "tagged enum without checker run — fallback width is wrong"
                );
                clog2(e.variants.len())
            });
            Ok(Width {
                bits,
                signed: false,
            })
        } else {
            let (bits, signed) = type_width(ty, ints)?;
            Ok(Width { bits, signed })
        }
    };

    // Bundle-typed signal names at this module level: used by `Rw::field` to
    // rewrite `req.valid` → `req_valid` (the flat scalar name).
    // HashSet: O(1) lookup, no ordering needed.
    let mut bundle_sigs: HashSet<String> = HashSet::new();
    // Map each bundle signal name to its AST type (for O(1) lookup in Drive arm).
    let mut bundle_sig_types: HashMap<String, ast::Type> = HashMap::new();
    // Pre-scan to collect bundle signal names before building rw0 (which needs them).
    for it in &m.items {
        match it {
            ModuleItem::Port { name, ty, .. } | ModuleItem::Wire { name, ty, .. }
                if is_bundle_ty(ty, bundle_reg, &enums) =>
            {
                bundle_sigs.insert(name.name.clone());
                bundle_sig_types.insert(name.name.clone(), ty.clone());
            }
            _ => {}
        }
    }

    let no_subst: HashMap<String, Expr> = HashMap::new();
    let rw0 = Rw {
        insts: &insts,
        enums: &enums,
        bundle_sigs: &bundle_sigs,
        consts: &consts,
        subst: &no_subst,
    };

    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut wires = Vec::new();
    let mut regs = Vec::new();
    let mut mems = Vec::new();
    let mut comb: BTreeMap<String, Expr> = BTreeMap::new();
    let mut procs = Vec::new();
    let mut clocks = Vec::new();
    let mut resets = Vec::new();
    // Bit-indexed drives (`sum[i] = …`, from `repeat`), assembled into one
    // whole-signal Concat driver after the loop.
    let mut bit_drives: BTreeMap<String, BTreeMap<u32, Expr>> = BTreeMap::new();
    let mut flat = Flat::default();

    // `sync loop` is lowered to plain Port/Reg/On/Drive items (the same
    // desugaring the Verilog emitter uses, `ast::lower_sync_loop` — Task 4)
    // BEFORE the worklist starts, so the loop below never sees a `SyncLoop`
    // node: `lowered_sync_loops` must outlive the whole function since `work`
    // holds `&ModuleItem` borrows into it. `collect_lowered_sync_loops`
    // recurses into `const if` winning branches (at any nesting depth) to
    // find every `SyncLoop`, mirroring `emit_verilog::module::flatten_items`'s
    // recursive ConstIf/SyncLoop handling — a `SyncLoop` nested inside a
    // `const if` is checker-accepted and emitter-supported, so the simulator
    // must lower it too (see the final whole-branch review). The main
    // worklist loop's own `ConstIf` arm below filters raw `SyncLoop` items out
    // of whichever branch it pushes, so none of them reach the worklist
    // un-lowered (the main match's fallback would otherwise panic via the
    // `unreachable!()` arm — the same class of gap Task 9 fixed in the
    // emitter).
    let mut lowered_sync_loops: Vec<ModuleItem> = Vec::new();
    collect_lowered_sync_loops(&m.items, &consts, &mut lowered_sync_loops);

    // `foreach` is pure sugar for `repeat`/bare `loop` (see
    // `ast::foreach_lower`) — lowered up front for the same reason as
    // `sync loop` above: `work` holds `&ModuleItem` borrows into
    // `lowered_foreach`, which must outlive the whole function.
    // `collect_lowered_foreach` recurses into `const if` winning branches the
    // same way `collect_lowered_sync_loops` does.
    let mut lowered_foreach: Vec<ModuleItem> = Vec::new();
    collect_lowered_foreach(&m.items, &consts, &mut lowered_foreach);

    let mut work: Vec<&ModuleItem> = m
        .items
        .iter()
        .filter(|it| !matches!(it, ModuleItem::SyncLoop(_) | ModuleItem::ForEach(_)))
        .rev()
        .chain(lowered_sync_loops.iter().rev())
        .chain(lowered_foreach.iter().rev())
        .collect();
    while let Some(it) = work.pop() {
        match it {
            ModuleItem::Port { dir, name, ty } => {
                // Bundle port → N scalar signals named `portname_fieldname`.
                if let Some((bname, args)) = bundle_type_info(ty, bundle_reg, &enums) {
                    let fields = resolve_bundle_fields_sim(
                        bundle_reg,
                        &file.imports,
                        &bname,
                        &args,
                        &consts,
                    )?;
                    for (fname, fwidth) in fields {
                        let flat_name = format!("{}_{}", name.name, fname);
                        let sig = Signal {
                            name: flat_name,
                            width: fwidth,
                        };
                        match dir {
                            Dir::In => inputs.push(sig),
                            Dir::Out => outputs.push(sig),
                        }
                    }
                } else {
                    let sig = Signal {
                        name: name.name.clone(),
                        width: width_of(ty, &consts)?,
                    };
                    match dir {
                        Dir::In => inputs.push(sig),
                        Dir::Out => outputs.push(sig),
                    }
                }
            }
            ModuleItem::Clock(n) => clocks.push(n.name.clone()),
            // The cycle-based kernel applies reset at the clock edge; an async
            // reset is observationally identical at the per-cycle sample points
            // (sub-cycle timing is out of the kernel's model), so `is_async` is
            // an emitter-only distinction and the sim just records the name. The
            // path to modeling sub-cycle reset timing is the three-tier fidelity
            // roadmap in docs/plan/phase-1.5-simulator.md (currently Tier 3:
            // delegate timing-faithful runs to the Verilog/Icarus oracle).
            ModuleItem::Reset { name: n, .. } => resets.push(n.name.clone()),
            ModuleItem::Wire { name, ty, init } => {
                // Bundle wire → N scalar wires, each driven by the corresponding
                // field of the bundle init expression (must be a BundleLit).
                if let Some((bname, args)) = bundle_type_info(ty, bundle_reg, &enums) {
                    let fields = resolve_bundle_fields_sim(
                        bundle_reg,
                        &file.imports,
                        &bname,
                        &args,
                        &consts,
                    )?;
                    for (fname, fwidth) in fields {
                        let flat_name = format!("{}_{}", name.name, fname);
                        wires.push(Signal {
                            name: flat_name.clone(),
                            width: fwidth,
                        });
                        // The driver for each scalar field comes from the BundleLit init.
                        let field_init = bundle_field_expr(init, &fname, init.span);
                        comb.insert(flat_name, rw0.expr(&field_init)?);
                    }
                } else {
                    wires.push(Signal {
                        name: name.name.clone(),
                        width: width_of(ty, &consts)?,
                    });
                    comb.insert(name.name.clone(), rw0.expr(init)?);
                }
            }
            ModuleItem::Reg { name, ty, reset } => {
                let width = width_of(ty, &consts)?;
                let reset = const_eval(&rw0.expr(reset)?, &consts)?;
                regs.push(Reg {
                    name: name.name.clone(),
                    width,
                    reset,
                    clock: String::new(),
                    edge: Edge::Rise,
                });
            }
            ModuleItem::Mem {
                name,
                ty,
                depth,
                init,
            } => {
                let width = width_of(ty, &consts)?;
                // The sim runs WITHOUT the checker, so guard the depth here too.
                let d = const_eval(&rw0.expr(depth)?, &consts)?;
                let depth = u128::try_from(d).ok().filter(|d| *d >= 1).ok_or_else(|| {
                    format!("memory `{}` has a non-positive depth ({d})", name.name)
                })?;
                let init = const_eval(&rw0.expr(init)?, &consts)?;
                mems.push(Mem {
                    name: name.name.clone(),
                    width,
                    depth,
                    init,
                    clock: String::new(),
                    edge: Edge::Rise,
                });
            }
            ModuleItem::Drive { lhs, rhs } => {
                // Bundle drive: `rsp = req` where `rsp` is a bundle signal.
                // Expand to one scalar drive per field: `rsp_valid = req_valid`, etc.
                if bundle_sigs.contains(&lhs.base.name) && lhs.index.is_none() {
                    // O(1) lookup: bundle type from the pre-scan map.
                    if let Some(ty) = bundle_sig_types.get(&lhs.base.name)
                        && let Some((bname, args)) = bundle_type_info(ty, bundle_reg, &enums)
                    {
                        let fields = resolve_bundle_fields_sim(
                            bundle_reg,
                            &file.imports,
                            &bname,
                            &args,
                            &consts,
                        )?;
                        for (fname, _) in &fields {
                            let flat_lhs = format!("{}_{}", lhs.base.name, fname);
                            // The RHS field: either `rhs.fname` (if rhs is a bundle ident)
                            // or a BundleLit field extraction.
                            let field_rhs = bundle_field_expr(rhs, fname, rhs.span);
                            comb.insert(flat_lhs, rw0.expr(&field_rhs)?);
                        }
                    } else {
                        record_drive(lhs, rhs, &rw0, &consts, &mut comb, &mut bit_drives)?;
                    }
                } else {
                    record_drive(lhs, rhs, &rw0, &consts, &mut comb, &mut bit_drives)?
                }
            }
            ModuleItem::On(on) => {
                // `foreach` inside an `on`-block body is pure sugar over
                // `loop` (see `ast::foreach_lower`) — lowered here, before
                // `Rw::seq` ever runs, so `Rw::seq`/`assigns`/the kernel's
                // `run_seq` never see a raw `SeqStmt::ForEach` node.
                let lowered_body = lower_foreach_in_seq(&on.body, &m.items);
                procs.push(Process {
                    clock: on.clock.name.clone(),
                    edge: on.edge,
                    body: lowered_body
                        .iter()
                        .map(|s| rw0.seq(s, &|n| n.to_string()))
                        .collect::<Result<_, _>>()?,
                })
            }
            // Consts are folded above; enum decls become the encoding above.
            ModuleItem::Const(_) | ModuleItem::Enum(_) => {}
            // Unreachable: elaboration runs on a strict-parsed tree, which
            // carries no `Error` placeholder.
            ModuleItem::Error(_) => {}
            // Unreachable: every `SyncLoop` is lowered and filtered out of
            // `m.items` before the worklist runs (see this function's Step 1,
            // just above `let mut work: Vec<&ModuleItem> = ...`) — the match
            // arm exists only so a future refactor that breaks that filter
            // fails loudly instead of silently dropping the construct.
            ModuleItem::SyncLoop(_) => {
                unreachable!(
                    "SyncLoop is lowered before the worklist runs — see elaborate_module's Step 1"
                )
            }
            // Unreachable: every `ForEach` is lowered (to `Repeat`) and
            // filtered out of `m.items` before the worklist runs, same as
            // `SyncLoop` just above — see `collect_lowered_foreach` and
            // `lowered_foreach` in this function's Step 1.
            ModuleItem::ForEach(_) => {
                unreachable!(
                    "ForEach is lowered before the worklist runs — see elaborate_module's Step 1"
                )
            }
            ModuleItem::Inst(inst) => {
                let f = flatten_instance(
                    reg,
                    extern_reg,
                    func_reg,
                    bundle_reg,
                    &file.imports,
                    &consts,
                    &insts,
                    &enums,
                    &no_subst,
                    inst,
                    &inst.name.name,
                    depth,
                    mode,
                )?;
                flat.absorb(f);
            }
            ModuleItem::Repeat(r) => {
                let lo = const_eval(&r.lo, &consts)?;
                let hi = const_eval(&r.hi, &consts)?;
                // `checked_sub`: extreme bounds (`hi - lo` past i128::MAX) must not
                // overflow-panic — treat an out-of-range span as over-budget.
                let count = hi.checked_sub(lo).unwrap_or(i128::MAX).max(0);
                if count > REPEAT_BUDGET {
                    return Err(format!(
                        "`repeat` would unroll {count} times, over the limit of {REPEAT_BUDGET}"
                    ));
                }
                for iv in lo..hi {
                    let mut ci = consts.clone();
                    ci.insert(r.var.name.clone(), iv);
                    let subst = HashMap::from([(r.var.name.clone(), int_expr(iv, r.span))]);
                    let rwi = Rw {
                        insts: &insts,
                        enums: &enums,
                        bundle_sigs: &bundle_sigs,
                        consts: &ci,
                        subst: &subst,
                    };
                    for body_it in &r.items {
                        match body_it {
                            ModuleItem::Inst(inst) => {
                                let iname = match &inst.index {
                                    Some(_) => format!("{}__{}", inst.name.name, iv),
                                    None => inst.name.name.clone(),
                                };
                                let f = flatten_instance(
                                    reg,
                                    extern_reg,
                                    func_reg,
                                    bundle_reg,
                                    &file.imports,
                                    &ci,
                                    &insts,
                                    &enums,
                                    &subst,
                                    inst,
                                    &iname,
                                    depth,
                                    mode,
                                )?;
                                flat.absorb(f);
                            }
                            ModuleItem::Drive { lhs, rhs } => {
                                record_drive(lhs, rhs, &rwi, &ci, &mut comb, &mut bit_drives)?
                            }
                            ModuleItem::Repeat(_) => {
                                return Err(
                                    "nested `repeat` is not supported by the simulator yet".into(),
                                );
                            }
                            _ => {
                                return Err(
                                    "a `repeat` body may only contain instances and drives".into(),
                                );
                            }
                        }
                    }
                }
            }
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                // NOTE(deferred): `unwrap_or(0)` here is a safe latent fallback —
                // the checker rejects non-const `const if` conditions (E0811) before
                // anything reaches the simulator, so `const_eval` always returns Ok
                // in practice. If the simulator ever runs without the checker
                // (e.g. a direct API consumer), a non-const condition silently takes
                // the `then` branch. Fix: thread the error upward instead of defaulting.
                let val = const_eval(cond, &consts).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                // Any `SyncLoop`/`ForEach` in this branch was already lowered
                // (into `lowered_sync_loops`/`lowered_foreach`) by
                // `collect_lowered_sync_loops`/`collect_lowered_foreach`
                // above, before the worklist started — filter the raw nodes
                // back out here so neither is pushed a second time
                // un-lowered (which would hit the `unreachable!()` arm
                // below — there is no `ModuleItem::ForEach` match arm).
                work.extend(
                    branch
                        .iter()
                        .filter(|it| {
                            !matches!(it, ModuleItem::SyncLoop(_) | ModuleItem::ForEach(_))
                        })
                        .rev(),
                );
            }
            ModuleItem::BundleDestructure { .. } => {
                return Err(
                    "bundle destructure in module body is not yet supported by the simulator"
                        .to_string(),
                );
            }
        }
    }

    // Merge inlined-instance pieces, then assemble bit-indexed drives into one
    // whole-signal Concat (widest bit first, Verilog concat order).
    let unknown_signals: HashSet<String> = flat.unknown.iter().cloned().collect();
    wires.extend(flat.wires);
    regs.extend(flat.regs);
    // Merge instance drivers, erroring on a name collision instead of silently
    // overwriting (a parent signal named like a flattened `inst_port` wire).
    for (name, driver) in flat.comb {
        if comb.insert(name.clone(), driver).is_some() {
            return Err(format!(
                "flattened instance signal `{name}` collides with an existing signal in `{}`",
                m.name.name
            ));
        }
    }
    procs.extend(flat.procs);
    mems.extend(flat.mems);
    for (sig, bits) in bit_drives {
        let width = inputs
            .iter()
            .chain(&outputs)
            .chain(&wires)
            .find(|s| s.name == sig)
            .map(|s| s.width.bits)
            .ok_or_else(|| format!("bit-driven signal `{sig}` has no declaration"))?;
        let mut parts = Vec::with_capacity(width as usize);
        for b in (0..width).rev() {
            let e = bits
                .get(&b)
                .ok_or_else(|| format!("signal `{sig}` bit {b} is not driven"))?;
            parts.push(e.clone());
        }
        let span = parts.first().map(|e| e.span).unwrap_or(m.span);
        comb.insert(
            sig,
            Expr {
                kind: ExprKind::Concat(parts),
                span,
            },
        );
    }

    // Each reg's clock is the clock of the `on` block that assigns it (the
    // checker guarantees a reg has exactly one owning block). Covers inlined
    // child regs too — their assigning proc carries the connected parent clock.
    for proc in &procs {
        for reg in &mut regs {
            if assigns(&proc.body, &reg.name) {
                reg.clock = proc.clock.clone();
                reg.edge = proc.edge;
            }
        }
        for mem in &mut mems {
            if assigns(&proc.body, &mem.name) {
                mem.clock = proc.clock.clone();
                mem.edge = proc.edge;
            }
        }
    }

    Ok(Design {
        module: m.name.name.clone(),
        consts,
        inputs,
        outputs,
        wires,
        regs,
        mems,
        comb,
        procs,
        clocks,
        resets,
        funcs,
        unknown_signals,
    })
}

/// The flat pieces one instance contributes to its parent.
#[derive(Default)]
struct Flat {
    wires: Vec<Signal>,
    regs: Vec<Reg>,
    mems: Vec<Mem>,
    comb: Vec<(String, Expr)>,
    procs: Vec<Process>,
    /// Names of driverless signals (extern-instance outputs in `warn`
    /// [`SimMode`]) — see `Design::unknown_signals`.
    unknown: Vec<String>,
}

impl Flat {
    fn absorb(&mut self, other: Flat) {
        self.wires.extend(other.wires);
        self.regs.extend(other.regs);
        self.mems.extend(other.mems);
        self.comb.extend(other.comb);
        self.procs.extend(other.procs);
        self.unknown.extend(other.unknown);
    }
}

/// Elaborate the child module of `inst` and inline it into the parent: every
/// child signal becomes a parent wire/reg named `{inst}_{name}`, child inputs
/// are driven by their connection expressions, and child clock/reset map to the
/// connected parent signals. Mirrors the Verilog emitter's instance lowering so
/// the simulator agrees bit-for-bit.
#[allow(clippy::too_many_arguments)]
fn flatten_instance(
    reg: &Registry,
    extern_reg: &ExternRegistry,
    func_reg: &FuncRegistry<'_>,
    bundle_reg: &BundleRegistry<'_>,
    parent_imports: &[ast::Import],
    parent_consts: &BTreeMap<String, i128>,
    parent_insts: &HashSet<String>,
    parent_enums: &HashMap<String, &ast::EnumDecl>,
    parent_subst: &HashMap<String, Expr>,
    inst: &ast::Inst,
    iname: &str,
    depth: u32,
    mode: SimMode,
) -> Result<Flat, String> {
    let (cfile, cm) = match resolve_target(reg, extern_reg, parent_imports, &inst.module)
        .map_err(|e| format!("instance `{}` {e}", inst.name.name))?
    {
        (Some(f), ast::ModuleTarget::Real(m)) => (f, m),
        (None, ast::ModuleTarget::Extern(em)) => {
            return flatten_extern_instance(em, parent_consts, inst, iname, mode);
        }
        (_, target) => unreachable!(
            "resolve_target always pairs ModuleTarget::Real with Some(file) and \
             ModuleTarget::Extern with None — got is_extern={}",
            target.is_extern()
        ),
    };

    // Child parameter bindings: an explicit `arg` (evaluated in the PARENT's
    // consts) wins; otherwise the child default (in the child's own consts).
    let mut cp: BTreeMap<String, i128> = BTreeMap::new();
    for p in &cm.params {
        let v = if let Some(a) = inst.args.iter().find(|a| a.name.name == p.name.name) {
            const_eval(&a.value, parent_consts)?
        } else if let Some(d) = &p.default {
            const_eval(d, &cp)?
        } else {
            return Err(format!(
                "instance `{}`: parameter `{}` has no value",
                inst.name.name, p.name.name
            ));
        };
        cp.insert(p.name.name.clone(), v);
    }

    let child = elaborate_module(
        reg,
        extern_reg,
        func_reg,
        bundle_reg,
        cfile,
        cm,
        &cp,
        depth + 1,
        mode,
    )?;
    let pfx = format!("{iname}_");

    // Parent-context rewriter for connection expressions: folds the `repeat`
    // loop var and resolves nested `arr[i-1].port` reads.
    // Empty bundle_sigs: the child's own signals are already flattened to
    // scalars by this point, so there's no dot-access left to rewrite.
    let no_bundle_sigs: HashSet<String> = HashSet::new();
    let prw = Rw {
        insts: parent_insts,
        enums: parent_enums,
        bundle_sigs: &no_bundle_sigs,
        consts: parent_consts,
        subst: parent_subst,
    };

    // The child body is already flat (no `Field`/enum nodes survive its own
    // elaboration), so a subst-only rewriter suffices: child const → literal,
    // child signal → prefixed name, child clock/reset → connected parent signal.
    let no_insts: HashSet<String> = HashSet::new();
    let no_enums: HashMap<String, &ast::EnumDecl> = HashMap::new();
    let mut subst: HashMap<String, Expr> = HashMap::new();
    for (n, &v) in &child.consts {
        subst.insert(n.clone(), int_expr(v, inst.span));
    }
    for s in child
        .inputs
        .iter()
        .chain(&child.outputs)
        .chain(&child.wires)
    {
        subst.insert(
            s.name.clone(),
            ident_expr(format!("{pfx}{}", s.name), inst.span),
        );
    }
    for r in &child.regs {
        subst.insert(
            r.name.clone(),
            ident_expr(format!("{pfx}{}", r.name), inst.span),
        );
    }
    for mem in &child.mems {
        subst.insert(
            mem.name.clone(),
            ident_expr(format!("{pfx}{}", mem.name), inst.span),
        );
    }

    // Clock/reset: explicit connection, else the same-named parent signal.
    let mut clock_map: HashMap<String, String> = HashMap::new();
    for c in child.clocks.iter().chain(&child.resets) {
        let parent = inst
            .conns
            .iter()
            .find(|cn| cn.port.name == *c)
            .map(|cn| conn_signal_name(&prw.expr(&cn.signal)?))
            .transpose()?
            .unwrap_or_else(|| c.clone());
        subst.insert(c.clone(), ident_expr(parent.clone(), inst.span));
        clock_map.insert(c.clone(), parent);
    }

    let crw = Rw {
        insts: &no_insts,
        enums: &no_enums,
        bundle_sigs: &no_bundle_sigs,
        consts: &child.consts,
        subst: &subst,
    };
    let mut flat = Flat::default();

    // Child inputs: a parent wire driven by the (required) connection.
    for s in &child.inputs {
        let conn = inst
            .conns
            .iter()
            .find(|cn| cn.port.name == s.name)
            .ok_or_else(|| {
                format!(
                    "instance `{}`: input `{}` of `{}` is not connected",
                    inst.name.name, s.name, cm.name.name
                )
            })?;
        flat.wires.push(Signal {
            name: format!("{pfx}{}", s.name),
            width: s.width,
        });
        flat.comb
            .push((format!("{pfx}{}", s.name), prw.expr(&conn.signal)?));
    }
    // Child outputs + wires: a parent wire driven by the child's (rewritten) logic.
    // A child wire/output with no `comb` driver is, by construction, one the
    // child itself marked as unknown-tainted (an extern-instance output read
    // through, however many levels deep) — copy it into the parent's own
    // `unknown` set (not just drop it) so a grandparent that re-exposes it
    // still finds a live wire + taint marker instead of a dangling name.
    for s in child.outputs.iter().chain(&child.wires) {
        if let Some(drv) = child.comb.get(&s.name) {
            flat.wires.push(Signal {
                name: format!("{pfx}{}", s.name),
                width: s.width,
            });
            flat.comb.push((format!("{pfx}{}", s.name), crw.expr(drv)?));
        } else if child.unknown_signals.contains(&s.name) {
            let flat_name = format!("{pfx}{}", s.name);
            flat.wires.push(Signal {
                name: flat_name.clone(),
                width: s.width,
            });
            flat.unknown.push(flat_name);
        }
    }
    // Child registers (clock filled by the parent's reg-clock pass).
    for r in &child.regs {
        flat.regs.push(Reg {
            name: format!("{pfx}{}", r.name),
            width: r.width,
            reset: r.reset,
            clock: String::new(),
            edge: r.edge,
        });
    }
    // Child memories (clock filled by the parent's clock-binding pass).
    for mem in &child.mems {
        flat.mems.push(Mem {
            name: format!("{pfx}{}", mem.name),
            width: mem.width,
            depth: mem.depth,
            init: mem.init,
            clock: String::new(),
            edge: mem.edge,
        });
    }
    // Child processes: prefix assigned regs, rewrite bodies, map the clock.
    for p in &child.procs {
        let clk = clock_map
            .get(&p.clock)
            .cloned()
            .unwrap_or_else(|| p.clock.clone());
        let rename = |n: &str| format!("{pfx}{n}");
        flat.procs.push(Process {
            clock: clk,
            edge: p.edge,
            body: p
                .body
                .iter()
                .map(|s| crw.seq(s, &rename))
                .collect::<Result<_, _>>()?,
        });
    }
    Ok(flat)
}

/// Handle an extern-module instance: it has no body, so there's nothing to
/// recursively elaborate. `strict` mode refuses to simulate around missing
/// hardware behavior; `warn` mode lowers every output port to an
/// unconstrained (`Val::unknown`) read and prints one warning per instance
/// (this function runs exactly once per `Inst` node during elaboration, so
/// "once per distinct instance" falls out for free — no dedup bookkeeping
/// needed).
fn flatten_extern_instance(
    em: &ast::ExternModule,
    parent_consts: &BTreeMap<String, i128>,
    inst: &ast::Inst,
    iname: &str,
    mode: SimMode,
) -> Result<Flat, String> {
    if mode == SimMode::Strict {
        return Err(format!(
            "instance `{}` instantiates extern module `{}` — no simulation model; \
             extern modules are Verilog-emission only (pass a `warn`-mode config to \
             simulate around it)",
            inst.name.name, em.name.name
        ));
    }
    eprintln!(
        "warning: instance `{}` instantiates extern module `{}` — its output(s) \
         are unconstrained (X) in simulation; only Verilog emission models its real \
         behavior",
        inst.name.name, em.name.name
    );

    // Child parameter bindings — same precedence as a real module's instance
    // (an explicit `arg` wins, else the extern's own default), needed to
    // fold a param-dependent output width (e.g. `out y: bits[WIDTH]`).
    let mut cp: BTreeMap<String, i128> = BTreeMap::new();
    for p in &em.params {
        let v = if let Some(a) = inst.args.iter().find(|a| a.name.name == p.name.name) {
            const_eval(&a.value, parent_consts)?
        } else if let Some(d) = &p.default {
            const_eval(d, &cp)?
        } else {
            return Err(format!(
                "instance `{}`: parameter `{}` has no value",
                inst.name.name, p.name.name
            ));
        };
        cp.insert(p.name.name.clone(), v);
    }

    let pfx = format!("{iname}_");
    let mut flat = Flat::default();
    // Extern ports are scalar-only (bit/bits[N]/signed[N]) — the checker
    // enforces this on the declaration (Task 3), so `type_width` (the same
    // width-resolution helper the real child-elaboration path uses) never
    // hits its enum/bundle/array error arms here.
    for it in &em.items {
        if let ModuleItem::Port {
            dir: Dir::Out,
            name,
            ty,
        } = it
        {
            let (bits, signed) = type_width(ty, &cp)?;
            let flat_name = format!("{pfx}{}", name.name);
            flat.wires.push(Signal {
                name: flat_name.clone(),
                width: Width { bits, signed },
            });
            flat.unknown.push(flat_name);
        }
    }
    Ok(flat)
}

/// A clock/reset connection must be a plain signal name.
fn conn_signal_name(e: &Expr) -> Result<String, String> {
    match &e.kind {
        ExprKind::Ident(n) => Ok(n.clone()),
        _ => Err("a clock/reset connection must be a plain signal name".into()),
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
                value: v as u128,
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
                    value: mag,
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

/// Collect every instance name (top-level and inside `repeat`), so an
/// instance-port read resolves whether the instance is plain or an array.
fn collect_inst_names(items: &[ModuleItem], out: &mut HashSet<String>) {
    for it in items {
        match it {
            ModuleItem::Inst(i) => {
                out.insert(i.name.name.clone());
            }
            ModuleItem::Repeat(r) => collect_inst_names(&r.items, out),
            _ => {}
        }
    }
}

/// Recursively find every `SyncLoop` reachable from `items` — including one
/// nested inside a `const if`'s winning branch, at any nesting depth — and
/// push its lowering (`ast::lower_sync_loop`) onto `out`. Mirrors
/// `emit_verilog::module::flatten_items`'s recursive `ConstIf`/`SyncLoop`
/// handling: same `const_eval(cond, consts)` resolution the main worklist
/// loop's own `ConstIf` arm uses, so a branch that resolves one way here
/// resolves the same way there. Called once, before the worklist starts —
/// see `elaborate_module`'s `lowered_sync_loops`.
fn collect_lowered_sync_loops(
    items: &[ModuleItem],
    consts: &BTreeMap<String, i128>,
    out: &mut Vec<ModuleItem>,
) {
    for it in items {
        match it {
            ModuleItem::SyncLoop(sl) => out.extend(ast::lower_sync_loop(sl)),
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                // Same fallback as the main worklist loop's ConstIf arm — the
                // checker rejects a non-const condition (E0811) before this
                // ever runs in practice.
                let val = const_eval(cond, consts).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                collect_lowered_sync_loops(branch, consts, out);
            }
            _ => {}
        }
    }
}

/// Recursively find every `ForEach` reachable from `items` — including one
/// nested inside a `const if`'s winning branch, at any nesting depth — and
/// push its lowering (`ast::lower_foreach_item`) onto `out`. Mirrors
/// `collect_lowered_sync_loops` exactly, for the same reason: `foreach` is
/// pure sugar over `repeat`/bare `loop` (`ast::foreach_lower`'s doc comment),
/// so the simulator lowers it the same way the emitter does. Called once,
/// before the worklist starts — see `elaborate_module`'s `lowered_foreach`.
fn collect_lowered_foreach(
    items: &[ModuleItem],
    consts: &BTreeMap<String, i128>,
    out: &mut Vec<ModuleItem>,
) {
    for it in items {
        match it {
            ModuleItem::ForEach(fe) => {
                // `None` here means the Elements-form source name didn't
                // resolve to an array/mem — the checker rejects this
                // (E0417) before `mimz build`/`mimz test` ever reach here,
                // but `mimz sim`/`mimz test` can run unchecked programs, so
                // this is a reachable path in this file. We silently drop
                // the loop rather than lowering it; any signal the dropped
                // body would have driven surfaces as a driverless-signal
                // read error downstream instead of a crash or wrong output.
                if let Some(lowered) = ast::lower_foreach_item(fe, items) {
                    out.extend(lowered);
                }
            }
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                // Same fallback as the main worklist loop's ConstIf arm — the
                // checker rejects a non-const `const if` condition (E0811)
                // before this ever runs in practice.
                let val = const_eval(cond, consts).unwrap_or(0);
                let branch: &[ModuleItem] = if val != 0 {
                    then
                } else {
                    els.as_deref().unwrap_or(&[])
                };
                collect_lowered_foreach(branch, consts, out);
            }
            _ => {}
        }
    }
}

/// Recursively lower every `SeqStmt::ForEach` reachable in `stmts` — inside
/// `If` branches and `Loop` bodies too, at any nesting depth — into its
/// `SeqStmt::Loop` equivalent via `ast::lower_foreach_seq`. Called once, on
/// an `on`-block's raw body, before it becomes a `Process` — so `Rw::seq`,
/// `assigns`, and the kernel's `run_seq` never see a raw `ForEach` node
/// (mirrors why `ModuleItem::ForEach` is pre-lowered before the worklist
/// starts, Task 8). An Elements-form source that doesn't resolve (`None`)
/// silently drops that `foreach`'s statements, matching this file's other
/// `None`-handling for the same construct (see `collect_lowered_foreach`).
fn lower_foreach_in_seq(stmts: &[SeqStmt], module_items: &[ModuleItem]) -> Vec<SeqStmt> {
    let mut out = Vec::new();
    for s in stmts {
        match s {
            SeqStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                if let Some(lowered) =
                    ast::lower_foreach_seq(var, source, body, *span, module_items)
                {
                    out.extend(lower_foreach_in_seq(&lowered, module_items));
                }
            }
            SeqStmt::If { cond, then, els } => {
                out.push(SeqStmt::If {
                    cond: cond.clone(),
                    then: lower_foreach_in_seq(then, module_items),
                    els: els.as_ref().map(|e| lower_foreach_in_seq(e, module_items)),
                });
            }
            SeqStmt::Loop {
                var,
                lo,
                hi,
                body,
                span,
            } => {
                out.push(SeqStmt::Loop {
                    var: var.clone(),
                    lo: lo.clone(),
                    hi: hi.clone(),
                    body: lower_foreach_in_seq(body, module_items),
                    span: *span,
                });
            }
            other => out.push(other.clone()),
        }
    }
    out
}

/// Record one combinational drive. A whole-signal drive becomes a `comb` entry;
/// a bit-indexed drive (`sum[i] = …`, from `repeat`) is collected per bit and
/// assembled into a Concat after the item loop. Slice drives are not yet handled.
fn record_drive(
    lhs: &ast::LValue,
    rhs: &Expr,
    rw: &Rw,
    consts: &BTreeMap<String, i128>,
    comb: &mut BTreeMap<String, Expr>,
    bit_drives: &mut BTreeMap<String, BTreeMap<u32, Expr>>,
) -> Result<(), String> {
    match &lhs.index {
        None => {
            comb.insert(lhs.base.name.clone(), rw.expr(rhs)?);
        }
        Some((idx, None)) => {
            let bit = const_eval(&rw.expr(idx)?, consts)?;
            // Bound to the evaluator's max width BEFORE `as u32`, so an oversized
            // index can't silently truncate into a valid bit.
            if !(0..128).contains(&bit) {
                return Err(format!(
                    "bit index {bit} driving `{}` is out of range (0..128)",
                    lhs.base.name
                ));
            }
            bit_drives
                .entry(lhs.base.name.clone())
                .or_default()
                .insert(bit as u32, rw.expr(rhs)?);
        }
        Some((_, Some(_))) => {
            return Err(format!(
                "driving a slice of `{}` is not supported by the simulator yet",
                lhs.base.name
            ));
        }
    }
    Ok(())
}

/// `clog2` matching the Verilog emitter and the `clog2` const-builtin: the bit
/// width of an `n`-variant enum encoding (one source of truth, so they agree).
fn clog2(n: usize) -> u32 {
    mimz_core::checker::consteval::clog2_bits(n as u128)
}

/// Returns true if `ty` is a bundle type (either `Type::Bundle` or a
/// `Type::Named` that names a registered bundle — not an enum).
fn is_bundle_ty(
    ty: &ast::Type,
    bundle_reg: &BundleRegistry<'_>,
    enums: &HashMap<String, &ast::EnumDecl>,
) -> bool {
    match ty {
        ast::Type::Bundle { .. } => true,
        ast::Type::Named(id) => {
            bundle_reg.contains_key(&id.name.name) && !enums.contains_key(&id.name.name)
        }
        _ => false,
    }
}

/// Extract `(bundle_qual_ident, args)` from a bundle type, or `None` for
/// non-bundle types. Returns the full `QualIdent` (not just the bare name)
/// so the caller can resolve it against a same-named bundle in another
/// file via [`resolve_bundle`] instead of collapsing to a bare-name lookup.
fn bundle_type_info(
    ty: &ast::Type,
    bundle_reg: &BundleRegistry<'_>,
    enums: &HashMap<String, &ast::EnumDecl>,
) -> Option<(ast::QualIdent, Vec<NamedArg>)> {
    match ty {
        ast::Type::Bundle { name, args } => Some((name.clone(), args.clone())),
        ast::Type::Named(id)
            if bundle_reg.contains_key(&id.name.name) && !enums.contains_key(&id.name.name) =>
        {
            Some((id.clone(), vec![]))
        }
        _ => None,
    }
}

/// Extract the expression for a named field from a bundle expression.
/// - If `expr` is a `BundleLit`, returns the matching field's value.
/// - If `expr` is an `Ident` (a bundle signal reference), returns `expr.field`
///   (dot-access, which `Rw::field` will flatten to `ident_fieldname`).
/// - Otherwise, falls back to a dot-access node.
fn bundle_field_expr(expr: &Expr, field: &str, span: mimz_core::span::Span) -> Expr {
    // OR-mux form: `lhs ?? rhs` where both operands (and the result) stay
    // bundle-typed. `merged.valid = lhs.valid || rhs.valid` (built as
    // `if lhs.valid { true } else { rhs.valid }`); every other field is
    // `if lhs.valid { lhs.field } else { rhs.field }`. Extracted per-field
    // by RECURSING into `bundle_field_expr` for `lhs`/`rhs` rather than
    // wrapping them in a bare `Field` node — `??` is left-associative and
    // chains (`x ?? y ?? z` parses as `Coalesce(Coalesce(x, y), z)`), so
    // `lhs` can itself be a `Coalesce` node: a bundle-typed compound
    // expression, not a plain signal reference. Recursing here re-enters
    // this same match and expands the nested chain correctly; wrapping it
    // in `Field { base: lhs, field }` instead would hand a `Coalesce` base
    // to `Rw::field`'s fallback, which recurses through `Rw::expr`'s
    // generic (unwrap-form) `Coalesce` arm — the wrong semantics for a
    // still-bundle-typed nested operand. Mirrors the fix applied to
    // `emit_verilog`'s `coalesce_field_expr` in Task 8's review.
    if let ExprKind::Binary {
        op: BinOp::Coalesce,
        lhs,
        rhs,
    } = &expr.kind
    {
        let cond = bundle_field_expr(lhs, "valid", span);
        let (then, els) = if field == "valid" {
            (
                Expr {
                    kind: ExprKind::Bool(true),
                    span,
                },
                bundle_field_expr(rhs, "valid", span),
            )
        } else {
            (
                bundle_field_expr(lhs, field, span),
                bundle_field_expr(rhs, field, span),
            )
        };
        return Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            span,
        };
    }
    if let ExprKind::BundleLit(inits) = &expr.kind {
        if let Some(fi) = inits.iter().find(|fi| fi.name.name == field) {
            return fi.value.clone();
        } else {
            unreachable!(
                "BundleLit missing field `{}` — checker should have rejected this",
                field
            )
        }
    }
    // RHS is a bundle ident or other expr: emit `expr.field` — Rw::field will flatten it.
    Expr {
        kind: ExprKind::Field {
            base: Box::new(expr.clone()),
            field: ast::Ident {
                name: field.to_string(),
                span,
            },
        },
        span,
    }
}

/// Rewrites expressions/statements during elaboration: enum-variant reads
/// (`State.Red`) → their index literal, instance-port reads (`add.sum`,
/// `fa[i].sum`) → the flat wire name, `match` variant patterns → their index
/// (tag-only) or IntMask (tagged), binding names in arm bodies → payload slice
/// expressions, plus a name substitution map (the `repeat` loop var and
/// inlined child signals). `consts` folds an array index to its concrete value.
///
/// Two lifetimes: `'d` for the module/file decls (long), `'s` for the
/// substitution map (may be a local extended map for match arm bindings).
struct Rw<'d, 's> {
    insts: &'d HashSet<String>,
    enums: &'d HashMap<String, &'d ast::EnumDecl>,
    /// Bundle-typed signal names: `req.valid` → `req_valid` via `field()`.
    bundle_sigs: &'d HashSet<String>,
    consts: &'d BTreeMap<String, i128>,
    subst: &'s HashMap<String, Expr>,
}

impl<'d, 's> Rw<'d, 's> {
    fn expr(&self, e: &Expr) -> Result<Expr, String> {
        let kind = match &e.kind {
            ExprKind::Int { .. } | ExprKind::Bool(_) => e.kind.clone(),
            ExprKind::Ident(n) => {
                if let Some(r) = self.subst.get(n) {
                    return Ok(r.clone());
                }
                e.kind.clone()
            }
            ExprKind::Field { base, field } => return self.field(e, base, field),
            ExprKind::Unary { op, expr } => ExprKind::Unary {
                op: *op,
                expr: Box::new(self.expr(expr)?),
            },
            // `raw ?? 0` (unwrap form only — the operand stays scalar-typed
            // after the `??`). Rewrite to `if raw.valid { raw.data } else { 0 }`
            // as an `IfExpr` (not rendered text — this AST is what the
            // event-driven kernel interprets at runtime). Mirrors
            // `emit_verilog`'s Task 7 ternary lowering. The OR-mux form
            // (`x ?? y` where both sides — and the result — stay
            // bundle-typed) never reaches this generic scalar-expression
            // rewrite; it's handled at the bundle-field level in
            // `bundle_field_expr` (Task 10).
            ExprKind::Binary {
                op: BinOp::Coalesce,
                lhs,
                rhs,
            } => {
                let valid_field = Expr {
                    kind: ExprKind::Field {
                        base: lhs.clone(),
                        field: ast::Ident {
                            name: "valid".to_string(),
                            span: lhs.span,
                        },
                    },
                    span: lhs.span,
                };
                let data_field = Expr {
                    kind: ExprKind::Field {
                        base: lhs.clone(),
                        field: ast::Ident {
                            name: "data".to_string(),
                            span: lhs.span,
                        },
                    },
                    span: lhs.span,
                };
                return self.expr(&Expr {
                    kind: ExprKind::IfExpr {
                        cond: Box::new(valid_field),
                        then: Box::new(data_field),
                        els: rhs.clone(),
                    },
                    span: e.span,
                });
            }
            ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
                op: *op,
                lhs: Box::new(self.expr(lhs)?),
                rhs: Box::new(self.expr(rhs)?),
            },
            ExprKind::IfExpr { cond, then, els } => ExprKind::IfExpr {
                cond: Box::new(self.expr(cond)?),
                then: Box::new(self.expr(then)?),
                els: Box::new(self.expr(els)?),
            },
            ExprKind::Match { scrutinee, arms } => {
                let rw_scrutinee = self.expr(scrutinee)?;
                let rw_arms = arms
                    .iter()
                    .map(|a| {
                        // For tagged variant patterns with bindings, extract
                        // each binding as a payload slice of the scrutinee so
                        // the runtime evaluator never sees Pattern::Variant.
                        let binding_subst = self.variant_bindings(&a.patterns, &rw_scrutinee)?;
                        let rw_value = if binding_subst.is_empty() {
                            self.expr(&a.value)?
                        } else {
                            let mut ext_subst: HashMap<String, Expr> = self
                                .subst
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            ext_subst.extend(binding_subst);
                            let ext_rw = Rw {
                                insts: self.insts,
                                enums: self.enums,
                                bundle_sigs: self.bundle_sigs,
                                consts: self.consts,
                                subst: &ext_subst,
                            };
                            ext_rw.expr(&a.value)?
                        };
                        Ok::<_, String>(ast::Arm {
                            patterns: a
                                .patterns
                                .iter()
                                .map(|p| self.pattern(p))
                                .collect::<Result<_, _>>()?,
                            value: rw_value,
                        })
                    })
                    .collect::<Result<_, _>>()?;
                ExprKind::Match {
                    scrutinee: Box::new(rw_scrutinee),
                    arms: rw_arms,
                }
            }
            ExprKind::Concat(parts) => ExprKind::Concat(
                parts
                    .iter()
                    .map(|p| self.expr(p))
                    .collect::<Result<_, _>>()?,
            ),
            ExprKind::Replicate { count, parts } => ExprKind::Replicate {
                count: Box::new(self.expr(count)?),
                parts: parts
                    .iter()
                    .map(|p| self.expr(p))
                    .collect::<Result<_, _>>()?,
            },
            ExprKind::Index { base, index } => ExprKind::Index {
                base: Box::new(self.expr(base)?),
                index: Box::new(self.expr(index)?),
            },
            ExprKind::Slice { base, hi, lo } => ExprKind::Slice {
                base: Box::new(self.expr(base)?),
                hi: Box::new(self.expr(hi)?),
                lo: Box::new(self.expr(lo)?),
            },
            ExprKind::Call { func, args } => ExprKind::Call {
                func: *func,
                args: args
                    .iter()
                    .map(|a| self.expr(a))
                    .collect::<Result<_, _>>()?,
            },
            // Pass FnCall through with args rewritten (const-folded, signal-names
            // substituted). The runtime evaluator handles the call.
            ExprKind::FnCall { name, args } => ExprKind::FnCall {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|a| self.expr(a))
                    .collect::<Result<_, _>>()?,
            },
            ExprKind::BundleLit(_) => {
                return Err("bundle literal in unsupported expression position".to_string());
            }
            ExprKind::ArrayLit(elems) => ExprKind::ArrayLit(
                elems
                    .iter()
                    .map(|e| self.expr(e))
                    .collect::<Result<_, _>>()?,
            ),
            ExprKind::EnumConstruct {
                enum_name,
                variant,
                args,
            } => return self.enum_construct(enum_name, variant, args, e.span),
        };
        Ok(Expr { kind, span: e.span })
    }

    /// Lowers `ExprKind::EnumConstruct` into the same tag+payload bit
    /// layout `pattern()`/`variant_bindings()` (above) already assume —
    /// mirrors their exact `tag_w`/`max_payload_w` computation. Produces a
    /// plain `ExprKind::Int` for a tag-only (zero-arg) variant, or an
    /// `ExprKind::Concat` otherwise — both already fully evaluated by
    /// `crate::sim::value`, so no new interpreter code is needed.
    fn enum_construct(
        &self,
        enum_name: &ast::Ident,
        variant: &ast::Ident,
        args: &[Expr],
        span: mimz_core::span::Span,
    ) -> Result<Expr, String> {
        let edecl = self
            .enums
            .get(&enum_name.name)
            .ok_or_else(|| format!("unknown enum `{}`", enum_name.name))?;
        let idx = edecl
            .variants
            .iter()
            .position(|v| v.name.name == variant.name)
            .ok_or_else(|| {
                format!(
                    "enum `{}` has no variant `{}`",
                    enum_name.name, variant.name
                )
            })?;
        let total_w = edecl
            .inferred_total_width
            .get()
            .expect("checker must run before elaborate") as u128;
        let tag_w = clog2(edecl.variants.len()) as u128;
        let max_payload_w = total_w - tag_w;
        if max_payload_w == 0 {
            return Ok(int_expr(idx as i128, span));
        }
        let decl_v = &edecl.variants[idx];
        // Every part of the concat must carry its EXACT bit width: a bare
        // `ExprKind::Int` literal evaluates to its own minimal width (e.g.
        // `0` is 1 bit — see `Val::from_int`), not the tag/field/padding
        // width the layout requires, so tag, args, and padding are all
        // wrapped in `extend(_, N)` to pin their width explicitly. A
        // non-literal arg (an ident/signal) already carries its declared
        // width; `extend` to that same width is then a no-op.
        let mut parts = Vec::new();
        // `tag_w` is `clog2(variant_count)`, which floors at 1 for any
        // legal (>=1-variant) enum — this branch always taken in practice.
        // Guarded anyway as defense in depth, matching the padding guard
        // below for the symmetric zero-width case.
        if tag_w > 0 {
            parts.push(extend_to(int_expr(idx as i128, span), tag_w as u32, span));
        }
        let mut used_w = 0u128;
        for (a, field) in args.iter().zip(decl_v.fields.iter()) {
            // Identical inline match to `variant_bindings`'s own field-width
            // computation above in this same file.
            let field_w: u128 = match &field.ty {
                ast::Type::Bit => 1,
                ast::Type::Bits(e) | ast::Type::Signed(e) => {
                    const_eval(e, self.consts).unwrap_or(0) as u128
                }
                ast::Type::Named(_) | ast::Type::Bundle { .. } | ast::Type::Array { .. } => 0,
            };
            used_w += field_w;
            parts.push(extend_to(self.expr(a)?, field_w as u32, span));
        }
        let padding_w = max_payload_w - used_w;
        if padding_w > 0 {
            parts.push(extend_to(int_expr(0, span), padding_w as u32, span));
        }
        Ok(Expr {
            kind: ExprKind::Concat(parts),
            span,
        })
    }

    fn field(&self, e: &Expr, base: &Expr, field: &ast::Ident) -> Result<Expr, String> {
        if let ExprKind::Ident(b) = &base.kind {
            // `Enum.Variant` → its index literal.
            if let Some(edecl) = self.enums.get(b) {
                let idx = edecl
                    .variants
                    .iter()
                    .position(|v| v.name.name == field.name)
                    .ok_or_else(|| format!("enum `{b}` has no variant `{}`", field.name))?;
                return Ok(int_expr(idx as i128, e.span));
            }
            // `inst.port` → flat wire `inst_port`.
            if self.insts.contains(b) {
                return Ok(ident_expr(format!("{b}_{}", field.name), e.span));
            }
            // `bundle_signal.field` → flat scalar `bundle_signal_field`.
            if self.bundle_sigs.contains(b) {
                return Ok(ident_expr(format!("{b}_{}", field.name), e.span));
            }
        }
        // `arr[i].port` → flat wire `arr__<i>_port` (the index folds here).
        if let ExprKind::Index { base: arr, index } = &base.kind
            && let ExprKind::Ident(arr_name) = &arr.kind
            && self.insts.contains(arr_name)
        {
            let n = const_eval(&self.expr(index)?, self.consts)?;
            return Ok(ident_expr(
                format!("{arr_name}__{n}_{}", field.name),
                e.span,
            ));
        }
        // Some other field access — keep the shape (recurse into the base).
        Ok(Expr {
            kind: ExprKind::Field {
                base: Box::new(self.expr(base)?),
                field: field.clone(),
            },
            span: e.span,
        })
    }

    fn pattern(&self, p: &Pattern) -> Result<Pattern, String> {
        match p {
            Pattern::Variant {
                enum_name,
                variant,
                bindings: _,
            } => {
                let edecl = self
                    .enums
                    .get(&enum_name.name)
                    .ok_or_else(|| format!("unknown enum `{}`", enum_name.name))?;
                let idx = edecl
                    .variants
                    .iter()
                    .position(|v| v.name.name == variant.name)
                    .ok_or_else(|| {
                        format!(
                            "enum `{}` has no variant `{}`",
                            enum_name.name, variant.name
                        )
                    })?;
                // note: fallback only correct for tag-only enums
                let total_w = edecl
                    .inferred_total_width
                    .get()
                    .unwrap_or_else(|| {
                        debug_assert!(
                            edecl.variants.iter().all(|v| v.fields.is_empty()),
                            "tagged enum reached pattern fallback without inferred_total_width — run checker first"
                        );
                        clog2(edecl.variants.len())
                    })
                    as u128;
                let tag_w = clog2(edecl.variants.len()) as u128;
                let max_payload_w = total_w - tag_w;
                if max_payload_w == 0 {
                    // Tag-only: simple integer comparison (existing behaviour).
                    Ok(Pattern::Int {
                        value: idx as u128,
                        raw: idx.to_string(),
                    })
                } else {
                    // Tagged: check only the tag bits (MSBs) via IntMask so
                    // payload bits are ignored by the pattern comparison.
                    let tag_val = (idx as u128) << max_payload_w;
                    let tag_mask = ((1u128 << tag_w) - 1) << max_payload_w;
                    Ok(Pattern::IntMask {
                        value: tag_val,
                        mask: tag_mask,
                        width: total_w as u32,
                        raw: idx.to_string(),
                    })
                }
            }
            other => Ok(other.clone()),
        }
    }

    /// Compute payload binding → slice-expression substitutions for the first
    /// variant pattern with bindings in `patterns`. Returns empty if no such
    /// pattern exists or the enum is tag-only.
    fn variant_bindings(
        &self,
        patterns: &[Pattern],
        scrutinee: &Expr,
    ) -> Result<HashMap<String, Expr>, String> {
        for p in patterns {
            let Pattern::Variant {
                enum_name,
                variant,
                bindings,
            } = p
            else {
                continue;
            };
            if bindings.is_empty() {
                continue;
            }
            let Some(edecl) = self.enums.get(&enum_name.name) else {
                continue;
            };
            let total_w = edecl
                .inferred_total_width
                .get()
                .unwrap_or_else(|| {
                    debug_assert!(
                        edecl.variants.iter().all(|v| v.fields.is_empty()),
                        "tagged enum reached pattern fallback without inferred_total_width — run checker first"
                    );
                    clog2(edecl.variants.len())
                }) as u128;
            let tag_w = clog2(edecl.variants.len()) as u128;
            let max_payload_w = total_w - tag_w;
            if max_payload_w == 0 {
                continue; // tag-only, no payload to extract
            }
            let Some(vdecl) = edecl.variants.iter().find(|v| v.name.name == variant.name) else {
                continue;
            };
            // Fields are packed MSB-first inside [max_payload_w-1 : 0].
            let mut cursor = max_payload_w;
            let mut out: HashMap<String, Expr> = HashMap::new();
            for (field, binding) in vdecl.fields.iter().zip(bindings.iter()) {
                let field_w: u128 = match &field.ty {
                    ast::Type::Bit => 1,
                    ast::Type::Bits(e) | ast::Type::Signed(e) => {
                        const_eval(e, self.consts).unwrap_or(0) as u128
                    }
                    ast::Type::Named(_) => 0, // E0807: already rejected by checker
                    ast::Type::Bundle { .. } => 0, // E0807 rejects bundle payload fields in enums
                    // Arrays are the same category as bundles here (not a scalar
                    // bit-vector payload field): fold to 0, matching the sibling
                    // arm exactly rather than inventing new behavior.
                    ast::Type::Array { .. } => 0,
                };
                debug_assert!(
                    field_w > 0,
                    "E0807 should have rejected zero-width payload fields before emit/sim"
                );
                if field_w == 0 {
                    continue;
                }
                let hi = cursor - 1;
                let lo = cursor - field_w;
                cursor -= field_w;
                let span = binding.span;
                // binding → scrutinee[hi:lo]
                out.insert(
                    binding.name.clone(),
                    Expr {
                        kind: ExprKind::Slice {
                            base: Box::new(scrutinee.clone()),
                            hi: Box::new(int_expr(hi as i128, span)),
                            lo: Box::new(int_expr(lo as i128, span)),
                        },
                        span,
                    },
                );
            }
            return Ok(out);
        }
        Ok(HashMap::new())
    }

    /// Rewrite a sequential statement, renaming each assignment target via
    /// `rename` (prefixing inlined child regs; identity for the parent).
    fn seq(&self, s: &SeqStmt, rename: &dyn Fn(&str) -> String) -> Result<SeqStmt, String> {
        Ok(match s {
            SeqStmt::Assign { lhs, rhs } => SeqStmt::Assign {
                lhs: ast::LValue {
                    base: ast::Ident {
                        name: rename(&lhs.base.name),
                        span: lhs.base.span,
                    },
                    index: match &lhs.index {
                        Some((a, b)) => {
                            Some((self.expr(a)?, b.as_ref().map(|x| self.expr(x)).transpose()?))
                        }
                        None => None,
                    },
                    span: lhs.span,
                },
                rhs: self.expr(rhs)?,
            },
            SeqStmt::If { cond, then, els } => SeqStmt::If {
                cond: self.expr(cond)?,
                then: then
                    .iter()
                    .map(|x| self.seq(x, rename))
                    .collect::<Result<_, _>>()?,
                els: match els {
                    Some(e) => Some(
                        e.iter()
                            .map(|x| self.seq(x, rename))
                            .collect::<Result<_, _>>()?,
                    ),
                    None => None,
                },
            },
            SeqStmt::Default { name, val, span } => SeqStmt::Default {
                name: ast::Ident {
                    name: rename(&name.name),
                    span: name.span,
                },
                val: self.expr(val)?,
                span: *span,
            },
            SeqStmt::Loop {
                var,
                lo,
                hi,
                body,
                span,
            } => SeqStmt::Loop {
                var: var.clone(),
                lo: self.expr(lo)?,
                hi: self.expr(hi)?,
                body: body
                    .iter()
                    .map(|x| self.seq(x, rename))
                    .collect::<Result<_, _>>()?,
                span: *span,
            },
            // Unreachable: every `SeqStmt::ForEach` in an `on`-block body is
            // lowered before `Rw::seq` ever runs — see
            // `elaborate_module`'s `ModuleItem::On` arm.
            SeqStmt::ForEach { .. } => unreachable!(
                "ForEach is lowered before Rw::seq/assigns/run_seq ever run — see elaborate_module's ModuleItem::On arm"
            ),
            // Unreachable on the sim path (strict-parsed tree); pass through.
            SeqStmt::Error(sp) => SeqStmt::Error(*sp),
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ast::File {
        mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses")
    }

    fn design(src: &str) -> Design {
        elaborate(&parse(src), None, &BTreeMap::new()).expect("elaborates")
    }

    const COUNTER: &str = "module Counter(WIDTH: int = 8) {\n  \
        clock clk\n  reset rst\n  out count: bits[WIDTH]\n  \
        reg value: bits[WIDTH] = 0\n  on rise(clk) { value <- value +% 1 }\n  \
        count = value\n}\n";

    #[test]
    fn elaborates_the_counter() {
        let d = design(COUNTER);
        assert_eq!(d.module, "Counter");
        assert_eq!(d.consts["WIDTH"], 8);
        assert_eq!(d.inputs, vec![]);
        assert_eq!(
            d.outputs,
            vec![Signal {
                name: "count".into(),
                width: Width {
                    bits: 8,
                    signed: false
                }
            }]
        );
        assert_eq!(
            d.regs,
            vec![Reg {
                name: "value".into(),
                width: Width {
                    bits: 8,
                    signed: false
                },
                reset: 0,
                clock: "clk".into(),
                edge: Edge::Rise,
            }]
        );
        assert!(d.comb.contains_key("count")); // count = value
        assert_eq!(d.clocks, vec!["clk".to_string()]);
        assert_eq!(d.resets, vec!["rst".to_string()]);
        assert_eq!(d.procs.len(), 1);
        assert_eq!(d.procs[0].clock, "clk");
    }

    #[test]
    fn param_override_folds_widths() {
        let d = elaborate(
            &parse(COUNTER),
            None,
            &BTreeMap::from([("WIDTH".into(), 4)]),
        )
        .expect("elaborates");
        assert_eq!(d.consts["WIDTH"], 4);
        assert_eq!(d.outputs[0].width.bits, 4);
        assert_eq!(d.regs[0].width.bits, 4);
    }

    #[test]
    fn elaborates_a_combinational_module() {
        // No clock/reset/reg → empty sequential parts, comb drivers only.
        let d = design(
            "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[9]\n  y = a + b\n}\n",
        );
        assert_eq!(d.inputs.len(), 2);
        assert_eq!(d.outputs.len(), 1);
        assert!(d.regs.is_empty());
        assert!(d.procs.is_empty());
        assert!(d.clocks.is_empty());
        assert!(d.resets.is_empty());
        assert!(d.comb.contains_key("y"));
    }

    #[test]
    fn reg_takes_a_nonzero_folded_reset_value() {
        let d = design(
            "module R {\n  clock clk\n  reset rst\n  out y: bits[8]\n  \
             reg r: bits[8] = 5\n  on rise(clk) { r <- r +% 1 }\n  y = r\n}\n",
        );
        assert_eq!(d.regs[0].reset, 5);
        assert_eq!(d.regs[0].clock, "clk");
    }

    #[test]
    fn flattens_a_same_file_instance() {
        // `Top` instantiates a combinational `Add`; the child's signals inline as
        // `u_a`/`u_b`/`u_s`, and the parent's `u.s` field-read becomes `u_s`.
        let d = elaborate(
            &parse(
                "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out s: bits[9]\n  \
                 s = a + b\n}\n\
                 module Top {\n  in x: bits[8]\n  in y: bits[8]\n  out t: bits[9]\n  \
                 let u = Add() { a: x, b: y }\n  t = u.s\n}\n",
            ),
            Some("Top"),
            &BTreeMap::new(),
        )
        .expect("flattens");
        assert_eq!(d.module, "Top");
        let wire_names: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
        assert!(wire_names.contains(&"u_a"), "wires: {wire_names:?}");
        assert!(wire_names.contains(&"u_b"), "wires: {wire_names:?}");
        assert!(wire_names.contains(&"u_s"), "wires: {wire_names:?}");
        // `t = u.s` → `t = u_s`; child output `u_s` is driven by `u_a + u_b`.
        assert!(d.comb.contains_key("t"));
        assert!(d.comb.contains_key("u_s"));
        assert!(d.regs.is_empty() && d.procs.is_empty());
    }

    #[test]
    fn rejects_unknown_instance_module() {
        let err = elaborate(
            &parse(
                "module Top {\n  out y: bits[8]\n  \
                 let u = Missing() { }\n  y = 0\n}\n",
            ),
            None,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("unknown module"), "got: {err}");
    }

    #[test]
    fn unrolls_repeat_with_instance_array_and_bit_drives() {
        // `repeat` inlines one `Xor` per bit; `s[i] = fa[i].o` collects bit drives
        // that assemble into a whole-signal Concat.
        let d = elaborate(
            &parse(
                "module Xor {\n  in a: bit\n  in b: bit\n  out o: bit\n  o = a ^ b\n}\n\
                 module R {\n  in x: bits[2]\n  in y: bits[2]\n  out s: bits[2]\n  \
                 repeat i: 0..2 {\n    let fa[i] = Xor() { a: x[i], b: y[i] }\n    \
                 s[i] = fa[i].o\n  }\n}\n",
            ),
            Some("R"),
            &BTreeMap::new(),
        )
        .expect("unrolls");
        let wires: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
        assert!(wires.contains(&"fa__0_o"), "wires: {wires:?}");
        assert!(wires.contains(&"fa__1_o"), "wires: {wires:?}");
        // `s` assembled from its per-bit drives.
        assert!(
            matches!(d.comb["s"].kind, ExprKind::Concat(_)),
            "s not a concat"
        );
    }

    #[test]
    fn unrolls_foreach_range_form_same_as_repeat() {
        // `foreach i in 0..2` is pure sugar over `repeat i: 0..2` (Task 8) —
        // same source as `unrolls_repeat_with_instance_array_and_bit_drives`
        // above, with `repeat i: 0..2` swapped for `foreach i in 0..2`, must
        // elaborate identically.
        let d = elaborate(
            &parse(
                "module Xor {\n  in a: bit\n  in b: bit\n  out o: bit\n  o = a ^ b\n}\n\
                 module R {\n  in x: bits[2]\n  in y: bits[2]\n  out s: bits[2]\n  \
                 foreach i in 0..2 {\n    let fa[i] = Xor() { a: x[i], b: y[i] }\n    \
                 s[i] = fa[i].o\n  }\n}\n",
            ),
            Some("R"),
            &BTreeMap::new(),
        )
        .expect("foreach range form unrolls");
        let wires: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
        assert!(wires.contains(&"fa__0_o"), "wires: {wires:?}");
        assert!(wires.contains(&"fa__1_o"), "wires: {wires:?}");
        assert!(
            matches!(d.comb["s"].kind, ExprKind::Concat(_)),
            "s not a concat"
        );
    }

    #[test]
    fn foreach_elements_form_substitutes_var_with_mem_index() {
        // Elements-form `foreach v in values` over a single-element `mem`
        // (module-level array-typed ports/wires/regs are E0416 — `mem` is
        // the only array-like module-level signal, mirroring the checker's
        // `foreach_elements_form_at_module_level_checks_clean`) lowers to a
        // `Repeat` over a synthesized index, substituting `v` with
        // `values[idx]` throughout the body (Task 8's `lower_foreach_item`,
        // Task 3). A single-element `mem` keeps the unrolled `sum = v`
        // drive single-valued, so the resulting comb driver for `sum` is
        // exactly `values[0]` — proving the substitution really flows
        // through this crate's own `elaborate_module`, not just the
        // AST-level unit test in `ast::foreach_lower`.
        let d = elaborate(
            &parse(
                "module M {\n  mem values: bits[8][1] = 0\n  out sum: bits[8]\n  \
                 foreach v in values {\n    sum = v\n  }\n}\n",
            ),
            Some("M"),
            &BTreeMap::new(),
        )
        .expect("foreach elements form over a mem elaborates");
        assert!(
            d.mems.iter().any(|m| m.name == "values"),
            "mems: {:?}",
            d.mems
        );
        assert!(d.comb.contains_key("sum"), "comb: {:?}", d.comb);
        assert!(
            matches!(&d.comb["sum"].kind, ExprKind::Index { base, .. } if matches!(&base.kind, ExprKind::Ident(n) if n == "values")),
            "sum must be driven by an index into `values`, got {:?}",
            d.comb["sum"]
        );
    }

    #[test]
    fn foreach_nested_inside_if_in_on_block_lowers_via_recursion() {
        // `lower_foreach_in_seq` must recurse into `If`'s `then` body, not
        // just dispatch on the on-block's top-level statements — a `foreach`
        // sitting inside an `if` inside `on rise(clk)` must still be
        // replaced by a `SeqStmt::Loop` before the block becomes a
        // `Process`, so `Rw::seq`/the kernel's `run_seq` never see a raw
        // `SeqStmt::ForEach` node at any nesting depth.
        let d = design(
            "module M {\n  clock clk\n  reset rst\n  in cond: bit\n  reg acc: bits[8] = 0\n  \
             on rise(clk) {\n    if cond {\n      foreach i in 0..4 {\n        acc <- acc +% 1\n      }\n    }\n  }\n}\n",
        );
        assert_eq!(d.procs.len(), 1);
        let SeqStmt::If { then, .. } = &d.procs[0].body[0] else {
            panic!(
                "expected the on-block's top-level `if` to survive, got {:?}",
                d.procs[0].body
            );
        };
        assert!(
            matches!(then.first(), Some(SeqStmt::Loop { .. })),
            "foreach nested inside if must lower to Loop, got {then:?}"
        );
        assert!(
            !then.iter().any(|s| matches!(s, SeqStmt::ForEach { .. })),
            "raw ForEach must not survive lowering, got {then:?}"
        );
    }

    #[test]
    fn elaborates_an_enum_signal_and_match() {
        // `reg st: S` width = clog2(2) = 1; `S.A` reset = 0; the match over the
        // enum elaborates (variant patterns rewritten to their indices).
        let d = design(
            "module FSM {\n  clock clk\n  reset rst\n  out o: bit\n  \
             enum S { A, B }\n  reg st: S = S.A\n  \
             on rise(clk) { st <- match st { S.A => S.B\n S.B => S.A } }\n  \
             o = st == S.B\n}\n",
        );
        let st = d.regs.iter().find(|r| r.name == "st").expect("reg st");
        assert_eq!(st.width.bits, 1);
        assert_eq!(st.reset, 0);
        assert!(d.comb.contains_key("o"));
    }

    // ---- C1–C4 hardening regressions (SEC-6 / the 2026 audit) ----

    #[test]
    fn recursive_instantiation_errors_not_overflows() {
        // SIM-1: `mimz sim`/`test` skip the checker, so a self-instantiating
        // module must error on the depth bound, not recurse into a stack overflow.
        let err = elaborate(
            &parse("module A {\n  out y: bits[8]\n  let u = A() { }\n  y = 0\n}\n"),
            None,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("nesting"), "got: {err}");
    }

    #[test]
    fn extreme_repeat_bounds_error_not_overflow() {
        // SIM-2: `hi - lo` past i128::MAX must be a clean over-budget error, not an
        // overflow panic.
        let big = "100000000000000000000000000000000000000"; // ~1e38, fits i128
        let src = format!(
            "module R {{\n  out y: bit\n  repeat i: -{big}..{big} {{\n    y[i] = 0\n  }}\n}}\n"
        );
        let err = elaborate(&parse(&src), None, &BTreeMap::new()).unwrap_err();
        assert!(err.contains("unroll"), "got: {err}");
    }

    #[test]
    fn an_out_of_range_bit_index_errors() {
        // SIM-3: a bit index past 128 must error, not truncate via `as u32`.
        let err = elaborate(
            &parse("module R {\n  out y: bits[4]\n  y[200] = 0\n}\n"),
            None,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn a_flatten_name_collision_errors() {
        // SIM-4: a parent signal named like a flattened `inst_port` wire must error
        // instead of silently overwriting.
        let err = elaborate(
            &parse(
                "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out s: bits[9]\n  \
                 s = a + b\n}\n\
                 module Top {\n  in x: bits[8]\n  out t: bits[9]\n  wire u_s: bits[9] = 0\n  \
                 let u = Add() { a: x, b: x }\n  t = u_s\n}\n",
            ),
            Some("Top"),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("collides"), "got: {err}");
    }

    #[test]
    fn two_same_named_modules_flatten_their_own_instance() {
        // SIM analogue of `emit_verilog::Project`'s Task 7 regression test
        // (`two_same_named_modules_emit_their_own_bodies`): file A's `Fifo`
        // and file B's `Fifo` have DIFFERENT bodies (different output
        // widths, so the assertion doesn't need to inspect expr content);
        // `M` instantiates each via a distinct qualified path. Before the
        // fix, `build_registry`'s bare-name `HashMap` let the LAST-inserted
        // file silently win for EVERY instance regardless of its qualifier
        // — both instances would flatten with the same (wrong, for one of
        // them) body. Hand-wires `path`/`resolved_file` the same way
        // `checker::tests::qualified_module_reference_resolves_unambiguously`
        // does — nothing in the pipeline computes this from real `import`
        // statements yet (that pass doesn't exist for `Inst.module` in
        // production either; only `Import.resolved_file` is set, by
        // `project.rs` at load time).
        let a = parse("module Fifo {\n  out y: bits[4]\n  y = 0\n}\n");
        let b = parse("module Fifo {\n  out y: bits[8]\n  y = 0\n}\n");
        let mut user = parse("module M {\n  let x = Fifo() { }\n  let z = Fifo() { }\n}\n");
        if let ast::TopItem::Module(m) = &mut user.items[0] {
            let mut insts = m.items.iter_mut().filter_map(|it| {
                if let ModuleItem::Inst(i) = it {
                    Some(i)
                } else {
                    None
                }
            });
            let x = insts.next().unwrap();
            x.module.path.push(ast::Ident {
                name: "a".into(),
                span: x.module.span,
            });
            x.module.resolved_file.set(Some(1));
            let z = insts.next().unwrap();
            z.module.path.push(ast::Ident {
                name: "b".into(),
                span: z.module.span,
            });
            z.module.resolved_file.set(Some(2));
        }
        let files = [user, a, b];
        let d = elaborate_project(&files, Some("M"), &BTreeMap::new()).expect("flattens");
        let width_of = |name: &str| d.wires.iter().find(|w| w.name == name).unwrap().width.bits;
        assert_eq!(width_of("x_y"), 4, "x must flatten file A's 4-bit Fifo");
        assert_eq!(width_of("z_y"), 8, "z must flatten file B's 8-bit Fifo");
    }

    #[test]
    fn qualified_instance_reference_resolves_via_a_real_import_path() {
        // Sim-side analogue of `checker::tests::
        // qualified_reference_actually_resolves_via_a_real_import_path`.
        // Unlike `two_same_named_modules_flatten_their_own_instance` above
        // (which hand-pokes `Inst.module.resolved_file` directly — the gap
        // Task 9 closes), this test has a real `import b` statement and a
        // real qualified `b.Fifo()` instantiation; only `Import.resolved_file`
        // is set (mimicking `project::load_project`, Task 3). `mimz sim`/
        // `mimz test` never run the checker, so `resolve_module` itself must
        // compute the match from `q.path` against `user`'s own `imports`.
        let a = parse("module Fifo {\n  out y: bits[4]\n  y = 0\n}\n");
        let b = parse("module Fifo {\n  out y: bits[8]\n  y = 0\n}\n");
        let user = parse("import b\n\nmodule M {\n  let z = b.Fifo() { }\n}\n");
        assert_eq!(user.imports.len(), 1, "sanity: `import b` parsed");
        user.imports[0].resolved_file.set(Some(2));
        let files = [user, a, b];
        let d = elaborate_project(&files, Some("M"), &BTreeMap::new())
            .expect("qualified instance must resolve via the real import match");
        let width = d
            .wires
            .iter()
            .find(|w| w.name == "z_y")
            .expect("flattened wire z_y")
            .width
            .bits;
        assert_eq!(
            width, 8,
            "z must flatten file B's 8-bit Fifo via the import match"
        );
    }

    #[test]
    fn ambiguous_bare_module_reference_errors_instead_of_silently_picking_one() {
        // Unlike `emit_verilog` (which only ever runs after the checker has
        // already rejected this as E0110), `mimz sim`/`mimz test` elaborate
        // the raw parse tree directly (see the module doc comment) — nothing
        // gates an ambiguous bare reference before it reaches here, so it
        // must be a real error, not a silent last-file-wins pick.
        let a = parse("module Fifo {\n  out y: bit\n  y = 1\n}\n");
        let b = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n");
        let user = parse("module M {\n  let u = Fifo() { }\n}\n");
        let files = [user, a, b];
        let err = elaborate_project(&files, Some("M"), &BTreeMap::new()).unwrap_err();
        assert!(err.contains("ambiguous"), "got: {err}");
    }

    #[test]
    fn an_i128_min_const_elaborates_without_overflow() {
        // SIM-5: a flattened child const that evaluates to i128::MIN must not
        // overflow-panic the negation in `int_expr`. i128::MAX is
        // 170141183460469231731687303715884105727, so `-MAX - 1` is i128::MIN,
        // reachable via checked arithmetic even on the checker-skipping sim path.
        let res = elaborate(
            &parse(
                "module Child {\n  \
                 const M: int = -170141183460469231731687303715884105727 - 1\n  \
                 out y: bit\n  y = 0\n}\n\
                 module Top {\n  out t: bit\n  let u = Child() { }\n  t = u_y\n}\n",
            ),
            Some("Top"),
            &BTreeMap::new(),
        );
        assert!(
            res.is_ok(),
            "i128::MIN const should elaborate, got: {res:?}"
        );
    }

    // --- `sync loop` elaboration timing (Task 10) ---

    fn sim(src: &str) -> super::super::kernel::Sim {
        super::super::kernel::Sim::new(design(src))
    }

    /// `start` pulsed for one cycle → `done` pulses exactly `hi - lo + 1`
    /// cycles later (counting the cycle `start` was sampled as cycle 1); a
    /// held-high `start` does not re-trigger the run mid-flight, because the
    /// lowered FSM only samples `start` from its idle branch (see
    /// `ast::sync_loop_lower::lower_sync_loop`'s `running_r` gate) — while
    /// `running_r` is set, the running branch never re-reads `start` at all.
    /// This exercises the lowered `Reg`/`On` items flowing through the real
    /// `kernel::Sim`, i.e. `kernel.rs`'s existing `tick_edge` dispatch with
    /// zero changes to that file.
    #[test]
    fn sync_loop_timing_and_no_mid_run_retrigger() {
        let mut s = sim(
            "module M {\n  clock clk\n  reset rst\n  sync loop s on rise(clk) (i: 0..4) -> result: bits[4] = 0 {\n    result <- result + 1\n  }\n}\n",
        );
        s.set("rst", 1).unwrap();
        s.tick("clk").unwrap();
        s.set("rst", 0).unwrap();
        s.set("s_start", 1).unwrap();
        s.tick("clk").unwrap(); // idle -> running, cnt = lo = 0
        s.set("s_start", 1).unwrap(); // held high through the run — must not re-trigger
        for _ in 0..3 {
            assert_eq!(
                s.peek("s_done").unwrap(),
                0,
                "must not pulse done before hi - lo cycles elapse"
            );
            s.tick("clk").unwrap();
        }
        assert_eq!(
            s.peek("s_done").unwrap(),
            0,
            "still one cycle short of hi - lo + 1"
        );
        s.tick("clk").unwrap();
        assert_eq!(
            s.peek("s_done").unwrap(),
            1,
            "done must pulse exactly hi - lo + 1 cycles after start was sampled"
        );
        assert_eq!(s.peek("s_result").unwrap(), 4);
    }

    /// Final whole-branch review, Finding 1: a `SyncLoop` nested inside a
    /// `const if` winning branch is checker-accepted (`checker::names`
    /// recurses into `ConstIf` branches when declaring names) and
    /// emitter-supported (`emit_verilog::module::flatten_items` recurses the
    /// same way) — the simulator must lower it too, instead of pushing the
    /// raw `SyncLoop` node onto the worklist where it hits the `unreachable!()`
    /// arm. Regression for the pre-fix panic (`elaborate_module`'s
    /// `lowered_sync_loops` only scanned direct `m.items` children).
    #[test]
    fn sync_loop_nested_in_const_if_elaborates_and_ticks() {
        let mut s = sim("module M {\n  clock clk\n  reset rst\n  \
             const if (1) {\n    \
             sync loop s on rise(clk) (i: 0..4) -> result: bits[4] = 0 {\n      result <- result + 1\n    }\n  \
             }\n}\n");
        s.set("rst", 1).unwrap();
        s.tick("clk").unwrap();
        s.set("rst", 0).unwrap();
        s.set("s_start", 1).unwrap();
        s.tick("clk").unwrap();
        for _ in 0..4 {
            s.tick("clk").unwrap();
        }
        assert_eq!(s.peek("s_done").unwrap(), 1);
        assert_eq!(s.peek("s_result").unwrap(), 4);
    }

    /// Same as above, but the `SyncLoop` sits in the `const if`'s losing
    /// branch — the winning (`else`) branch has no `SyncLoop` at all, so
    /// elaboration must succeed with no lowered items and no panic.
    #[test]
    fn sync_loop_in_const_if_losing_branch_is_not_lowered() {
        let d = design(
            "module M {\n  clock clk\n  reset rst\n  \
             const if (0) {\n    \
             sync loop s on rise(clk) (i: 0..4) -> result: bits[4] = 0 {\n      result <- result + 1\n    }\n  \
             } else {\n    wire w: bit = 0\n  }\n}\n",
        );
        assert!(d.wires.iter().any(|w| w.name == "w"));
        assert!(d.regs.iter().all(|r| r.name != "s_cnt"));
    }
}
