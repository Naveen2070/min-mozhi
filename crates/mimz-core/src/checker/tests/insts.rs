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
    let src = "module Tick {\n  clock clk\n  reset rst\n  out q: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- !v\n  }\n  q = v\n}\nmodule M {\n  out y: bit\n  let u = Tick() {}\n  y = u.q\n}\n";
    check_one(src).expect("clock/reset connect implicitly by name — never E0302");
}

#[test]
fn connecting_an_input_twice_is_e0302() {
    let src = format!(
        "{FA2}module M {{\n  in x: bit\n  out y: bit\n  let u = FA() {{ a: x, a: x, b: x }}\n  y = u.s\n}}\n"
    );
    let d = first_err(&src, "E0302");
    assert!(d.msg.contains("twice"));
}
