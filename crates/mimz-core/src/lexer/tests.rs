//! Lexer unit tests: trilingual keywords, Tamil identifiers, numbers,
//! operators, newline policy, and teaching rejections.

use super::*;
use token::Kw;

fn kinds(src: &str) -> Vec<TokKind> {
    lex(src).unwrap().into_iter().map(|t| t.kind).collect()
}

#[test]
fn lexes_mixed_flavors() {
    let toks = lex("module pathivedu வெளியீடு").unwrap();
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
fn dont_care_binary_literal_lexes_to_masked_int() {
    // `0b1??` — the high bit cares (value 0b100), the low two are don't-care.
    assert!(matches!(
        kinds("0b1??")[0],
        TokKind::MaskedInt {
            value: 0b100,
            mask: 0b100,
            width: 3,
            ..
        }
    ));
    // A plain binary literal is still an `Int` — no regression.
    assert!(matches!(
        kinds("0b101")[0],
        TokKind::Int { value: 0b101, .. }
    ));
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
fn a_reserved_word_is_an_error() {
    // A reserved-but-not-yet-active word (e.g. `inout`) is a clean E1005, not a
    // silent identifier. (`fall` was promoted in A3, `mem` in A4, `sync` in A6, so
    // other reserved words carry this check.)
    let errs = lex("inout").unwrap_err();
    assert_eq!(errs[0].code, Some("E1005"));
    assert!(errs[0].msg.contains("reserved"));
}

#[test]
fn mem_is_an_active_keyword() {
    // A4 promoted `mem` from reserved to a keyword, with provisional Tanglish/Tamil
    // spellings (pending native review). All three flavors lex to `Kw::Mem`.
    for src in ["mem", "ninaivagam", "நினைவகம்"] {
        let toks = lex(src).unwrap();
        assert!(toks[0].is_kw(Kw::Mem), "`{src}` should lex to Kw::Mem");
    }
}

#[test]
fn async_is_an_active_keyword() {
    // A5 promoted `async` from reserved to a keyword, with provisional Tanglish/Tamil
    // spellings (pending native review). All three flavors lex to `Kw::Async`.
    for src in ["async", "otthisaivatra", "ஒத்திசைவற்ற"] {
        let toks = lex(src).unwrap();
        assert!(toks[0].is_kw(Kw::Async), "`{src}` should lex to Kw::Async");
    }
}

#[test]
fn fn_keyword_lexes_in_all_flavors() {
    for src in ["fn f", "function f", "saarbu f", "சார்பு f"] {
        let toks = lex(src).unwrap();
        assert!(matches!(toks[0].kind, TokKind::Kw(Kw::Fn)), "{src}");
    }
}

#[test]
fn rarrow_token_lexes() {
    let toks = lex("-> ").unwrap();
    assert!(matches!(toks[0].kind, TokKind::RArrow));
}

#[test]
fn lexes_question_and_question_question() {
    let toks = lex("? ??").unwrap();
    assert_eq!(toks[0].kind, TokKind::Question);
    assert_eq!(toks[1].kind, TokKind::QQ);
}
