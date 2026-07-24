//! Per-phase compiler micro-benchmarks (`cargo bench`).
//!
//! This is a SEPARATE harness from `mimz-bench` (the end-to-end corpus tool):
//! it isolates the lexer, parser, checker, and emitter so a regression in one
//! phase shows up on its own, with `criterion`'s statistical warmup and
//! outlier detection. See docs/Ideas/benchmark_plan.md Phase 2.
//!
//! It runs over one self-contained example (no imports), so each phase can be
//! driven directly through the public library API without project loading.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mimz::lexer::lex;
use mimz::parser::parse;
use mimz::{ast, checker, emit_verilog};

/// A representative single-file example (FSM: exercises match/enum, several
/// ports, and a clocked block). Self-contained — no `import`.
const SOURCE: &str = include_str!("../examples/english/traffic_light.mimz");

fn phases(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile");

    // Lexer: raw source text -> tokens.
    group.bench_function("lexer", |b| {
        b.iter(|| lex(black_box(SOURCE)).expect("example lexes"));
    });

    // Parser: pre-lexed tokens -> AST. `parse` consumes the token vec, so
    // clone a fresh copy each iteration to time parsing only, not lexing.
    let tokens = lex(SOURCE).expect("example lexes");
    group.bench_function("parser", |b| {
        b.iter(|| parse(black_box(tokens.clone())).expect("example parses"));
    });

    // Checker: pre-parsed AST -> validated (six passes).
    let file = parse(tokens.clone()).expect("example parses");
    let asts = vec![file];
    group.bench_function("checker", |b| {
        b.iter(|| checker::check(black_box(&asts)).expect("example checks clean"));
    });

    // Emitter: AST -> Verilog text. transliterate mutates in place, so clone
    // the AST each iteration to time emission from a clean state.
    group.bench_function("emit", |b| {
        b.iter(|| {
            let mut asts: Vec<ast::File> = asts.clone();
            emit_verilog::transliterate(&mut asts);
            let proj = emit_verilog::Project::from_files(&asts).expect("project builds");
            emit_verilog::emit(black_box(&proj), black_box(&asts)).expect("example emits")
        });
    });

    // Simulator: narrow (u128 fast-path, confirms the Small/Wide dispatch
    // added zero overhead to the existing case) and wide (>128-bit
    // slow-path, confirms it's reasonably fast) — BUG-13 layer 1.
    use mimz::sim::elaborate::elaborate;
    use mimz::sim::run::{SimOpts, run};
    use std::collections::BTreeMap;

    const NARROW_SRC: &str = "module Counter(WIDTH: int = 32) {\n  clock clk\n  reset rst\n  out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";
    const WIDE_SRC: &str = "module Counter(WIDTH: int = 512) {\n  clock clk\n  reset rst\n  out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";

    let narrow_file =
        parse(lex(NARROW_SRC).expect("narrow example lexes")).expect("narrow example parses");
    let narrow_design =
        elaborate(&narrow_file, None, &BTreeMap::new()).expect("narrow example elaborates");
    group.bench_function("sim_narrow_128cycles", |b| {
        b.iter(|| {
            run(
                black_box(narrow_design.clone()),
                &SimOpts {
                    clock: None,
                    inputs: BTreeMap::new(),
                    cycles: 128,
                    reset_cycles: 1,
                },
            )
            .expect("narrow example runs")
        });
    });

    let wide_file = parse(lex(WIDE_SRC).expect("wide example lexes")).expect("wide example parses");
    let wide_design =
        elaborate(&wide_file, None, &BTreeMap::new()).expect("wide example elaborates");
    group.bench_function("sim_wide_128cycles", |b| {
        b.iter(|| {
            run(
                black_box(wide_design.clone()),
                &SimOpts {
                    clock: None,
                    inputs: BTreeMap::new(),
                    cycles: 128,
                    reset_cycles: 1,
                },
            )
            .expect("wide example runs")
        });
    });

    group.finish();
}

criterion_group!(benches, phases);
criterion_main!(benches);
