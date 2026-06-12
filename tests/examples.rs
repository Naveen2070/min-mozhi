//! Integration tests over examples/ (RULES R6: examples always match the
//! spec). The folder has four flavor directories — english/, tanglish/,
//! tamil/, mixed/ — each holding the SAME example set with identical
//! identifiers, so every example must lex + parse clean, compile to
//! Verilog, and compile to byte-identical Verilog in all four flavors.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The flavor folders under examples/. Every base example exists in each.
const FLAVORS: [&str; 4] = ["english", "tanglish", "tamil", "mixed"];

/// Every base example name (relative path without extension). Each appears
/// once per flavor folder — `4 * BASE_EXAMPLES.len()` files total.
const BASE_EXAMPLES: [&str; 12] = [
    "adder",
    "alu",
    "blinker",
    "chained",
    "comparator",
    "counter",
    "edge_detector",
    "lib/full_adder",
    "mux4",
    "ripple_adder",
    "shift_register",
    "traffic_light",
];

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Every `.mimz` file under examples/, recursively (the flavor folders
/// have a `lib/` subfolder for dotted-import targets).
fn all_mimz_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "mimz") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&examples_dir(), &mut files);
    files.sort();
    files
}

#[test]
fn every_example_checks_clean() {
    let files = all_mimz_files();
    assert!(
        files.len() >= FLAVORS.len() * BASE_EXAMPLES.len(),
        "expected at least {} examples (4 flavors x {} base examples), found {}",
        FLAVORS.len() * BASE_EXAMPLES.len(),
        BASE_EXAMPLES.len(),
        files.len()
    );
    for path in files {
        let out = mimz().arg("check").arg(&path).output().unwrap();
        assert!(
            out.status.success(),
            "`mimz check {}` failed:\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// The headline guarantee: EVERY example in the folder compiles to
/// Verilog — including the lib/ helpers, which are valid stand-alone
/// modules. A new example that doesn't compile fails CI by name.
#[test]
fn every_example_compiles() {
    for path in all_mimz_files() {
        compile_file(&path);
    }
}

/// One AST, four skins: each base example must compile to byte-identical
/// Verilog from its english/tanglish/tamil/mixed versions. This is the
/// project's thesis as a test.
#[test]
fn all_four_flavors_compile_to_identical_verilog() {
    for base in BASE_EXAMPLES {
        let reference = compile_example(&format!("english/{base}.mimz"));
        for flavor in &FLAVORS[1..] {
            let v = compile_example(&format!("{flavor}/{base}.mimz"));
            assert_eq!(
                reference, v,
                "{flavor}/{base}.mimz must produce the same Verilog as english/{base}.mimz — one AST, keyword skins only"
            );
        }
    }
}

#[test]
fn counter_compiles_to_verilog() {
    let v = compile_example("english/counter.mimz");
    assert!(v.contains("module Counter"));
    assert!(v.contains("parameter WIDTH = 8"));
    assert!(v.contains("always @(posedge clk)"));
    assert!(v.contains("if (rst)"));
    assert!(v.contains("value <= 0;"), "reset value should be generated");
    assert!(v.contains("assign count = value;"));
}

#[test]
fn alu_with_import_compiles() {
    let v = compile_example("english/alu.mimz");
    assert!(v.contains("module Alu"));
    assert!(v.contains("module Top"));
    assert!(
        v.contains("module Adder"),
        "imported module must be emitted too"
    );
    assert!(v.contains("Adder #(.WIDTH(8)) add"), "instance with params");
    assert!(
        v.contains("wire") && v.contains("add_sum"),
        "auto-wired child output"
    );
}

/// `include` is an English alias of `import` — prove it works through the
/// whole pipeline (english/chained.mimz uses `include lib.full_adder`),
/// not just in the keyword table.
#[test]
fn include_alias_compiles_with_dotted_path() {
    let v = compile_example("english/chained.mimz");
    assert!(v.contains("module Chained"));
    assert!(
        v.contains("module FullAdder"),
        "`include lib.full_adder` must pull in lib/full_adder.mimz"
    );
    assert!(
        v.contains("FullAdder fa0") && v.contains("FullAdder fa1"),
        "both chained instances must be emitted"
    );
}

/// `repeat` unrolls at compile time: the WIDTH=4 ripple adder must emit
/// exactly four FullAdder instances with the carry chained stage to stage,
/// and the loop variable folded into every index. This is the headline
/// proof that compile-time generation works end to end.
#[test]
fn ripple_adder_unrolls_repeat() {
    let v = compile_example("english/ripple_adder.mimz");
    assert!(v.contains("module RippleAdder"));
    assert!(
        v.contains("module FullAdder"),
        "the dotted import must pull lib/full_adder in"
    );
    // Four unrolled instances with flattened array names.
    for i in 0..4 {
        assert!(
            v.contains(&format!("FullAdder fa__{i} (")),
            "instance fa__{i} must be emitted"
        );
    }
    assert!(
        !v.contains("fa__4"),
        "the half-open range 0..4 stops at 3 — no fa__4"
    );
    // The carry chain: bit 0 takes cin, bit 1 takes bit 0's carry-out.
    assert!(v.contains(".cin(cin)"), "bit 0 takes the module carry-in");
    assert!(
        v.contains(".cin(fa__0_cout)"),
        "bit 1 takes bit 0's carry-out — the `if i==0` folded away"
    );
    // Folded indices in the drives and the final carry-out.
    assert!(v.contains("assign sum[0] = fa__0_sum;"));
    assert!(v.contains("assign sum[3] = fa__3_sum;"));
    assert!(
        v.contains("assign cout = fa__3_cout;"),
        "cout = fa[WIDTH-1].cout folds WIDTH-1 to 3"
    );
    // `const WIDTH` folded into the port widths (no symbolic WIDTH left).
    assert!(
        v.contains("[(4)-1:0] a"),
        "const WIDTH folds to 4 in widths"
    );
    assert!(
        !v.contains("WIDTH"),
        "a const is compile-time, never emitted"
    );
}

#[test]
fn traffic_light_fsm_compiles() {
    let v = compile_example("english/traffic_light.mimz");
    assert!(v.contains("localparam") && v.contains("STATE_RED"));
    assert!(v.contains("STATE_GREEN") && v.contains("STATE_YELLOW"));
}

/// Compile one example (path relative to examples/) and return the Verilog.
fn compile_example(example: &str) -> String {
    compile_file(&examples_dir().join(example))
}

fn compile_file(path: &Path) -> String {
    let name = path.display().to_string().replace(['\\', '/', ':'], "_");
    let out_v = std::env::temp_dir().join(format!("mimz_test_{name}.v"));
    let out = mimz()
        .arg("compile")
        .arg(path)
        .arg("-o")
        .arg(&out_v)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`mimz compile {}` failed:\n{}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(&out_v).unwrap()
}
