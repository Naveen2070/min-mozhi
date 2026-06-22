//! Identifier transliteration: Tamil-script names (legal Min-Mozhi) become
//! readable ASCII Verilog names. Runs as an AST pre-pass — `transliterate`
//! rewrites every identifier IN PLACE before `Project::from_files`, so the
//! emitter itself never sees a non-ASCII name (its `check_ascii` stays as
//! the backstop for direct API users who skip the pre-pass).
//!
//! Scheme (decision 2026-06-12, spec/02 section 2 note):
//! - ASCII names pass through untouched.
//! - Tamil block (U+0B80–U+0BFF) romanizes via a pragmatic ASCII table
//!   (ISO-15919-flavored, diacritics folded): consonants carry the
//!   inherent `a` unless a vowel sign or virama follows — விளக்கு →
//!   `villakku`, நிலை → `nilai`.
//! - Any other non-ASCII char becomes `_uXXXX` (uppercase hex).
//! - Deterministic: the same source name always produces the same output.
//!   Collisions (two source names romanizing identically, or a romanized
//!   name landing on an existing ASCII name or a Verilog keyword) get
//!   `_2`, `_3`, … in first-seen order — which is source order, so the
//!   four flavor folders still emit byte-identical Verilog.

use std::collections::{HashMap, HashSet};

use crate::ast::*;

/// Verilog words a romanized name must never land on. (ASCII user names
/// pass through unchecked — same behavior as before this pass existed.)
const VERILOG_RESERVED: &[&str] = &[
    "always",
    "assign",
    "begin",
    "case",
    "default",
    "else",
    "end",
    "endcase",
    "endmodule",
    "for",
    "if",
    "initial",
    "inout",
    "input",
    "integer",
    "localparam",
    "module",
    "negedge",
    "output",
    "parameter",
    "posedge",
    "reg",
    "signed",
    "wire",
];

/// Rewrite every identifier in `files` to ASCII, in place. Call after the
/// checker (names are validated against the ORIGINAL spelling) and before
/// `Project::from_files` (the symbol table must see the final names).
pub fn transliterate(files: &mut [File]) {
    // Pass 1: every ASCII name claims its spelling, so a romanization can
    // never collide with a name the user already wrote.
    let mut used: HashSet<String> = VERILOG_RESERVED.iter().map(|s| s.to_string()).collect();
    for f in files.iter_mut() {
        for_each_name(f, &mut |name| {
            if name.is_ascii() {
                used.insert(name.clone());
            }
        });
    }
    // Pass 2: rewrite non-ASCII names through one shared map (the same
    // source spelling maps identically everywhere, across files too).
    let mut map: HashMap<String, String> = HashMap::new();
    for f in files.iter_mut() {
        for_each_name(f, &mut |name| {
            if name.is_ascii() {
                return;
            }
            if let Some(out) = map.get(name) {
                *name = out.clone();
                return;
            }
            let base = romanize(name);
            let mut candidate = base.clone();
            let mut n = 2;
            while used.contains(&candidate) {
                candidate = format!("{base}_{n}");
                n += 1;
            }
            used.insert(candidate.clone());
            map.insert(name.clone(), candidate.clone());
            *name = candidate;
        });
    }
}

/// One name, romanized — pure and total (any string in, valid ASCII
/// identifier out). No uniquing here; `transliterate` owns that.
///
/// `pub(crate)` so `mimz translate --romanize-names` can reuse the exact same
/// scheme to convert Tamil identifiers to readable Latin in source (the result
/// then transliterates to the SAME Verilog — see `tests/translate.rs`).
pub(crate) fn romanize(name: &str) -> String {
    let mut out = String::new();
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii() {
            out.push(c);
        } else if let Some(base) = consonant(c) {
            out.push_str(base);
            match chars.peek().copied() {
                Some(m) if m == VIRAMA => {
                    chars.next(); // bare consonant — no vowel
                }
                Some(m) if matra(m).is_some() => {
                    out.push_str(matra(m).unwrap());
                    chars.next();
                }
                _ => out.push('a'), // the inherent vowel
            }
        } else if let Some(v) = vowel(c) {
            out.push_str(v);
        } else {
            out.push_str(&format!("_u{:04X}", c as u32));
        }
    }
    match out.chars().next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => out,
        Some(_) => format!("_{out}"),
        None => "_u0".to_string(),
    }
}

const VIRAMA: char = '\u{0BCD}'; // ் — strips the inherent `a`

/// Tamil consonants (and the grantha letters), WITHOUT the inherent vowel.
/// ந and ன both map to `n` — a real distinction ISO 15919 makes with
/// diacritics ASCII cannot carry; the collision counter disambiguates the
/// rare clash.
fn consonant(c: char) -> Option<&'static str> {
    Some(match c {
        'க' => "k",
        'ங' => "ng",
        'ச' => "s",
        'ஞ' => "nj",
        'ட' => "t",
        'ண' => "nn",
        'த' => "th",
        'ந' | 'ன' => "n",
        'ப' => "p",
        'ம' => "m",
        'ய' => "y",
        'ர' => "r",
        'ல' => "l",
        'வ' => "v",
        'ழ' => "zh",
        'ள' => "ll",
        'ற' => "rr",
        'ஜ' => "j",
        'ஶ' | 'ஷ' => "sh",
        'ஸ' => "ss",
        'ஹ' => "h",
        _ => return None,
    })
}

