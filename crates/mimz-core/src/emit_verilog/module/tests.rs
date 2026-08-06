use super::*;
use crate::span::Span;
use crate::{lexer, parser};

/// Parse + emit one self-contained source (no imports) to Verilog text.
/// Mirrors `emit_verilog::mod::tests::emit_src` — duplicated locally so
/// this test stays inside `module.rs` per Task 9's scoping.
fn emit_src(src: &str) -> String {
    let files = [parser::parse(lexer::lex(src).unwrap()).unwrap()];
    let project = Project::from_files(&files).unwrap();
    emit(&project, &files).expect("emit should succeed")
}

/// Minimal `Emitter` for unit-level tests that need to call a `&self`/
/// `&mut self` method directly without driving the whole `emit()`
/// pipeline. There is no existing helper for this anywhere in
/// `emit_verilog` (every other inline test here and in `mod.rs` goes
/// through `emit_src`/`emit()`) — this mirrors `emit()`'s own `Emitter`
/// struct literal (`emit_verilog/mod.rs`) field-for-field, the one
/// other place this struct gets constructed.
fn test_emitter<'a>(project: &'a Project<'a>) -> Emitter<'a> {
    Emitter {
        project,
        out: String::new(),
        diags: Vec::new(),
        cur_file: 0,
        env: Env::new(),
        module_envs: HashMap::new(),
        repeat_budget: REPEAT_BUDGET,
        clog2_fn_used: false,
        emitting_port: false,
        funcs_used: Vec::new(),
        bundle_sigs: HashMap::new(),
        hoist_counter: 0,
        hoisted_decls: String::new(),
        cur_decls: HashMap::new(),
        cover_ordinals: HashMap::new(),
    }
}

#[test]
fn comb_assert_emits_a_guarded_always_block() {
    let v = emit_src("module M {\n  in a: bit\n  out y: bit\n  assert(a)\n  y = a\n}\n");
    assert!(v.contains("`ifndef SYNTHESIS"), "got:\n{v}");
    assert!(v.contains("always @(*)"), "got:\n{v}");
    assert!(v.contains("if (!(a)) begin"), "got:\n{v}");
    assert!(v.contains("$fatal(1)"), "got:\n{v}");
    assert!(v.contains("`endif"), "got:\n{v}");
}

#[test]
fn comb_assert_with_a_message_embeds_it() {
    let v = emit_src(
        "module M {\n  in a: bit\n  out y: bit\n  assert(a, \"a must be set\")\n  y = a\n}\n",
    );
    assert!(v.contains("a must be set"), "got:\n{v}");
}

#[test]
fn comb_cover_emits_a_sensitized_counter() {
    let v = emit_src("module M {\n  in a: bit\n  out y: bit\n  cover(a)\n  y = a\n}\n");
    assert!(v.contains("`ifndef SYNTHESIS"), "got:\n{v}");
    assert!(v.contains("reg [31:0]"), "got:\n{v}");
    assert!(
        v.contains(" = 0;"),
        "counter must be zero-initialized:\n{v}"
    );
    assert!(
        v.contains("if (") && v.contains("count = ") && v.contains("+ 1"),
        "got:\n{v}"
    );
    assert!(v.contains("`endif"), "got:\n{v}");
}

#[test]
fn clocked_cover_emits_an_inline_increment() {
    let v = emit_src(
        "module M {\n  clock clk\n  in a: bit\n  out y: bit\n  reg r: bit = 0\n  \
         on rise(clk) {\n    cover(a)\n    r <- a\n  }\n  y = r\n}\n",
    );
    assert!(v.contains("always @(posedge clk)"), "got:\n{v}");
    assert!(v.contains("`ifndef SYNTHESIS"), "got:\n{v}");
    assert!(v.contains("reg [31:0]"), "got:\n{v}");
    assert!(
        v.contains("<= ") && v.contains("count") && v.contains("+ 1"),
        "got:\n{v}"
    );
    assert!(v.contains("`endif"), "got:\n{v}");
}

#[test]
fn clocked_assert_emits_inline_in_the_posedge_block() {
    let v = emit_src(
        "module M {\n  clock clk\n  in a: bit\n  out y: bit\n  reg r: bit = 0\n  \
         on rise(clk) {\n    assert(a)\n    r <- a\n  }\n  y = r\n}\n",
    );
    assert!(v.contains("always @(posedge clk)"), "got:\n{v}");
    assert!(v.contains("`ifndef SYNTHESIS"), "got:\n{v}");
    assert!(v.contains("if (!(a)) begin"), "got:\n{v}");
    assert!(v.contains("$fatal(1)"), "got:\n{v}");
}

#[test]
fn sync_double_flop_emits_a_plain_reg_chain() {
    let src = "module M {\n\
             clock clk_src\n\
             clock clk_dst\n\
             in fast_bit: bit\n\
             reg slow_bit: bit = 0\n\
             reset rst\n\
             out o: bit\n\
             on rise(clk_dst) {\n\
                 slow_bit <- sync.double_flop(fast_bit, clk_src, clk_dst)\n\
             }\n\
             o = slow_bit\n\
           }";
    let v = emit_src(src);
    assert_eq!(
        v.matches("reg __sync_slow_bit_stage0;").count(),
        1,
        "expected exactly one hidden stage reg in the output:\n{v}"
    );
    assert!(
        v.contains("posedge clk_dst"),
        "the hidden stage must be clocked on clk_dst:\n{v}"
    );
}

