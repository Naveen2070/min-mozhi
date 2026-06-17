//! `mimz translate --to <flavor>` validated against the four-flavor corpus.
//!
//! The `examples/{english,tanglish,tamil}/` folders are the same programs with
//! only their KEYWORDS swapped (RULES R9), so they are a ready-made oracle:
//!
//! 1. **Round-trip is byte-identical.** Translating a file to another flavor
//!    and back reproduces the original byte-for-byte — translation is lossless
//!    (comments, layout, identifiers all preserved verbatim).
//! 2. **Cross-flavor match at the token level.** Translating english `X` to
//!    flavor `T` lexes to the SAME token stream as the committed `T/X`. We
//!    compare tokens, not bytes, because the corpus files carry a flavor-tagged
//!    note in their header COMMENT ("Tamil flavor — only the keywords change");
//!    comments are deliberately preserved verbatim by the reskin, so they
//!    differ across flavors while the code does not.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use mimz::lexer::lex;
use mimz::lexer::token::{Flavor, TokKind};
use mimz::parser::parse;
use mimz::pretty::{Order, pretty_print};
use mimz::translate::{
    NameMap, TranslateOpts, restore_with_map, romanize_with_map, translate, translate_opts,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Base example file names present in ALL three pure-flavor folders.
fn base_examples() -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root().join("examples/english")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let name = entry.file_name().into_string().unwrap();
            if name.ends_with(".mimz") {
                names.push(name);
            }
        }
    }
    names.sort();
    assert!(
        names.len() >= 10,
        "expected the example corpus, got {names:?}"
    );
    names
}

fn read(flavor: &str, base: &str) -> String {
    fs::read_to_string(root().join("examples").join(flavor).join(base))
        .unwrap_or_else(|e| panic!("read examples/{flavor}/{base}: {e}"))
}

