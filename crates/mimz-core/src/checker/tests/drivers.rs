use super::*;

// ---- Pass 8: drivers (E0501–E0505) ------------------------------------

#[test]
fn driving_a_signal_twice_is_e0501() {
    let d = first_err(
        "module M {\n  in a: bit\n  out y: bit\n  y = a\n  y = a\n}\n",
        "E0501",
    );
    assert!(d.msg.contains("more than one driver"));
}

#[test]
fn driving_a_wire_after_its_declaration_is_e0501() {
    let src = "module M {\n  in a: bit\n  out y: bit\n  wire w: bit = a\n  w = a\n  y = w\n}\n";
    let d = first_err(src, "E0501");
    assert!(d.msg.contains("declaration"));
}

#[test]
fn overlapping_slice_drives_are_e0501() {
    let src = "module M {\n  in a: bits[4]\n  out y: bits[8]\n  y[3:0] = a\n  y[4:1] = a\n}\n";
    first_err(src, "E0501");
}

#[test]
fn an_undriven_output_is_e0502() {
    let src = "module M {\n  in a: bit\n  out y: bit\n  out z: bit\n  y = a\n}\n";
    let d = first_err(src, "E0502");
    assert!(d.msg.contains('z') && d.msg.contains("never driven"));
}

#[test]
fn a_partially_driven_output_is_e0502_naming_the_bit() {
    let src = "module M {\n  in a: bits[4]\n  out y: bits[8]\n  y[3:0] = a\n  y[6:4] = a[2:0]\n}\n";
    let d = first_err(src, "E0502");
    assert!(d.msg.contains('7'), "the undriven bit is named");
}

#[test]
fn a_reg_assigned_in_two_on_blocks_is_e0503() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- 1\n  }\n  on rise(clk) {\n    v <- 0\n  }\n  y = v\n}\n";
    let d = first_err(src, "E0503");
    assert!(d.msg.contains("more than one"));
}

#[test]
fn a_reg_never_assigned_is_e0503() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  y = v\n}\n";
    let d = first_err(src, "E0503");
    assert!(d.msg.contains("never assigned"));
}

#[test]
fn a_self_referential_wire_is_e0504() {
    let src = "module M {\n  out y: bit\n  wire w: bit = w\n  y = w\n}\n";
    let d = first_err(src, "E0504");
    assert!(d.msg.contains("w -> w"));
}

#[test]
fn a_two_wire_cycle_is_e0504_showing_the_path() {
    let src = "module M {\n  out y: bit\n  wire a: bit = b\n  wire b: bit = a\n  y = a\n}\n";
    let d = first_err(src, "E0504");
    assert!(d.msg.contains("->"), "path shown");
    assert!(d.help.unwrap().contains("reg"), "teaches the fix");
}

#[test]
fn a_cycle_through_instances_is_e0504() {
    let src = "module Inv {\n  in d: bit\n  out q: bit\n  q = !d\n}\nmodule M {\n  out y: bit\n  let i1 = Inv() { d: i2.q }\n  let i2 = Inv() { d: i1.q }\n  y = i1.q\n}\n";
    let d = first_err(src, "E0504");
    assert!(d.msg.contains("i1.q") && d.msg.contains("i2.q"));
}

/// An earlier-declared instance forward-referencing a later-declared
/// instance's unknown output field must still get E0104, regardless of
/// declaration order (`collect_decls`: "declaration order in a module is
/// free"). Same forward-reference shape as `a_cycle_through_instances_is_e0504`,
/// but `i2` is declared and unambiguous — the only issue is the typo'd
/// field `i2.zzz`, which `i1` (declared first) reads before `i2`'s own
/// `check_inst` has run.
#[test]
fn forward_reference_to_unknown_output_field_is_e0104() {
    let src = "module Inv {\n  in d: bit\n  out q: bit\n  q = !d\n}\nmodule M {\n  out y: bit\n  let i1 = Inv() { d: i2.zzz }\n  let i2 = Inv() { d: 0 }\n  y = i1.q\n}\n";
    let d = first_err(src, "E0104");
    assert!(d.msg.contains("zzz"));
}

#[test]
fn arrow_assignment_to_a_wire_is_e0505() {
    let src = "module M {\n  clock clk\n  in a: bit\n  out y: bit\n  wire w: bit = a\n  on rise(clk) {\n    w <- a\n  }\n  y = w\n}\n";
    let d = first_err(src, "E0505");
    assert!(d.help.unwrap().contains("registers"));
}

#[test]
fn combinational_drive_of_a_reg_is_e0505() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- 1\n  }\n  v = 1\n  y = v\n}\n";
    let d = first_err(src, "E0505");
    assert!(d.help.unwrap().contains("<-"));
}

#[test]
fn disjoint_per_bit_drives_via_repeat_pass() {
    let src = "module M {\n  in data: bits[8]\n  out y: bits[8]\n  repeat i: 0..8 {\n    y[i] = data[i]\n  }\n}\n";
    check_one(src).expect("disjoint constant-index drives are one driver per bit");
}

#[test]
fn feedback_through_a_register_is_not_a_cycle() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  wire next: bit = !v\n  on rise(clk) {\n    v <- next\n  }\n  y = v\n}\n";
    check_one(src).expect("a loop broken by a reg is the normal shape of hardware");
}

#[test]
fn repeat_instance_array_ripple_carry_is_not_a_cycle() {
    let src = "module FA {\n  in a: bit\n  in cin: bit\n  out s: bit\n  out cout: bit\n  s = a ^ cin\n  cout = a & cin\n}\nmodule M {\n  in x: bits[4]\n  out y: bits[4]\n  out c: bit\n  wire seed: bit = 0\n  repeat i: 0..4 {\n    let fa[i] = FA() { a: x[i], cin: if i == 0 { seed } else { fa[i-1].cout } }\n    y[i] = fa[i].s\n  }\n  c = fa[3].cout\n}\n";
    check_one(src).expect("fa[1] -> fa[0] is a chain, not a loop — per-index nodes");
}

#[test]
fn defaultless_module_with_param_indexed_drives_is_not_e0501() {
    let src = "module C(W: int) {\n  in a: bits[W]\n  out y: bits[W]\n  y[W - 1] = a[0]\n  y[0] = a[1]\n}\nmodule M {\n  in x: bits[2]\n  out y: bits[2]\n  let c = C(W: 2) { a: x }\n  y = c.y\n}\n";
    check_one(src).expect("unevaluable extents never conflict (no false positives)");
}
