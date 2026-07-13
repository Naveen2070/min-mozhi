//! Integration tests over examples/ (RULES R6: examples always match the
//! spec). The folder has four flavor directories — english/, tanglish/,
//! tamil/, mixed/. The `BASE_EXAMPLES` set exists in ALL FOUR with identical
//! identifiers (only keywords differ), so each compiles to byte-identical
//! Verilog across flavors — the project's keyword-skin thesis.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The flavor folders under examples/. Every base example exists in each.
const FLAVORS: [&str; 4] = ["english", "tanglish", "tamil", "mixed"];

/// Every base example name (relative path without extension). Each appears
/// once per flavor folder — `4 * BASE_EXAMPLES.len()` files total.
const BASE_EXAMPLES: [&str; 40] = [
    "adder",
    "alu",
    "async_reset",
    "bitops",
    "blinker",
    "chained",
    "comparator",
    "counter",
    "datapath",
    "dual_edge",
    "edge_detector",
    "fn_array_search",
    "fn_const_local",
    "fn_mac",
    "fn_mac_local",
    "fn_return_guard",
    "fn_with_const",
    "foreach_fill",
    "foreach_sum",
    "lib/full_adder",
    "mux4",
    "std/debouncer",
    "std/seg7",
    "std/pwm",
    "std/fifo",
    "std/uart_tx",
    "priority",
    "pulse_gen",
    "regfile",
    "replicate",
    "ripple_adder",
    "shift_register",
    "shift",
    "signed_math",
    "traffic_light",
    "vilakku",
    "window",
    "tested_adder",
    "tagged_packet",
    "debug_wrapper",
];

/// Pure-Tamil showcase examples (Tamil keywords AND identifiers), each paired
/// with the English base example it mirrors. They live ONLY in
/// `examples/tamil-pure/` — they are language-pure, so they are NOT byte-identical
/// to any other flavor (localized names). Instead they are golden-locked and
/// proven equivalent to their counterpart by canonical identifier renaming
/// (see `pure_tamil_examples_are_equivalent_to_their_counterparts`).
const PURE_TAMIL: [(&str, &str); 16] = [
    ("kanakki", "counter"),
    ("cimitti", "blinker"),
    ("oppidi", "comparator"),
    ("perukki", "fn_mac"),
    ("thervi", "mux4"),
    ("kuutti", "adder"),
    ("saalaivilakku", "traffic_light"),
    ("tested_kuutti", "tested_adder"),
    ("nilaippaduthi", "std/debouncer"),
    ("ennkaatti", "std/seg7"),
    ("minukki", "std/pwm"),
    ("nakartthi", "shift"),
    ("varisai", "std/fifo"),
    ("anuppi", "std/uart_tx"),
    ("sirappu_pothi", "tagged_packet"),
    ("kootu", "foreach_sum"),
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

/// Golden-file comparison: every base example's emitted Verilog must match
/// `tests/golden/<base>.v` EXACTLY (english flavor — the other three are
/// already byte-identity-locked to it). The generator banner line is
/// stripped on both sides so a version bump doesn't churn every golden.
///
/// To regenerate after an INTENDED emitter change:
/// `MIMZ_UPDATE_GOLDENS=1 cargo test --test examples goldens` — then review
/// the diff like any other code change (docs/code/08, recipe).
#[test]
fn emitted_verilog_matches_the_goldens() {
    let update = std::env::var("MIMZ_UPDATE_GOLDENS").is_ok();
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden");
    for base in BASE_EXAMPLES {
        // Distinct temp tag: other tests compile the SAME example files in
        // parallel; sharing their temp paths is a torn-read race.
        let v = strip_banner(&compile_file_tagged(
            &examples_dir().join(format!("english/{base}.mimz")),
            "golden_",
        ));
        let golden_path = golden_dir.join(format!("{}.v", base.replace('/', "_")));
        if update {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&golden_path, &v).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| {
                panic!(
                    "missing golden {} — run with MIMZ_UPDATE_GOLDENS=1 to create it",
                    golden_path.display()
                )
            })
            .replace("\r\n", "\n");
        if v != want {
            let diff_line = v
                .lines()
                .zip(want.lines())
                .position(|(a, b)| a != b)
                .map(|i| i + 1)
                .unwrap_or_else(|| v.lines().count().min(want.lines().count()) + 1);
            panic!(
                "{base}: emitted Verilog differs from the golden at line {diff_line}.\n\
                 got:  {}\n\
                 want: {}\n\
                 If the change is intended, regenerate with MIMZ_UPDATE_GOLDENS=1 \
                 and review the diff.",
                v.lines().nth(diff_line - 1).unwrap_or("<end of output>"),
                want.lines().nth(diff_line - 1).unwrap_or("<end of golden>"),
            );
        }
    }
}

