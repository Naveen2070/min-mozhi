use super::*;

// ---- instantiation completeness (E0302) -----------------------------------

const FA2: &str = "module FA {\n  in a: bit\n  in b: bit\n  out s: bit\n  s = a ^ b\n}\n";

#[test]
fn unconnected_input_is_e0302_naming_it() {
    let src = format!(
        "{FA2}module M {{\n  in x: bit\n  out y: bit\n  let u = FA() {{ a: x }}\n  y = u.s\n}}\n"
    );
    let d = first_err(&src, "E0302");
    assert!(d.msg.contains('b'), "names the missing input: {}", d.msg);
}

#[test]
fn several_unconnected_inputs_are_listed_in_one_error() {
    let src = format!("{FA2}module M {{\n  out y: bit\n  let u = FA() {{}}\n  y = u.s\n}}\n");
    let d = first_err(&src, "E0302");
    assert!(d.msg.contains('a') && d.msg.contains('b'));
}

#[test]
fn clock_and_reset_ports_may_be_omitted() {
    let src = "module Tick {\n  clock clk\n  reset rst\n  out q: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- !v\n  }\n  q = v\n}\nmodule M {\n  clock clk\n  reset rst\n  out y: bit\n  let u = Tick() {}\n  y = u.q\n}\n";
    check_one(src).expect("clock/reset connect implicitly by name — never E0302");
}

// ---- implicit clock/reset needs a same-named parent signal (E0304) --------

const TICK: &str = "module Tick {\n  clock clk\n  reset rst\n  out q: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- !v\n  }\n  q = v\n}\n";

#[test]
fn omitted_clock_with_no_parent_signal_is_e0304() {
    let src = format!(
        "{TICK}module M {{\n  clock sysclk\n  reset rst\n  out y: bit\n  let u = Tick() {{}}\n  y = u.q\n}}\n"
    );
    let d = first_err(&src, "E0304");
    assert!(
        d.msg.contains("`clk`") && d.msg.contains("`Tick`"),
        "names the clock and the child: {}",
        d.msg
    );
}

#[test]
fn omitted_reset_with_no_parent_signal_is_e0304() {
    let src = format!(
        "{TICK}module M {{\n  clock clk\n  reset nrst\n  out y: bit\n  let u = Tick() {{}}\n  y = u.q\n}}\n"
    );
    let d = first_err(&src, "E0304");
    assert!(d.msg.contains("`rst`"), "names the reset: {}", d.msg);
}

#[test]
fn omitted_extern_clock_with_no_parent_signal_is_e0304() {
    let src = "extern module Pll {\n  clock clk_in\n  out clk_out: bit\n}\nmodule Top {\n  clock sysclk\n  out o: bit\n  let u = Pll() {}\n  o = u.clk_out\n}\n";
    let d = first_err(src, "E0304");
    assert!(d.msg.contains("`clk_in`"), "names the clock: {}", d.msg);
}

#[test]
fn omitted_clock_named_like_a_parent_const_is_e0304() {
    let src = format!(
        "{TICK}module M {{\n  const clk: int = 1\n  clock sysclk\n  reset rst\n  out y: bit\n  let u = Tick() {{}}\n  y = u.q\n}}\n"
    );
    first_err(&src, "E0304");
}

#[test]
fn explicitly_connected_clock_needs_no_parent_namesake() {
    let src = format!(
        "{TICK}module M {{\n  clock sysclk\n  reset rst\n  out y: bit\n  let u = Tick() {{ clk: sysclk }}\n  y = u.q\n}}\n"
    );
    check_one(&src).expect("an explicit connection is never E0304");
}

#[test]
fn connecting_an_input_twice_is_e0302() {
    let src = format!(
        "{FA2}module M {{\n  in x: bit\n  out y: bit\n  let u = FA() {{ a: x, a: x, b: x }}\n  y = u.s\n}}\n"
    );
    let d = first_err(&src, "E0302");
    assert!(d.msg.contains("twice"));
}