/// Independent vowels and ஃ (aytham).
fn vowel(c: char) -> Option<&'static str> {
    Some(match c {
        'அ' => "a",
        'ஆ' => "aa",
        'இ' => "i",
        'ஈ' => "ii",
        'உ' => "u",
        'ஊ' => "uu",
        'எ' => "e",
        'ஏ' => "ee",
        'ஐ' => "ai",
        'ஒ' => "o",
        'ஓ' => "oo",
        'ஔ' => "au",
        'ஃ' => "ah",
        _ => return None,
    })
}

/// Vowel signs (matras) — they REPLACE a consonant's inherent `a`.
fn matra(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{0BBE}' => "aa", // ா
        '\u{0BBF}' => "i",  // ி
        '\u{0BC0}' => "ii", // ீ
        '\u{0BC1}' => "u",  // ு
        '\u{0BC2}' => "uu", // ூ
        '\u{0BC6}' => "e",  // ெ
        '\u{0BC7}' => "ee", // ே
        '\u{0BC8}' => "ai", // ை
        '\u{0BCA}' => "o",  // ொ
        '\u{0BCB}' => "oo", // ோ
        '\u{0BCC}' => "au", // ௌ
        _ => return None,
    })
}

// ---- the walker -----------------------------------------------------------

/// Visit every identifier NAME in a file, mutably. One walker serves both
/// passes (collect and rewrite). Import paths are deliberately skipped —
/// they name files on disk, not Verilog identifiers.
fn for_each_name(f: &mut File, visit: &mut dyn FnMut(&mut String)) {
    for item in &mut f.items {
        match item {
            TopItem::Const(c) => {
                visit(&mut c.name.name);
                expr(&mut c.value, visit);
            }
            TopItem::Enum(e) => enum_decl(e, visit),
            TopItem::Module(m) => {
                visit(&mut m.name.name);
                for p in &mut m.params {
                    visit(&mut p.name.name);
                    if let Some(d) = &mut p.default {
                        expr(d, visit);
                    }
                }
                module_items(&mut m.items, visit);
            }
            TopItem::Test(t) => {
                visit(&mut t.module.name);
                for a in &mut t.args {
                    visit(&mut a.name.name);
                    expr(&mut a.value, visit);
                }
                test_stmts(&mut t.body, visit);
            }
            // Unreachable on the codegen path: `parse` rejects a tree with any
            // `Error` node, so transliteration never sees one.
            TopItem::Error(_) => {}
        }
    }
}

fn enum_decl(e: &mut EnumDecl, visit: &mut dyn FnMut(&mut String)) {
    visit(&mut e.name.name);
    for v in &mut e.variants {
        visit(&mut v.name);
    }
}

fn module_items(items: &mut [ModuleItem], visit: &mut dyn FnMut(&mut String)) {
    for item in items {
        match item {
            ModuleItem::Port { name, ty, .. } => {
                visit(&mut name.name);
                type_widths(ty, visit);
            }
            ModuleItem::Clock(n) | ModuleItem::Reset { name: n, .. } => visit(&mut n.name),
            ModuleItem::Wire { name, ty, init } => {
                visit(&mut name.name);
                type_widths(ty, visit);
                expr(init, visit);
            }
            ModuleItem::Reg { name, ty, reset } => {
                visit(&mut name.name);
                type_widths(ty, visit);
                expr(reset, visit);
            }
            ModuleItem::Mem {
                name,
                ty,
                depth,
                init,
            } => {
                visit(&mut name.name);
                type_widths(ty, visit);
                expr(depth, visit);
                expr(init, visit);
            }
            ModuleItem::Const(c) => {
                visit(&mut c.name.name);
                expr(&mut c.value, visit);
            }
            ModuleItem::Enum(e) => enum_decl(e, visit),
            ModuleItem::Inst(i) => {
                visit(&mut i.name.name);
                if let Some(idx) = &mut i.index {
                    expr(idx, visit);
                }
                visit(&mut i.module.name);
                for a in &mut i.args {
                    visit(&mut a.name.name);
                    expr(&mut a.value, visit);
                }
                for c in &mut i.conns {
                    visit(&mut c.port.name);
                    expr(&mut c.signal, visit);
                }
            }
            ModuleItem::On(on) => {
                visit(&mut on.clock.name);
                seq_stmts(&mut on.body, visit);
            }
            ModuleItem::Drive { lhs, rhs } => {
                lvalue(lhs, visit);
                expr(rhs, visit);
            }
            ModuleItem::Repeat(r) => {
                visit(&mut r.var.name);
                expr(&mut r.lo, visit);
                expr(&mut r.hi, visit);
                module_items(&mut r.items, visit);
            }
            ModuleItem::Error(_) => {} // unreachable on the codegen path
        }
    }
}

