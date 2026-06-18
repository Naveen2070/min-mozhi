//! Tests for the in-memory `mimz::compile_string` entry point (the embedding
//! API behind the WASM playground). It runs the full pipeline on a source
//! string with no filesystem access — so these assert behavior the browser sees.

use mimz::compile_string;

/// A valid combinational module compiles to Verilog naming that module.
#[test]
fn compiles_valid_module_to_verilog() {
    let src = "\
module And2 {
  in a: bit
  in b: bit
  out y: bit
  y = a and b
}
";
    let verilog = compile_string(src).expect("And2 should compile");
    assert!(verilog.contains("module And2"), "got:\n{verilog}");
    // The generated header marks it as mimz output (not hand-written Verilog).
    assert!(
        verilog.contains("mimz"),
        "expected generator banner, got:\n{verilog}"
    );
}

/// The trilingual thesis holds through the in-memory path too: the same circuit
/// in Tamil keywords yields byte-identical Verilog to the English source.
#[test]
fn tamil_and_english_emit_identical_verilog() {
    let english = "\
module Buf {
  in a: bit
  out y: bit
  y = a
}
";
    let tamil = "\
தொகுதி Buf {
  உள்ளீடு a: bit
  வெளியீடு y: bit
  y = a
}
";
    let en = compile_string(english).expect("english compiles");
    let ta = compile_string(tamil).expect("tamil compiles");
    assert_eq!(en, ta, "trilingual output must be byte-identical");
}

/// A checker error (width mismatch) comes back as rendered diagnostics carrying
/// the stable E-code, not as Verilog.
#[test]
fn width_mismatch_returns_rendered_diagnostic() {
    let src = "\
module Narrow {
  in a: bits[4]
  out y: bits[2]
  y = a
}
";
    let err = compile_string(src).expect_err("width mismatch must fail");
    assert!(err.contains("E0401"), "expected E0401 in:\n{err}");
}

/// A lexer/parser error is reported (rendered), not silently dropped.
#[test]
fn syntax_error_is_reported() {
    let err = compile_string("module {").expect_err("garbage must fail");
    assert!(!err.is_empty(), "a syntax error should render a message");
}

/// `import` cannot be resolved in single-file mode and is rejected with a clear
/// message (no file system to resolve against).
#[test]
fn import_is_rejected() {
    let src = "\
import lib.adder
module M {
  in a: bit
  out y: bit
  y = a
}
";
    let err = compile_string(src).expect_err("import must be rejected");
    assert!(
        err.contains("import"),
        "expected an import message in:\n{err}"
    );
}
