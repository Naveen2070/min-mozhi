//! `mimz build`'s I/O half: board presets, pin files and the synthesis
//! toolchain, and the flow that runs them. The pure IR -> Verilog backend lives in
//! `mimz_core::backend`.

pub mod boards;
pub mod flow;
pub mod pins;
pub mod toolchain;
