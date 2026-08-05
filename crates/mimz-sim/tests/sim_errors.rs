//! `S0xxx` fixture-per-code contract test (R2 design, Phase 5) — mirrors
//! `tests/errors.rs`'s checker-code contract at the workspace root: every
//! code in [`mimz_sim::sim::ALL_SIM_CODES`] must fire at least once.
//!
//! Unlike the checker's own contract (which always runs through the real
//! `mimz` binary's `check` command, since the checker gate is unconditional),
//! most `S0xxx` conditions are conditions the CHECKER ALSO independently
//! rejects — `mimz sim`/`mimz eval`/`mimz test` gate on `checker::check`
//! before ever reaching the simulator (the A2 fix, `docs/audit/
//! review-2026-07-17.md` §3.1), so a fixture that went through that gate
//! first would never reach the runtime code it's meant to exercise. Every
//! fixture here therefore calls straight into `mimz-sim`'s public API
//! (`elaborate::elaborate_project(_with_mode)`, `comb::eval_outputs`,
//! `value::eval` against a small hand-built [`Resolver`], `kernel::Sim`,
//! `harness::run_test`), bypassing `checker::check` entirely — exactly the
//! way the design's own module doc comments describe `mimz sim`/`mimz test`
//! elaborating "the raw parse tree directly".

use std::collections::{BTreeMap, HashMap};

use mimz_core::ast::{self, TopItem};

use mimz_sim::sim::elaborate::{self, SimMode};
use mimz_sim::sim::host::{Direction, EmulationHost};
use mimz_sim::sim::value::{Resolver, Val};
use mimz_sim::sim::{ALL_SIM_CODES, Diag, comb, harness, kernel, value};

fn parse(src: &str) -> ast::File {
    mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses")
}

/// The lone module in a freshly parsed single-module file.
fn only_module(f: &ast::File) -> &ast::Module {
    f.items
        .iter()
        .find_map(|i| match i {
            TopItem::Module(m) => Some(m),
            _ => None,
        })
        .expect("file has a module")
}

