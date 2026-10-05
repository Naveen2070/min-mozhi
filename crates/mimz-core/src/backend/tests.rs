use super::legal_name;
use super::verilog::emit;
use crate::emit_verilog::translit::romanize;
use crate::ir::failure::{FailureKind, Stage};
use std::collections::{BTreeMap, HashSet};

/// IR line text -> Verilog.
fn from_ir(text: &str) -> String {
    emit(&crate::ir::parse_line::parse(text).expect("parses")).text
}

/// Source -> check -> elaborate (`top`) -> lower -> optimize -> Verilog.
fn from_src(src: &str, top: Option<&str>) -> String {
    emit(&lowered(src, top)).text
}

/// Source -> check -> elaborate (`top`) -> lower -> optimize -> IR.
fn lowered(src: &str, top: Option<&str>) -> crate::ir::Module {
    let file = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
    crate::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    let design = crate::elaborate::elaborate_project_with_mode(
        std::slice::from_ref(&file),
        top,
        &BTreeMap::new(),
        crate::elaborate::SimMode::Lower,
    )
    .expect("elaborates");
    let mut m = crate::ir::lower(&design);
    crate::ir::opt::optimize(&mut m);
    assert!(crate::ir::validate::validate(&m).is_empty());
    m
}

const ADD: &str = "module m\nport in a[0:8]\nport in b[0:8]\nport out o[0:9]\n\n";

#[test]
fn signed_arithmetic_sign_extends_both_operands() {
    // `ir::exec`'s `arith`: signed when either pin is, applied to both.
    let v = from_ir(&format!(
        "{ADD}cell $add :0 a=a[0:8]s b=b[0:8] out=o[0:9]s\n"
    ));
    assert!(v.contains("{{1{a[7]}}, a} + {{1{b[7]}}, b}"), "{v}");
}

#[test]
fn unsigned_arithmetic_zero_extends_both_operands() {
    let v = from_ir(&format!("{ADD}cell $add :0 a=a[0:8] b=b[0:8] out=o[0:9]\n"));
    assert!(v.contains("{1'b0, a} + {1'b0, b}"), "{v}");
}

#[test]
fn a_left_shift_zero_extends_to_its_output_width() {
    let v = from_ir(
        "module m\nport in a[0:4]\nport in b[0:2]\nport out o[0:7]\n\ncell $shl :0 a=a[0:4]s b=b[0:2] out=o[0:7]\n",
    );
    assert!(v.contains("{3'b0, a} << b"), "{v}");
}

#[test]
fn a_signed_comparison_uses_signed_operands() {
    let v = from_ir(
        "module m\nport in a[0:8]\nport in b[0:8]\nport out o[0:1]\n\ncell $lt[signed] :0 a=a[0:8] b=b[0:8] out=o[0:1]\n",
    );
    assert!(v.contains("$signed(a) < $signed(b)"), "{v}");
}

#[test]
fn a_mux_selects_a_when_sel_is_one() {
    let v = from_ir(
        "module m\nport in s[0:1]\nport in a[0:4]\nport in b[0:4]\nport out o[0:4]\n\ncell $mux :0 a=a[0:4] b=b[0:4] out=o[0:4] sel=s[0:1]\n",
    );
    assert!(v.contains("s ? a : b"), "{v}");
}

#[test]
fn a_constant_is_a_sized_literal() {
    let v = from_ir("module m\nport out o[0:9]\n\ncell $const[9'd509] :0 out=o[0:9]\n");
    assert!(v.contains("9'd509"), "{v}");
}

#[test]
fn logic_operations_are_spelled_out() {
    let v = from_ir(
        "module m\nport in a[0:4]\nport in b[0:4]\nport out o[0:1]\n\ncell $logic_and :0 a=a[0:4] b=b[0:4] out=o[0:1]\n",
    );
    assert!(v.contains("(|a) && (|b)"), "{v}");
}

#[test]
fn an_async_reset_register_on_a_falling_edge() {
    let v = from_src(
        "module M {\n  clock clk\n  async reset rst\n  in d: bits[4]\n  out q: bits[4]\n  reg r: bits[4] = 5\n  on fall(clk) {\n    r <- d\n  }\n  q = r\n}\n",
        None,
    );
    assert!(v.contains("always @(negedge clk or posedge rst)"), "{v}");
    assert!(v.contains("if (rst)") && v.contains("4'd5"), "{v}");
}