/// Helper that compiles a file with `--emit-testbench` and returns the generated `_tb.v` content.
/// The tag ensures temp files don't collide when tests run in parallel.
fn compile_file_tb_tagged(path: &Path, tag: &str) -> Option<String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let name = path.display().to_string().replace(['\\', '/', ':'], "_");
    let base_v = std::env::temp_dir().join(format!(
        "mimz_tb_{tag}{}_{name}.v",
        N.fetch_add(1, Ordering::Relaxed)
    ));
    // The testbench emitter creates a sidecar file next to `base_v` named `{base}_tb.v`.
    let mut tb_v = base_v.clone();
    let mut tb_name = tb_v.file_stem().unwrap().to_os_string();
    tb_name.push("_tb");
    tb_v.set_file_name(tb_name);
    tb_v.set_extension("v");

    let out = mimz()
        .arg("compile")
        .arg(path)
        .arg("-o")
        .arg(&base_v)
        .arg("--emit-testbench")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "`mimz compile --emit-testbench {}` failed:\n{}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );

    if tb_v.exists() {
        Some(std::fs::read_to_string(&tb_v).unwrap())
    } else {
        None
    }
}

/// `--emit-testbench` on a source with NO `test` blocks must still succeed,
/// write only the `.v` (no stray `_tb.v`), and print a clear note that the flag
/// had no effect — not silently produce nothing.
#[test]
fn emit_testbench_without_test_blocks_notes_and_writes_only_v() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let uniq = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let in_path = dir.join(format!("mimz_no_tests_{uniq}.mimz"));
    let out_v = dir.join(format!("mimz_no_tests_{uniq}.v"));
    let mut tb_v = out_v.clone();
    let mut tb_name = tb_v.file_stem().unwrap().to_os_string();
    tb_name.push("_tb");
    tb_v.set_file_name(tb_name);
    tb_v.set_extension("v");
    // Clear any leftovers so the `_tb.v` existence check below is meaningful.
    let _ = std::fs::remove_file(&out_v);
    let _ = std::fs::remove_file(&tb_v);
    std::fs::write(
        &in_path,
        "module Buf {\n  in a: bit\n  out y: bit\n  y = a\n}\n",
    )
    .unwrap();

    let out = mimz()
        .arg("compile")
        .arg(&in_path)
        .arg("-o")
        .arg(&out_v)
        .arg("--emit-testbench")
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "compile must still succeed with no tests:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out_v.exists(), "the `.v` must be written");
    assert!(
        !tb_v.exists(),
        "no `_tb.v` may be written when there are no tests"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no `test` blocks"),
        "expected a no-tests note on stderr, got:\n{stderr}"
    );

    let _ = std::fs::remove_file(&in_path);
    let _ = std::fs::remove_file(&out_v);
}

/// Golden-file comparison for auto-generated testbenches: any base example that
/// has inline `test` blocks must generate a `_tb.v` that exactly matches the
/// `tests/golden/<base>_tb.v` golden file.
#[test]
fn emitted_testbench_matches_the_goldens() {
    let update = std::env::var("MIMZ_UPDATE_GOLDENS").is_ok();
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden");

    let mut checks = Vec::new();
    for base in BASE_EXAMPLES {
        checks.push((
            format!("english/{base}.mimz"),
            format!("{}_tb.v", base.replace('/', "_")),
        ));
    }
    for (pure, _base) in PURE_TAMIL {
        checks.push((
            format!("tamil-pure/{pure}.mimz"),
            format!("tamil_pure_{pure}_tb.v"),
        ));
    }

    for (input_rel, golden_name) in checks {
        let input_path = examples_dir().join(&input_rel);
        let Some(tb) = compile_file_tb_tagged(&input_path, "golden_tb_") else {
            // No testbench generated for this example (no `test` blocks), skip.
            continue;
        };
        let tb = strip_banner(&tb);
        let golden_path = golden_dir.join(&golden_name);

        if update {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&golden_path, &tb).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| {
                panic!(
                    "missing testbench golden {} — run with MIMZ_UPDATE_GOLDENS=1 to create it",
                    golden_path.display()
                )
            })
            .replace("\r\n", "\n");

        if tb != want {
            let diff_line = tb
                .lines()
                .zip(want.lines())
                .position(|(a, b)| a != b)
                .map(|i| i + 1)
                .unwrap_or_else(|| tb.lines().count().min(want.lines().count()) + 1);
            panic!(
                "{input_rel}: emitted testbench differs from the golden at line {diff_line}.\n\
                 got:  {}\n\
                 want: {}\n\
                 If the change is intended, regenerate with MIMZ_UPDATE_GOLDENS=1 \
                 and review the diff.",
                tb.lines().nth(diff_line - 1).unwrap_or("<end of output>"),
                want.lines().nth(diff_line - 1).unwrap_or("<end of golden>"),
            );
        }
    }
}

