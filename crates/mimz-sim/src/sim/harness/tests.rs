use super::*;

#[test]
fn show_renders_a_wide_unsigned_value_in_decimal() {
    let v = Val::new_wide(crate::sim::wide::from_u128(u128::MAX, 200), 200, false);
    // u128::MAX == 340282366920938463463374607431768211455
    assert_eq!(show(v), "340282366920938463463374607431768211455");
}

#[test]
fn show_renders_a_wide_negative_signed_value_in_decimal() {
    let v = Val::new_wide(
        crate::sim::wide::neg(&crate::sim::wide::from_u128(1, 200), 200),
        200,
        true,
    );
    assert_eq!(show(v), "-1");
}

/// Minimal `EmulationHost` test double — only `led`/`speaker`/`uart_tx`
/// (Output) and `uart_rx` (Input) are known peripherals, mirroring the
/// real registry closely enough that the bind-validation tests below
/// exercise the same "unknown peripheral" / direction-mismatch paths.
struct NullHost;
impl EmulationHost for NullHost {
    fn bind(
        &mut self,
        _port: &str,
        peripheral: &str,
        _width: Width,
        _args: &[ast::BindArg],
        _speed_hz: Option<u64>,
    ) -> Result<(), String> {
        match peripheral {
            "led" | "speaker" | "uart_tx" | "uart_rx" => Ok(()),
            other => Err(format!("unknown peripheral `{other}`")),
        }
    }
    fn direction_of(&self, name: &str) -> Option<Direction> {
        match name {
            "led" | "speaker" | "uart_tx" => Some(Direction::Output),
            "uart_rx" => Some(Direction::Input),
            _ => None,
        }
    }
    fn on_change(&mut self, _name: &str, _val: &Val) {}
    fn on_tick(&mut self, _name: &str, _val: &Val) -> Result<(), String> {
        Ok(())
    }
    fn drive(&mut self, _name: &str) -> Option<u64> {
        None
    }
    fn frame(&mut self, _cycle: u64) -> Result<bool, String> {
        Ok(false)
    }
    fn finish(&mut self) -> Result<bool, String> {
        Ok(false)
    }
}

fn run_test_headless(files: &[ast::File], src: &str, decl: &TestDecl) -> Result<Outcome, String> {
    run_test(files, src, decl, Box::new(NullHost), false, false, true)
}

fn run(src: &str) -> Vec<Outcome> {
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
    f.items
        .iter()
        .filter_map(|i| match i {
            ast::TopItem::Test(t) => {
                Some(run_test_headless(std::slice::from_ref(&f), src, t).expect("runs"))
            }
            _ => None,
        })
        .collect()
}

const COUNTER: &str = "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  \
        out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
        on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";

fn passes(o: &Outcome) -> bool {
    matches!(o.result, TestResult::Pass)
}

#[test]
fn a_passing_test_counts_its_checks() {
    let src = format!(
        "{COUNTER}\ntest \"counts up\" for Counter(WIDTH: 4) {{\n  \
             rst = 1\n  tick(clk)\n  expect count == 0\n  \
             rst = 0\n  tick(clk)\n  expect count == 1\n  \
             tick(clk, 3)\n  expect count == 4\n}}\n"
    );
    let outs = run(&src);
    assert_eq!(outs.len(), 1);
    assert!(passes(&outs[0]));
    assert_eq!(outs[0].checks, 3);
}

#[test]
fn a_failing_expect_halts_with_a_teaching_message() {
    let src = format!(
        "{COUNTER}\ntest \"wrong\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  expect count == 9\n}}\n"
    );
    let outs = run(&src);
    match &outs[0].result {
        TestResult::Fail(m) => {
            assert!(m.contains("count == 9"), "no expression in: {m}");
            assert!(m.contains("left"), "no operand values in: {m}");
            assert!(m.contains("= 1"), "expected actual count=1 in: {m}");
        }
        TestResult::Pass => panic!("should have failed"),
        TestResult::Skipped(reason) => panic!("should have failed, was skipped: {reason}"),
    }
    // The check that failed is still counted.
    assert_eq!(outs[0].checks, 1);
}