#[test]
fn a_power_of_two_memory_reads_without_a_guard() {
    let v = from_src(
        "module R {\n  clock clk\n  in we: bit\n  in wa: bits[2]\n  in wd: bits[8]\n  in ra: bits[2]\n  out rd: bits[8]\n  mem m: bits[8][4] = 0\n  on rise(clk) {\n    if we {\n      m[wa] <- wd\n    }\n  }\n  rd = m[ra]\n}\n",
        None,
    );
    assert!(v.contains("always @(posedge clk) if ("), "{v}");
    assert!(!v.contains(" < 4) ?"), "{v}");
}

#[test]
fn a_non_power_of_two_memory_guards_out_of_range_reads() {
    let v = from_src(
        "module R {\n  clock clk\n  in we: bit\n  in wa: bits[3]\n  in wd: bits[8]\n  in ra: bits[3]\n  out rd: bits[8]\n  mem m: bits[8][5] = 7\n  on rise(clk) {\n    if we {\n      m[wa] <- wd\n    }\n  }\n  rd = m[ra]\n}\n",
        None,
    );
    assert!(v.contains(" < 5) ? ") && v.contains(" : 8'd7"), "{v}");
}

#[test]
fn a_rom_has_no_write_block() {
    let v = from_src(
        "module R {\n  in ra: bits[2]\n  out rd: bits[8]\n  mem m: bits[8][4] = 3\n  rd = m[ra]\n}\n",
        None,
    );
    assert!(!v.contains("always"), "{v}");
}

#[test]
fn a_parameterized_extern_is_instantiated_with_its_parameters_and_clock() {
    let v = from_src(
        "extern module Pll(MULT: int = 2) {\n  doc: \"x\"\n  clock clk_in\n  out clk_out: bit\n  out locked: bit\n}\n\nmodule ExternDemo {\n  clock sysclk\n  out fast_clk: bit\n  out pll_ok: bit\n  let u = Pll(MULT: 4) { clk_in: sysclk }\n  fast_clk = u.clk_out\n  pll_ok = u.locked\n}\n",
        Some("ExternDemo"),
    );
    assert!(v.contains("Pll #(.MULT(4)) "), "{v}");
    assert!(v.contains(".clk_in(sysclk)"), "{v}");
}

#[test]
fn an_aliased_extern_uses_its_verilog_name() {
    let v = from_src(
        "extern module Pll = \"PLL_HARD_IP_v2\" {\n  clock clk_in\n  out clk_out: bit\n}\n\nmodule AliasDemo {\n  clock sysclk\n  out fast_clk: bit\n  let u = Pll() { clk_in: sysclk }\n  fast_clk = u.clk_out\n}\n",
        Some("AliasDemo"),
    );
    assert!(v.contains("PLL_HARD_IP_v2 "), "{v}");
}

#[test]
fn tamil_names_emit_legal_ascii_verilog() {
    let v = from_src(
        "module M {\n  in அ: bits[2]\n  out ஆ: bits[2]\n  ஆ = அ\n}\n",
        None,
    );
    assert!(v.is_ascii(), "{v}");
    let (i, o) = (romanize("அ"), romanize("ஆ"));
    assert!(v.contains(&format!("input wire [1:0] {i}")), "{v}");
    assert!(v.contains(&format!("output wire [1:0] {o}")), "{v}");
}

const TAMIL_EXTERN: &str = "extern module பிஎல்(பெருக்கு: int = 2) {\n  clock உள்கடிகை\n  out வெளி: bit\n}\n\nmodule M {\n  clock clk\n  out y: bit\n  let u = பிஎல்(பெருக்கு: 4) { உள்கடிகை: clk }\n  y = u.வெளி\n}\n";

