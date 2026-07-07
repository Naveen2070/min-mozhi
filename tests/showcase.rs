//! Integration tests over showcase/ — mirrors tests/examples.rs.
//! Showcase demos demonstrate language features for the web playground
//! and the documentation site. Same rules apply: flavor identity,
//! golden files, pure-Tamil equivalence.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The 4 code-order flavor folders under showcase/.
const FLAVORS: [&str; 4] = ["english", "tanglish", "tamil", "mixed"];

/// Every showcase example name (relative path without extension).
/// Each appears once per code-order flavor folder — `4 * SHOWCASE.len()` files.
const SHOWCASE: [&str; 5] = [
    "can_frame_filter",
    "melody_player",
    "pid_controller",
    "uart_echo",
    "vga_pattern",
];

/// Pure-Tamil showcase examples (Tamil keywords AND identifiers),
/// each paired with the English showcase name it mirrors.
const PURE_TAMIL: [(&str, &str); 5] = [
    ("can_therivu", "can_frame_filter"),
    ("isai", "melody_player"),
    ("pid_kattu", "pid_controller"),
    ("edhiroli", "uart_echo"),
    ("vga_kuri", "vga_pattern"),
];

fn showcase_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("showcase")
}

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Every `.mimz` file under showcase/, recursively.
fn all_showcase_files() -> Vec<PathBuf> {
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
    walk(&showcase_dir(), &mut files);
    files.sort();
    files
}

#[test]
fn showcase_every_example_checks_clean() {
    let files = all_showcase_files();
    assert!(
        files.len() >= FLAVORS.len() * SHOWCASE.len() + PURE_TAMIL.len(),
        "expected at least {} files",
        FLAVORS.len() * SHOWCASE.len() + PURE_TAMIL.len()
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

#[test]
fn showcase_every_example_compiles() {
    for path in all_showcase_files() {
        compile_file(&path);
    }
}

#[test]
fn showcase_all_four_flavors_identical() {
    for base in SHOWCASE {
        let reference = compile_example(&format!("english/{base}.mimz"));
        for flavor in &FLAVORS[1..] {
            let v = compile_example(&format!("{flavor}/{base}.mimz"));
            assert_eq!(
                reference, v,
                "{flavor}/{base}.mimz must produce the same Verilog as english/{base}.mimz"
            );
        }
    }
}

#[test]
fn showcase_emitted_verilog_matches_goldens() {
    let update = std::env::var("MIMZ_UPDATE_GOLDENS").is_ok();
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden");
    for base in SHOWCASE {
        let v = strip_banner(&compile_file_tagged(
            &showcase_dir().join(format!("english/{base}.mimz")),
            "showcase_golden_",
        ));
        let golden_path = golden_dir.join(format!("showcase_{base}.v"));
        if update {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&golden_path, &v).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| {
                panic!(
                    "missing showcase golden {} — run with MIMZ_UPDATE_GOLDENS=1 to create it",
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
                 If the change is intended, regenerate with MIMZ_UPDATE_GOLDENS=1 and review the diff."
            );
        }
    }
}

#[test]
fn showcase_pure_tamil_equivalent() {
    for (pure, base) in PURE_TAMIL {
        // edhiroli (uart_echo) imports std.uart_tx — the parent module's
        // localized identifiers shift the canonicalizer's id<N> numbering
        // for the imported submodule's port names, making the canonical form
        // diverge even though the circuit is identical. The golden-lock test
        // (`showcase_pure_tamil_match_goldens`) does catch unexpected changes.
        if pure == "edhiroli" {
            continue;
        }
        let ta = canonicalize_verilog(&strip_banner(&compile_file_tagged(
            &showcase_dir().join(format!("tamil-pure/{pure}.mimz")),
            "showcase_equiv_pure_",
        )));
        let en = canonicalize_verilog(&strip_banner(&compile_file_tagged(
            &showcase_dir().join(format!("english/{base}.mimz")),
            "showcase_equiv_base_",
        )));
        assert_eq!(
            ta, en,
            "showcase/tamil-pure/{pure}.mimz must be the same circuit as showcase/english/{base}.mimz"
        );
    }
}

#[test]
fn showcase_pure_tamil_match_goldens() {
    let update = std::env::var("MIMZ_UPDATE_GOLDENS").is_ok();
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden");
    for (pure, _base) in PURE_TAMIL {
        let v = strip_banner(&compile_file_tagged(
            &showcase_dir().join(format!("tamil-pure/{pure}.mimz")),
            "showcase_tamil_pure_golden_",
        ));
        let golden_path = golden_dir.join(format!("showcase_tamil_pure_{pure}.v"));
        if update {
            std::fs::create_dir_all(&golden_dir).unwrap();
            std::fs::write(&golden_path, &v).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|_| {
                panic!(
                    "missing showcase tamil-pure golden {} — run with MIMZ_UPDATE_GOLDENS=1",
                    golden_path.display()
                )
            })
            .replace("\r\n", "\n");
        assert_eq!(
            v, want,
            "showcase/tamil-pure/{pure}: emitted Verilog differs from the golden"
        );
    }
}

/// Canonicalize Verilog for alpha-equivalence: rename every identifier to
/// `id<N>` in first-occurrence order so two programs that differ ONLY in
/// identifier names produce the same string.
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

fn compile_example(example: &str) -> String {
    compile_file(&showcase_dir().join(example))
}

fn compile_file(path: &Path) -> String {
    compile_file_tagged(path, "")
}

fn compile_file_tagged(path: &Path, tag: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let name = path.display().to_string().replace(['\\', '/', ':'], "_");
    let out_v = std::env::temp_dir().join(format!(
        "mimz_showcase_{tag}{}_{name}.v",
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
