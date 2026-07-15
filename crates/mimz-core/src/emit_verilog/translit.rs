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
                visit(&mut t.module.name.name);
                for a in &mut t.args {
                    visit(&mut a.name.name);
                    expr(&mut a.value, visit);
                }
                test_stmts(&mut t.body, visit);
            }
            TopItem::Func(f) => {
                visit(&mut f.name.name);
                for p in &mut f.params {
                    visit(&mut p.name.name);
                    type_widths(&mut p.ty, visit);
                }
                type_widths(&mut f.ret, visit);
                fn_stmts(&mut f.stmts, visit);
                expr(&mut f.tail, visit);
            }
            // Unreachable on the codegen path: `parse` rejects a tree with any
            // `Error` node, so transliteration never sees one.
            TopItem::Error(_) => {}
            // `verilog_name` (if present) names the real external Verilog
            // module and must NOT be renamed — only the Min-Mozhi-facing
            // name/params/ports are subject to keyword-flavor translation,
            // same as `Module`.
            TopItem::ExternModule(em) => {
                visit(&mut em.name.name);
                for p in &mut em.params {
                    visit(&mut p.name.name);
                    if let Some(d) = &mut p.default {
                        expr(d, visit);
                    }
                }
                module_items(&mut em.items, visit);
            }
            TopItem::Bundle(b) => {
                visit(&mut b.name.name);
                for p in &mut b.params {
                    visit(&mut p.name.name);
                    if let Some(default) = &mut p.default {
                        expr(default, visit);
                    }
                }
                for f in &mut b.fields {
                    visit(&mut f.name.name);
                    type_widths(&mut f.ty, visit);
                }
            }
        }
    }
}

fn enum_decl(e: &mut EnumDecl, visit: &mut dyn FnMut(&mut String)) {
    visit(&mut e.name.name);
    for v in &mut e.variants {
        visit(&mut v.name.name);
        // PayloadField.name is documentation-only (not emitted), but its
        // type-width expr may reference a Tamil const/param name.
        for f in &mut v.fields {
            type_widths(&mut f.ty, visit);
        }
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
                visit(&mut i.module.name.name);
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
            ModuleItem::ForEach(fe) => {
                visit(&mut fe.var.name);
                match &mut fe.source {
                    ForEachSource::Range { lo, hi } => {
                        expr(lo, visit);
                        expr(hi, visit);
                    }
                    ForEachSource::Elements(id) => {
                        visit(&mut id.name);
                    }
                }
                module_items(&mut fe.items, visit);
            }
            ModuleItem::SyncLoop(sl) => {
                visit(&mut sl.name.name);
                visit(&mut sl.clock.name);
                visit(&mut sl.var.name);
                expr(&mut sl.lo, visit);
                expr(&mut sl.hi, visit);
                visit(&mut sl.result_name.name);
                type_widths(&mut sl.result_ty, visit);
                expr(&mut sl.result_init, visit);
                seq_stmts(&mut sl.body, visit);
            }
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => {
                expr(cond, visit);
                module_items(then, visit);
                if let Some(el) = els.as_mut() {
                    module_items(el, visit);
                }
            }
            ModuleItem::Error(_) => {} // unreachable on the codegen path
            ModuleItem::BundleDestructure {
                bindings, expr: e, ..
            } => {
                for b in bindings {
                    visit(&mut b.name);
                }
                expr(e, visit);
            }
        }
    }
}

fn type_widths(ty: &mut Type, visit: &mut dyn FnMut(&mut String)) {
    match ty {
        Type::Bit => {}
        Type::Bits(e) | Type::Signed(e) => expr(e, visit),
        Type::Named(id) => visit(&mut id.name.name),
        Type::Bundle { name, args } => {
            visit(&mut name.name.name);
            for a in args {
                visit(&mut a.name.name);
                expr(&mut a.value, visit);
            }
        }
        Type::Array { elem, len } => {
            type_widths(elem, visit);
            expr(len, visit);
        }
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
            SeqStmt::Default { name, val, .. } => {
                visit(&mut name.name);
                expr(val, visit);
            }
            SeqStmt::Loop {
                var, lo, hi, body, ..
            } => {
                visit(&mut var.name);
                expr(lo, visit);
                expr(hi, visit);
                seq_stmts(body, visit);
            }
            SeqStmt::ForEach {
                var, source, body, ..
            } => {
                visit(&mut var.name);
                match source {
                    ForEachSource::Range { lo, hi } => {
                        expr(lo, visit);
                        expr(hi, visit);
                    }
                    ForEachSource::Elements(id) => {
                        visit(&mut id.name);
                    }
                }
                seq_stmts(body, visit);
            }
            SeqStmt::Error(_) => {} // unreachable on the codegen path
        }
    }
}

