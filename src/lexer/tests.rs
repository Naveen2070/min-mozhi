//! Lexer unit tests: trilingual keywords, Tamil identifiers, numbers,
//! operators, newline policy, and teaching rejections.

use super::*;
use token::Kw;

fn kinds(src: &str) -> Vec<TokKind> {
    lex(src).unwrap().into_iter().map(|t| t.kind).collect()
}

#[test]
fn lexes_mixed_flavors() {
    let toks = lex("module nilai வெளி").unwrap();
    assert!(toks[0].is_kw(Kw::Module));
    assert!(toks[1].is_kw(Kw::Reg));
    assert!(toks[2].is_kw(Kw::Out));
    assert_eq!(toks[0].flavor, Some(token::Flavor::English));
    assert_eq!(toks[1].flavor, Some(token::Flavor::Tanglish));
    assert_eq!(toks[2].flavor, Some(token::Flavor::Tamil));
}

#[test]
fn tamil_identifiers_work() {
    let toks = lex("எண்ணி").unwrap();
    assert!(matches!(&toks[0].kind, TokKind::Ident(s) if s == "எண்ணி"));
}

#[test]
fn numbers() {
    assert!(matches!(
        kinds("0b1010_0001")[0],
        TokKind::Int { value: 0xA1, .. }
    ));
    assert!(matches!(kinds("0xA1")[0], TokKind::Int { value: 0xA1, .. }));
    assert!(matches!(kinds("161")[0], TokKind::Int { value: 161, .. }));
}

#[test]
fn wrapping_operators() {
    assert_eq!(kinds("a +% b")[1], TokKind::PlusPct);
    assert_eq!(kinds("a -% b")[1], TokKind::MinusPct);
}

#[test]
fn larrow_vs_comparison() {
    assert_eq!(kinds("a <- b")[1], TokKind::LArrow);
    assert_eq!(kinds("a <= b")[1], TokKind::Le);
    assert_eq!(kinds("a << b")[1], TokKind::Shl);
}

#[test]
fn newline_continuation_after_operator() {
    // `a +\n b` — newline suppressed; `a\n b` — newline kept.
    let k = kinds("x = a +\n b");
    assert!(!k.contains(&TokKind::Newline));
    let k = kinds("x = a\nb");
    assert!(k.contains(&TokKind::Newline));
}

#[test]
fn division_is_rejected_with_teaching_error() {
    let errs = lex("a / b").unwrap_err();
    assert_eq!(errs[0].code, Some("E1006"));
    assert!(errs[0].msg.contains("division"));
    assert!(errs[0].help.as_ref().unwrap().contains("shifts"));
}

#[test]
fn fall_is_reserved_error() {
    let errs = lex("on fall(clk)").unwrap_err();
    assert_eq!(errs[0].code, Some("E1005"));
    assert!(errs[0].msg.contains("reserved"));
}
