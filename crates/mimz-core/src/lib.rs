#![forbid(unsafe_code)]

pub const REPEAT_BUDGET: i128 = 4096;

pub mod analysis;
pub mod ast;
pub mod checker;
pub mod diag;
pub mod emit_verilog;
pub mod explain;
pub mod lexer;
pub mod lint;
pub mod morph;
pub mod parser;
pub mod pretty;
pub mod project;
pub mod span;
pub mod stdlib;
pub mod translate;
pub mod version;