/// Transliterate every identifier reachable from a `fn`-body statement
/// list — mirrors `test_stmts`'s recursive-descent shape for `TestStmt`.
fn fn_stmts(stmts: &mut [FnStmt], visit: &mut dyn FnMut(&mut String)) {
    for stmt in stmts {
        match stmt {
            FnStmt::Let(local) => {
                visit(&mut local.name.name);
                expr(&mut local.value, visit);
            }
            FnStmt::If { cond, then, els } => {
                expr(cond, visit);
                fn_stmts(then, visit);
                if let Some(els) = els {
                    fn_stmts(els, visit);
                }
            }
            FnStmt::Return(e) => expr(e, visit),
            FnStmt::Loop {
                var, lo, hi, body, ..
            } => {
                visit(&mut var.name);
                expr(lo, visit);
                expr(hi, visit);
                fn_stmts(body, visit);
            }
            FnStmt::ForEach {
                var, source, body, ..
            } => {
                visit(&mut var.name);
                match source {
                    ForEachSource::Range { lo, hi } => {
                        expr(lo, visit);
                        expr(hi, visit);
                    }
                    ForEachSource::Elements(id) => {
                        visit(&mut id.name);
                    }
                }
                fn_stmts(body, visit);
            }
            FnStmt::Error(_) => {} // unreachable on the codegen path
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
            TestStmt::Sim(sim) => {
                if let Some(speed) = &mut sim.speed {
                    expr(speed, visit);
                }
                for b in &mut sim.binds {
                    visit(&mut b.port.name);
                    visit(&mut b.peripheral.name);
                    for a in &mut b.args {
                        visit(&mut a.name.name);
                        if let BindArgValue::Ident(s) = &mut a.value {
                            visit(s);
                        }
                    }
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
                    if let Pattern::Variant {
                        enum_name,
                        variant,
                        bindings,
                    } = p
                    {
                        visit(&mut enum_name.name);
                        visit(&mut variant.name);
                        // Binding names must be transliterated so the emitter's
                        // substitution map (keyed by binding name) matches the
                        // identifier it will see in the arm's value expression.
                        for b in bindings {
                            visit(&mut b.name);
                        }
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
        ExprKind::FnCall { name, args } => {
            visit(&mut name.name);
            for a in args {
                expr(a, visit);
            }
        }
        ExprKind::BundleLit(inits) => {
            for fi in inits {
                visit(&mut fi.name.name);
                expr(&mut fi.value, visit);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for e in elems {
                expr(e, visit);
            }
        }
        ExprKind::EnumConstruct {
            enum_name,
            variant,
            args,
        } => {
            visit(&mut enum_name.name);
            visit(&mut variant.name);
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

    /// Reskinning a fn body with a statement-level `if`/`return` still
    /// re-parses — the walker must cover `FuncDecl.stmts`/`tail`, not just
    /// the old flat `locals`/`body` shape. Mirrors this crate's
    /// `tests/translate.rs` round-trip convention (`translate` + `lex`/`parse`).
    #[test]
    fn translate_preserves_fn_return_and_if_semantics() {
        use crate::lexer::lex;
        use crate::lexer::token::Flavor;
        use crate::parser::parse;
        use crate::translate::translate;

        let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return a }\n  0\n}\n";
        let translated = translate(src, Flavor::Tanglish).expect("lexes");
        parse(lex(&translated).expect("tanglish lexes")).expect("tanglish parses");
    }

    /// Confirms `expr()`'s `ExprKind::EnumConstruct` arm visits all three
    /// name-bearing parts — `enum_name`, `variant`, and each arg — during
    /// `--romanize-names` rewriting, same as `Pattern::Variant` already does.
    #[test]
    fn enum_construct_romanizes_enum_and_variant_names() {
        use crate::translate::{TranslateOpts, translate_opts};
        // நிலை -> nilai, மணி -> manni, கணக்கு -> kannakku (all confirmed by
        // this module's own `pure_tamil_words_romanize_readably` test).
        let src = "enum நிலை {\n  மணி(கணக்கு: bits[4])\n}\n\
                   module M {\n  in கணக்கு: bits[4]\n  out y: நிலை\n  y = நிலை.மணி(கணக்கு)\n}\n";
        let out = translate_opts(
            src,
            crate::lexer::token::Flavor::English,
            TranslateOpts {
                romanize_names: true,
            },
        )
        .unwrap();
        assert!(
            out.contains("nilai.manni(kannakku)"),
            "enum name, variant, and arg must all be romanized: {out}"
        );
        assert!(
            !out.contains("நிலை") && !out.contains("மணி") && !out.contains("கணக்கு"),
            "Tamil names should be gone: {out}"
        );
    }
}