#[test]
fn an_extern_with_tamil_names_matches_the_ast_emitter() {
    let v = from_src(TAMIL_EXTERN, Some("M"));
    assert!(v.is_ascii(), "{v}");
    let (m, p, c, o) = (
        romanize("பிஎல்"),
        romanize("பெருக்கு"),
        romanize("உள்கடிகை"),
        romanize("வெளி"),
    );
    assert!(v.contains(&format!("{m} #(.{p}(4)) ")), "{v}");
    assert!(v.contains(&format!(".{c}(clk)")), "{v}");
    assert!(v.contains(&format!(".{o}(")), "{v}");
    // The AST emitter (`mimz compile`) spells the same instance the same way.
    let file = crate::parser::parse(crate::lexer::lex(TAMIL_EXTERN).unwrap()).unwrap();
    let mut asts = vec![file];
    crate::emit_verilog::transliterate(&mut asts);
    let project = crate::emit_verilog::Project::from_files(&asts).unwrap();
    let ast_v = crate::emit_verilog::emit(&project, &asts).unwrap();
    assert!(ast_v.contains(&format!("{m} #(.{p}(4)) ")), "{ast_v}");
    assert!(ast_v.contains(&format!(".{c}(clk)")), "{ast_v}");
    assert!(ast_v.contains(&format!(".{o}(")), "{ast_v}");
}

/// The AST emitter (`mimz compile`) over the same source.
fn ast_verilog(src: &str) -> String {
    let file = crate::parser::parse(crate::lexer::lex(src).unwrap()).unwrap();
    let mut asts = vec![file];
    crate::emit_verilog::transliterate(&mut asts);
    let project = crate::emit_verilog::Project::from_files(&asts).unwrap();
    crate::emit_verilog::emit(&project, &asts).unwrap()
}

/// The connection name written for `signal`, as in `.name(signal)`.
fn conn_for(text: &str, signal: &str) -> String {
    let tail = format!("{signal})");
    text.split('.')
        .find_map(|p| {
            let (name, rest) = p.split_once('(')?;
            let ident = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
            (ident && rest.starts_with(&tail)).then(|| name.to_string())
        })
        .unwrap_or_else(|| panic!("no `.x({signal})` in:\n{text}"))
}

/// The module emitted twice: with only its own names (`emit`) and with the
/// project's name map, as `mimz build` does (`emit_with_names`).
fn plain_and_project(src: &str, top: &str) -> (String, String) {
    let file = crate::parser::parse(crate::lexer::lex(src).unwrap()).unwrap();
    let names = crate::emit_verilog::project_names(std::slice::from_ref(&file));
    let m = lowered(src, Some(top));
    (
        emit(&m).text,
        super::verilog::emit_with_names(&m, &names).text,
    )
}

#[test]
fn tamil_extern_ports_that_romanize_alike_stay_distinct_like_the_ast_emitter() {
    // ந and ன both romanize to `n`: `நீ` and `னீ` are both `nii`. `னீ` is
    // declared first, so a pin allocation in sorted-name order would swap them.
    let src = "extern module E {\n  in னீ: bit\n  in நீ: bit\n  out y: bit\n}\n\nmodule M {\n  in a: bit\n  in b: bit\n  out o: bit\n  let u = E() { நீ: a, னீ: b }\n  o = u.y\n}\n";
    let (plain, project) = plain_and_project(src, "M");
    let ast = ast_verilog(src);
    for text in [&plain, &project] {
        assert_eq!(conn_for(text, "b"), conn_for(&ast, "b"), "{text}");
        assert_eq!(conn_for(text, "a"), conn_for(&ast, "a"), "{text}");
        assert_ne!(conn_for(text, "a"), conn_for(text, "b"), "{text}");
    }
}

#[test]
fn tamil_extern_parameters_that_romanize_alike_stay_distinct() {
    let src = "extern module E(நீ: int = 1, னீ: int = 2) {\n  in a: bit\n  out y: bit\n}\n\nmodule M {\n  in a: bit\n  out o: bit\n  let u = E(நீ: 3, னீ: 4) { a: a }\n  o = u.y\n}\n";
    let (plain, project) = plain_and_project(src, "M");
    let ast = ast_verilog(src);
    for text in [&plain, &project] {
        assert_eq!(conn_for(text, "3"), conn_for(&ast, "3"), "{text}");
        assert_eq!(conn_for(text, "4"), conn_for(&ast, "4"), "{text}");
        assert_ne!(conn_for(text, "3"), conn_for(text, "4"), "{text}");
    }
}