fn type_widths(ty: &mut Type, visit: &mut dyn FnMut(&mut String)) {
    match ty {
        Type::Bit => {}
        Type::Bits(e) | Type::Signed(e) => expr(e, visit),
        Type::Named(id) => visit(&mut id.name),
    }
}

fn seq_stmts(stmts: &mut [SeqStmt], visit: &mut dyn FnMut(&mut String)) {
    for s in stmts {
        match s {
            SeqStmt::Assign { lhs, rhs } => {
                lvalue(lhs, visit);
                expr(rhs, visit);
            }
            SeqStmt::If { cond, then, els } => {
                expr(cond, visit);
                seq_stmts(then, visit);
                if let Some(els) = els {
                    seq_stmts(els, visit);
                }
            }
            SeqStmt::Error(_) => {} // unreachable on the codegen path
        }
    }
}

fn test_stmts(stmts: &mut [TestStmt], visit: &mut dyn FnMut(&mut String)) {
    for s in stmts {
        match s {
            TestStmt::Tick { clock, count } => {
                visit(&mut clock.name);
                if let Some(c) = count {
                    expr(c, visit);
                }
            }
            TestStmt::Expect(e) => expr(e, visit),
            TestStmt::Drive { name, value } => {
                visit(&mut name.name);
                expr(value, visit);
            }
            TestStmt::If { cond, then, els } => {
                expr(cond, visit);
                test_stmts(then, visit);
                if let Some(els) = els {
                    test_stmts(els, visit);
                }
            }
            TestStmt::Error(_) => {} // unreachable on the codegen path
        }
    }
}

fn lvalue(l: &mut LValue, visit: &mut dyn FnMut(&mut String)) {
    visit(&mut l.base.name);
    if let Some((i, hi)) = &mut l.index {
        expr(i, visit);
        if let Some(hi) = hi {
            expr(hi, visit);
        }
    }
}

fn expr(e: &mut Expr, visit: &mut dyn FnMut(&mut String)) {
    match &mut e.kind {
        ExprKind::Int { .. } | ExprKind::Bool(_) => {}
        ExprKind::Ident(name) => visit(name),
        ExprKind::Field { base, field } => {
            expr(base, visit);
            visit(&mut field.name);
        }
        ExprKind::Unary { expr: x, .. } => expr(x, visit),
        ExprKind::Binary { lhs, rhs, .. } => {
            expr(lhs, visit);
            expr(rhs, visit);
        }
        ExprKind::IfExpr { cond, then, els } => {
            expr(cond, visit);
            expr(then, visit);
            expr(els, visit);
        }
        ExprKind::Match { scrutinee, arms } => {
            expr(scrutinee, visit);
            for arm in arms {
                for p in &mut arm.patterns {
                    if let Pattern::Variant { enum_name, variant } = p {
                        visit(&mut enum_name.name);
                        visit(&mut variant.name);
                    }
                }
                expr(&mut arm.value, visit);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                expr(p, visit);
            }
        }
        ExprKind::Replicate { count, parts } => {
            expr(count, visit);
            for p in parts {
                expr(p, visit);
            }
        }
        ExprKind::Index { base, index } => {
            expr(base, visit);
            expr(index, visit);
        }
        ExprKind::Slice { base, hi, lo } => {
            expr(base, visit);
            expr(hi, visit);
            expr(lo, visit);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                expr(a, visit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_tamil_words_romanize_readably() {
        assert_eq!(romanize("விளக்கு"), "villakku");
        assert_eq!(romanize("நிலை"), "nilai");
        assert_eq!(romanize("மணி"), "manni");
        assert_eq!(romanize("ஒளி"), "olli");
    }

    #[test]
    fn ascii_and_mixed_names_keep_their_ascii() {
        assert_eq!(romanize("count"), "count");
        assert_eq!(romanize("மணி2"), "manni2");
    }

    #[test]
    fn non_tamil_unicode_falls_back_to_hex() {
        assert_eq!(romanize("числo"), "_u0447_u0438_u0441_u043Bo");
    }

    #[test]
    fn results_always_start_like_an_identifier() {
        // The hex fallback already starts with `_`; a (hypothetical)
        // digit-leading result gets a `_` prefix.
        assert_eq!(romanize("٣x"), "_u0663x");
        assert_eq!(romanize("2x"), "_2x");
    }

    #[test]
    fn the_two_n_letters_romanize_identically() {
        // ந and ன both map to `n` (ISO 15919 needs diacritics ASCII
        // cannot carry) — `transliterate`'s collision counter is what
        // keeps such names distinct in the output.
        assert_eq!(romanize("நீ"), romanize("னீ"));
    }
}
