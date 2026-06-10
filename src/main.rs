//! mimz — the Min-Mozhi (மின்மொழி) compiler.
//!
//! Phase 1 pipeline (docs/architecture.md):
//! lexer → parser → AST → checker → Verilog emitter.

fn main() {
    println!("mimz {} — Min-Mozhi · மின்மொழி", env!("CARGO_PKG_VERSION"));
    println!("The compiler is under construction. See docs/plan/phase-1-verilog-backend.md");
}
