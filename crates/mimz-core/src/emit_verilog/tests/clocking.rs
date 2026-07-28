use super::*;

#[test]
fn on_fall_emits_negedge() {
    let v = emit_src(
        "module M {\n  clock clk\n  reset rst\n  out q: bit\n  reg r: bit = 0\n  on fall(clk) { r <- !r }\n  q = r\n}\n",
    );
    assert!(
        v.contains("always @(negedge clk)"),
        "`on fall` must lower to a negedge block:\n{v}"
    );
}

#[test]
fn async_reset_widens_the_sensitivity_list() {
    let v = emit_src(
        "module M {\n  clock clk\n  async reset rst\n  out q: bit\n  reg r: bit = 0\n  on rise(clk) { r <- !r }\n  q = r\n}\n",
    );
    assert!(
        v.contains("always @(posedge clk or posedge rst)"),
        "`async reset` must add `or posedge rst` to the sensitivity list:\n{v}"
    );
}

#[test]
fn a_sync_reset_stays_clock_only() {
    let v = emit_src(
        "module M {\n  clock clk\n  reset rst\n  out q: bit\n  reg r: bit = 0\n  on rise(clk) { r <- !r }\n  q = r\n}\n",
    );
    assert!(
        v.contains("always @(posedge clk) begin"),
        "a plain `reset` must NOT widen the sensitivity list:\n{v}"
    );
}
