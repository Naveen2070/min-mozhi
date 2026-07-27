use super::*;

// ---- clock-domain ownership (E0701) ---------------------------------------

#[test]
fn two_clocks_with_separate_logic_pass() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- a\n  }\n  ya = ra\n  yb = rb\n}\n";
    check_one(src).expect("independent domains never touch — clean");
}

#[test]
fn reading_another_domains_reg_is_e0701() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- ra\n  }\n  ya = ra\n  yb = rb\n}\n";
    let d = first_err(src, "E0701");
    assert!(d.msg.contains("cka") && d.msg.contains("ckb"));
    assert!(d.help.unwrap().contains("sync"));
}

#[test]
fn cross_domain_through_a_wire_is_e0701() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  wire w: bit = !ra\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- w\n  }\n  ya = ra\n  yb = rb\n}\n";
    let d = first_err(src, "E0701");
    assert!(
        d.msg.contains("cka"),
        "the wire carries cka's domain: {}",
        d.msg
    );
}

#[test]
fn a_wire_mixing_two_domains_is_e0701() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out y: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  wire w: bit = ra ^ rb\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- a\n  }\n  y = w\n}\n";
    let d = first_err(src, "E0701");
    assert!(d.msg.contains("mixes"));
}

#[test]
fn same_domain_logic_under_two_declared_clocks_passes() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out y: bit\n  reg r1: bit = 0\n  reg r2: bit = 0\n  wire w: bit = r1 ^ r2\n  on rise(cka) {\n    r1 <- a\n    r2 <- w\n  }\n  y = w\n}\n";
    check_one(src).expect("everything lives on cka — the unused ckb changes nothing");
}

// ---- `sync.double_flop`/`sync.pulse` arg-shape (E0702/E0703) --------------

#[test]
fn sync_double_flop_with_non_clock_second_arg_is_e0702() {
    let src = "module M {\n  clock clk_dst\n  in fast_bit: bit\n  in not_a_clock: bit\n  reg slow_bit: bit = 0\n  reset rst\n  on rise(clk_dst) {\n    slow_bit <- sync.double_flop(fast_bit, not_a_clock, clk_dst)\n  }\n}\n";
    let d = first_err(src, "E0702");
    assert!(d.msg.contains("clock"));
}

#[test]
fn sync_double_flop_with_matching_src_and_dst_clock_is_e0702() {
    let src = "module M {\n  clock clk\n  in fast_bit: bit\n  reg slow_bit: bit = 0\n  reset rst\n  on rise(clk) {\n    slow_bit <- sync.double_flop(fast_bit, clk, clk)\n  }\n}\n";
    let d = first_err(src, "E0702");
    assert!(d.msg.contains("clk"));
}

#[test]
fn sync_double_flop_with_a_2_bit_signal_is_e0703() {
    let src = "module M {\n  clock clk_src\n  clock clk_dst\n  in fast_bits: bits[2]\n  reg slow_bits: bits[2] = 0\n  reset rst\n  on rise(clk_dst) {\n    slow_bits <- sync.double_flop(fast_bits, clk_src, clk_dst)\n  }\n}\n";
    let d = first_err(src, "E0703");
    assert!(d.msg.contains("1 bit"));
}

// ---- `sync.double_flop`/`sync.pulse` domain + placement (E0704/E0705) ----

#[test]
fn sync_double_flop_signal_from_a_third_unrelated_clock_is_e0704() {
    let src = "module M {\n  clock clk_a\n  clock clk_b\n  clock clk_dst\n  reg reg_a: bit = 0\n  reset rst\n  reg slow_bit: bit = 0\n  on rise(clk_a) {\n    reg_a <- 1\n  }\n  on rise(clk_dst) {\n    slow_bit <- sync.double_flop(reg_a, clk_b, clk_dst)\n  }\n}\n";
    assert!(any_code(src, "E0704"), "{:?}", errs(src));
}

