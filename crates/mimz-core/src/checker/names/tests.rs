use crate::{checker::check, diag::Diag, lexer, parser};

/// Parse + run the full checker; panics if it doesn't parse (this file's
/// other checker tests live in `checker::tests`, which does the same
/// via its own private `parse`/`errs` helpers — this test lives here
/// instead, self-contained, so this commit touches only `names.rs`).
fn diags_for(src: &str) -> Vec<Diag> {
    let toks = lexer::lex(src).expect("lexes");
    let file = parser::parse(toks).expect("parses");
    check(&[file]).expect_err("expected checker errors")
}

#[test]
fn sync_loop_generated_name_collision_is_e0003() {
    let src = "module M {\n  clock clk\n  in find_first_start: bit\n  sync loop find_first on rise(clk) (i: 0..4) -> result: bit = 0 {\n    result <- 1\n  }\n}\n";
    let diags = diags_for(src);
    assert!(diags.iter().any(|d| d.code == Some("E0003")));
}

#[test]
fn sync_double_flop_generated_name_collision_is_e0003() {
    // `sync.double_flop`'s hidden stage reg is named off its own `<-`
    // target (`slow_bit` -> `__sync_slow_bit_stage0`), deterministically
    // — no counter involved, so this collision is reproducible on every
    // run (unlike a global-atomic-counter scheme would be).
    let src = "module M {\n\
                 clock clk_src\n\
                 clock clk_dst\n\
                 in fast_bit: bit\n\
                 reg slow_bit: bit = 0\n\
                 reg __sync_slow_bit_stage0: bit = 0\n\
                 reset rst\n\
                 on rise(clk_dst) {\n\
                     slow_bit <- sync.double_flop(fast_bit, clk_src, clk_dst)\n\
                 }\n\
               }";
    let diags = diags_for(src);
    assert!(diags.iter().any(|d| d.code == Some("E0003")), "{diags:?}");
}

#[test]
fn sync_pulse_generated_name_collision_is_e0003() {
    // Same, off the wire's own name (`dst_pulse` -> `__sync_dst_pulse_toggle`).
    let src = "module M {\n\
                 clock clk_src\n\
                 clock clk_dst\n\
                 in src_pulse: bit\n\
                 reg __sync_dst_pulse_toggle: bit = 0\n\
                 reset rst\n\
                 wire dst_pulse: bit = sync.pulse(src_pulse, clk_src, clk_dst)\n\
                 out o: bit\n\
                 o = dst_pulse\n\
               }";
    let diags = diags_for(src);
    assert!(diags.iter().any(|d| d.code == Some("E0003")), "{diags:?}");
}