/// The `init` expression of the named `wire` in a module — used to extract a
/// raw, not-yet-rewritten `Expr` (a `BundleLit`/`ArrayLit`/`EnumConstruct`)
/// straight from the parser, bypassing `elaborate::Rw` entirely, for the
/// handful of `S02xx` codes that only fire on an expression `elaborate`
/// itself would normally have already rewritten away.
fn wire_init<'a>(m: &'a ast::Module, name: &str) -> &'a ast::Expr {
    m.items
        .iter()
        .find_map(|it| match it {
            ast::ModuleItem::Wire { name: n, init, .. } if n.name == name => Some(init),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no wire `{name}` in module"))
}

/// The RHS expression of the named signal's `Drive` in a module — same
/// bypass-`elaborate::Rw` reasoning as [`wire_init`], for a plain `name =
/// expr` drive instead of a `wire`'s own init.
fn drive_rhs<'a>(m: &'a ast::Module, name: &str) -> &'a ast::Expr {
    m.items
        .iter()
        .find_map(|it| match it {
            ast::ModuleItem::Drive { lhs, rhs } if lhs.base.name == name => Some(rhs),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no drive of `{name}` in module"))
}

/// The lone `test` block in a freshly parsed file.
fn only_test(f: &ast::File) -> &ast::TestDecl {
    f.items
        .iter()
        .find_map(|i| match i {
            TopItem::Test(t) => Some(t),
            _ => None,
        })
        .expect("file has a test block")
}

/// Asserts `result` is an `Err` carrying exactly `code`; panics (naming
/// `label`) otherwise, printing whatever code/message actually fired.
fn assert_code<T>(label: &str, code: &str, result: Result<T, Box<Diag>>) {
    match result {
        Ok(_) => panic!("{label}: expected {code} to fire, but the call succeeded"),
        Err(e) => assert_eq!(
            e.code,
            Some(code),
            "{label}: expected {code}, got {:?} ({})",
            e.code,
            e.msg
        ),
    }
}

fn empty_params() -> BTreeMap<String, i128> {
    BTreeMap::new()
}

/// A minimal, fully-configurable [`Resolver`] for driving `value::eval`
/// directly — used for the `S02xx` conditions the module-level evaluators
/// (`comb::eval_outputs`, the event-driven kernel) structurally can't reach
/// (no array/memory modeling at that level; see each fixture's own comment).
#[derive(Default)]
struct TestResolver {
    signals: BTreeMap<String, Val>,
    ints: BTreeMap<String, i128>,
    arrays: BTreeMap<String, u32>,
    mem: Option<(String, Vec<Val>)>,
    funcs: Option<HashMap<String, ast::FuncDecl>>,
}

impl Resolver for TestResolver {
    fn signal(&mut self, name: &str) -> Result<Val, String> {
        self.signals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown name `{name}`"))
    }
    fn ints(&self) -> &BTreeMap<String, i128> {
        &self.ints
    }
    fn is_mem(&self, name: &str) -> bool {
        self.mem.as_ref().is_some_and(|(n, _)| n == name)
    }
    fn mem_read(&mut self, name: &str, addr: u128) -> Result<Val, String> {
        let (n, cells) = self
            .mem
            .as_ref()
            .ok_or_else(|| format!("memory `{name}` is not available in this context"))?;
        if n != name {
            return Err(format!("memory `{name}` is not available in this context"));
        }
        cells
            .get(addr as usize)
            .cloned()
            .ok_or_else(|| format!("memory `{name}` address {addr} out of range"))
    }
    fn funcs(&self) -> Option<&HashMap<String, ast::FuncDecl>> {
        self.funcs.as_ref()
    }
    fn array_len(&self, name: &str) -> Option<u32> {
        self.arrays.get(name).copied()
    }
}

// ---------------------------------------------------------------------
// S01xx — elaboration/wiring (sim/elaborate/*.rs).
// ---------------------------------------------------------------------

// S0101 was retired 2026-08-01 (BUG-26): `resolve_module`'s own "unknown
// module" arm was dead code (its only caller, `resolve_target`, always
// pre-checks the same `reg.contains_key` lookup first), so there was never
// a real condition to fixture here — see `sim/diag.rs`'s `ALL_SIM_CODES`
// doc comment.

#[test]
fn s0102_ambiguous_bare_reference() {
    let entry = parse("module M {\n  let s = Sub() {}\n}\n");
    let a = parse("module Sub {\n}\n");
    let b = parse("module Sub {\n}\n");
    let files = [entry, a, b];
    assert_code(
        "S0102",
        "S0102",
        elaborate::elaborate_project(&files, None, &empty_params()),
    );
}

#[test]
fn s0103_qualified_path_matches_no_import() {
    // No real `import` machinery here (mimz-sim has no filesystem) — a
    // qualified reference whose `Import.resolved_file` was never set (as
    // it would be by `project::load_project_with_lib`, the shell crate's
    // own loader) always fails this way, the same outcome a genuinely
    // stale/mismatched import path produces.
    let entry =
        parse("import somewhere.remote\nmodule M {\n  let s = somewhere.remote.Sub() {}\n}\n");
    let sub = parse("module Sub {\n}\n");
    let files = [entry, sub];
    assert_code(
        "S0103",
        "S0103",
        elaborate::elaborate_project(&files, None, &empty_params()),
    );
}

#[test]
fn s0104_qualified_reference_resolved_import_lacks_name() {
    let entry = parse("import lib\nmodule M {\n  let s = lib.Sub() {}\n}\n");
    // Manually resolve the import to `files[1]` ("NotSub"'s file), simulating
    // what `project::load_project_with_lib` normally does — `Sub` is
    // registered globally (via `files[2]`), so `resolve_target` calls
    // `resolve_module` at all, but the QUALIFIED path resolves to the WRONG
    // file, so `Sub` still isn't found there.
    if let Some(imp) = entry.imports.first() {
        imp.resolved_file.set(Some(1));
    }
    let not_sub = parse("module NotSub {\n}\n");
    let sub = parse("module Sub {\n}\n");
    let files = [entry, not_sub, sub];
    assert_code(
        "S0104",
        "S0104",
        elaborate::elaborate_project(&files, None, &empty_params()),
    );
}

#[test]
fn s0105_unknown_module_or_extern_combined_lookup() {
    let f = parse("module M {\n  let x = Bogus() {}\n}\n");
    assert_code(
        "S0105",
        "S0105",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0106_unknown_bundle_reference() {
    // The bare (`Type::Named`) form of an unrecognized type name always
    // resolves through `width_of`'s enum lookup (S0122), never reaching
    // `resolve_bundle` at all — only the PARENTHESIZED bundle-type form
    // (`Type::Bundle`, used for a bundle's own compile-time param overrides)
    // is looked up unconditionally via `resolve_bundle_fields_sim`, so an
    // unknown name there reaches the real "unknown bundle" check.
    let f = parse("module M {\n  in b: NoSuchBundle()\n}\n");
    assert_code(
        "S0106",
        "S0106",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0109_instance_parameter_has_no_value() {
    let entry = parse(
        "module Sub(N: int) {\n  out y: bit\n  y = 0\n}\nmodule M {\n  let s = Sub() {}\n}\n",
    );
    let files = [entry];
    // Two modules in one file — `pick_module` needs an explicit name, else
    // it would hit S0220 instead of reaching instance elaboration.
    assert_code(
        "S0109",
        "S0109",
        elaborate::elaborate_project(&files, Some("M"), &empty_params()),
    );
}

#[test]
fn s0112_instance_input_port_not_connected() {
    let entry = parse(
        "module Sub {\n  in a: bit\n  out y: bit\n  y = a\n}\nmodule M {\n  let s = Sub() {}\n}\n",
    );
    let files = [entry];
    assert_code(
        "S0112",
        "S0112",
        elaborate::elaborate_project(&files, Some("M"), &empty_params()),
    );
}

#[test]
fn s0113_extern_instance_has_no_model_in_strict_mode() {
    let entry = parse("extern module Ext {\n  out y: bit\n}\nmodule M {\n  let e = Ext() {}\n}\n");
    let files = [entry];
    assert_code(
        "S0113",
        "S0113",
        elaborate::elaborate_project_with_mode(&files, Some("M"), &empty_params(), SimMode::Strict),
    );
}

#[test]
fn s0115_unknown_enum_in_construct() {
    let f = parse("module M {\n  wire x: bit = Bogus.Foo()\n}\n");
    assert_code(
        "S0115",
        "S0115",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0116_enum_has_no_such_variant() {
    let f = parse("enum Color { Red, Green }\nmodule M {\n  wire x: bit = Color.Blue()\n}\n");
    assert_code(
        "S0116",
        "S0116",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0117_bundle_literal_in_unsupported_position() {
    // A bundle literal reaching `Rw::expr` in a position it never handles
    // directly (only `Wire`'s init and a bundle `Drive`'s RHS special-case
    // it) — nested inside a `Concat`, here.
    let f =
        parse("bundle Hs { valid: bit }\nmodule M {\n  out y: bit\n  y = { { valid: 1 } }\n}\n");
    assert_code(
        "S0117",
        "S0117",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0119_instance_nesting_exceeds_max_depth() {
    // `M` instantiates itself — unchecked (the checker rejects recursive
    // instantiation before this ever runs for real), so elaboration
    // recurses until the depth guard fires.
    let f = parse("module M {\n  let m = M() {}\n}\n");
    assert_code(
        "S0119",
        "S0119",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0121_module_parameter_has_no_default() {
    let f = parse("module M(N: int) {\n  out y: bit\n  y = 0\n}\n");
    assert_code(
        "S0121",
        "S0121",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0122_unknown_enum_type_in_signal_declaration() {
    let f = parse("module M {\n  in c: NoSuchEnum\n}\n");
    assert_code(
        "S0122",
        "S0122",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0123_memory_has_non_positive_depth() {
    let f = parse("module M {\n  mem m: bits[8][0] = 0\n}\n");
    assert_code(
        "S0123",
        "S0123",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0124_repeat_would_unroll_past_budget() {
    let f = parse("module M {\n  repeat i: 0..5000 {\n  }\n}\n");
    assert_code(
        "S0124",
        "S0124",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0125_nested_repeat_not_supported() {
    let f = parse("module M {\n  repeat i: 0..2 {\n    repeat j: 0..2 {\n    }\n  }\n}\n");
    assert_code(
        "S0125",
        "S0125",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0126_repeat_body_item_neither_instance_nor_drive() {
    let f = parse("module M {\n  repeat i: 0..2 {\n    reg r: bit = 0\n  }\n}\n");
    assert_code(
        "S0126",
        "S0126",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0127_bundle_destructure_in_module_body() {
    let f = parse("bundle Hs { valid: bit }\nmodule M {\n  in bus: Hs\n  let { valid } = bus\n}\n");
    assert_code(
        "S0127",
        "S0127",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0128_flattened_instance_signal_collides() {
    let entry = parse(
        "module Sub {\n  out x: bit\n  x = 1\n}\nmodule M {\n  wire sub_x: bit = 0\n  let sub = Sub() {}\n}\n",
    );
    let files = [entry];
    assert_code(
        "S0128",
        "S0128",
        elaborate::elaborate_project(&files, Some("M"), &empty_params()),
    );
}

#[test]
fn s0129_bit_driven_signal_has_no_declaration() {
    // `repeat`'s bit-indexed drive targets a name that's never declared as
    // a port/wire anywhere in the module.
    let f = parse("module M {\n  repeat i: 0..4 {\n    ghost[i] = 1\n  }\n}\n");
    assert_code(
        "S0129",
        "S0129",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0130_bit_driven_signal_bit_not_driven() {
    // `y` is 4 bits wide but only bit 0 is ever driven via a bit-indexed
    // drive — bits 1..3 are never assigned.
    let f = parse("module M {\n  out y: bits[4]\n  repeat i: 0..1 {\n    y[i] = 1\n  }\n}\n");
    assert_code(
        "S0130",
        "S0130",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0131_no_files_to_elaborate() {
    assert_code(
        "S0131",
        "S0131",
        elaborate::elaborate_project(&[], None, &empty_params()),
    );
}

#[test]
fn s0133_clock_reset_connection_not_a_plain_signal_name() {
    let entry = parse(
        "module Sub {\n  clock clk\n  out y: bit\n  y = 0\n}\nmodule M {\n  in a: bit\n  in b: bit\n  let s = Sub() { clk: a & b }\n}\n",
    );
    let files = [entry];
    assert_code(
        "S0133",
        "S0133",
        elaborate::elaborate_project(&files, Some("M"), &empty_params()),
    );
}

#[test]
fn s0134_bit_indexed_drive_index_out_of_range() {
    let f = parse("module M {\n  out y: bits[8]\n  y[200] = 1\n}\n");
    assert_code(
        "S0134",
        "S0134",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0135_slice_indexed_drive_bound_out_of_range() {
    let f = parse("module M {\n  out y: bits[8]\n  y[200:190] = 1\n}\n");
    assert_code(
        "S0135",
        "S0135",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0136_slice_indexed_drive_bounds_reversed() {
    let f = parse("module M {\n  out y: bits[8]\n  y[2:5] = 1\n}\n");
    assert_code(
        "S0136",
        "S0136",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

// S0137/S0138/S0139 fire inside `runner.rs`'s private `parse_source` — every
// one of its 6 callers bridges its `Diag` down to a flat `.msg` string
// (`.map_err(|e| e.msg)`, Task 1.5's deliberate decision), so no public API
// preserves the `.code` field for these three. The conditions themselves ARE
// exercised below (via the same public `run_command` entry point every
// caller shares) and asserted by message text — the strongest check
// available without either exposing `parse_source` or re-litigating Task
// 1.5's bridging decision, neither of which is this task's job. See
// `every_sim_code_has_a_fixture_above`'s `known_gaps` comment.

#[test]
fn s0137_std_import_must_be_two_segments() {
    let out = mimz_sim::run_command(
        "import std\nmodule M {\n  out y: bit\n  y = 0\n}\n",
        "check",
        &[],
    );
    let err = out.expect_err("a bare `import std` must fail");
    assert!(err.contains("exactly two segments"), "got: {err}");
}

#[test]
fn s0138_unknown_standard_library_module() {
    let out = mimz_sim::run_command(
        "import std.no_such_module\nmodule M {\n  out y: bit\n  y = 0\n}\n",
        "check",
        &[],
    );
    let err = out.expect_err("an unknown std module must fail");
    assert!(
        err.contains("unknown standard library module"),
        "got: {err}"
    );
}

#[test]
fn s0139_non_std_import_unsupported_in_memory() {
    let out = mimz_sim::run_command(
        "import some.other.file\nmodule M {\n  out y: bit\n  y = 0\n}\n",
        "check",
        &[],
    );
    let err = out.expect_err("a non-std import must fail in single-source mode");
    assert!(
        err.contains("not supported when compiling a single in-memory source"),
        "got: {err}"
    );
}

// ---------------------------------------------------------------------
// S02xx — expression evaluation at runtime (sim/value/*.rs, sim/comb.rs).
// ---------------------------------------------------------------------

#[test]
fn s0201_unknown_signal_reference() {
    let f = parse("module M {\n  out y: bit\n  y = zzz\n}\n");
    assert_code(
        "S0201",
        "S0201",
        comb::eval_outputs(
            std::slice::from_ref(&f),
            None,
            &BTreeMap::new(),
            &empty_params(),
        ),
    );
}

#[test]
fn s0202_no_match_arm_matched() {
    let f =
        parse("module M {\n  in a: bits[2]\n  out y: bit\n  y = match a {\n    0 => 1\n  }\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(3));
    assert_code(
        "S0202",
        "S0202",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0203_concat_or_replication_exceeds_max_width() {
    let f = parse("module M {\n  in a: bit\n  out y: bit\n  y = {1000001{a}}\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(1));
    assert_code(
        "S0203",
        "S0203",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0204_replication_count_must_be_at_least_one() {
    let f = parse("module M {\n  in a: bit\n  out y: bit\n  y = {0{a}}\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(1));
    assert_code(
        "S0204",
        "S0204",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0205_array_has_no_elements_to_index() {
    let f = parse("module M {\n  out y: bit\n  y = arr[0]\n}\n");
    let m = only_module(&f);
    let expr = drive_rhs(m, "y");
    let mut r = TestResolver {
        arrays: BTreeMap::from([("arr".to_string(), 0)]),
        ..Default::default()
    };
    assert_code("S0205", "S0205", value::eval(&mut r, expr));
}

#[test]
fn s0206_memory_read_fails() {
    let f = parse("module M {\n  out y: bit\n  y = m[0]\n}\n");
    let m = only_module(&f);
    let expr = drive_rhs(m, "y");
    // Force `is_mem("m")` true with no matching cell, so `mem_read` itself
    // fails (an empty cell list — any `addr` is out of range).
    let mut r = TestResolver {
        mem: Some(("m".to_string(), vec![])),
        ..Default::default()
    };
    assert_code("S0206", "S0206", value::eval(&mut r, expr));
}

#[test]
fn s0207_index_out_of_range_for_value_width() {
    let f = parse("module M {\n  in a: bits[8]\n  out y: bit\n  y = a[20]\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(5));
    assert_code(
        "S0207",
        "S0207",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0208_value_level_slice_bounds_reversed() {
    let f = parse("module M {\n  in a: bits[8]\n  out y: bits[4]\n  y = a[2:5]\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(5));
    assert_code(
        "S0208",
        "S0208",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0209_field_access_not_supported_by_evaluator() {
    let f = parse("module M {\n  in a: bit\n  out y: bit\n  y = a.nope\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(1));
    assert_code(
        "S0209",
        "S0209",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0210_bundle_literal_reaches_value_evaluator_unexpanded() {
    let f = parse("bundle Hs { valid: bit }\nmodule M {\n  wire x: Hs = { valid: 1 }\n}\n");
    let m = only_module(&f);
    let expr = wire_init(m, "x");
    let mut r = TestResolver::default();
    assert_code("S0210", "S0210", value::eval(&mut r, expr));
}

#[test]
fn s0211_array_literal_outside_fn_arg_or_let() {
    let f = parse("module M {\n  wire x: bit = [1, 2, 3]\n}\n");
    let m = only_module(&f);
    let expr = wire_init(m, "x");
    let mut r = TestResolver::default();
    assert_code("S0211", "S0211", value::eval(&mut r, expr));
}

#[test]
fn s0212_enum_construct_reaches_value_evaluator_unexpanded() {
    let f = parse("module M {\n  wire x: bit = Bogus.Foo()\n}\n");
    let m = only_module(&f);
    let expr = wire_init(m, "x");
    let mut r = TestResolver::default();
    assert_code("S0212", "S0212", value::eval(&mut r, expr));
}

#[test]
fn s0213_signal_of_enum_type_not_modeled() {
    // Extern-module output ports fold their width via `type_width`
    // directly (no enum special-casing the way a real module's own
    // `width_of` has) — a named (enum-shaped) output type reaches it as-is.
    let f = parse("extern module Ext {\n  out y: Color\n}\nmodule M {\n  let e = Ext() {}\n}\n");
    let files = [f];
    assert_code(
        "S0213",
        "S0213",
        elaborate::elaborate_project(&files, Some("M"), &empty_params()),
    );
}

#[test]
fn s0214_bundle_type_reaches_type_width_unflattened() {
    // A `reg`/`mem` declaration folds its width via `width_of` without the
    // bundle-vs-scalar branch `Port`/`Wire` get — a bundle-typed `reg`
    // reaches `type_width`'s generic `Type::Bundle` arm directly. Must be
    // the PARENTHESIZED bundle-type form (`Type::Bundle`, not the bare
    // `Type::Named` a plain `Hs` would parse as) — `width_of` special-cases
    // `Type::Named` as a potential enum first, so only `Type::Bundle` falls
    // straight through to `type_width`.
    let f = parse("bundle Hs { valid: bit }\nmodule M {\n  reg r: Hs() = 0\n}\n");
    assert_code(
        "S0214",
        "S0214",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0215_array_type_reaches_type_width_unexpanded() {
    let f = parse("module M {\n  reg r: bits[8][4] = 0\n}\n");
    assert_code(
        "S0215",
        "S0215",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0216_width_must_be_at_least_one() {
    let f = parse("module M {\n  out y: bits[0]\n  y = 0\n}\n");
    assert_code(
        "S0216",
        "S0216",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0217_width_exceeds_maximum() {
    let f = parse("module M {\n  out y: bits[2000000]\n  y = 0\n}\n");
    assert_code(
        "S0217",
        "S0217",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0218_no_module_with_the_given_name() {
    let f = parse("module M {\n  out y: bit\n  y = 0\n}\n");
    assert_code(
        "S0218",
        "S0218",
        elaborate::elaborate_project(std::slice::from_ref(&f), Some("NotHere"), &empty_params()),
    );
}

#[test]
fn s0219_file_defines_no_module() {
    let f = parse("const N: int = 8\n");
    assert_code(
        "S0219",
        "S0219",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0220_file_defines_multiple_modules() {
    let f = parse("module A {\n  out y: bit\n  y = 0\n}\nmodule B {\n  out y: bit\n  y = 0\n}\n");
    assert_code(
        "S0220",
        "S0220",
        elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params()),
    );
}

#[test]
fn s0221_shift_amount_cannot_be_signed() {
    let f = parse(
        "module M {\n  in a: bits[8]\n  in s: signed[4]\n  out y: bits[8]\n  y = a << s\n}\n",
    );
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(1));
    inputs.insert("s".to_string(), value::Bits::Small(1));
    assert_code(
        "S0221",
        "S0221",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0222_coalesce_reaches_binary_known_unlowered() {
    // `??` is normally desugared to an `IfExpr` by `elaborate::Rw::expr`
    // before evaluation — `comb.rs`'s lighter pipeline has no such rewrite
    // pass, so a raw `??` reaches `eval_ctx`'s `Binary` arm (and, through
    // it, `binary_known`) exactly as parsed.
    let f =
        parse("module M {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[8]\n  y = a ?? b\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(1));
    inputs.insert("b".to_string(), value::Bits::Small(2));
    assert_code(
        "S0222",
        "S0222",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0223_function_table_unavailable() {
    let f = parse("module M {\n  wire x: bit = f()\n}\n");
    let m = only_module(&f);
    let expr = wire_init(m, "x");
    let mut r = TestResolver::default();
    assert_code("S0223", "S0223", value::eval(&mut r, expr));
}

#[test]
fn s0224_undefined_function() {
    let f = parse("module M {\n  wire x: bit = f()\n}\n");
    let m = only_module(&f);
    let expr = wire_init(m, "x");
    let mut r = TestResolver {
        funcs: Some(HashMap::new()),
        ..Default::default()
    };
    assert_code("S0224", "S0224", value::eval(&mut r, expr));
}

#[test]
fn s0225_array_parameter_has_invalid_length() {
    let fn_src =
        parse("fn f(arr: bits[8][0 - 1]) -> bit {\n  0\n}\nmodule M {\n  wire x: bit = f()\n}\n");
    let func = fn_src
        .items
        .iter()
        .find_map(|i| match i {
            TopItem::Func(fd) => Some(fd.clone()),
            _ => None,
        })
        .expect("fn declared");
    let m = only_module(&fn_src);
    let expr = wire_init(m, "x");
    let mut r = TestResolver {
        funcs: Some(HashMap::from([(func.name.name.clone(), func)])),
        ..Default::default()
    };
    assert_code("S0225", "S0225", value::eval(&mut r, expr));
}

#[test]
fn s0226_function_called_with_too_few_arguments() {
    let fn_src = parse("fn f(x: bits[8]) -> bit {\n  0\n}\nmodule M {\n  wire y: bit = f()\n}\n");
    let func = fn_src
        .items
        .iter()
        .find_map(|i| match i {
            TopItem::Func(fd) => Some(fd.clone()),
            _ => None,
        })
        .expect("fn declared");
    let m = only_module(&fn_src);
    let expr = wire_init(m, "y");
    let mut r = TestResolver {
        funcs: Some(HashMap::from([(func.name.name.clone(), func)])),
        ..Default::default()
    };
    assert_code("S0226", "S0226", value::eval(&mut r, expr));
}

#[test]
fn s0227_loop_would_unroll_past_budget() {
    let fn_src = parse(
        "fn f() -> bit {\n  loop i: 0..5000 {\n  }\n  0\n}\nmodule M {\n  wire y: bit = f()\n}\n",
    );
    let func = fn_src
        .items
        .iter()
        .find_map(|i| match i {
            TopItem::Func(fd) => Some(fd.clone()),
            _ => None,
        })
        .expect("fn declared");
    let m = only_module(&fn_src);
    let expr = wire_init(m, "y");
    let mut r = TestResolver {
        funcs: Some(HashMap::from([(func.name.name.clone(), func)])),
        ..Default::default()
    };
    assert_code("S0227", "S0227", value::eval(&mut r, expr));
}

#[test]
fn s0228_extend_narrower_than_value() {
    let f = parse("module M {\n  in a: bits[8]\n  out y: bits[8]\n  y = extend(a, 2)\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(1));
    assert_code(
        "S0228",
        "S0228",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0229_clog2_is_compile_time_only() {
    let f = parse("module M {\n  in a: bits[8]\n  out y: bits[8]\n  y = clog2(a)\n}\n");
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(4));
    assert_code(
        "S0229",
        "S0229",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

// ---------------------------------------------------------------------
// S02xx — the combinational-only evaluator (sim/comb.rs).
// ---------------------------------------------------------------------

#[test]
fn s0230_eval_outputs_no_files() {
    assert_code(
        "S0230",
        "S0230",
        comb::eval_outputs(&[], None, &BTreeMap::new(), &empty_params()),
    );
}

#[test]
fn s0231_module_has_reg_state() {
    let f = parse(
        "module M {\n  clock clk\n  reset rst\n  in a: bit\n  out y: bit\n  reg r: bit = 0\n  on rise(clk) { r <- a }\n  y = r\n}\n",
    );
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), value::Bits::Small(1));
    assert_code(
        "S0231",
        "S0231",
        comb::eval_outputs(std::slice::from_ref(&f), None, &inputs, &empty_params()),
    );
}

#[test]
fn s0232_module_has_on_block() {
    let f = parse("module M {\n  clock clk\n  out y: bit\n  on rise(clk) {\n  }\n  y = 0\n}\n");
    assert_code(
        "S0232",
        "S0232",
        comb::eval_outputs(
            std::slice::from_ref(&f),
            None,
            &BTreeMap::new(),
            &empty_params(),
        ),
    );
}

#[test]
fn s0233_module_instantiates_a_sub_module() {
    let entry =
        parse("module Sub {\n  out y: bit\n  y = 0\n}\nmodule M {\n  let s = Sub() {}\n}\n");
    let files = [entry];
    assert_code(
        "S0233",
        "S0233",
        comb::eval_outputs(&files, Some("M"), &BTreeMap::new(), &empty_params()),
    );
}

#[test]
fn s0234_module_uses_repeat() {
    let f = parse("module M {\n  out y: bit\n  repeat i: 0..1 {\n    y = 1\n  }\n}\n");
    assert_code(
        "S0234",
        "S0234",
        comb::eval_outputs(
            std::slice::from_ref(&f),
            None,
            &BTreeMap::new(),
            &empty_params(),
        ),
    );
}

#[test]
fn s0235_module_uses_sync_loop() {
    let f = parse(
        "module M {\n  clock clk\n  in e: bits[8]\n  sync loop find_first on rise(clk) (i: 0..8) -> result: signed[4] = 0 - 1 {\n    if e[i] == 1 { result <- i }\n  }\n}\n",
    );
    assert_code(
        "S0235",
        "S0235",
        comb::eval_outputs(
            std::slice::from_ref(&f),
            None,
            &BTreeMap::new(),
            &empty_params(),
        ),
    );
}

#[test]
fn s0236_missing_value_for_input() {
    let f = parse("module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n");
    assert_code(
        "S0236",
        "S0236",
        comb::eval_outputs(
            std::slice::from_ref(&f),
            None,
            &BTreeMap::new(),
            &empty_params(),
        ),
    );
}

#[test]
fn s0237_signal_is_never_driven() {
    let f = parse("module M {\n  out y: bit\n}\n");
    assert_code(
        "S0237",
        "S0237",
        comb::eval_outputs(
            std::slice::from_ref(&f),
            None,
            &BTreeMap::new(),
            &empty_params(),
        ),
    );
}

#[test]
fn s0238_combinational_cycle_fires_with_its_own_code() {
    // BUG-27 (FIXED): a cycle can only ever be DETECTED on a re-entrant
    // `resolve()` call, and every re-entrant call is reached through
    // `Env::signal` (the `Resolver` trait boundary) — which, by Phase 2's
    // own "leave the trait alone" design, still bridges its result down to
    // a plain `String`. `Env::signal` now smuggles `resolve`'s own code
    // through that bridge (`sim::diag::bridge_code`) instead of discarding
    // it, and `eval_ctx`'s Ident-read recovers it (`diag_from_bridged`)
    // instead of always re-coding to the generic `S0201`.
    let f = parse("module M {\n  out y: bit\n  wire a: bit = b\n  wire b: bit = a\n  y = a\n}\n");
    assert_code(
        "S0238",
        "S0238",
        comb::eval_outputs(
            std::slice::from_ref(&f),
            None,
            &BTreeMap::new(),
            &empty_params(),
        ),
    );
}

// ---------------------------------------------------------------------
// S02xx — the event-driven kernel (sim/kernel.rs).
// ---------------------------------------------------------------------

#[test]
fn s0239_sim_set_not_a_drivable_signal() {
    let f = parse(
        "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg r: bit = 0\n  on rise(clk) { r <- 1 }\n  y = r\n}\n",
    );
    let design = elaborate::elaborate_project(std::slice::from_ref(&f), None, &empty_params())
        .expect("elaborates cleanly");
    let mut sim = kernel::Sim::new(design);
    assert_code(
        "S0239",
        "S0239",
        sim.set("not_a_signal", value::Bits::Small(1)),
    );
}

// ---------------------------------------------------------------------
// S03xx — test-harness control flow (sim/harness/mod.rs's `Run::exec`).
// ---------------------------------------------------------------------

struct NoOpHost;
impl EmulationHost for NoOpHost {
    fn bind(
        &mut self,
        _port: &str,
        peripheral: &str,
        _width: elaborate::Width,
        _args: &[ast::BindArg],
        _speed_hz: Option<u64>,
    ) -> Result<(), String> {
        match peripheral {
            "led" | "speaker" | "uart_tx" | "uart_rx" => Ok(()),
            other => Err(format!("unknown peripheral `{other}`")),
        }
    }
    fn direction_of(&self, name: &str) -> Option<Direction> {
        match name {
            "led" | "speaker" | "uart_tx" => Some(Direction::Output),
            "uart_rx" => Some(Direction::Input),
            _ => None,
        }
    }
    fn on_change(&mut self, _name: &str, _val: &Val) {}
    fn on_tick(&mut self, _name: &str, _val: &Val) -> Result<(), String> {
        Ok(())
    }
    fn drive(&mut self, _name: &str) -> Option<u64> {
        None
    }
    fn frame(&mut self, _cycle: u64) -> Result<bool, String> {
        Ok(false)
    }
    fn finish(&mut self) -> Result<bool, String> {
        Ok(false)
    }
}

fn run_test_headless(f: &ast::File, src: &str) -> Result<harness::Outcome, Box<Diag>> {
    let decl = only_test(f);
    harness::run_test(
        std::slice::from_ref(f),
        src,
        decl,
        Box::new(NoOpHost),
        false,
        false,
        false,
    )
}

#[test]
fn s0301_tick_names_an_unknown_clock() {
    let src =
        "module M {\n  clock clk\n  out y: bit\n  y = 0\n}\ntest \"t\" for M {\n  tick(nope)\n}\n";
    let f = parse(src);
    assert_code("S0301", "S0301", run_test_headless(&f, src));
}

#[test]
fn s0302_tick_count_evaluated_negative() {
    // `-1` (unary minus on a literal), not `0 - 1` — hardware subtraction on
    // bit-vectors wraps rather than going negative, so `0 - 1` would NOT
    // evaluate to a negative `Val` the way a signed literal does.
    let src = "module M {\n  clock clk\n  out y: bit\n  y = 0\n}\ntest \"t\" for M {\n  tick(clk, -1)\n}\n";
    let f = parse(src);
    assert_code("S0302", "S0302", run_test_headless(&f, src));
}

#[test]
fn s0303_tick_exceeds_simulation_cycle_limit() {
    let src = "module M {\n  clock clk\n  out y: bit\n  y = 0\n}\ntest \"t\" for M {\n  tick(clk, 500000001)\n}\n";
    let f = parse(src);
    assert_code("S0303", "S0303", run_test_headless(&f, src));
}

#[test]
fn s0304_sim_speed_not_positive() {
    let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\ntest \"t\" for M {\n  sim {\n    speed hz(0)\n    bind playing -> led()\n  }\n  tick(clk)\n}\n";
    let f = parse(src);
    assert_code("S0304", "S0304", run_test_headless(&f, src));
}

#[test]
fn s0305_tick_count_wider_than_a_plain_integer() {
    let src = "module M {\n  clock clk\n  out y: bit\n  y = 0\n}\ntest \"t\" for M {\n  tick(clk, 99999999999999999999999999999999999999999999999999)\n}\n";
    let f = parse(src);
    assert_code("S0305", "S0305", run_test_headless(&f, src));
}

// ---------------------------------------------------------------------
// S04xx — peripheral bind errors (sim/harness/mod.rs's `TestStmt::Sim`).
// ---------------------------------------------------------------------

#[test]
fn s0401_unknown_peripheral() {
    let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\ntest \"t\" for M {\n  sim {\n    bind playing -> microphone()\n  }\n  tick(clk)\n}\n";
    let f = parse(src);
    assert_code("S0401", "S0401", run_test_headless(&f, src));
}

#[test]
fn s0402_bind_direction_mismatch() {
    let src = "module M {\n  clock clk\n  in start: bit\n  out playing: bit\n  playing = start\n}\ntest \"t\" for M {\n  sim {\n    bind start -> led()\n  }\n  tick(clk)\n}\n";
    let f = parse(src);
    assert_code("S0402", "S0402", run_test_headless(&f, src));
}

#[test]
fn s0403_no_port_of_the_needed_direction() {
    let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\ntest \"t\" for M {\n  sim {\n    bind nope -> led()\n  }\n  tick(clk)\n}\n";
    let f = parse(src);
    assert_code("S0403", "S0403", run_test_headless(&f, src));
}

struct RejectingHost;
impl EmulationHost for RejectingHost {
    fn bind(
        &mut self,
        _port: &str,
        _peripheral: &str,
        _width: elaborate::Width,
        _args: &[ast::BindArg],
        _speed_hz: Option<u64>,
    ) -> Result<(), String> {
        Err("simulated hardware unavailable".to_string())
    }
    fn direction_of(&self, name: &str) -> Option<Direction> {
        match name {
            "led" => Some(Direction::Output),
            _ => None,
        }
    }
    fn on_change(&mut self, _name: &str, _val: &Val) {}
    fn on_tick(&mut self, _name: &str, _val: &Val) -> Result<(), String> {
        Ok(())
    }
    fn drive(&mut self, _name: &str) -> Option<u64> {
        None
    }
    fn frame(&mut self, _cycle: u64) -> Result<bool, String> {
        Ok(false)
    }
    fn finish(&mut self) -> Result<bool, String> {
        Ok(false)
    }
}

#[test]
fn s0404_peripheral_itself_rejects_the_bind() {
    let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\ntest \"t\" for M {\n  sim {\n    bind playing -> led()\n  }\n  tick(clk)\n}\n";
    let f = parse(src);
    let decl = only_test(&f);
    let result = harness::run_test(
        std::slice::from_ref(&f),
        src,
        decl,
        Box::new(RejectingHost),
        false,
        false,
        false,
    );
    assert_code("S0404", "S0404", result);
}

// ---------------------------------------------------------------------
// S05xx — assertion failures (sim/kernel.rs, sim/run.rs; GAP-6).
// ---------------------------------------------------------------------

#[test]
fn s0501_clocked_assert_fires() {
    let src = "module M {\n  clock clk\n  out y: bit\n  reg r: bit = 0\n  \
               on rise(clk) {\n    assert(r == 0)\n    r <- 1\n  }\n  y = r\n}\n\
               test \"t\" for M {\n  tick(clk, 2)\n}\n";
    let f = parse(src);
    assert_code("S0501", "S0501", run_test_headless(&f, src));
}

// ---------------------------------------------------------------------
// Coverage: every code in `ALL_SIM_CODES` must have fired above at least
// once. This list is maintained by hand alongside the fixtures above —
// deliberately, not auto-collected across independently-run `#[test]`
// functions (which can't share state safely) — mirroring
// `tests/errors.rs`'s own fixture-per-code contract in spirit.
// ---------------------------------------------------------------------

#[test]
fn every_sim_code_has_a_fixture_above() {
    // Codes intentionally not covered by their OWN dedicated fixture above,
    // with why:
    let known_gaps: &[&str] = &[
        // Fire correctly (verified by message text in their own fixtures
        // above) but their `.code` isn't observable through any PUBLIC API:
        // every caller of the private `parse_source` bridges its `Diag`
        // down to a flat `.msg` string (Task 1.5's deliberate decision).
        "S0137", "S0138", "S0139",
    ];
    let covered: &[&str] = &[
        "S0102", "S0103", "S0104", "S0105", "S0106", "S0109", "S0112", "S0113", "S0115", "S0116",
        "S0117", "S0119", "S0121", "S0122", "S0123", "S0124", "S0125", "S0126", "S0127", "S0128",
        "S0129", "S0130", "S0131", "S0133", "S0134", "S0135", "S0136", "S0201", "S0202", "S0203",
        "S0204", "S0205", "S0206", "S0207", "S0208", "S0209", "S0210", "S0211", "S0212", "S0213",
        "S0214", "S0215", "S0216", "S0217", "S0218", "S0219", "S0220", "S0221", "S0222", "S0223",
        "S0224", "S0225", "S0226", "S0227", "S0228", "S0229", "S0230", "S0231", "S0232", "S0233",
        "S0234", "S0235", "S0236", "S0237", "S0238", "S0239", "S0301", "S0302", "S0303", "S0304",
        "S0305", "S0401", "S0402", "S0403", "S0404", "S0501",
    ];
    let missing: Vec<&str> = ALL_SIM_CODES
        .iter()
        .copied()
        .filter(|c| !covered.contains(c) && !known_gaps.contains(c))
        .collect();
    assert!(
        missing.is_empty(),
        "these S0xxx codes have no fixture in this file: {missing:?}"
    );
}
