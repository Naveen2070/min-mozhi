use super::*;

/// Full lex → parse → check → emit pipeline for an import-free source — the
/// module-level `emit_src` skips `checker::check`, but the enum-width facts
/// this test's `State` type needs (`inferred_total_width`) are only filled
/// in by the checker.
fn compile(src: &str) -> String {
    let asts = [parse(src)];
    crate::checker::check(&asts).expect("checks");
    let project = Project::from_files(&asts).unwrap();
    emit(&project, &asts).expect("emit should succeed")
}

/// Fuzzer regression (`lex_parse_compile`, crash-459469cb…): two sibling
/// modules in the SAME file each declaring their own module-local
/// `enum State { .. }` — an ordinary, legal FSM pattern (module-local enums
/// are scoped to their own module, so reusing the name across sibling
/// modules is not a conflict). `Project::resolve_enum`'s flat per-file
/// table used to see 2+ candidates for the bare name `State` and report a
/// false ambiguity, which `build_decls`'s `resolved_kind` then misread as
/// "not an enum, must be a nested bundle" and panicked. Regression for the
/// fix in `Emitter::resolve_enum` (`module/ports.rs`), which checks the
/// CURRENT module's own local enums first.
#[test]
fn sibling_modules_with_same_named_local_enum_do_not_panic() {
    let v = compile(
        "module A {\n  clock clk\n  reset rst\n  in go: bit\n  out done: bit\n  \
         enum State { Idle, Run }\n  reg state: State = State.Idle\n  \
         on rise(clk) {\n    if state == State.Idle {\n      if go { state <- State.Run }\n    } \
         else {\n      state <- State.Idle\n    }\n  }\n  \
         done = match state {\n    State.Idle => 1\n    State.Run => 0\n  }\n}\n\
         module B {\n  clock clk\n  reset rst\n  in go: bit\n  out done: bit\n  \
         enum State { Idle, Run }\n  reg state: State = State.Idle\n  \
         on rise(clk) {\n    if state == State.Idle {\n      if go { state <- State.Run }\n    } \
         else {\n      state <- State.Idle\n    }\n  }\n  \
         done = match state {\n    State.Idle => 1\n    State.Run => 0\n  }\n}\n",
    );
    assert!(v.contains("module A"));
    assert!(v.contains("module B"));
}
