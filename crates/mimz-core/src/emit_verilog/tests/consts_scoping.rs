use super::*;

#[test]
fn child_consts_fold_into_parent_auto_wires() {
    // The child's ports are sized by ITS OWN `const W`. The parent's
    // auto-wire for `u.y` must fold that const to a literal — the
    // symbolic name `W` does not exist in the parent's Verilog.
    // (Found 2026-06-12: `wire [(W)-1:0]` leaked and iverilog
    // rejected it — "Unable to bind parameter `W`".)
    let v = emit_src(
        "module C {\n  const W: int = 4\n  in a: bits[W]\n  out y: bits[W]\n  y = a\n}\n\
         module Top {\n  in x: bits[4]\n  out z: bits[4]\n  let u = C() { a: x }\n  z = u.y\n}\n",
    );
    assert!(
        v.contains("wire [(4)-1:0] u_y;"),
        "child const W must fold to 4 in the auto-wire:\n{v}"
    );
}

#[test]
fn parent_const_never_substitutes_into_child_widths() {
    // Same const NAME, different values: the auto-wire must use the
    // CHILD's 4, never the parent's 8 — silently wrong hardware
    // otherwise.
    let v = emit_src(
        "module C {\n  const W: int = 4\n  in a: bits[W]\n  out y: bits[W]\n  y = a\n}\n\
         module Top {\n  const W: int = 8\n  in x: bits[4]\n  out z: bits[4]\n  let u = C() { a: x }\n  z = u.y\n}\n",
    );
    assert!(
        v.contains("wire [(4)-1:0] u_y;"),
        "the CHILD's W=4 sizes the wire, not the parent's W=8:\n{v}"
    );
}

/// Project-level diagnostics must say WHICH file they point into —
/// `render_diags` uses this to pick the right source excerpt.
#[test]
fn diags_carry_the_file_index() {
    // Duplicate module: file 1 defines `A` twice (same-file uniqueness —
    // reusing a name ACROSS files is legal, spec/02 section 1.5b).
    let files = [
        parse("module Unrelated {\n}\n"),
        parse("module A {\n}\nmodule A {\n}\n"),
    ];
    let diags = Project::from_files(&files).err().expect("duplicate");
    assert_eq!(diags[0].file, Some(1), "error is in the second file");

    // Emitter error (non-ASCII identifier — transliteration is Phase C)
    // inside the second file.
    let files = [
        parse("module A {\n}\n"),
        parse("module B {\n  out மணி: bits[4]\n  மணி = 0\n}\n"),
    ];
    let project = Project::from_files(&files).unwrap();
    let diags = emit(&project, &files).expect_err("non-ASCII identifier unsupported");
    assert_eq!(diags[0].file, Some(1), "error is in the second file");
}

#[test]
fn two_same_named_modules_emit_their_own_bodies() {
    // Mirrors Task 6's driver-check test, one layer further down the
    // pipeline: file A's `Fifo` and file B's `Fifo` have DIFFERENT bodies;
    // both get instantiated (via distinct qualified paths); the emitted
    // Verilog for each instance must come from the RIGHT one.
    let a = parse("module Fifo {\n  out y: bit\n  y = 1\n}\n"); // y = 1
    let b = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n"); // y = 0
    let mut user = parse("module M {\n  let x = Fifo() { }\n  let z = Fifo() { }\n}\n");
    if let TopItem::Module(m) = &mut user.items[0] {
        let mut insts = m.items.iter_mut().filter_map(|it| {
            if let ModuleItem::Inst(i) = it {
                Some(i)
            } else {
                None
            }
        });
        let x = insts.next().unwrap();
        x.module.resolved_file.set(Some(1));
        let z = insts.next().unwrap();
        z.module.resolved_file.set(Some(2));
    }
    let files = [user, a, b];
    let project = Project::from_files(&files).expect("builds");
    // Assert the emitted module bodies for the two Fifo definitions differ
    // — i.e. Project correctly holds BOTH under the name "Fifo", keyed
    // apart by file, not one silently shadowing the other.
    let fifos = project
        .modules
        .get("Fifo")
        .expect("both Fifo decls present");
    assert_eq!(
        fifos.len(),
        2,
        "both same-named modules must coexist in the table"
    );
}
