//! Default stimulus + timeline capture (Phase 1.5, steps B4/B5).
//!
//! `mimz sim` has no `test` block to drive the design, so [`run`] applies a
//! standard stimulus: hold reset asserted for the first `reset_cycles`, hold the
//! inputs at their given values, and toggle one clock for `cycles` cycles —
//! capturing a [`Timeline`] of per-cycle snapshots. The clock is rendered as a
//! square wave (high for the first half of each cycle, low for the second) so
//! the VCD shows real edges; the rising-edge frame carries the settled state.
//!
//! The timeline is the single source both the VCD writer ([`super::vcd`]) and
//! the console tracer ([`super::trace`]) consume.

use std::collections::BTreeMap;

use super::elaborate::{Design, Signal};
use super::kernel::Sim;

/// How to drive a `mimz sim` run.
pub struct SimOpts {
    /// Which clock to toggle. `None` picks the design's only clock.
    pub clock: Option<String>,
    /// Input values, held constant for the whole run.
    pub inputs: BTreeMap<String, u128>,
    /// Number of clock cycles to run.
    pub cycles: u64,
    /// How many initial cycles to hold reset asserted (active-high).
    pub reset_cycles: u64,
}

/// One captured instant: a timestamp, the cycle number for a rising-edge frame
/// (`None` for the falling half), and every signal's value.
#[derive(Clone, Debug)]
pub struct Frame {
    pub time: u64,
    pub cycle: Option<u64>,
    pub values: BTreeMap<String, u128>,
}

/// A captured run: the (stable) signal list with widths, plus the frames.
#[derive(Clone, Debug)]
pub struct Timeline {
    pub module: String,
    pub signals: Vec<Signal>,
    pub frames: Vec<Frame>,
}

/// Half-period and period, in `$timescale` units, for the rendered clock.
const HALF: u64 = 5;
const PERIOD: u64 = 2 * HALF;

/// Run `design` under the default stimulus and capture its [`Timeline`].
pub fn run(design: Design, opts: &SimOpts) -> Result<Timeline, String> {
    let clock = match &opts.clock {
        Some(c) => {
            if !design.clocks.iter().any(|d| d == c) {
                return Err(format!(
                    "no clock named `{c}` in module `{}`",
                    design.module
                ));
            }
            c.clone()
        }
        None => match design.clocks.as_slice() {
            [one] => one.clone(),
            [] => {
                return Err(format!(
                    "module `{}` has no clock — `mimz sim` runs clocked designs; \
                     use `mimz eval` for combinational modules",
                    design.module
                ));
            }
            many => {
                return Err(format!(
                    "module `{}` has {} clocks — choose one with --clock <name>",
                    design.module,
                    many.len()
                ));
            }
        },
    };
    let module = design.module.clone();
    let resets = design.resets.clone();

    let mut sim = Sim::new(design);
    for (name, value) in &opts.inputs {
        sim.set(name, *value)?; // an unknown input name is a clean error
    }

    // The signal list is stable across the run — take it once.
    let signals: Vec<Signal> = sim
        .snapshot()?
        .into_iter()
        .map(|(name, _, width)| Signal { name, width })
        .collect();

    let mut frames = Vec::new();
    for cycle in 0..opts.cycles {
        let rst = (cycle < opts.reset_cycles) as u128;
        for r in &resets {
            sim.set(r, rst)?;
        }
        // Rising edge: clock high, advance state, capture the settled frame.
        sim.set(&clock, 1)?;
        sim.tick(&clock)?;
        frames.push(Frame {
            time: cycle * PERIOD,
            cycle: Some(cycle),
            values: values(&sim)?,
        });
        // Falling edge: clock low, state held.
        sim.set(&clock, 0)?;
        frames.push(Frame {
            time: cycle * PERIOD + HALF,
            cycle: None,
            values: values(&sim)?,
        });
    }
    Ok(Timeline {
        module,
        signals,
        frames,
    })
}

/// A name → value snapshot of the current state (drops the widths, which the
/// timeline already carries in `signals`).
fn values(sim: &Sim) -> Result<BTreeMap<String, u128>, String> {
    Ok(sim
        .snapshot()?
        .into_iter()
        .map(|(name, value, _)| (name, value))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::elaborate::elaborate;

    const COUNTER: &str = "module Counter(WIDTH: int = 8) {\n  \
        clock clk\n  reset rst\n  out count: bits[WIDTH]\n  \
        reg value: bits[WIDTH] = 0\n  on rise(clk) { value <- value +% 1 }\n  \
        count = value\n}\n";

    fn design(src: &str) -> Design {
        let f = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
        elaborate(&f, None, &BTreeMap::new()).expect("elaborates")
    }

    fn opts(cycles: u64) -> SimOpts {
        SimOpts {
            clock: None,
            inputs: BTreeMap::new(),
            cycles,
            reset_cycles: 1,
        }
    }

    #[test]
    fn counter_timeline_counts_after_reset() {
        let tl = run(design(COUNTER), &opts(4)).expect("runs");
        let rows: Vec<&Frame> = tl.frames.iter().filter(|f| f.cycle.is_some()).collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].values["count"], 0); // reset cycle
        assert_eq!(rows[1].values["count"], 1);
        assert_eq!(rows[2].values["count"], 2);
        assert_eq!(rows[3].values["count"], 3);
        // The clock is a square wave: high on rising frames, low on falling.
        assert_eq!(rows[1].values["clk"], 1);
        let falling: Vec<&Frame> = tl.frames.iter().filter(|f| f.cycle.is_none()).collect();
        assert_eq!(falling[1].values["clk"], 0);
    }

    #[test]
    fn inputs_are_held_for_the_run() {
        let src = "module Add {\n  clock clk\n  reset rst\n  in x: bits[8]\n  out y: bits[8]\n  \
                   reg r: bits[8] = 0\n  on rise(clk) { r <- r +% x }\n  y = r\n}\n";
        let mut o = opts(3);
        o.inputs.insert("x".into(), 10);
        let tl = run(design(src), &o).expect("runs");
        let rows: Vec<&Frame> = tl.frames.iter().filter(|f| f.cycle.is_some()).collect();
        assert_eq!(rows[0].values["y"], 0); // reset
        assert_eq!(rows[1].values["y"], 10); // +x
        assert_eq!(rows[2].values["y"], 20);
    }

    #[test]
    fn a_clockless_module_is_rejected() {
        let err = run(
            design("module C {\n  in a: bits[8]\n  out y: bits[8]\n  y = a\n}\n"),
            &opts(4),
        )
        .unwrap_err();
        assert!(err.contains("no clock"), "got: {err}");
    }

    #[test]
    fn an_unknown_input_is_rejected() {
        let mut o = opts(2);
        o.inputs.insert("nope".into(), 1);
        assert!(run(design(COUNTER), &o).is_err());
    }
}