/// The token KINDS of a source (comments/whitespace are not tokens), as the
/// flavor-blind fingerprint of a program. Two files with the same identifiers,
/// numbers, structure, and keywords share this exactly.
fn token_kinds(src: &str) -> Vec<TokKind> {
    lex(src)
        .unwrap_or_else(|d| panic!("source must lex, got {} diags", d.len()))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

const FLAVORS: [(&str, Flavor); 3] = [
    ("english", Flavor::English),
    ("tanglish", Flavor::Tanglish),
    ("tamil", Flavor::Tamil),
];

#[test]
fn round_trip_to_every_flavor_is_byte_identical() {
    for base in base_examples() {
        // Translation normalizes accepted aliases to the canonical spelling
        // (e.g. `include` -> `import`), by design. So anchor the round-trip on
        // the canonical form: once canonical, reskinning is a perfect byte-level
        // bijection — translate to any flavor and back changes nothing.
        let canonical = translate(&read("english", &base), Flavor::English).expect("lexes");
        for (_, target) in FLAVORS {
            let there = translate(&canonical, target).expect("lexes");
            let back = translate(&there, Flavor::English).expect("lexes");
            assert_eq!(
                back, canonical,
                "round-trip english -> {target:?} -> english changed `{base}`"
            );
        }
    }
}

#[test]
fn translating_english_matches_the_committed_flavor_token_for_token() {
    for base in base_examples() {
        let english = read("english", &base);
        for (name, target) in FLAVORS {
            let translated = translate(&english, target).expect("lexes");
            let committed = read(name, &base);
            assert_eq!(
                token_kinds(&translated),
                token_kinds(&committed),
                "translate(english/{base} -> {name}) does not match the committed examples/{name}/{base} at the token level"
            );
        }
    }
}

#[test]
fn every_keyword_token_is_in_the_target_flavor() {
    // After translating to Tamil, no English keyword spelling should survive as
    // a keyword token (proves the reskin actually fired, not just round-tripped).
    let english = read("english", "counter.mimz");
    let tamil = translate(&english, Flavor::Tamil).expect("lexes");
    assert!(tamil.contains("தொகுதி"), "expected Tamil `module`");
    assert!(!tamil.contains("module"), "English `module` should be gone");
}

// ---- `--romanize-names`: Tamil identifiers -> readable Latin (opt-in) ----
//
// The pure-Tamil showcase (examples/tamil-pure/) has Tamil identifiers. With the
// flag, translate rewrites them to the same Latin the emitter uses; without it,
// they ride through verbatim (the lossless default).

const PURE_TAMIL: [&str; 4] = ["kanakki", "cimitti", "oppidi", "thervi"];

/// With `--romanize-names`, Tamil identifiers become Latin in the CODE (comments
/// may keep the original spelling — they are trivia, preserved verbatim).
#[test]
fn romanize_names_converts_tamil_identifiers_to_latin() {
    let src = read("tamil-pure", "kanakki.mimz");
    let out = translate_opts(
        &src,
        Flavor::Tanglish,
        TranslateOpts {
            romanize_names: true,
        },
    )
    .expect("lexes");
    assert!(
        out.contains("thoguthi kannakki("),
        "module decl: Tanglish keyword + romanized name"
    );
    assert!(out.contains("pathivedu mathippu:"), "reg name romanized");
    assert!(out.contains("kannakku = mathippu"), "body romanized");
    // No Tamil-script identifier survives in code (strip line comments first).
    for line in out.lines() {
        let code = line.split("//").next().unwrap_or("");
        assert!(
            !code.chars().any(|c| ('\u{0B80}'..='\u{0BFF}').contains(&c)),
            "Tamil script leaked into code: {code:?}"
        );
    }
}

/// The romanization matches the emitter's, so romanized-then-compiled is
/// byte-identical to compiling the original Tamil source — meaning preserved.
#[test]
fn romanized_translation_compiles_to_the_same_verilog() {
    for name in PURE_TAMIL {
        let src = read("tamil-pure", &format!("{name}.mimz"));
        let romanized = translate_opts(
            &src,
            Flavor::English,
            TranslateOpts {
                romanize_names: true,
            },
        )
        .expect("lexes");
        let from_original = compile_file(
            &root()
                .join("examples/tamil-pure")
                .join(format!("{name}.mimz")),
        );
        let from_romanized = compile_src(&romanized);
        assert_eq!(
            from_romanized, from_original,
            "{name}: romanized translation must compile to the same Verilog"
        );
    }
}

/// The DEFAULT (no flag) stays lossless even for Tamil-named files: the names
/// ride through untouched, so Tamil -> English -> Tamil round-trips to identity.
#[test]
fn pure_tamil_round_trips_losslessly() {
    for name in PURE_TAMIL {
        let canonical =
            translate(&read("tamil-pure", &format!("{name}.mimz")), Flavor::Tamil).expect("lexes");
        let there = translate(&canonical, Flavor::English).expect("lexes");
        let back = translate(&there, Flavor::Tamil).expect("lexes");
        assert_eq!(
            back, canonical,
            "{name}: default translate must round-trip losslessly (Tamil names preserved)"
        );
    }
}

/// With the sidecar name-map, romanization is fully reversible: Tamil →
/// (romanize) Latin → (restore via map) Tamil reproduces the canonical Tamil
/// source byte-for-byte. This is the lossless guarantee the map exists for.
#[test]
fn romanized_round_trips_losslessly_via_the_name_map() {
    for name in PURE_TAMIL {
        let canonical =
            translate(&read("tamil-pure", &format!("{name}.mimz")), Flavor::Tamil).expect("lexes");
        let (romanized, map) = romanize_with_map(&canonical, Flavor::Tanglish).expect("lexes");
        let restored = restore_with_map(&romanized, Flavor::Tamil, &map).expect("lexes");
        assert_eq!(
            restored, canonical,
            "{name}: romanize → restore via the name-map must reproduce the Tamil source"
        );
    }
}

/// Regression (2026-06-15 fuzz audit): a numeric literal directly abutting a
/// Tamil keyword or identifier — separated only by the Latin/Tamil script change
/// — must not glue into an invalid lexeme when reskinned to ASCII. `42தொகுதி`
/// (number + `module` keyword) and `42கணக்கி` (number + Tamil name) once became
/// `42module` / `42kannakki`, which no longer lex. The boundary guard inserts a
/// separating space so the output stays lexable and token-equivalent.
#[test]
fn number_abutting_tamil_keeps_a_separator_when_reskinned() {
    let norm = |s: &str| s.split_whitespace().collect::<String>();
    // number + Tamil keyword -> ASCII keyword (default keyword reskin).
    let kw = "module M {\n  reg x: bits[8] = 42தொகுதி\n}\n";
    if lex(kw).is_ok() {
        let out = translate(kw, Flavor::English).expect("lexes");
        assert!(lex(&out).is_ok(), "reskin output must re-lex: {out}");
        assert!(
            out.contains("42 module") || !out.contains("42module"),
            "got: {out}"
        );
    }
    // number + Tamil identifier -> Latin (romanize) and back via the map.
    let id = "module M {\n  reg x: bits[8] = 42கணக்கி\n}\n";
    if lex(id).is_ok() {
        let (rom, map) = romanize_with_map(id, Flavor::English).expect("lexes");
        assert!(lex(&rom).is_ok(), "romanized output must re-lex: {rom}");
        assert!(!rom.contains("42kannakki"), "must not glue: {rom}");
        let back = restore_with_map(&rom, Flavor::Tamil, &map).expect("lexes");
        // Token-equivalent to the canonical Tamil (a separator space may differ).
        let canonical = translate(id, Flavor::Tamil).expect("lexes");
        assert_eq!(
            norm(&back),
            norm(&canonical),
            "round-trip must be token-equivalent"
        );
    }
}

/// End-to-end through the real binary: `--romanize-names -o` writes a parseable
/// `<out>.names.json`, and a reverse run with `--names-map` restores the exact
/// Tamil source.
#[test]
fn cli_romanize_then_restore_round_trips() {
    static N: AtomicUsize = AtomicUsize::new(0);
    let tag = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let fwd = dir.join(format!("mimz_namemap_fwd_{tag}.mimz"));
    let rev = dir.join(format!("mimz_namemap_rev_{tag}.mimz"));
    let sidecar = dir.join(format!("mimz_namemap_fwd_{tag}.mimz.names.json"));
    let input = root().join("examples/tamil-pure/kanakki.mimz");

    // Forward: romanize to Tanglish, writing the sidecar beside the output.
    let out = mimz()
        .arg("translate")
        .arg(&input)
        .args(["--to", "tanglish", "--romanize-names", "-o"])
        .arg(&fwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "forward translate failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sidecar_json = fs::read_to_string(&sidecar).expect("sidecar name-map was written");
    let map: NameMap = serde_json::from_str(&sidecar_json).expect("sidecar parses as NameMap");
    assert_eq!(map.names.get("kannakki").map(String::as_str), Some("கணக்கி"));

    // Reverse: restore Tamil from the sidecar.
    let out = mimz()
        .arg("translate")
        .arg(&fwd)
        .args(["--to", "tamil", "--names-map"])
        .arg(&sidecar)
        .args(["-o"])
        .arg(&rev)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "reverse translate failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let restored = fs::read_to_string(&rev).unwrap();
    let canonical = translate(&fs::read_to_string(&input).unwrap(), Flavor::Tamil).expect("lexes");
    assert_eq!(restored, canonical, "CLI round-trip lost the Tamil source");
}

// ---- `--order` AST pretty-printer (Phase 1.8) ----
//
// The pretty-printer emits from the AST, so it normalizes layout and drops
// comments (NOT byte-identical to the input — that is the `--to` token path).
// The contracts it MUST keep are semantic: (1) the output compiles to the same
// Verilog as the input, and (2) it is a stable canonical form (idempotent).

const ORDERS: [Order; 2] = [Order::Code, Order::Thamizh];

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Pretty-print an example's English source in the given flavor + order.
fn pretty(base: &str, flavor: Flavor, order: Order) -> String {
    let src = read("english", base);
    let tokens = lex(&src).expect("example lexes");
    let file = parse(tokens).unwrap_or_else(|d| panic!("example parses, got {} diags", d.len()));
    pretty_print(&file, flavor, order)
}

/// Compile a `.mimz` file via the real binary; return its Verilog minus the
/// `// Generated by mimz` banner (mirrors `tests/grammar.rs`).
fn compile_file(path: &std::path::Path) -> String {
    static N: AtomicUsize = AtomicUsize::new(0);
    let out_v = std::env::temp_dir().join(format!(
        "mimz_pretty_{}.v",
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
    let v = fs::read_to_string(&out_v).unwrap();
    let body: Vec<&str> = v.lines().skip(1).collect();
    body.join("\n")
}

/// Write a source string to a temp `.mimz`, compile it, return banner-less
/// Verilog.
fn compile_src(src: &str) -> String {
    static N: AtomicUsize = AtomicUsize::new(0);
    let in_f = std::env::temp_dir().join(format!(
        "mimz_pretty_in_{}.mimz",
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&in_f, src).unwrap();
    compile_file(&in_f)
}

/// Examples that `import` other files — their pretty output cannot be compiled
/// standalone from a temp dir (the import would not resolve). The idempotency
/// test still covers them; the Verilog oracle skips them.
fn imports_others(base: &str) -> bool {
    let src = read("english", base);
    src.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("import") || t.starts_with("include") || t.starts_with("serkka")
    })
}

/// (1) Semantic identity: pretty-printing in ANY flavor × order, then
/// compiling, yields byte-identical Verilog to the original example. Proves
/// reordering and reskinning never change meaning.
#[test]
fn pretty_print_preserves_verilog_across_flavor_and_order() {
    for base in base_examples() {
        if imports_others(&base) {
            continue; // covered by the idempotency test below
        }
        let original = compile_file(&root().join("examples/english").join(&base));
        for (_, flavor) in FLAVORS {
            for order in ORDERS {
                let printed = pretty(&base, flavor, order);
                let got = compile_src(&printed);
                assert_eq!(
                    got, original,
                    "pretty_print({base}, {flavor:?}, {order:?}) changed the Verilog"
                );
            }
        }
    }
}

/// (2) Idempotency: the pretty-printer is a stable canonical form —
/// re-printing its own output (re-lexed and re-parsed) is a fixed point. Runs
/// over ALL examples, including those that import (parsing needs no
/// resolution).
#[test]
fn pretty_print_is_idempotent() {
    for base in base_examples() {
        for (_, flavor) in FLAVORS {
            for order in ORDERS {
                let once = pretty(&base, flavor, order);
                let toks = lex(&once).expect("pretty output lexes");
                let file = parse(toks)
                    .unwrap_or_else(|d| panic!("pretty output re-parses, {} diags", d.len()));
                let twice = pretty_print(&file, flavor, order);
                assert_eq!(
                    once, twice,
                    "pretty_print is not idempotent for {base} ({flavor:?}, {order:?})"
                );
            }
        }
    }
}

/// Thamizh order emits the `syntax thamizh` directive (in the target flavor) so
/// the output re-parses; code order emits no directive.
#[test]
fn thamizh_order_emits_the_directive() {
    let en = pretty("counter.mimz", Flavor::English, Order::Thamizh);
    assert!(en.starts_with("syntax thamizh"), "english thamizh header");
    let ta = pretty("counter.mimz", Flavor::Tamil, Order::Thamizh);
    assert!(ta.starts_with("இலக்கணம் தமிழ்"), "tamil thamizh header");
    let code = pretty("counter.mimz", Flavor::English, Order::Code);
    assert!(
        !code.contains("syntax thamizh"),
        "code order must not emit the directive"
    );
}

/// End-to-end CLI: `mimz translate --order thamizh --to tamil` on a code-order
/// English file produces compilable Tamil thamizh-order source.
#[test]
fn cli_translate_order_thamizh_compiles() {
    let printed = pretty("traffic_light.mimz", Flavor::Tamil, Order::Thamizh);
    assert!(printed.contains("போது")); // `on` in Tamil
    assert!(printed.contains("தேர்ந்தெடு")); // `match` in Tamil
    let got = compile_src(&printed);
    let original = compile_file(&root().join("examples/english/traffic_light.mimz"));
    assert_eq!(got, original, "Tamil thamizh traffic_light lost meaning");
}

/// The test-header flip (B7) round-trips through the pretty-printer: a code-order
/// `test "…" for M(args) { }` printed in thamizh order becomes the flipped
/// `M(args) for "…" test { }`, and that re-parses to the identical `TestDecl`.
#[test]
fn pretty_print_thamizh_flips_the_test_header_and_reparses() {
    use mimz::ast::TopItem;

    let src = "module Counter(WIDTH: int = 4) {\n  clock clk\n  reset rst\n  \
        out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
        on rise(clk) { value <- value +% 1 }\n  count = value\n}\n\
        test \"counts up\" for Counter(WIDTH: 4) {\n  rst = 0\n  tick(clk)\n  \
        expect count == 1\n}\n";
    let file = parse(lex(src).expect("lexes")).expect("parses");

    let printed = pretty_print(&file, Flavor::English, Order::Thamizh);
    // The module under test now LEADS its trailing `for`/`test` clause heads.
    assert!(
        printed.contains("Counter(WIDTH: 4) for \"counts up\" test {"),
        "header was not flipped:\n{printed}"
    );

    // Re-parse the thamizh-order output and compare the recovered test.
    let back = parse(lex(&printed).expect("re-lexes")).expect("re-parses");
    let orig = file.items.iter().find_map(|i| match i {
        TopItem::Test(t) => Some(t),
        _ => None,
    });
    let round = back.items.iter().find_map(|i| match i {
        TopItem::Test(t) => Some(t),
        _ => None,
    });
    let (orig, round) = (orig.expect("orig test"), round.expect("round test"));
    assert_eq!(orig.name, round.name);
    assert_eq!(orig.module.name, round.module.name);
    assert_eq!(orig.args.len(), round.args.len());
    assert_eq!(orig.body.len(), round.body.len());
}