/// `mimz build` uses the project's names: the parent's ASCII `a` already owns
/// that spelling, so the AST emitter writes the extern's `அ` pin as `a_2`.
#[test]
fn project_names_make_the_backend_spell_externs_like_the_ast_emitter() {
    let src = "extern module E {\n  in அ: bit\n  out y: bit\n}\n\nmodule M {\n  in a: bit\n  out o: bit\n  let u = E() { அ: a }\n  o = u.y\n}\n";
    let (plain, project) = plain_and_project(src, "M");
    let ast = ast_verilog(src);
    assert_eq!(conn_for(&ast, "a"), "a_2", "{ast}");
    assert_eq!(conn_for(&project, "a"), conn_for(&ast, "a"), "{project}");
    // Without a project, `emit` keeps the per-extern fallback.
    assert_eq!(conn_for(&plain, "a"), "a", "{plain}");
}

/// A Tamil clock declared before a port that romanizes alike: the AST pass
/// allocates in declaration order, which the IR alone cannot recover.
#[test]
fn project_names_cover_a_clock_declared_before_a_colliding_port() {
    let src = "extern module E {\n  clock நீ\n  in னீ: bit\n  out y: bit\n}\n\nmodule M {\n  clock clk\n  in b: bit\n  out o: bit\n  let u = E() { நீ: clk, னீ: b }\n  o = u.y\n}\n";
    let (_, project) = plain_and_project(src, "M");
    let ast = ast_verilog(src);
    assert_eq!(
        conn_for(&project, "clk"),
        conn_for(&ast, "clk"),
        "{project}"
    );
    assert_eq!(conn_for(&project, "b"), conn_for(&ast, "b"), "{project}");
}

#[test]
fn an_aliased_name_is_kept_verbatim_and_an_unaliased_one_is_romanized() {
    let mut m = lowered(TAMIL_EXTERN, Some("M"));
    // Un-aliased: `verilog_name == module_name` (Tamil), emitted romanized.
    assert!(emit(&m).text.contains(&format!("{} ", romanize("பிஎல்"))));
    // Aliased with the same spelling: kept verbatim, not romanized.
    for c in &mut m.cells {
        if let crate::ir::CellKind::BlackBox { aliased, .. } = &mut c.kind {
            *aliased = true;
        }
    }
    assert!(emit(&m).text.contains("பிஎல் #("));
}

#[test]
fn a_zero_width_port_is_a_limitation() {
    let m = crate::ir::parse_line::parse("module m\nport out o[0:0]\n\n").expect("parses");
    let f = crate::ir::failure::catch(Stage::Emit, false, || emit(&m)).map(|_| ());
    let f = f.expect_err("zero-width port");
    assert_eq!(f.kind, FailureKind::Limitation, "{}", f.message);
}

#[test]
fn sign_extending_an_empty_operand_is_a_limitation() {
    let ir =
        "module m\nport in b[0:8]\nport out o[0:9]\n\ncell $add :0 a={}s b=b[0:8] out=o[0:9]s\n";
    let m = crate::ir::parse_line::parse(ir).expect("parses");
    let f = crate::ir::failure::catch(Stage::Emit, false, || emit(&m)).map(|_| ());
    let f = f.expect_err("empty signed operand");
    assert_eq!(f.kind, FailureKind::Limitation, "{}", f.message);
}

#[test]
fn a_mismatched_output_width_on_a_bitwise_cell_is_a_limitation() {
    for kind in ["$not", "$and", "$or", "$xor"] {
        let b = if kind == "$not" { "" } else { " b=b[0:4]" };
        let ir = format!(
            "module m\nport in a[0:4]\nport in b[0:4]\nport out o[0:8]\n\ncell {kind} :0 a=a[0:4]{b} out=o[0:8]\n"
        );
        let m = crate::ir::parse_line::parse(&ir).expect("parses");
        let f = crate::ir::failure::catch(Stage::Emit, false, || emit(&m)).map(|_| ());
        let f = f.expect_err(kind);
        assert_eq!(f.kind, FailureKind::Limitation, "{kind}: {}", f.message);
    }
}

#[test]
fn legal_name_escapes_keywords_digits_and_duplicates() {
    let mut used = HashSet::new();
    assert_eq!(legal_name("begin", &mut used), "begin_");
    assert_eq!(legal_name("table", &mut used), "table_");
    assert_eq!(legal_name("event", &mut used), "event_");
    assert_eq!(legal_name("module", &mut used), "module_");
    assert_eq!(legal_name("3x", &mut used), "_3x");
    assert_eq!(legal_name("a.b", &mut used), "a_b");
    assert_eq!(legal_name("a", &mut used), "a");
    assert_eq!(legal_name("a", &mut used), "a_1");
}