/// Each pure-Tamil showcase example must be the SAME circuit as its English
/// counterpart. They are NOT byte-identical (the identifiers are localized to
/// Tamil), so we compare modulo identifier renaming: `canonicalize_verilog`
/// rewrites every identifier to `id<N>` in first-occurrence order, leaving
/// keywords/numbers/punctuation alone. Equal canonical forms ⇒ same circuit,
/// just named in Tamil. This is the showcase folder's correctness guarantee.
#[test]
fn pure_tamil_examples_are_equivalent_to_their_counterparts() {
    for (pure, base) in PURE_TAMIL {
        let ta = canonicalize_verilog(&strip_banner(&compile_file_tagged(
            &examples_dir().join(format!("tamil-pure/{pure}.mimz")),
            "equiv_pure_",
        )));
        let en = canonicalize_verilog(&strip_banner(&compile_file_tagged(
            &examples_dir().join(format!("english/{base}.mimz")),
            "equiv_base_",
        )));
        assert_eq!(
            ta, en,
            "tamil-pure/{pure}.mimz must be the same circuit as english/{base}.mimz (identifier names aside)"
        );
    }
}

/// Golden lock for the pure-Tamil showcase — pins the transliterated Verilog so
/// a romanization change can't slip through silently. Regenerate with
/// `MIMZ_UPDATE_GOLDENS=1` like the four-flavor goldens.
#[test]
fn pure_tamil_examples_match_goldens() {
    let update = std::env::var("MIMZ_UPDATE_GOLDENS").is_ok();
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden");
    for (pure, _base) in PURE_TAMIL {
        let v = strip_banner(&compile_file_tagged(
            &examples_dir().join(format!("tamil-pure/{pure}.mimz")),
            "tamil_pure_golden_",
        ));
        let golden_path = golden_dir.join(format!("tamil_pure_{pure}.v"));
        if update {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&golden_path, &v).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| {
                panic!(
                    "missing golden {} — run with MIMZ_UPDATE_GOLDENS=1 to create it",
                    golden_path.display()
                )
            })
            .replace("\r\n", "\n");
        assert_eq!(v, want, "{pure}: emitted Verilog differs from the golden");
    }
}

/// Canonicalize Verilog for alpha-equivalence: rename every identifier to
/// `id<N>` in first-occurrence order so two programs that differ ONLY in
/// identifier names produce the same string. Verilog keywords, numbers, and
/// punctuation are left untouched; a base letter right after `'` (sized
/// literals like `2'b00`) is not treated as an identifier.
fn canonicalize_verilog(v: &str) -> String {
    use std::collections::HashMap;
    const KEYWORDS: &[&str] = &[
        "module",
        "endmodule",
        "input",
        "output",
        "inout",
        "wire",
        "reg",
        "assign",
        "always",
        "begin",
        "end",
        "if",
        "else",
        "case",
        "endcase",
        "default",
        "posedge",
        "negedge",
        "parameter",
        "localparam",
        "signed",
        "integer",
        "initial",
        "for",
    ];
    let bytes = v.as_bytes();
    let mut out = String::with_capacity(v.len());
    let mut map: HashMap<&str, usize> = HashMap::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphabetic() || c == '_' {
            let prev = if i > 0 { bytes[i - 1] as char } else { '\0' };
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() {
                let d = bytes[j] as char;
                if d.is_ascii_alphanumeric() || d == '_' {
                    j += 1;
                } else {
                    break;
                }
            }
            let word = &v[start..j];
            if prev == '\'' || KEYWORDS.contains(&word) {
                out.push_str(word);
            } else {
                let n = map.len();
                let id = *map.entry(word).or_insert(n);
                out.push_str("id");
                out.push_str(&id.to_string());
            }
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Drop the `// Generated by mimz <version>` banner — it carries the crate
/// version, which must not invalidate goldens.
fn strip_banner(v: &str) -> String {
    let mut lines = v.lines();
    let first = lines.next().unwrap_or("");
    if first.starts_with("// Generated by mimz") {
        let rest: Vec<&str> = lines.collect();
        format!("{}\n", rest.join("\n").trim_start_matches('\n'))
    } else {
        v.replace("\r\n", "\n")
    }
}

/// Compile one example (path relative to examples/) and return the Verilog.
fn compile_example(example: &str) -> String {
    compile_file(&examples_dir().join(example))
}

fn compile_file(path: &Path) -> String {
    compile_file_tagged(path, "")
}

/// Like [`compile_file`], with a tag in the temp filename. A process-wide
/// counter makes every output path UNIQUE — tests run in parallel and
/// often compile the same example; a shared path is a torn-read race
/// (it bit the golden test on 2026-06-12, then the flavor-identity test).
fn compile_file_tagged(path: &Path, tag: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let name = path.display().to_string().replace(['\\', '/', ':'], "_");
    let out_v = std::env::temp_dir().join(format!(
        "mimz_test_{tag}{}_{name}.v",
        N.fetch_add(1, Ordering::Relaxed)
    ));
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
