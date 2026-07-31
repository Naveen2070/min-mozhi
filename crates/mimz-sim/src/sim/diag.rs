//! Runtime diagnostic codes for `mimz-sim` (R2:
//! `docs/audit/review-2026-07-17.md`, design:
//! `docs/superpowers/specs/2026-07-30-sim-runtime-diagnostics-r2-design.local.md`).
//!
//! These are a DIFFERENT catalog from the checker's `E0xxx` codes
//! (`mimz_core::diag::ALL_CHECKER_CODES`) — a checker code fires at
//! compile time, before a design ever runs; an `S0xxx` code fires at
//! elaboration/execution time, after the checker has already accepted
//! the program. Four ranges by category: `S01xx` elaboration/wiring,
//! `S02xx` expression evaluation, `S03xx` test-harness control flow,
//! `S04xx` peripheral bind. Append-only, like the checker's own catalog
//! — a code is never renumbered or reused once assigned.
//!
//! Every code here must have a firing fixture in the sim-errors
//! contract test (Phase 5 of the design spec above).

/// Every stable `mimz-sim` runtime error code. Grows as later tasks
/// convert `Result<_, String>` sites to `Result<_, Box<mimz_core::diag::Diag>>`.
pub const ALL_SIM_CODES: [&str; 79] = [
    // S01xx — elaboration/wiring (sim/elaborate/registry.rs, instance.rs,
    // mod.rs, module.rs, rewrite.rs).
    "S0101", // unknown module reference
    "S0102", // ambiguous bare reference (module/extern-module/bundle)
    "S0103", // qualified reference's path doesn't match any `import`
    "S0104", // qualified reference resolved to an import lacking the name
    "S0105", // unknown module/extern-module reference (combined lookup miss)
    "S0106", // unknown bundle reference
    "S0109", // instance parameter has no value (no arg, no default)
    "S0112", // instance input port not connected
    "S0113", // extern-module instance has no simulation model (strict mode)
    "S0115", // unknown enum reference (rewrite: construct/field/pattern)
    "S0116", // enum has no such variant (rewrite: construct/field/pattern)
    "S0117", // bundle literal in an unsupported expression position
    "S0119", // instance nesting exceeds the max recursion depth
    "S0121", // module parameter has no default and no override was given
    "S0122", // unknown enum type in a declared signal's type
    "S0123", // memory has a non-positive depth
    "S0124", // `repeat` would unroll past REPEAT_BUDGET
    "S0125", // nested `repeat` is not supported
    "S0126", // a `repeat` body item is neither an instance nor a drive
    "S0127", // bundle destructure in a module body is not yet supported
    "S0128", // a flattened instance signal collides with an existing signal
    "S0129", // a bit-driven signal has no declaration
    "S0130", // a bit-driven signal's bit position is never driven
    "S0131", // no files to elaborate (defensive; unreachable via real callers)
    "S0133", // a clock/reset connection is not a plain signal name
    "S0134", // a bit-indexed drive's index is out of range (0..128)
    "S0135", // a slice-indexed drive's bound is out of range (0..128)
    "S0136", // a slice-indexed drive's bounds are reversed
    // S01xx — in-memory import resolution (runner.rs's `parse_source`,
    // used by the playground/embedder single-source path).
    "S0137", // std import path must be exactly `std.<module>` (two segments)
    "S0138", // unknown standard library module
    "S0139", // `import` of a non-std module unsupported in single-source mode
    // S02xx — expression evaluation at runtime (sim/value/{mod,binary,
    // fn_eval}.rs). Const-folding errors delegate to and preserve the
    // checker's own `E0xxx`-coded `Diag` (`checker::consteval::eval`)
    // instead of reassigning a coarser `S02xx` code — see `const_eval`'s
    // doc comment in `sim/value/mod.rs`.
    "S0201", // unknown signal reference (Resolver::signal/mem_read boundary)
    "S0202", // no `match` arm matched the value
    "S0203", // concatenation/replication exceeds the max width
    "S0204", // replication count must be at least 1
    "S0205", // array has no elements to index
    "S0206", // memory read failed (Resolver::mem_read boundary)
    "S0207", // a bit index or slice bound is out of range for the value's width
    "S0208", // slice bounds reversed (write `[hi:lo]`, msb first)
    "S0209", // enum-variant / instance-port access not supported by the evaluator
    "S0210", // BundleLit reached the value evaluator unexpanded
    "S0211", // array literal only valid as a `fn` argument or `let` binding
    "S0212", // EnumConstruct reached the value evaluator unexpanded
    "S0213", // signal of enum type — not modeled by the simulator
    "S0214", // Type::Bundle reached type_width unflattened
    "S0215", // Type::Array reached type_width unexpanded
    "S0216", // width must be at least 1
    "S0217", // width exceeds the shared maximum
    "S0218", // no module with the given name in this file
    "S0219", // file defines no module
    "S0220", // file defines multiple modules — none picked
    "S0221", // a shift amount cannot be `signed`
    "S0222", // `??` reached binary_known unlowered
    "S0223", // function table unavailable in this evaluation context
    "S0224", // undefined function
    "S0225", // array parameter has an invalid (non-positive) length
    "S0226", // function called with too few arguments
    "S0227", // `loop` would unroll past REPEAT_BUDGET
    "S0228", // `extend` target narrower than the value's own width
    "S0229", // `clog2` is compile-time only
    // S02xx — the combinational-only evaluator (sim/comb.rs). Shares
    // several codes with the elaboration-time equivalents in
    // sim/elaborate/module.rs (S0121/S0129/S0130/S0134/S0135/S0136) since
    // the conditions are structurally identical, just checked in a
    // different (single-module, no-instances) evaluation context.
    "S0230", // eval_outputs: no files (defensive; unreachable via real callers)
    "S0231", // module has `reg` state — unsupported by the combinational evaluator
    "S0232", // module has an `on` block — unsupported by the combinational evaluator
    "S0233", // module instantiates a sub-module — unsupported by the combinational evaluator
    "S0234", // module uses `repeat` — unsupported by the combinational evaluator
    "S0235", // module uses `sync loop` — unsupported by the combinational evaluator
    "S0236", // missing value for a declared input
    "S0237", // signal is never driven
    "S0238", // combinational cycle through a signal
    // S02xx — the event-driven kernel (sim/kernel.rs).
    "S0239", // `Sim::set`: name is not a drivable input/clock/reset
    // S03xx — test-harness control flow (sim/harness/mod.rs's `Run::exec`).
    // The peripheral-bind-validation `Stop::Err` sites in the same match arm
    // (unknown peripheral / direction mismatch / no such port) are Task
    // 4.1's `S04xx` numbering, not these — left uncoded for now since
    // `Stop::Err`'s payload type had to move onto `Diag` in one sweep.
    "S0301", // `tick(clk, ...)`: `clk` is not a declared clock of this module
    "S0302", // `tick(clk, n)`: `n` evaluated negative
    "S0303", // a tick would exceed the (headless) simulation cycle limit
    "S0304", // `sim { speed ... }`: the rate evaluated to zero or negative
    "S0305", // a `tick`/`speed` expression is wider than a plain integer
    // S04xx — peripheral bind errors (sim/harness/mod.rs's `TestStmt::Sim`
    // handling). `sim/host.rs`'s `EmulationHost` trait itself is untouched —
    // every impl's own `bind` error is wrapped here, at the call site, using
    // the `Bind`'s own span.
    "S0401", // `bind port -> peripheral(...)`: unknown peripheral kind
    "S0402", // bind direction mismatch (port exists, wrong direction)
    "S0403", // no port of the needed direction with that name on the design
    "S0404", // the peripheral itself rejected the bind (host-specific reason)
];
