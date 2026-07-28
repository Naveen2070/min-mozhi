use super::*;

#[test]
fn module_const_folds_in_widths_and_emits_no_hardware() {
    let v = emit_src(
        "module M {\n  const N: int = 3\n  out y: bits[N]\n  repeat i: 0..N {\n    y[i] = 0\n  }\n}\n",
    );
    assert!(
        v.contains("[(3)-1:0] y"),
        "const N folds to 3 in the port width"
    );
    assert!(v.contains("assign y[2] = 0;"), "0..N runs to N-1");
    assert!(
        !v.contains("[(N)"),
        "the symbolic const name must not survive into widths"
    );
}

#[test]
fn tamil_identifiers_emit_as_romanized_verilog() {
    // Identifiers only — பதிவேடு etc. are KEYWORD spellings
    // (keywords.toml) and can never be identifiers.
    let v = emit_src_translit(
        "module விளக்கு {\n  clock மணி\n  reset மீள்\n  out ஒளி: bit\n  reg சுடர்: bit = 0\n  on rise(மணி) {\n    சுடர் <- !சுடர்\n  }\n  ஒளி = சுடர்\n}\n",
    );
    assert!(
        v.contains("module villakku ("),
        "module name romanizes:\n{v}"
    );
    assert!(v.contains("input wire manni"), "clock romanizes");
    assert!(v.contains("output wire olli"), "output romanizes");
    assert!(v.contains("reg sutar;"), "reg romanizes:\n{v}");
    assert!(
        v.contains("always @(posedge manni)"),
        "the on-block clock uses the SAME romanization"
    );
    // Only the generator banner COMMENT may carry Tamil; every line
    // of actual Verilog must be pure ASCII.
    for line in v.lines().filter(|l| !l.starts_with("//")) {
        assert!(line.is_ascii(), "non-ASCII outside a comment: {line}");
    }
}

#[test]
fn colliding_romanizations_get_suffixes_and_ascii_names_are_safe() {
    // ந and ன both romanize to `n`; the user also owns plain ASCII
    // `nii` — first-seen Tamil name takes `nii_2`, the second `nii_3`.
    let v = emit_src_translit(
        "module M {\n  in nii: bit\n  in நீ: bit\n  in னீ: bit\n  out y: bit\n  y = nii ^ நீ ^ னீ\n}\n",
    );
    assert!(v.contains("input wire nii,"), "the ASCII name is untouched");
    assert!(v.contains("nii_2"), "first Tamil clash gets _2:\n{v}");
    assert!(v.contains("nii_3"), "second Tamil clash gets _3:\n{v}");
}