#[test]
fn sync_pulse_emits_a_toggle_reg_and_a_src_clock_on_block() {
    // `fast_reg` lives in `clk_src`'s own domain (via its `on rise`
    // block) rather than a domain-free `in` port — `sync.pulse`
    // requires exactly that (unlike `double_flop`, which permits a
    // domain-free signal); a domain-free signal here is real-checker-
    // rejected (E0704), so this fixture stays representative of actual
    // checker-clean input even though `emit_src` itself bypasses the
    // checker.
    let src = "module M {\n\
             clock clk_src\n\
             clock clk_dst\n\
             reg fast_reg: bit = 0\n\
             reset rst\n\
             on rise(clk_src) {\n\
                 fast_reg <- 1\n\
             }\n\
             wire dst_pulse: bit = sync.pulse(fast_reg, clk_src, clk_dst)\n\
             out o: bit\n\
             o = dst_pulse\n\
           }";
    let v = emit_src(src);
    assert_eq!(
        v.matches("wire dst_pulse;").count(),
        1,
        "expected exactly one declaration of the rewritten wire:\n{v}"
    );
    assert_eq!(
        v.matches("assign dst_pulse =").count(),
        1,
        "expected exactly one drive of the rewritten wire:\n{v}"
    );
    assert!(
        v.contains("__sync_dst_pulse_stage1 ^ __sync_dst_pulse_stage2"),
        "the wire must be driven by stage1 XOR stage2:\n{v}"
    );
    assert!(
        v.contains("reg __sync_dst_pulse_toggle"),
        "expected the hidden toggle reg in the output:\n{v}"
    );
    assert!(
        v.contains("posedge clk_src"),
        "the toggle reg's own on-block must be clocked on clk_src:\n{v}"
    );
}

/// Smoke test: build_decls's own logic is exercised indirectly through
/// a normal compile (Port/Wire declarations still render), before
/// Step 3's direct unit test below checks its return value.
#[test]
fn build_decls_resolves_port_and_wire_widths() {
    let src = "module M {\n  in p0: bits[8]\n  in p1: signed[4]\n  \
                wire w: bits[3] = 0\n  out y: bit\n  y = p0[0:0]\n}\n";
    let v = emit_src(src);
    assert!(v.contains("input"));
    assert!(v.contains("wire"));
}

#[test]
fn build_decls_maps_names_to_kinds() {
    let files = [parser::parse(lexer::lex("module M {}\n").unwrap()).unwrap()];
    let project = Project::from_files(&files).unwrap();
    let emitter = test_emitter(&project);
    let flat = vec![
        ModuleItem::Port {
            dir: Dir::In,
            name: Ident {
                name: "p0".to_string(),
                span: Span::new(0, 0),
            },
            ty: Type::Bits(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 8u128.into(),
                    raw: "8".to_string(),
                },
                span: Span::new(0, 0),
            })),
        },
        ModuleItem::Port {
            dir: Dir::In,
            name: Ident {
                name: "p1".to_string(),
                span: Span::new(0, 0),
            },
            ty: Type::Signed(Box::new(Expr {
                kind: ExprKind::Int {
                    value: 4u128.into(),
                    raw: "4".to_string(),
                },
                span: Span::new(0, 0),
            })),
        },
    ];
    let decls = emitter.build_decls(&flat);
    assert_eq!(
        decls["p0"],
        crate::width_rules::Kind {
            width: 8,
            signed: false
        }
    );
    assert_eq!(
        decls["p1"],
        crate::width_rules::Kind {
            width: 4,
            signed: true
        }
    );
}

/// Proves `sync loop` actually desugars in the emitter: its 4 ports, 4
/// regs, `on`-block FSM, and 3 output drives all reach real Verilog
/// through the existing Port/Reg/On/Drive codegen — not silently
/// dropped, which was the bug before `flatten_items` called
/// `lower_sync_loop`.
#[test]
fn sync_loop_emits_fsm_and_ports() {
    let src = "module Search {\n  clock clk\n  reset rst\n  mem m: bits[8][8] = 0\n  in key: bits[8]\n  sync loop find_first on rise(clk) (i: 0..8) -> result: signed[4] = 0 - 1 {\n    if m[i] == key { result <- i }\n  }\n}\n";
    let v = emit_src(src);
    // Ports (4): _start in, _done/_result/_running out.
    assert!(
        v.contains("input wire find_first_start"),
        "start port missing:\n{v}"
    );
    assert!(
        v.contains("output wire find_first_done"),
        "done port missing:\n{v}"
    );
    assert!(
        v.contains("output wire signed [(4)-1:0] find_first_result"),
        "signed result port missing or wrongly formatted:\n{v}"
    );
    assert!(
        v.contains("output wire find_first_running"),
        "running port missing:\n{v}"
    );
    // Counter reg: bits[clog2(8)] = bits[3] -> "[(3)-1:0]" (same folded-
    // literal-in-parens convention as the existing clog2-port-width test).
    assert!(
        v.contains("reg [(3)-1:0] find_first_cnt;"),
        "counter reg missing:\n{v}"
    );
    // FSM always-block, clocked on `clk`.
    assert!(
        v.contains("always @(posedge clk"),
        "always block missing:\n{v}"
    );
    // The 3 generated output drives.
    assert!(
        v.contains("assign find_first_done = find_first_done_r;"),
        "done drive missing:\n{v}"
    );
    assert!(
        v.contains("assign find_first_result = find_first_acc;"),
        "result drive missing:\n{v}"
    );
    assert!(
        v.contains("assign find_first_running = find_first_running_r;"),
        "running drive missing:\n{v}"
    );
}