#[test]
fn drive_then_tick_feeds_an_input() {
    let src = "module Acc {\n  clock clk\n  reset rst\n  in x: bits[8]\n  out y: bits[8]\n  \
             reg r: bits[8] = 0\n  on rise(clk) { r <- r +% x }\n  y = r\n}\n\
             test \"adds x\" for Acc {\n  rst = 0\n  x = 7\n  tick(clk)\n  expect y == 7\n  \
             tick(clk)\n  expect y == 14\n}\n";
    let outs = run(src);
    assert!(passes(&outs[0]));
}

#[test]
fn sync_double_flop_settles_after_two_dst_clock_cycles() {
    // `fast_reg` lives in `clk_src`'s own domain (via its `on rise`
    // block) rather than a bare domain-free `in` port — closer to real
    // CDC usage; `double_flop` also permits a domain-free signal, but
    // this exercises the more realistic path.
    let src = "module M {\n\
                 clock clk_src\n\
                 clock clk_dst\n\
                 reset rst\n\
                 in fast_bit: bit\n\
                 reg fast_reg: bit = 0\n\
                 reg slow_bit: bit = 0\n\
                 out o: bit\n\
                 on rise(clk_src) {\n\
                     fast_reg <- fast_bit\n\
                 }\n\
                 on rise(clk_dst) {\n\
                     slow_bit <- sync.double_flop(fast_reg, clk_src, clk_dst)\n\
                 }\n\
                 o = slow_bit\n\
               }\n\
               test \"crosses after 2 dst cycles\" for M {\n\
                 rst = 0\n\
                 fast_bit = 1\n\
                 tick(clk_src)\n\
                 expect o == 0\n\
                 tick(clk_dst)\n\
                 expect o == 0\n\
                 tick(clk_dst)\n\
                 expect o == 1\n\
               }\n";
    let outs = run(src);
    assert!(passes(&outs[0]), "{:?}", outs[0]);
}

#[test]
fn sync_pulse_produces_a_one_cycle_dst_pulse_after_toggle() {
    // Smoke test for the `Wire`-rewrite path `expand_sync_prims` takes
    // for `sync.pulse` (distinct from `double_flop`'s Reg/On-only path,
    // see Task 5's own emitter fixture needing a second dedicated test
    // for exactly this reason). `fast_reg` is driven high for exactly
    // one `clk_src` cycle, which flips the hidden toggle reg once; the
    // 3-stage `clk_dst` synchronizer then reproduces that as a single
    // one-cycle-wide pulse on `o`, two `clk_dst` edges after the toggle.
    let src = "module M {\n\
                 clock clk_src\n\
                 clock clk_dst\n\
                 reset rst\n\
                 in trigger: bit\n\
                 reg fast_reg: bit = 0\n\
                 on rise(clk_src) {\n\
                     fast_reg <- trigger\n\
                 }\n\
                 wire dst_pulse: bit = sync.pulse(fast_reg, clk_src, clk_dst)\n\
                 out o: bit\n\
                 o = dst_pulse\n\
               }\n\
               test \"one-cycle pulse two dst edges after the toggle\" for M {\n\
                 rst = 0\n\
                 trigger = 1\n\
                 tick(clk_src)\n\
                 trigger = 0\n\
                 tick(clk_src)\n\
                 tick(clk_dst)\n\
                 expect o == 0\n\
                 tick(clk_dst)\n\
                 expect o == 1\n\
                 tick(clk_dst)\n\
                 expect o == 0\n\
               }\n";
    let outs = run(src);
    assert!(passes(&outs[0]), "{:?}", outs[0]);
}

