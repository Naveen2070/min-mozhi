//! Integration tests: every example in examples/ must lex + parse clean
//! (RULES R6: examples always match the spec), and the simple ones must
//! compile to plausible Verilog via the CLI binary.

use std::path::PathBuf;
use std::process::Command;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

#[test]
fn every_example_checks_clean() {
    let mut checked = 0;
    for entry in std::fs::read_dir(examples_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "mimz") {
            let out = mimz().arg("check").arg(&path).output().unwrap();
            assert!(
                out.status.success(),
                "`mimz check {}` failed:\n{}",
                path.display(),
                String::from_utf8_lossy(&out.stderr)
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 5,
        "expected at least 5 examples, found {checked}"
    );
}

#[test]
fn counter_compiles_to_verilog() {
    let out_v = std::env::temp_dir().join("mimz_test_counter.v");
    let out = mimz()
        .arg("compile")
        .arg(examples_dir().join("counter.mimz"))
        .arg("-o")
        .arg(&out_v)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "compile failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = std::fs::read_to_string(&out_v).unwrap();
    assert!(v.contains("module Counter"));
    assert!(v.contains("parameter WIDTH = 8"));
    assert!(v.contains("always @(posedge clk)"));
    assert!(v.contains("if (rst)"));
    assert!(v.contains("value <= 0;"), "reset value should be generated");
    assert!(v.contains("assign count = value;"));
}

#[test]
fn tanglish_counter_compiles_to_identical_verilog() {
    let v_en = compile_to_string("counter.mimz");
    let v_ta = compile_to_string("counter.tanglish.mimz");
    assert_eq!(
        v_en, v_ta,
        "English and Tanglish flavors must produce identical Verilog — one AST, three skins"
    );
}

#[test]
fn alu_with_import_compiles() {
    let v = compile_to_string("alu.mimz");
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

#[test]
fn traffic_light_fsm_compiles() {
    let v = compile_to_string("traffic_light.mimz");
    assert!(v.contains("localparam") && v.contains("STATE_RED"));
    assert!(v.contains("STATE_GREEN") && v.contains("STATE_YELLOW"));
}

fn compile_to_string(example: &str) -> String {
    let out_v = std::env::temp_dir().join(format!("mimz_test_{example}.v"));
    let out = mimz()
        .arg("compile")
        .arg(examples_dir().join(example))
        .arg("-o")
        .arg(&out_v)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "compile {example} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(&out_v).unwrap()
}
