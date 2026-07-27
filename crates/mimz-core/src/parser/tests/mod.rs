//! Parser unit tests — including the locked-in safety behaviors
//! (precedence trap, latch teaching, `=` vs `<-`).

use super::*;
use crate::ast::{Builtin, ExprKind, FnStmt, ForEachSource, ModuleItem, TopItem, Type};
use crate::bits::Bits;
use crate::lexer::lex;
mod bundles;
mod calls_and_modules;
mod enums_and_tagged_unions;
mod extern_module_and_sync_builtins;
mod fn_decl_thamizh_and_stmts;
mod fn_decls;
mod item_grammar;
mod module_refs_and_arrays;
mod repeat_loop_foreach;
mod reset_and_thamizh_order;
mod safety_and_precedence;
mod test_blocks_sim_and_recovery;
mod valid_bundle_sugar;

fn parse_ok(src: &str) -> File {
    parse(lex(src).expect("lex error")).expect("parse error")
}

fn parse_err(src: &str) -> Vec<Diag> {
    match parse(lex(src).expect("lex error")) {
        Ok(_) => panic!("expected a parse error"),
        Err(d) => d,
    }
}

/// Parse `expr_src` as a combinational drive RHS inside a minimal module.
/// Wraps in `module M { in a: bits[4]; in x: bits[4]; in y: bits[4]; out z: bits[4]; z = <expr> }`.
fn parse_expr_ok(expr_src: &str) -> crate::ast::Expr {
    let src = format!(
        "module M {{\n  in a: bits[4]\n  in x: bits[4]\n  in y: bits[4]\n  out z: bits[4]\n  z = {expr_src}\n}}\n"
    );
    let f = parse(lex(&src).expect("lex error")).expect("parse error");
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    for item in &m.items {
        if let ModuleItem::Drive { rhs, .. } = item {
            return rhs.clone();
        }
    }
    panic!("no Drive item found")
}