#[test]
fn qq_unwrap_form_evaluates_in_a_test_block() {
    // `raw ?? 0` unwraps a `bits[8]?` to its `data` when `valid`, else the
    // fallback — purely combinational, so no clock/tick is needed; `expect`
    // settles `safe` on demand from the driven inputs (see `Sim::peek`).
    let src = "module M {\n  in c: bit\n  in d: bits[8]\n  out safe: bits[8]\n  \
             wire raw: bits[8]? = { valid: c, data: d }\n  \
             safe = raw ?? 0\n}\n\
             test \"unwrap\" for M {\n  c = 1\n  d = 5\n  expect safe == 5\n}\n\
             test \"unwrap-invalid\" for M {\n  c = 0\n  d = 5\n  expect safe == 0\n}\n";
    let outs = run(src);
    assert_eq!(outs.len(), 2);
    assert!(passes(&outs[0]), "unwrap: {:?}", outs[0].result);
    assert!(passes(&outs[1]), "unwrap-invalid: {:?}", outs[1].result);
}

#[test]
fn qq_or_mux_form_evaluates_at_wire_init() {
    // `x ?? y` where both operands (and the result) stay `bits[8]?` —
    // per-field mux: `merged.valid = x.valid || y.valid`,
    // `merged.data = x.valid ? x.data : y.data`. Wire-init call site
    // (`ModuleItem::Wire`'s `bundle_field_expr` extraction).
    let src = "module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
             wire x: bits[8]? = { valid: c1, data: d1 }\n  \
             wire y: bits[8]? = { valid: c2, data: d2 }\n  \
             wire merged: bits[8]? = x ?? y\n}\n\
             test \"lhs-valid\" for M {\n  c1 = 1\n  d1 = 5\n  c2 = 1\n  d2 = 9\n  \
             expect merged_valid == 1\n  expect merged_data == 5\n}\n\
             test \"lhs-invalid-falls-to-rhs\" for M {\n  c1 = 0\n  d1 = 5\n  c2 = 1\n  d2 = 9\n  \
             expect merged_valid == 1\n  expect merged_data == 9\n}\n";
    let outs = run(src);
    assert_eq!(outs.len(), 2);
    assert!(passes(&outs[0]), "lhs-valid: {:?}", outs[0].result);
    assert!(
        passes(&outs[1]),
        "lhs-invalid-falls-to-rhs: {:?}",
        outs[1].result
    );
}

#[test]
fn qq_or_mux_form_evaluates_via_drive() {
    // Same OR-mux semantics, but through a Drive statement onto a
    // bundle-typed `out` port (`ModuleItem::Drive`'s bundle-drive arm)
    // instead of a wire-init.
    let src = "module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
             out merged: bits[8]?\n  \
             wire x: bits[8]? = { valid: c1, data: d1 }\n  \
             wire y: bits[8]? = { valid: c2, data: d2 }\n  \
             merged = x ?? y\n}\n\
             test \"lhs-valid\" for M {\n  c1 = 1\n  d1 = 5\n  c2 = 1\n  d2 = 9\n  \
             expect merged_valid == 1\n  expect merged_data == 5\n}\n\
             test \"both-invalid\" for M {\n  c1 = 0\n  d1 = 5\n  c2 = 0\n  d2 = 9\n  \
             expect merged_valid == 0\n}\n";
    let outs = run(src);
    assert_eq!(outs.len(), 2);
    assert!(passes(&outs[0]), "lhs-valid: {:?}", outs[0].result);
    assert!(passes(&outs[1]), "both-invalid: {:?}", outs[1].result);
}

