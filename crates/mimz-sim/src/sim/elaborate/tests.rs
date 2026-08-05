use super::*;

fn parse(src: &str) -> ast::File {
    mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses")
}

fn design(src: &str) -> Design {
    elaborate(&parse(src), None, &BTreeMap::new()).expect("elaborates")
}

#[test]
fn a_module_body_assert_is_collected_into_design_asserts() {
    let f = parse("module M {\n  in a: bit\n  out y: bit\n  assert(a)\n  y = a\n}\n");
    let design = elaborate(&f, None, &BTreeMap::new()).expect("elaborates");
    assert_eq!(design.asserts.len(), 1);
}

const COUNTER: &str = "module Counter(WIDTH: int = 8) {\n  \
        clock clk\n  reset rst\n  out count: bits[WIDTH]\n  \
        reg value: bits[WIDTH] = 0\n  on rise(clk) { value <- value +% 1 }\n  \
        count = value\n}\n";

#[test]
fn elaborates_the_counter() {
    let d = design(COUNTER);
    assert_eq!(d.module, "Counter");
    assert_eq!(d.consts["WIDTH"], 8);
    assert_eq!(d.inputs, vec![]);
    assert_eq!(
        d.outputs,
        vec![Signal {
            name: "count".into(),
            width: Width {
                bits: 8,
                signed: false
            }
        }]
    );
    assert_eq!(
        d.regs,
        vec![Reg {
            name: "value".into(),
            width: Width {
                bits: 8,
                signed: false
            },
            reset: mimz_core::checker::consteval::ConstVal::zero(),
            clock: "clk".into(),
            edge: Edge::Rise,
        }]
    );
    assert!(d.comb.contains_key("count")); // count = value
    assert_eq!(d.clocks, vec!["clk".to_string()]);
    assert_eq!(d.resets, vec!["rst".to_string()]);
    assert_eq!(d.procs.len(), 1);
    assert_eq!(d.procs[0].clock, "clk");
}

#[test]
fn param_override_folds_widths() {
    let d = elaborate(
        &parse(COUNTER),
        None,
        &BTreeMap::from([("WIDTH".into(), 4)]),
    )
    .expect("elaborates");
    assert_eq!(d.consts["WIDTH"], 4);
    assert_eq!(d.outputs[0].width.bits, 4);
    assert_eq!(d.regs[0].width.bits, 4);
}

#[test]
fn elaborates_a_combinational_module() {
    // No clock/reset/reg → empty sequential parts, comb drivers only.
    let d = design(
        "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[9]\n  y = a + b\n}\n",
    );
    assert_eq!(d.inputs.len(), 2);
    assert_eq!(d.outputs.len(), 1);
    assert!(d.regs.is_empty());
    assert!(d.procs.is_empty());
    assert!(d.clocks.is_empty());
    assert!(d.resets.is_empty());
    assert!(d.comb.contains_key("y"));
}

#[test]
fn reg_takes_a_nonzero_folded_reset_value() {
    let d = design(
        "module R {\n  clock clk\n  reset rst\n  out y: bits[8]\n  \
             reg r: bits[8] = 5\n  on rise(clk) { r <- r +% 1 }\n  y = r\n}\n",
    );
    assert_eq!(
        d.regs[0].reset,
        mimz_core::checker::consteval::ConstVal::from_i128(5)
    );
    assert_eq!(d.regs[0].clock, "clk");
}

#[test]
fn flattens_a_same_file_instance() {
    // `Top` instantiates a combinational `Add`; the child's signals inline as
    // `u_a`/`u_b`/`u_s`, and the parent's `u.s` field-read becomes `u_s`.
    let d = elaborate(
        &parse(
            "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out s: bits[9]\n  \
                 s = a + b\n}\n\
                 module Top {\n  in x: bits[8]\n  in y: bits[8]\n  out t: bits[9]\n  \
                 let u = Add() { a: x, b: y }\n  t = u.s\n}\n",
        ),
        Some("Top"),
        &BTreeMap::new(),
    )
    .expect("flattens");
    assert_eq!(d.module, "Top");
    let wire_names: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
    assert!(wire_names.contains(&"u_a"), "wires: {wire_names:?}");
    assert!(wire_names.contains(&"u_b"), "wires: {wire_names:?}");
    assert!(wire_names.contains(&"u_s"), "wires: {wire_names:?}");
    // `t = u.s` → `t = u_s`; child output `u_s` is driven by `u_a + u_b`.
    assert!(d.comb.contains_key("t"));
    assert!(d.comb.contains_key("u_s"));
    assert!(d.regs.is_empty() && d.procs.is_empty());
}

