use super::*;

// ----- Regression tests for behaviors that were previously verified only -----
// ----- by hand-trace during earlier development, pinned down here as    -----
// ----- real tests (see each test's own comment for the gap it closes).  -----

#[test]
fn overlapping_import_prefixes_disambiguate_correctly() {
    // Task 9's deferred gap: both `import a` and `import a.b` present in the
    // same file, with one reference down each path (`a.Fifo` and
    // `a.b.Fifo`). Previously only verified by a reviewer's hand-trace of
    // `resolve_via_imports`'s exact-length guard (`imp.path.len() ==
    // self.path.len()`) — this pins it down as a real test.
    //
    // Each file's `Fifo` has a differently-named input port (`x` vs `w`), so
    // a prefix-only match (e.g. `a.b.Fifo` incorrectly matching `import a`)
    // would connect the wrong port name and trip E0107 — this isn't a
    // tautological clean-pass, it actually exercises which target each
    // qualifier resolved to.
    let a = parse("module Fifo {\n  in x: bit\n  out y: bit\n  y = x\n}\n");
    let b = parse("module Fifo {\n  in w: bit\n  out z: bit\n  z = w\n}\n");
    let user = parse(
        "import a\nimport a.b\n\nmodule M {\n  let u1 = a.Fifo() { x: 0 }\n  \
         let u2 = a.b.Fifo() { w: 0 }\n}\n",
    );
    assert_eq!(user.imports.len(), 2, "sanity: both imports parsed");
    // files: [user=0, a=1, b=2].
    user.imports[0].resolved_file.set(Some(1)); // `import a` -> file 1
    user.imports[1].resolved_file.set(Some(2)); // `import a.b` -> file 2
    check(&[user, a, b]).expect(
        "a.Fifo() must resolve to file 1's Fifo (input `x`) and a.b.Fifo() \
         must resolve to file 2's Fifo (input `w`) — a prefix-only match \
         would misconnect one of them and trip E0107",
    );
}

#[test]
fn no_default_param_module_only_discovered_via_instantiation_still_gets_width_checked() {
    // Task 12's deferred gap: both existing "two same-named modules" width
    // tests use param-less `Fifo`s, so `check_widths`'s unconditional seed
    // loop discovers both files' configs on its own — `check_inst_widths`'s
    // `found.push((child.file, ...))` never gets exercised as the SOLE
    // discovery path. A module with a parameter that has no default is
    // skipped by the seed loop entirely (`default_binding` requires every
    // param to have a default) — the ONLY way its body ever gets checked is
    // via `found.push` threading the correct `child.file` back into the
    // worklist from an instantiation site.
    //
    // Two different files each declare `Fifo(WIDTH: int)` (no default): one
    // has a real internal width bug (independent of WIDTH's value), the
    // other is clean. `user` instantiates both via a qualified reference. If
    // `found.push` dropped or mis-threaded `child.file` (the original Task
    // 12 bug), the pushed `Config` would fail the `(file, name)` lookup in
    // `check_widths`'s worklist and the buggy file's body would silently
    // never be checked — this test would then wrongly pass as clean.
    let buggy = parse(
        "module Fifo(WIDTH: int) {\n  out y: bits[WIDTH]\n  wire w: bits[WIDTH + 1] = 0\n  \
         y = w\n}\n",
    );
    let clean = parse("module Fifo(WIDTH: int) {\n  out y: bits[WIDTH]\n  y = 0\n}\n");
    let user = parse(
        "import buggy\nimport clean\n\nmodule M {\n  \
         let a = buggy.Fifo(WIDTH: 4) { }\n  let b = clean.Fifo(WIDTH: 4) { }\n}\n",
    );
    assert_eq!(user.imports.len(), 2, "sanity: both imports parsed");
    // files: [user=0, buggy=1, clean=2].
    user.imports[0].resolved_file.set(Some(1));
    user.imports[1].resolved_file.set(Some(2));
    let diags = errs_multi(&[user, buggy, clean]);
    assert!(
        diags.iter().any(|d| d.code == Some("E0401")),
        "buggy.Fifo's internal width mismatch must be caught even though it \
         has no default parameter and is only reachable via instantiation \
         (not the seed loop): got {diags:?}"
    );
}

#[test]
fn two_same_named_modules_each_get_their_own_clock_check_reversed_order() {
    // Task 12's deferred gap: the sibling test above
    // (`two_same_named_modules_each_get_their_own_clock_check`) always
    // registers the CLEAN module first (file 1) and the buggy one second
    // (file 2) — matching the old canonical filter's `.first()` pick, so it
    // only proves the second-registered file gets its own check. It can't
    // tell that apart from a hypothetically different-but-still-wrong
    // implementation that always checks only the LAST-registered file
    // instead of every file independently. Reversing which file holds the
    // bug (buggy registers FIRST here, clean SECOND) closes that gap: an
    // "always check the last file" implementation would report no E0701
    // here (since the last-registered file, clean, has no bug) and this
    // test would catch that.
    let a = parse(
        "module Fifo {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out yb: bit\n  \
         reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  \
         on rise(ckb) {\n    rb <- ra\n  }\n  yb = rb\n}\n",
    ); // real E0701: ckb-block reads cka-owned `ra` — registers FIRST (file 1)
    let b = parse(
        "module Fifo {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  \
         out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  \
         on rise(ckb) {\n    rb <- a\n  }\n  ya = ra\n  yb = rb\n}\n",
    ); // clean: independent domains — registers SECOND (file 2)
    let mut user = parse("module M {\n  let x = Fifo() { a: 0 }\n  let z = Fifo() { a: 0 }\n}\n");
    if let crate::ast::TopItem::Module(m) = &mut user.items[0] {
        let mut insts = m.items.iter_mut().filter_map(|it| {
            if let crate::ast::ModuleItem::Inst(i) = it {
                Some(i)
            } else {
                None
            }
        });
        let x = insts.next().unwrap();
        x.module.path.push(crate::ast::Ident {
            name: "a".into(),
            span: x.module.span,
        });
        x.module.resolved_file.set(Some(1)); // -> file A (real E0701)
        let z = insts.next().unwrap();
        z.module.path.push(crate::ast::Ident {
            name: "b".into(),
            span: z.module.span,
        });
        z.module.resolved_file.set(Some(2)); // -> file B (clean)
    }
    let diags = errs_multi(&[user, a, b]);
    assert!(
        diags.iter().any(|d| d.code == Some("E0701")),
        "file A's real cross-domain read must still be caught even though \
         it registers first and file B declares a same-named, clean `Fifo` \
         that registers second"
    );
}

#[test]
fn recursive_call_inside_return_is_e0805() {
    // The self-call is nested inside `if { return f(a) }` — only reachable
    // by walking `FuncDecl.stmts` (Task 1's statement-based fn body), not
    // the old flat `locals`/`body` fields.
    let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return f(a) }\n  a\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    first_err(src, "E0805");
}