#[test]
fn qq_or_mux_chain_evaluates_correctly() {
    // `x ?? y ?? z` (left-associative: `Coalesce(Coalesce(x,y), z)`).
    // `x`/`y` invalid, `z` valid: nested extraction must reach `z`'s
    // fields, not mis-render the nested `Coalesce(x,y)` sub-expression
    // as a plain signal (the bug class fixed in Task 8's emitter-side
    // review — this is its simulator-side regression test).
    let src = "module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
             in c3: bit\n  in d3: bits[8]\n  \
             wire x: bits[8]? = { valid: c1, data: d1 }\n  \
             wire y: bits[8]? = { valid: c2, data: d2 }\n  \
             wire z: bits[8]? = { valid: c3, data: d3 }\n  \
             wire merged: bits[8]? = x ?? y ?? z\n}\n\
             test \"falls-through-to-z\" for M {\n  c1 = 0\n  d1 = 1\n  c2 = 0\n  d2 = 2\n  \
             c3 = 1\n  d3 = 3\n  expect merged_valid == 1\n  expect merged_data == 3\n}\n\
             test \"middle-valid\" for M {\n  c1 = 0\n  d1 = 1\n  c2 = 1\n  d2 = 2\n  \
             c3 = 1\n  d3 = 3\n  expect merged_valid == 1\n  expect merged_data == 2\n}\n";
    let outs = run(src);
    assert_eq!(outs.len(), 2);
    assert!(passes(&outs[0]), "falls-through-to-z: {:?}", outs[0].result);
    assert!(passes(&outs[1]), "middle-valid: {:?}", outs[1].result);
}

#[test]
fn a_test_if_branches_on_state() {
    // `if` takes the true branch; the false branch's bogus expect never runs.
    let src = format!(
        "{COUNTER}\ntest \"branch\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  \
             if count == 1 {{ expect count == 1 }} else {{ expect count == 99 }}\n}}\n"
    );
    let outs = run(&src);
    assert!(passes(&outs[0]));
    assert_eq!(outs[0].checks, 1);
}