#[test]
fn rejects_unknown_instance_module() {
    let err = elaborate(
        &parse(
            "module Top {\n  out y: bits[8]\n  \
                 let u = Missing() { }\n  y = 0\n}\n",
        ),
        None,
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(err.msg.contains("unknown module"), "got: {}", err.msg);
}

#[test]
fn unrolls_repeat_with_instance_array_and_bit_drives() {
    // `repeat` inlines one `Xor` per bit; `s[i] = fa[i].o` collects bit drives
    // that assemble into a whole-signal Concat.
    let d = elaborate(
        &parse(
            "module Xor {\n  in a: bit\n  in b: bit\n  out o: bit\n  o = a ^ b\n}\n\
                 module R {\n  in x: bits[2]\n  in y: bits[2]\n  out s: bits[2]\n  \
                 repeat i: 0..2 {\n    let fa[i] = Xor() { a: x[i], b: y[i] }\n    \
                 s[i] = fa[i].o\n  }\n}\n",
        ),
        Some("R"),
        &BTreeMap::new(),
    )
    .expect("unrolls");
    let wires: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
    assert!(wires.contains(&"fa__0_o"), "wires: {wires:?}");
    assert!(wires.contains(&"fa__1_o"), "wires: {wires:?}");
    // `s` assembled from its per-bit drives.
    assert!(
        matches!(d.comb["s"].kind, ExprKind::Concat(_)),
        "s not a concat"
    );
}

#[test]
fn unrolls_foreach_range_form_same_as_repeat() {
    // `foreach i in 0..2` is pure sugar over `repeat i: 0..2` (Task 8) —
    // same source as `unrolls_repeat_with_instance_array_and_bit_drives`
    // above, with `repeat i: 0..2` swapped for `foreach i in 0..2`, must
    // elaborate identically.
    let d = elaborate(
        &parse(
            "module Xor {\n  in a: bit\n  in b: bit\n  out o: bit\n  o = a ^ b\n}\n\
                 module R {\n  in x: bits[2]\n  in y: bits[2]\n  out s: bits[2]\n  \
                 foreach i in 0..2 {\n    let fa[i] = Xor() { a: x[i], b: y[i] }\n    \
                 s[i] = fa[i].o\n  }\n}\n",
        ),
        Some("R"),
        &BTreeMap::new(),
    )
    .expect("foreach range form unrolls");
    let wires: Vec<&str> = d.wires.iter().map(|w| w.name.as_str()).collect();
    assert!(wires.contains(&"fa__0_o"), "wires: {wires:?}");
    assert!(wires.contains(&"fa__1_o"), "wires: {wires:?}");
    assert!(
        matches!(d.comb["s"].kind, ExprKind::Concat(_)),
        "s not a concat"
    );
}

#[test]
fn foreach_elements_form_substitutes_var_with_mem_index() {
    // Elements-form `foreach v in values` over a single-element `mem`
    // (module-level array-typed ports/wires/regs are E0416 — `mem` is
    // the only array-like module-level signal, mirroring the checker's
    // `foreach_elements_form_at_module_level_checks_clean`) lowers to a
    // `Repeat` over a synthesized index, substituting `v` with
    // `values[idx]` throughout the body (Task 8's `lower_foreach_item`,
    // Task 3). A single-element `mem` keeps the unrolled `sum = v`
    // drive single-valued, so the resulting comb driver for `sum` is
    // exactly `values[0]` — proving the substitution really flows
    // through this crate's own `elaborate_module`, not just the
    // AST-level unit test in `ast::foreach_lower`.
    let d = elaborate(
        &parse(
            "module M {\n  mem values: bits[8][1] = 0\n  out sum: bits[8]\n  \
                 foreach v in values {\n    sum = v\n  }\n}\n",
        ),
        Some("M"),
        &BTreeMap::new(),
    )
    .expect("foreach elements form over a mem elaborates");
    assert!(
        d.mems.iter().any(|m| m.name == "values"),
        "mems: {:?}",
        d.mems
    );
    assert!(d.comb.contains_key("sum"), "comb: {:?}", d.comb);
    assert!(
        matches!(&d.comb["sum"].kind, ExprKind::Index { base, .. } if matches!(&base.kind, ExprKind::Ident(n) if n == "values")),
        "sum must be driven by an index into `values`, got {:?}",
        d.comb["sum"]
    );
}

#[test]
fn foreach_nested_inside_if_in_on_block_lowers_via_recursion() {
    // `lower_foreach_in_seq` must recurse into `If`'s `then` body, not
    // just dispatch on the on-block's top-level statements — a `foreach`
    // sitting inside an `if` inside `on rise(clk)` must still be
    // replaced by a `SeqStmt::Loop` before the block becomes a
    // `Process`, so `Rw::seq`/the kernel's `run_seq` never see a raw
    // `SeqStmt::ForEach` node at any nesting depth.
    let d = design(
        "module M {\n  clock clk\n  reset rst\n  in cond: bit\n  reg acc: bits[8] = 0\n  \
             on rise(clk) {\n    if cond {\n      foreach i in 0..4 {\n        acc <- acc +% 1\n      }\n    }\n  }\n}\n",
    );
    assert_eq!(d.procs.len(), 1);
    let SeqStmt::If { then, .. } = &d.procs[0].body[0] else {
        panic!(
            "expected the on-block's top-level `if` to survive, got {:?}",
            d.procs[0].body
        );
    };
    assert!(
        matches!(then.first(), Some(SeqStmt::Loop { .. })),
        "foreach nested inside if must lower to Loop, got {then:?}"
    );
    assert!(
        !then.iter().any(|s| matches!(s, SeqStmt::ForEach { .. })),
        "raw ForEach must not survive lowering, got {then:?}"
    );
}

#[test]
fn elaborates_an_enum_signal_and_match() {
    // `reg st: S` width = clog2(2) = 1; `S.A` reset = 0; the match over the
    // enum elaborates (variant patterns rewritten to their indices).
    let d = design(
        "module FSM {\n  clock clk\n  reset rst\n  out o: bit\n  \
             enum S { A, B }\n  reg st: S = S.A\n  \
             on rise(clk) { st <- match st { S.A => S.B\n S.B => S.A } }\n  \
             o = st == S.B\n}\n",
    );
    let st = d.regs.iter().find(|r| r.name == "st").expect("reg st");
    assert_eq!(st.width.bits, 1);
    assert_eq!(st.reset, mimz_core::checker::consteval::ConstVal::zero());
    assert!(d.comb.contains_key("o"));
}

// ---- C1–C4 hardening regressions (SEC-6 / the 2026 audit) ----

#[test]
fn recursive_instantiation_errors_not_overflows() {
    // SIM-1: `mimz sim`/`test` skip the checker, so a self-instantiating
    // module must error on the depth bound, not recurse into a stack overflow.
    let err = elaborate(
        &parse("module A {\n  out y: bits[8]\n  let u = A() { }\n  y = 0\n}\n"),
        None,
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(err.msg.contains("nesting"), "got: {}", err.msg);
}

#[test]
fn extreme_repeat_bounds_error_not_overflow() {
    // SIM-2: `hi - lo` past i128::MAX must be a clean over-budget error, not an
    // overflow panic.
    let big = "100000000000000000000000000000000000000"; // ~1e38, fits i128
    let src = format!(
        "module R {{\n  out y: bit\n  repeat i: -{big}..{big} {{\n    y[i] = 0\n  }}\n}}\n"
    );
    let err = elaborate(&parse(&src), None, &BTreeMap::new()).unwrap_err();
    assert!(err.msg.contains("unroll"), "got: {}", err.msg);
}

#[test]
fn an_out_of_range_bit_index_errors() {
    // SIM-3: a bit index past 128 must error, not truncate via `as u32`.
    let err = elaborate(
        &parse("module R {\n  out y: bits[4]\n  y[200] = 0\n}\n"),
        None,
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(err.msg.contains("out of range"), "got: {}", err.msg);
}

#[test]
fn a_flatten_name_collision_errors() {
    // SIM-4: a parent signal named like a flattened `inst_port` wire must error
    // instead of silently overwriting.
    let err = elaborate(
        &parse(
            "module Add {\n  in a: bits[8]\n  in b: bits[8]\n  out s: bits[9]\n  \
                 s = a + b\n}\n\
                 module Top {\n  in x: bits[8]\n  out t: bits[9]\n  wire u_s: bits[9] = 0\n  \
                 let u = Add() { a: x, b: x }\n  t = u_s\n}\n",
        ),
        Some("Top"),
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(err.msg.contains("collides"), "got: {}", err.msg);
}

#[test]
fn two_same_named_modules_flatten_their_own_instance() {
    // SIM analogue of `emit_verilog::Project`'s Task 7 regression test
    // (`two_same_named_modules_emit_their_own_bodies`): file A's `Fifo`
    // and file B's `Fifo` have DIFFERENT bodies (different output
    // widths, so the assertion doesn't need to inspect expr content);
    // `M` instantiates each via a distinct qualified path. Before the
    // fix, `build_registry`'s bare-name `HashMap` let the LAST-inserted
    // file silently win for EVERY instance regardless of its qualifier
    // — both instances would flatten with the same (wrong, for one of
    // them) body. Hand-wires `path`/`resolved_file` the same way
    // `checker::tests::qualified_module_reference_resolves_unambiguously`
    // does — nothing in the pipeline computes this from real `import`
    // statements yet (that pass doesn't exist for `Inst.module` in
    // production either; only `Import.resolved_file` is set, by
    // `project.rs` at load time).
    let a = parse("module Fifo {\n  out y: bits[4]\n  y = 0\n}\n");
    let b = parse("module Fifo {\n  out y: bits[8]\n  y = 0\n}\n");
    let mut user = parse("module M {\n  let x = Fifo() { }\n  let z = Fifo() { }\n}\n");
    if let ast::TopItem::Module(m) = &mut user.items[0] {
        let mut insts = m.items.iter_mut().filter_map(|it| {
            if let ModuleItem::Inst(i) = it {
                Some(i)
            } else {
                None
            }
        });
        let x = insts.next().unwrap();
        x.module.path.push(ast::Ident {
            name: "a".into(),
            span: x.module.span,
        });
        x.module.resolved_file.set(Some(1));
        let z = insts.next().unwrap();
        z.module.path.push(ast::Ident {
            name: "b".into(),
            span: z.module.span,
        });
        z.module.resolved_file.set(Some(2));
    }
    let files = [user, a, b];
    let d = elaborate_project(&files, Some("M"), &BTreeMap::new()).expect("flattens");
    let width_of = |name: &str| d.wires.iter().find(|w| w.name == name).unwrap().width.bits;
    assert_eq!(width_of("x_y"), 4, "x must flatten file A's 4-bit Fifo");
    assert_eq!(width_of("z_y"), 8, "z must flatten file B's 8-bit Fifo");
}

#[test]
fn qualified_instance_reference_resolves_via_a_real_import_path() {
    // Sim-side analogue of `checker::tests::
    // qualified_reference_actually_resolves_via_a_real_import_path`.
    // Unlike `two_same_named_modules_flatten_their_own_instance` above
    // (which hand-pokes `Inst.module.resolved_file` directly — the gap
    // Task 9 closes), this test has a real `import b` statement and a
    // real qualified `b.Fifo()` instantiation; only `Import.resolved_file`
    // is set (mimicking `project::load_project`, Task 3). `mimz sim`/
    // `mimz test` never run the checker, so `resolve_module` itself must
    // compute the match from `q.path` against `user`'s own `imports`.
    let a = parse("module Fifo {\n  out y: bits[4]\n  y = 0\n}\n");
    let b = parse("module Fifo {\n  out y: bits[8]\n  y = 0\n}\n");
    let user = parse("import b\n\nmodule M {\n  let z = b.Fifo() { }\n}\n");
    assert_eq!(user.imports.len(), 1, "sanity: `import b` parsed");
    user.imports[0].resolved_file.set(Some(2));
    let files = [user, a, b];
    let d = elaborate_project(&files, Some("M"), &BTreeMap::new())
        .expect("qualified instance must resolve via the real import match");
    let width = d
        .wires
        .iter()
        .find(|w| w.name == "z_y")
        .expect("flattened wire z_y")
        .width
        .bits;
    assert_eq!(
        width, 8,
        "z must flatten file B's 8-bit Fifo via the import match"
    );
}

#[test]
fn ambiguous_bare_module_reference_errors_instead_of_silently_picking_one() {
    // Unlike `emit_verilog` (which only ever runs after the checker has
    // already rejected this as E0110), `mimz sim`/`mimz test` elaborate
    // the raw parse tree directly (see the module doc comment) — nothing
    // gates an ambiguous bare reference before it reaches here, so it
    // must be a real error, not a silent last-file-wins pick.
    let a = parse("module Fifo {\n  out y: bit\n  y = 1\n}\n");
    let b = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n");
    let user = parse("module M {\n  let u = Fifo() { }\n}\n");
    let files = [user, a, b];
    let err = elaborate_project(&files, Some("M"), &BTreeMap::new()).unwrap_err();
    assert!(err.msg.contains("ambiguous"), "got: {}", err.msg);
}

#[test]
fn an_i128_min_const_elaborates_without_overflow() {
    // SIM-5: a flattened child const that evaluates to i128::MIN must not
    // overflow-panic the negation in `int_expr`. i128::MAX is
    // 170141183460469231731687303715884105727, so `-MAX - 1` is i128::MIN,
    // reachable via checked arithmetic even on the checker-skipping sim path.
    let res = elaborate(
        &parse(
            "module Child {\n  \
                 const M: int = -170141183460469231731687303715884105727 - 1\n  \
                 out y: bit\n  y = 0\n}\n\
                 module Top {\n  out t: bit\n  let u = Child() { }\n  t = u_y\n}\n",
        ),
        Some("Top"),
        &BTreeMap::new(),
    );
    assert!(
        res.is_ok(),
        "i128::MIN const should elaborate, got: {res:?}"
    );
}

// --- `sync loop` elaboration timing (Task 10) ---

fn sim(src: &str) -> super::super::kernel::Sim {
    super::super::kernel::Sim::new(design(src))
}

/// `start` pulsed for one cycle → `done` pulses exactly `hi - lo + 1`
/// cycles later (counting the cycle `start` was sampled as cycle 1); a
/// held-high `start` does not re-trigger the run mid-flight, because the
/// lowered FSM only samples `start` from its idle branch (see
/// `ast::sync_loop_lower::lower_sync_loop`'s `running_r` gate) — while
/// `running_r` is set, the running branch never re-reads `start` at all.
/// This exercises the lowered `Reg`/`On` items flowing through the real
/// `kernel::Sim`, i.e. `kernel.rs`'s existing `tick_edge` dispatch with
/// zero changes to that file.
#[test]
fn sync_loop_timing_and_no_mid_run_retrigger() {
    let mut s = sim(
        "module M {\n  clock clk\n  reset rst\n  sync loop s on rise(clk) (i: 0..4) -> result: bits[4] = 0 {\n    result <- result + 1\n  }\n}\n",
    );
    use super::super::value::Bits;
    s.set("rst", Bits::Small(1)).unwrap();
    s.tick("clk").unwrap();
    s.set("rst", Bits::Small(0)).unwrap();
    s.set("s_start", Bits::Small(1)).unwrap();
    s.tick("clk").unwrap(); // idle -> running, cnt = lo = 0
    s.set("s_start", Bits::Small(1)).unwrap(); // held high through the run — must not re-trigger
    for _ in 0..3 {
        assert_eq!(
            s.peek("s_done").unwrap(),
            Bits::Small(0),
            "must not pulse done before hi - lo cycles elapse"
        );
        s.tick("clk").unwrap();
    }
    assert_eq!(
        s.peek("s_done").unwrap(),
        Bits::Small(0),
        "still one cycle short of hi - lo + 1"
    );
    s.tick("clk").unwrap();
    assert_eq!(
        s.peek("s_done").unwrap(),
        Bits::Small(1),
        "done must pulse exactly hi - lo + 1 cycles after start was sampled"
    );
    assert_eq!(s.peek("s_result").unwrap(), Bits::Small(4));
}

/// Final whole-branch review, Finding 1: a `SyncLoop` nested inside a
/// `const if` winning branch is checker-accepted (`checker::names`
/// recurses into `ConstIf` branches when declaring names) and
/// emitter-supported (`emit_verilog::module::flatten_items` recurses the
/// same way) — the simulator must lower it too, instead of pushing the
/// raw `SyncLoop` node onto the worklist where it hits the `unreachable!()`
/// arm. Regression for the pre-fix panic (`elaborate_module`'s
/// `lowered_sync_loops` only scanned direct `m.items` children).
#[test]
fn sync_loop_nested_in_const_if_elaborates_and_ticks() {
    let mut s = sim("module M {\n  clock clk\n  reset rst\n  \
             const if (1) {\n    \
             sync loop s on rise(clk) (i: 0..4) -> result: bits[4] = 0 {\n      result <- result + 1\n    }\n  \
             }\n}\n");
    use super::super::value::Bits;
    s.set("rst", Bits::Small(1)).unwrap();
    s.tick("clk").unwrap();
    s.set("rst", Bits::Small(0)).unwrap();
    s.set("s_start", Bits::Small(1)).unwrap();
    s.tick("clk").unwrap();
    for _ in 0..4 {
        s.tick("clk").unwrap();
    }
    assert_eq!(s.peek("s_done").unwrap(), Bits::Small(1));
    assert_eq!(s.peek("s_result").unwrap(), Bits::Small(4));
}

/// Same as above, but the `SyncLoop` sits in the `const if`'s losing
/// branch — the winning (`else`) branch has no `SyncLoop` at all, so
/// elaboration must succeed with no lowered items and no panic.
#[test]
fn sync_loop_in_const_if_losing_branch_is_not_lowered() {
    let d = design(
        "module M {\n  clock clk\n  reset rst\n  \
             const if (0) {\n    \
             sync loop s on rise(clk) (i: 0..4) -> result: bits[4] = 0 {\n      result <- result + 1\n    }\n  \
             } else {\n    wire w: bit = 0\n  }\n}\n",
    );
    assert!(d.wires.iter().any(|w| w.name == "w"));
    assert!(d.regs.iter().all(|r| r.name != "s_cnt"));
}

// ---- BUG-15: bundle-field expansion at instance ports / fn call args ----

#[test]
fn bundle_typed_instance_input_port_connection_flattens_per_field() {
    // A bundle-typed wire connected to a bundle-typed instance input port
    // used to fail entirely: the child's flattened field names
    // (`req_valid`/`req_data`) never matched the user-written connection's
    // port name (`req`), so the "input is not connected" error always fired.
    use super::super::value::Bits;
    let mut s = super::super::kernel::Sim::new(
        elaborate(
            &parse(
                "bundle Handshake(W: int = 8) {\n  valid: bit\n  data: bits[W]\n}\n\
                 module Child {\n  in req: Handshake(W: 8)\n  out y: bits[8]\n  \
                 y = if req.valid { req.data } else { 0 }\n}\n\
                 module Parent {\n  in v: bit\n  in d: bits[8]\n  out y: bits[8]\n  \
                 wire req: Handshake(W: 8) = { valid: v, data: d }\n  \
                 let c = Child() { req: req }\n  y = c.y\n}\n",
            ),
            Some("Parent"),
            &BTreeMap::new(),
        )
        .expect("bundle-typed instance connection elaborates"),
    );
    s.set("v", Bits::Small(1)).unwrap();
    s.set("d", Bits::Small(42)).unwrap();
    assert_eq!(s.peek("y").unwrap(), Bits::Small(42));
    s.set("v", Bits::Small(0)).unwrap();
    assert_eq!(s.peek("y").unwrap(), Bits::Small(0));
}

#[test]
fn bundle_typed_fn_call_argument_expands_to_one_arg_per_field() {
    // A bundle-typed value passed whole as a `fn` call argument used to
    // reach the evaluator as a single unresolvable identifier — the
    // callee's declared param has no bundle case in `eval_fn_call`'s
    // binding loop, and the caller's own bundle-typed signal was never
    // split into its constituent fields at the call site.
    use super::super::value::Bits;
    let mut s = super::super::kernel::Sim::new(
        elaborate(
            &parse(
                "bundle Handshake(W: int = 8) {\n  valid: bit\n  data: bits[W]\n}\n\
                 fn pick(req: Handshake(W: 8)) -> bits[8] {\n  \
                 if req.valid { return req.data }\n  0\n}\n\
                 module M {\n  in v: bit\n  in d: bits[8]\n  out y: bits[8]\n  \
                 wire req: Handshake(W: 8) = { valid: v, data: d }\n  \
                 y = pick(req)\n}\n",
            ),
            None,
            &BTreeMap::new(),
        )
        .expect("bundle-typed fn call argument elaborates"),
    );
    s.set("v", Bits::Small(1)).unwrap();
    s.set("d", Bits::Small(7)).unwrap();
    assert_eq!(s.peek("y").unwrap(), Bits::Small(7));
    s.set("v", Bits::Small(0)).unwrap();
    assert_eq!(s.peek("y").unwrap(), Bits::Small(0));
}
