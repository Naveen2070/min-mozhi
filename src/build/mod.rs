//! `mimz build`'s I/O half: board presets, pin files and the synthesis
//! toolchain (Task 6 adds the flow). The pure IR -> Verilog backend lives in
//! `mimz_core::backend`.

pub mod boards;
pub mod pins;
pub mod toolchain;