#[test]
fn an_unknown_clock_is_an_error() {
    let src = format!("{COUNTER}\ntest \"bad clock\" for Counter(WIDTH: 4) {{\n  tick(nope)\n}}\n");
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(&src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let err = run_test_headless(std::slice::from_ref(&f), &src, decl).unwrap_err();
    assert!(err.contains("not a clock"), "got: {err}");
}

#[test]
fn the_timeline_has_a_frame_per_tick() {
    let src = format!(
        "{COUNTER}\ntest \"frames\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
    );
    let outs = run(&src);
    // 1 initial frame + 3 ticks.
    assert_eq!(outs[0].timeline.frames.len(), 4);
    assert_eq!(outs[0].default_scope, vec!["count", "value"]);
}

#[test]
fn trace_false_skips_every_capture() {
    // A caller that never reads `Outcome.timeline` (e.g. `mimz test`
    // without `--trace`) shouldn't pay for a full-signal snapshot on
    // every simulated cycle.
    let src = format!(
        "{COUNTER}\ntest \"notrace\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
    );
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(&src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let outcome = run_test(
        std::slice::from_ref(&f),
        &src,
        decl,
        Box::new(NullHost),
        false,
        false,
        false,
    )
    .expect("runs");
    assert!(passes(&outcome));
    assert_eq!(
        outcome.timeline.frames.len(),
        0,
        "trace: false must skip every capture(), including the initial one"
    );
}

#[test]
fn sim_block_with_unknown_peripheral_errors() {
    let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\n\
                    test \"t\" for M {\n  sim {\n    bind playing -> microphone()\n  }\n  tick(clk)\n}\n";
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
    assert!(err.contains("unknown peripheral"), "got: {err}");
}

#[test]
fn sim_block_with_unknown_port_errors() {
    let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\n\
                    test \"t\" for M {\n  sim {\n    bind nope -> led()\n  }\n  tick(clk)\n}\n";
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
    assert!(err.contains("nope"), "got: {err}");
}

#[test]
fn sim_block_binding_an_input_to_an_output_peripheral_errors() {
    let src = "module M {\n  clock clk\n  in start: bit\n  out playing: bit\n  playing = start\n}\n\
                    test \"t\" for M {\n  sim {\n    bind start -> led()\n  }\n  tick(clk)\n}\n";
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
    // `start` genuinely exists as an input — this must produce the
    // direction-aware message, not the generic "no such port" one
    // (which would also happen to contain "output port" and "start",
    // so asserting on the specific phrase is what proves the
    // mismatch was actually detected, not coincidental).
    assert!(err.contains("binds to an output port, but"), "got: {err}");
}

#[test]
fn sim_block_binding_an_output_to_an_input_peripheral_errors() {
    let src = "module M {\n  clock clk\n  in start: bit\n  out playing: bit\n  playing = start\n}\n\
                    test \"t\" for M {\n  sim {\n    bind playing -> uart_rx()\n  }\n  tick(clk)\n}\n";
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
    // Mirror of the test above: `playing` genuinely exists as an output
    // — this must produce the direction-aware message, not the generic
    // "no such port" one.
    assert!(err.contains("binds to an input port, but"), "got: {err}");
}

#[test]
fn sim_block_with_speaker_bound_runs_fine_without_emulate() {
    let src = "module M {\n  clock clk\n  in start: bit\n  out tone: bit\n  tone = start\n}\n\
                    test \"t\" for M {\n  start = 1\n  sim {\n    speed mhz(1)\n    bind tone -> speaker()\n  }\n  tick(clk, 4)\n}\n";
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    // `live: false` (second-to-last arg) — `on_tick` never runs in this
    // mode, so `speaker`'s real audio device is never touched even
    // though it's bound. This is the proof that a headless/CI run is
    // safe.
    run_test_headless(std::slice::from_ref(&f), src, decl)
        .expect("test passes without touching audio hardware");
}

#[test]
fn batch_sizes_splits_evenly() {
    assert_eq!(batch_sizes(100, 30), vec![30, 30, 30, 10]);
    assert_eq!(batch_sizes(0, 30), Vec::<u64>::new());
    assert_eq!(batch_sizes(5, 30), vec![5]);
}

#[test]
fn cycles_per_frame_floors_to_one() {
    assert_eq!(cycles_per_frame(50_000_000), 50_000_000 / 30);
    assert_eq!(cycles_per_frame(10), 1); // sub-fps speed never batches to 0
}

#[test]
fn tick_without_sim_block_is_unaffected() {
    // A test with no `sim` block must behave exactly as before this
    // feature existed — same Outcome shape, same cycle count.
    let src = format!(
        "{COUNTER}\ntest \"counts\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
    );
    let outs = run(&src);
    assert!(passes(&outs[0]));
    assert_eq!(outs[0].checks, 1);
}

#[test]
fn has_sim_block_only_true_when_a_sim_block_is_present() {
    // A body with no `sim` block (the common case — most tests never
    // touch emulation) must not trigger the degrade note.
    let no_sim = format!(
        "{COUNTER}\ntest \"t\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  expect count == 1\n}}\n"
    );
    assert!(!has_sim_block(&test_body(&no_sim)));

    // A top-level `sim` block is detected.
    let with_sim = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\n\
                         test \"t\" for M {\n  sim {\n    bind playing -> led()\n  }\n  tick(clk)\n}\n";
    assert!(has_sim_block(&test_body(with_sim)));

    // A `sim` block nested inside an `if`/`else` branch is also detected
    // (the grammar allows it — `if`'s then/else reuse `test_block`).
    let nested = format!(
        "{COUNTER}\ntest \"t\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  \
             if count == 1 {{ expect count == 1 }} else {{ sim {{ bind count -> led() }} }}\n}}\n"
    );
    assert!(has_sim_block(&test_body(&nested)));
}

fn test_body(src: &str) -> Vec<TestStmt> {
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
    f.items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t.body.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn live_true_without_a_dashboard_still_passes() {
    // Proves passing `live: true` never breaks a headless test run even
    // with a no-op host — the CI-safety property `run_test` is supposed
    // to guarantee now that dashboard interactivity lives entirely in
    // the caller's `EmulationHost` impl.
    let src = format!(
        "{COUNTER}\ntest \"counts\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
    );
    let f = mimz_core::parser::parse(mimz_core::lexer::lex(&src).expect("lexes")).expect("parses");
    let decl = f
        .items
        .iter()
        .find_map(|i| match i {
            ast::TopItem::Test(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let outcome = run_test(
        std::slice::from_ref(&f),
        &src,
        decl,
        Box::new(NullHost),
        true,
        false,
        true,
    )
    .expect("runs");
    assert!(passes(&outcome));
}