#[test]
fn sync_pulse_signal_that_is_domain_free_is_e0704() {
    // `pulse` requires the source signal to ALREADY be synchronous to
    // `src_clock` (it samples it directly in the toggle register) — unlike
    // `double_flop`, a domain-free (e.g. plain input) signal is rejected.
    let src = "module M {\n  clock clk_src\n  clock clk_dst\n  in src_pulse: bit\n  wire dst_pulse: bit = sync.pulse(src_pulse, clk_src, clk_dst)\n  out o: bit\n  o = dst_pulse\n}\n";
    assert!(any_code(src, "E0704"), "{:?}", errs(src));
}

#[test]
fn sync_double_flop_used_outside_its_own_on_block_clock_is_e0705() {
    // `dst_clock` argument (`clk_other`) disagrees with the enclosing
    // `on rise(clk_dst)` block's own clock.
    let src = "module M {\n  clock clk_src\n  clock clk_dst\n  clock clk_other\n  in fast_bit: bit\n  reg slow_bit: bit = 0\n  reset rst\n  on rise(clk_dst) {\n    slow_bit <- sync.double_flop(fast_bit, clk_src, clk_other)\n  }\n}\n";
    assert!(any_code(src, "E0705"), "{:?}", errs(src));
}

#[test]
fn sync_pulse_used_as_a_reg_source_is_e0705() {
    // Placement violation: `pulse`'s result is combinational — legal only
    // as a `wire`'s direct initializer, never a `reg`'s `<-` source.
    let src = "module M {\n  clock clk_src\n  clock clk_dst\n  reg src_reg: bit = 0\n  reset rst\n  reg dst_reg: bit = 0\n  on rise(clk_src) {\n    src_reg <- 1\n  }\n  on rise(clk_dst) {\n    dst_reg <- sync.pulse(src_reg, clk_src, clk_dst)\n  }\n}\n";
    assert!(any_code(src, "E0705"), "{:?}", errs(src));
}

#[test]
fn sync_double_flop_hidden_in_a_reg_reset_value_is_e0705() {
    // Placement violation via a position `collect_all_sync_prim_calls`
    // must scan even though it isn't one of the two legal spots: a `reg`'s
    // reset value is an ordinary `Expr` (checked the same way as a wire's
    // init by `widths/mod.rs::walk_width_items`), so a `sync.*` call can be
    // written there syntactically and pass Task 2's arg-shape checks
    // clean (real, distinct clocks; 1-bit domain-free signal) — but it is
    // still neither an `on`-block `<-` RHS nor a `wire` initializer, so it
    // must be caught as a placement violation, not silently accepted.
    let src = "module M {\n  clock clk_a\n  clock clk_b\n  in x: bit\n  reset rst\n  reg r: bit = sync.double_flop(x, clk_a, clk_b)\n  on rise(clk_a) {\n    r <- x\n  }\n}\n";
    assert!(any_code(src, "E0705"), "{:?}", errs(src));
}

#[test]
fn sync_double_flop_hidden_in_a_sync_loop_body_is_e0705() {
    // Placement violation via a position `collect_all_sync_prim_calls`
    // must scan: `SyncLoop::body` (`Vec<SeqStmt>`) is walked directly, not
    // skipped as a no-op — `ast::sync_loop_lower`'s desugaring never
    // re-runs E0705 placement detection over calls inside the loop body,
    // so without this scan a `sync.*` call written there would pass the
    // checker clean and then panic the emitter/simulator's
    // `unreachable!()` guard instead of being rejected here.
    let src = "module M {\n  clock clk_a\n  clock clk_b\n  in x: bit\n  reset rst\n  reg r: bit = 0\n  on rise(clk_a) {\n    r <- x\n  }\n  sync loop s on rise(clk_b) (i: 0..2) -> result: bit = 0 {\n    result <- sync.double_flop(r, clk_a, clk_b)\n  }\n}\n";
    assert!(any_code(src, "E0705"), "{:?}", errs(src));
}
