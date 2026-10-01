//! `ir::failure`: panics inside lowering/optimizing become classified
//! `Failure`s. See `docs/superpowers/specs/2026-10-01-ir-pipeline-cli-design.local.md`.

use crate::ir::failure::{FailureKind, Stage, catch, enter_span, span_depth};
use crate::span::Span;
use std::collections::BTreeMap;

/// Real source that hits an `unimplemented!` in `ir::lower`: a
/// bundle-returning `fn` call read through a field.
const LIMITATION: &str = "bundle Handshake(W: int = 8) {\n  valid: bit\n  data:  bits[W]\n}\n\nfn make(v: bit) -> Handshake(W: 8) {\n  { valid: v, data: 0 }\n}\n\nmodule Top {\n  in  v: bit\n  out o: bit\n  wire h: Handshake(W: 8) = make(v)\n  o = h.valid\n}\n";

/// Real source that hits a broken invariant in `ir::lower`: the constant-`if`
/// fold's known limit (gaps.md, "limits left by the 2026-09-29 lowering
/// fixes") leaves the phantom `s[-1]` read under `!`.
const INTERNAL: &str = "module Pass {\n  in  x: bit\n  out y: bit\n  y = x\n}\n\nmodule Top {\n  in  a: bits[4]\n  out o: bits[4]\n  repeat i: 0..4 {\n    let s[i] = Pass() { x: a[i] }\n    o[i] = !(if i == 0 { 0 } else { s[i - 1].y })\n  }\n}\n";

fn lower_top(src: &str) -> Result<crate::ir::Module, crate::ir::failure::Failure> {
    let file = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
    crate::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    let design = crate::elaborate::elaborate_project(
        std::slice::from_ref(&file),
        Some("Top"),
        &BTreeMap::new(),
    )
    .expect("elaborates");
    catch(Stage::Lower, false, || crate::ir::lower(&design))
}

/// The whole line of `src` holding byte `at`.
fn line_of(src: &str, at: usize) -> &str {
    let start = src[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = src[at..].find('\n').map_or(src.len(), |i| at + i);
    &src[start..end]
}

#[test]
fn ok_passes_the_value_through() {
    assert_eq!(catch(Stage::Lower, false, || 7).unwrap(), 7);
}

#[test]
fn unimplemented_is_a_limitation() {
    let f = catch(Stage::Lower, false, || -> u8 {
        unimplemented!("field access")
    })
    .unwrap_err();
    assert_eq!(f.kind, FailureKind::Limitation);
    assert_eq!(f.stage, Stage::Lower);
    assert!(
        f.message.starts_with("not implemented: field access"),
        "{}",
        f.message
    );
    assert!(
        f.location.as_deref().unwrap().contains("failure.rs"),
        "{:?}",
        f.location
    );
}

#[test]
fn the_loop_budget_panic_is_a_limitation() {
    let f = catch(Stage::Lower, false, || -> u8 {
        panic!("`loop` would unroll 9999 times, over the limit of 4096 (S0227)")
    })
    .unwrap_err();
    assert_eq!(f.kind, FailureKind::Limitation);
}

#[test]
fn a_broken_invariant_is_internal() {
    let expect = catch(Stage::Lower, false, || -> u8 {
        std::hint::black_box(None::<u8>).expect("checker guarantees x")
    })
    .unwrap_err();
    assert_eq!(expect.kind, FailureKind::Internal);
    let unreachable =
        catch(Stage::Lower, false, || -> u8 { unreachable!("narrowed") }).unwrap_err();
    assert_eq!(unreachable.kind, FailureKind::Internal);
}

#[test]
fn every_optimize_failure_is_internal() {
    let f = catch(Stage::Optimize, false, || -> u8 { unimplemented!("x") }).unwrap_err();
    assert_eq!(f.kind, FailureKind::Internal);
}

#[test]
fn the_innermost_span_wins_and_the_stack_is_empty_after() {
    let f = catch(Stage::Lower, false, || -> u8 {
        let _outer = enter_span(Span::new(1, 9));
        let _inner = enter_span(Span::new(3, 5));
        panic!("boom")
    })
    .unwrap_err();
    assert_eq!(f.span, Some(Span::new(3, 5)));
    assert_eq!(span_depth(), 0, "guards popped while unwinding");
}

#[test]
fn a_synthesized_default_span_is_skipped() {
    let f = catch(Stage::Lower, false, || -> u8 {
        let _real = enter_span(Span::new(2, 4));
        let _synth = enter_span(Span::default());
        panic!("boom")
    })
    .unwrap_err();
    assert_eq!(f.span, Some(Span::new(2, 4)));
}

#[test]
fn a_nested_catch_leaves_the_outer_one_armed() {
    let outer = catch(Stage::Lower, false, || -> u8 {
        let _outer = enter_span(Span::new(10, 20));
        let inner = catch(Stage::Lower, false, || -> u8 {
            let _inner = enter_span(Span::new(12, 14));
            panic!("inner")
        });
        assert_eq!(inner.unwrap_err().span, Some(Span::new(12, 14)));
        assert_eq!(span_depth(), 1, "inner catch kept the outer span");
        panic!("outer")
    })
    .unwrap_err();
    assert_eq!(outer.message, "outer");
    assert_eq!(outer.span, Some(Span::new(10, 20)));
    assert!(outer.location.is_some(), "outer still captured by the hook");
}

#[test]
fn a_panic_on_another_thread_is_not_captured() {
    let f = catch(Stage::Lower, false, || -> u8 {
        let other = std::thread::spawn(|| -> u8 {
            let _s = enter_span(Span::new(40, 50));
            panic!("theirs")
        });
        assert!(other.join().is_err());
        let _s = enter_span(Span::new(1, 2));
        panic!("mine")
    })
    .unwrap_err();
    assert_eq!(f.message, "mine");
    assert_eq!(f.span, Some(Span::new(1, 2)));
}

#[test]
fn a_backtrace_only_when_asked() {
    let without = catch(Stage::Lower, false, || -> u8 { panic!("x") }).unwrap_err();
    assert!(without.backtrace.is_none());
    let with = catch(Stage::Lower, true, || -> u8 { panic!("x") }).unwrap_err();
    assert!(!with.backtrace.unwrap().is_empty());
}

#[test]
fn resume_re_raises_the_original_payload() {
    let f = catch(Stage::Lower, false, || -> u8 { panic!("original") }).unwrap_err();
    let again = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.resume())).unwrap_err();
    assert_eq!(again.downcast_ref::<&str>(), Some(&"original"));
}

#[test]
fn real_source_reaching_an_ir_limitation() {
    let f = lower_top(LIMITATION).unwrap_err();
    assert_eq!(f.kind, FailureKind::Limitation, "{}", f.message);
    let span = f
        .span
        .expect("a lowering panic sits inside a spanned expression");
    assert!(
        line_of(LIMITATION, span.start).contains("wire h"),
        "{span:?}"
    );
}

#[test]
fn real_source_reaching_a_broken_invariant() {
    let f = lower_top(INTERNAL).unwrap_err();
    assert_eq!(f.kind, FailureKind::Internal, "{}", f.message);
    assert!(f.message.contains("s__-1_y"), "{}", f.message);
    let span = f
        .span
        .expect("a lowering panic sits inside a spanned expression");
    assert!(line_of(INTERNAL, span.start).contains("o[i] ="), "{span:?}");
    assert!(
        f.location.as_deref().unwrap().contains("lower.rs"),
        "{:?}",
        f.location
    );
}
