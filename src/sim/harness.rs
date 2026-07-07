//! `test`-block execution (Phase 1.5, step B6) — the engine behind `mimz test`.
//!
//! [`run_test`] runs one `test "…" for M(args) { … }` block: elaborate `M` with
//! the test's parameters, build a [`Sim`], then interpret the body in order —
//! `name = value` drives an input, `tick(clk[, n])` advances rising edges,
//! `expect e` asserts `e` is true at the current cycle, and `if`/`else` branch
//! on a condition. A failing `expect` **halts that test** and produces a
//! teaching-quality message (the source of the expression, the cycle, and — for
//! a comparison — each side's actual value). `mimz test` exits non-zero if any
//! test fails.
//!
//! This is the shipped `tick`/`expect` form (spec/02 section 1.10, spec/05 section 3). The
//! `await clk.cycles(n)` sugar is a later increment: it needs the `await`
//! Tamil/Tanglish spelling from native review (R9/R11) before it can be parsed.
//!
//! Like `mimz sim` / `mimz eval`, this runs on the parsed AST without the
//! checker — semantic problems surface as runtime errors. Every run also
//! captures a [`Timeline`] (one frame per tick) so `--trace` works here too,
//! riding the same per-cycle snapshot the VCD writer consumes.

use std::collections::BTreeMap;

use crate::ast::{self, BinOp, Expr, ExprKind, TestDecl, TestStmt};

use super::elaborate::{Signal, elaborate_project};
#[cfg(feature = "hw-emulation")]
use super::emulate::{self, Peripheral};
use super::kernel::Sim;
use super::run::{Frame, MAX_SIM_CYCLES, Timeline};
use super::value::{self, Val};

/// Period between captured frames, in the timeline's time units (matches the
/// `mimz sim` clock period so a test trace lines up with a `sim` trace).
const PERIOD: u64 = 10;

/// Target dashboard refresh rate for frame-batched real-time pacing.
#[cfg(feature = "hw-emulation")]
const DASHBOARD_FPS: u64 = 30;

/// Split `total` ticks into batches of at most `cycles_per_frame`, so a
/// `sim` block with `speed mhz(50)` doesn't try to sleep on every
/// individual 20ns cycle (physically impossible — OS sleep resolution is
/// ~1ms). Each batch runs instantly; the caller sleeps between batches to
/// match wall-clock time. Pure and terminal-free, so it's unit-testable
/// without a TTY.
#[cfg(feature = "hw-emulation")]
fn batch_sizes(total: u64, cycles_per_frame: u64) -> Vec<u64> {
    if total == 0 {
        return Vec::new();
    }
    let cycles_per_frame = cycles_per_frame.max(1);
    let mut out = Vec::new();
    let mut remaining = total;
    while remaining > 0 {
        let n = remaining.min(cycles_per_frame);
        out.push(n);
        remaining -= n;
    }
    out
}

/// `cycles_per_frame = speed_hz / DASHBOARD_FPS`, floored to 1.
#[cfg(feature = "hw-emulation")]
fn cycles_per_frame(speed_hz: u64) -> u64 {
    (speed_hz / DASHBOARD_FPS).max(1)
}

/// The result of running one `test` block.
#[derive(Debug)]
pub struct Outcome {
    /// The quoted test name.
    pub name: String,
    /// How many `expect`s ran (on the taken path) before pass/fail.
    pub checks: usize,
    /// Pass, or fail with a teaching-quality message.
    pub result: TestResult,
    /// Per-tick capture, for `--trace` / `--trace=changes`.
    pub timeline: Timeline,
    /// Default trace scope: the module's interface + state (inputs, outputs,
    /// registers), in that order.
    pub default_scope: Vec<String>,
}

/// Whether a `test` block passed.
#[derive(Debug)]
pub enum TestResult {
    Pass,
    /// Failed with a teaching-quality message (already formatted, multi-line).
    Fail(String),
}

/// Run one `test` block. `files` is the loaded project (`files[0]` the entry,
/// the rest its imports) so a module-under-test that instantiates a sub-module
/// from another file flattens; `src` is the entry source (for rendering
/// expressions in failure messages). `Err` is a setup/semantic error; a normal
/// `expect` failure is `Ok(Outcome { result: Fail(..), .. })`.
pub fn run_test(
    files: &[ast::File],
    src: &str,
    decl: &TestDecl,
    emulate: bool,
) -> Result<Outcome, String> {
    let params = params(decl)?;
    let design = elaborate_project(files, Some(&decl.module.name.name), &params)?;

    let module = design.module.clone();
    let clocks = design.clocks.clone();
    let default_scope: Vec<String> = design
        .inputs
        .iter()
        .chain(&design.outputs)
        .map(|s| s.name.clone())
        .chain(design.regs.iter().map(|r| r.name.clone()))
        .collect();

    #[cfg(feature = "hw-emulation")]
    let outputs = design.outputs.clone();
    // Not read on this build: `emulate` only feeds `live` below, which is
    // itself feature-gated.
    #[cfg(not(feature = "hw-emulation"))]
    let _ = emulate;
    let sim = Sim::new(design);
    let signals: Vec<Signal> = sim
        .snapshot()?
        .into_iter()
        .map(|(name, _, width)| Signal { name, width })
        .collect();

    // Emulation is only ever "live" (throttled ticks + dashboard, wired up in
    // Task 7) when the caller asked for it AND a real terminal is attached —
    // this keeps `--emulate` a CI-safe no-op when stdout is piped/redirected.
    #[cfg(feature = "hw-emulation")]
    let live = {
        use std::io::IsTerminal;
        if emulate && !std::io::stdout().is_terminal() {
            eprintln!(
                "note: sim block emulation skipped (no terminal attached) — run with a TTY to see it"
            );
        }
        emulate && std::io::stdout().is_terminal()
    };

    let mut run = Run {
        sim,
        clocks,
        module: module.clone(),
        src,
        cycle: 0,
        frames: Vec::new(),
        checks: 0,
        #[cfg(feature = "hw-emulation")]
        outputs,
        #[cfg(feature = "hw-emulation")]
        active_sim: None,
        #[cfg(feature = "hw-emulation")]
        live,
    };
    run.capture()?; // the initial (pre-tick) state

    let result = match run.exec(&decl.body) {
        Ok(()) => TestResult::Pass,
        Err(Stop::Fail(m)) => TestResult::Fail(m),
        Err(Stop::Err(e)) => return Err(e),
    };

    Ok(Outcome {
        name: decl.name.clone(),
        checks: run.checks,
        result,
        timeline: Timeline {
            module,
            signals,
            frames: run.frames,
        },
        default_scope,
    })
}

/// Fold a test's `(NAME: expr, …)` parameters to integers; a later parameter may
/// reference an earlier one.
fn params(decl: &TestDecl) -> Result<BTreeMap<String, i128>, String> {
    let mut m = BTreeMap::new();
    for a in &decl.args {
        let v = value::const_eval(&a.value, &m)?;
        m.insert(a.name.name.clone(), v);
    }
    Ok(m)
}

/// How a test body stops early: a real `expect` failure (the test fails) vs. a
/// setup/semantic error (the whole command errors).
enum Stop {
    Fail(String),
    Err(String),
}

/// One test in flight.
struct Run<'a> {
    sim: Sim,
    clocks: Vec<String>,
    module: String,
    src: &'a str,
    cycle: u64,
    frames: Vec<Frame>,
    checks: usize,
    /// The module's output ports (name + folded width), captured before
    /// `design` moved into `Sim::new` — used to validate `bind` targets are
    /// outputs (`led`/`speaker` observe, they don't drive).
    #[cfg(feature = "hw-emulation")]
    outputs: Vec<Signal>,
    #[cfg(feature = "hw-emulation")]
    active_sim: Option<ActiveSim>,
    /// `emulate` requested AND stdout is a real terminal. Gates
    /// `TestStmt::Tick`'s batched pacing: when true and a `sim` block is
    /// active, ticks run in speed-sized batches with a wall-clock sleep
    /// between batches instead of unthrottled.
    #[cfg(feature = "hw-emulation")]
    live: bool,
}

/// The registered peripherals + real-world clock rate for the test's `sim`
/// block, if it has one. `speed_hz` sizes each batch (`batch_cycles`);
/// `peripherals` are notified once per batch (`notify_peripherals`).
#[cfg(feature = "hw-emulation")]
struct ActiveSim {
    /// Real-world clock rate in Hz, if a `speed` clause was given.
    speed_hz: Option<u64>,
    /// `(port name, peripheral)` — updated once per batched frame.
    peripherals: Vec<(String, Box<dyn Peripheral>)>,
}

impl Run<'_> {
    fn exec(&mut self, body: &[TestStmt]) -> Result<(), Stop> {
        for stmt in body {
            match stmt {
                TestStmt::Drive { name, value } => {
                    let v = self.sim.eval(value).map_err(Stop::Err)?;
                    self.sim.set(&name.name, v.masked()).map_err(Stop::Err)?;
                }
                TestStmt::Tick { clock, count } => {
                    if !self.clocks.iter().any(|c| c == &clock.name) {
                        return Err(Stop::Err(format!(
                            "`{}` is not a clock of `{}`",
                            clock.name, self.module
                        )));
                    }
                    let n = match count {
                        Some(e) => {
                            let v = self.sim.eval(e).map_err(Stop::Err)?.as_i128();
                            if v < 0 {
                                return Err(Stop::Err(format!("tick count {v} is negative")));
                            }
                            v as u64
                        }
                        None => 1,
                    };
                    // Bound total simulated cycles so a `tick(clk, <huge>)` in an
                    // untrusted test can't hang the tool or exhaust memory.
                    if self.cycle.saturating_add(n) > MAX_SIM_CYCLES {
                        return Err(Stop::Err(format!(
                            "test exceeds the {MAX_SIM_CYCLES}-cycle simulation limit"
                        )));
                    }
                    #[cfg(feature = "hw-emulation")]
                    let batched = self.live && self.active_sim.is_some();
                    #[cfg(not(feature = "hw-emulation"))]
                    let batched = false;
                    if batched {
                        #[cfg(feature = "hw-emulation")]
                        for batch in batch_sizes(n, self.batch_cycles()) {
                            let started = std::time::Instant::now();
                            for _ in 0..batch {
                                self.sim.tick(&clock.name).map_err(Stop::Err)?;
                                self.cycle += 1;
                                self.capture().map_err(Stop::Err)?;
                            }
                            self.notify_peripherals().map_err(Stop::Err)?;
                            if let Some(remaining) =
                                Self::frame_budget().checked_sub(started.elapsed())
                            {
                                if self.active_sim.as_ref().and_then(|a| a.speed_hz).is_some() {
                                    std::thread::sleep(remaining);
                                }
                            }
                        }
                    } else {
                        for _ in 0..n {
                            self.sim.tick(&clock.name).map_err(Stop::Err)?;
                            self.cycle += 1;
                            self.capture().map_err(Stop::Err)?;
                        }
                    }
                }
                TestStmt::Expect(e) => {
                    self.checks += 1;
                    let v = self.sim.eval(e).map_err(Stop::Err)?;
                    if v.bits & 1 != 1 {
                        return Err(Stop::Fail(self.fail_message(e)?));
                    }
                }
                TestStmt::If { cond, then, els } => {
                    if self.sim.eval(cond).map_err(Stop::Err)?.bits & 1 == 1 {
                        self.exec(then)?;
                    } else if let Some(e) = els {
                        self.exec(e)?;
                    }
                }
                #[cfg(feature = "hw-emulation")]
                TestStmt::Sim(block) => {
                    let speed_hz = match &block.speed {
                        Some(e) => {
                            let v = self.sim.eval(e).map_err(Stop::Err)?.as_i128();
                            if v <= 0 {
                                return Err(Stop::Err(format!(
                                    "sim block's speed must be positive, got {v}"
                                )));
                            }
                            Some(v as u64)
                        }
                        None => None,
                    };
                    let registry = emulate::registry();
                    let mut peripherals = Vec::new();
                    for b in &block.binds {
                        let ctor = registry.get(b.peripheral.name.as_str()).ok_or_else(|| {
                            Stop::Err(format!(
                                "unknown peripheral `{}` — known: led",
                                b.peripheral.name
                            ))
                        })?;
                        let width = self.port_width(&b.port.name).ok_or_else(|| {
                            Stop::Err(format!(
                                "`{}` has no output port `{}` to bind",
                                self.module, b.port.name
                            ))
                        })?;
                        let p = ctor(width, &b.args).map_err(Stop::Err)?;
                        peripherals.push((b.port.name.clone(), p));
                    }
                    self.active_sim = Some(ActiveSim {
                        speed_hz,
                        peripherals,
                    });
                }
                // Without `hw-emulation`, a `sim` block parses but has no
                // dashboard/peripherals to run against — nothing to execute.
                #[cfg(not(feature = "hw-emulation"))]
                TestStmt::Sim(_) => {}
                // Unreachable: the sim runs on a strict-parsed tree, which
                // carries no `Error` placeholder.
                TestStmt::Error(_) => {}
            }
        }
        Ok(())
    }

    /// The folded `Width` of an output port, or `None` if `name` isn't one
    /// (bind targets are always outputs for `led`/`speaker`; `uart_rx`
    /// binding an INPUT is Spec 2's concern, not this one's).
    #[cfg(feature = "hw-emulation")]
    fn port_width(&self, name: &str) -> Option<super::elaborate::Width> {
        self.outputs
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.width)
    }

    /// How many ticks make up one batch: the declared `speed`'s
    /// cycles-per-frame, or `u64::MAX` (one batch, no pacing) if the `sim`
    /// block gave no `speed`.
    #[cfg(feature = "hw-emulation")]
    fn batch_cycles(&self) -> u64 {
        self.active_sim
            .as_ref()
            .and_then(|a| a.speed_hz)
            .map(cycles_per_frame)
            .unwrap_or(u64::MAX)
    }

    /// Call `on_change` for every bound peripheral whose port appears in the
    /// latest captured frame. Reads the frame already pushed by the tick
    /// loop above rather than re-evaluating signals.
    #[cfg(feature = "hw-emulation")]
    fn notify_peripherals(&mut self) -> Result<(), String> {
        let latest = self.frames.last().ok_or("no captured frame yet")?;
        let outputs = &self.outputs;
        let Some(active) = &mut self.active_sim else {
            return Ok(());
        };
        for (port, peripheral) in &mut active.peripherals {
            let (Some(&raw), Some(width)) = (
                latest.values.get(port),
                outputs.iter().find(|s| &s.name == port).map(|s| s.width),
            ) else {
                continue;
            };
            peripheral.on_change(&Val::new(raw, width.bits, width.signed));
        }
        Ok(())
    }

    /// One dashboard frame's wall-clock budget.
    #[cfg(feature = "hw-emulation")]
    fn frame_budget() -> std::time::Duration {
        std::time::Duration::from_millis(1000 / DASHBOARD_FPS)
    }

    /// Capture the current state as a timeline frame (for `--trace`).
    fn capture(&mut self) -> Result<(), String> {
        let values = self
            .sim
            .snapshot()?
            .into_iter()
            .map(|(name, v, _)| (name, v))
            .collect();
        self.frames.push(Frame {
            time: self.cycle * PERIOD,
            cycle: Some(self.cycle),
            values,
        });
        Ok(())
    }

    /// Build the teaching message for a failed `expect`: the source of the
    /// expression, the cycle, and — when it is a comparison — each side's value.
    fn fail_message(&self, e: &Expr) -> Result<String, Stop> {
        let mut msg = format!("expect {} — false at cycle {}", self.snippet(e), self.cycle);
        if let ExprKind::Binary { op, lhs, rhs } = &e.kind {
            if is_cmp(*op) {
                let l = self.sim.eval(lhs).map_err(Stop::Err)?;
                let r = self.sim.eval(rhs).map_err(Stop::Err)?;
                msg.push_str(&format!(
                    "\n  left  {} = {}\n  right {} = {}",
                    self.snippet(lhs),
                    show(l),
                    self.snippet(rhs),
                    show(r),
                ));
            }
        }
        Ok(msg)
    }

    /// The exact source text of `e` (its span), for a faithful message.
    fn snippet(&self, e: &Expr) -> &str {
        self.src
            .get(e.span.start..e.span.end)
            .map(str::trim)
            .unwrap_or("<expr>")
    }
}

fn is_cmp(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

/// Render a value the way a reader expects: signed values as a signed integer,
/// otherwise the unsigned magnitude.
fn show(v: Val) -> String {
    if v.signed {
        v.as_i128().to_string()
    } else {
        v.masked().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Outcome> {
        let f = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
        f.items
            .iter()
            .filter_map(|i| match i {
                ast::TopItem::Test(t) => {
                    Some(run_test(std::slice::from_ref(&f), src, t, false).expect("runs"))
                }
                _ => None,
            })
            .collect()
    }

    const COUNTER: &str = "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  \
        out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
        on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";

    fn passes(o: &Outcome) -> bool {
        matches!(o.result, TestResult::Pass)
    }

    #[test]
    fn a_passing_test_counts_its_checks() {
        let src = format!(
            "{COUNTER}\ntest \"counts up\" for Counter(WIDTH: 4) {{\n  \
             rst = 1\n  tick(clk)\n  expect count == 0\n  \
             rst = 0\n  tick(clk)\n  expect count == 1\n  \
             tick(clk, 3)\n  expect count == 4\n}}\n"
        );
        let outs = run(&src);
        assert_eq!(outs.len(), 1);
        assert!(passes(&outs[0]));
        assert_eq!(outs[0].checks, 3);
    }

    #[test]
    fn a_failing_expect_halts_with_a_teaching_message() {
        let src = format!(
            "{COUNTER}\ntest \"wrong\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  expect count == 9\n}}\n"
        );
        let outs = run(&src);
        match &outs[0].result {
            TestResult::Fail(m) => {
                assert!(m.contains("count == 9"), "no expression in: {m}");
                assert!(m.contains("left"), "no operand values in: {m}");
                assert!(m.contains("= 1"), "expected actual count=1 in: {m}");
            }
            TestResult::Pass => panic!("should have failed"),
        }
        // The check that failed is still counted.
        assert_eq!(outs[0].checks, 1);
    }

    #[test]
    fn drive_then_tick_feeds_an_input() {
        let src = "module Acc {\n  clock clk\n  reset rst\n  in x: bits[8]\n  out y: bits[8]\n  \
             reg r: bits[8] = 0\n  on rise(clk) { r <- r +% x }\n  y = r\n}\n\
             test \"adds x\" for Acc {\n  rst = 0\n  x = 7\n  tick(clk)\n  expect y == 7\n  \
             tick(clk)\n  expect y == 14\n}\n";
        let outs = run(src);
        assert!(passes(&outs[0]));
    }

    #[test]
    fn a_test_if_branches_on_state() {
        // `if` takes the true branch; the false branch's bogus expect never runs.
        let src = format!(
            "{COUNTER}\ntest \"branch\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  \
             if count == 1 {{ expect count == 1 }} else {{ expect count == 99 }}\n}}\n"
        );
        let outs = run(&src);
        assert!(passes(&outs[0]));
        assert_eq!(outs[0].checks, 1);
    }

    #[test]
    fn an_unknown_clock_is_an_error() {
        let src =
            format!("{COUNTER}\ntest \"bad clock\" for Counter(WIDTH: 4) {{\n  tick(nope)\n}}\n");
        let f = crate::parser::parse(crate::lexer::lex(&src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let err = run_test(std::slice::from_ref(&f), &src, decl, false).unwrap_err();
        assert!(err.contains("not a clock"), "got: {err}");
    }

    #[test]
    fn the_timeline_has_a_frame_per_tick() {
        let src = format!(
            "{COUNTER}\ntest \"frames\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
        );
        let outs = run(&src);
        // 1 initial frame + 3 ticks.
        assert_eq!(outs[0].timeline.frames.len(), 4);
        assert_eq!(outs[0].default_scope, vec!["count", "value"]);
    }

    #[cfg(feature = "hw-emulation")]
    #[test]
    fn sim_block_with_unknown_peripheral_errors() {
        let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\n\
                    test \"t\" for M {\n  sim {\n    bind playing -> speaker(waveform: square)\n  }\n  tick(clk)\n}\n";
        let f = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let err = run_test(std::slice::from_ref(&f), src, decl, false).unwrap_err();
        assert!(err.contains("unknown peripheral"), "got: {err}");
    }

    #[cfg(feature = "hw-emulation")]
    #[test]
    fn batch_sizes_splits_evenly() {
        assert_eq!(batch_sizes(100, 30), vec![30, 30, 30, 10]);
        assert_eq!(batch_sizes(0, 30), Vec::<u64>::new());
        assert_eq!(batch_sizes(5, 30), vec![5]);
    }

    #[cfg(feature = "hw-emulation")]
    #[test]
    fn cycles_per_frame_floors_to_one() {
        assert_eq!(cycles_per_frame(50_000_000), 50_000_000 / 30);
        assert_eq!(cycles_per_frame(10), 1); // sub-fps speed never batches to 0
    }

    #[test]
    fn tick_without_sim_block_is_unaffected() {
        // A test with no `sim` block must behave exactly as before this
        // feature existed — same Outcome shape, same cycle count.
        let src = format!(
            "{COUNTER}\ntest \"counts\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
        );
        let outs = run(&src);
        assert!(passes(&outs[0]));
        assert_eq!(outs[0].checks, 1);
    }

    #[cfg(feature = "hw-emulation")]
    #[test]
    fn emulate_true_without_tty_still_passes() {
        // Can't fake `IsTerminal` in-process without a real terminal, so this
        // test only proves the emulating-but-headless path (this test process
        // itself is never a TTY under `cargo test`) still produces a normal
        // Pass — i.e. passing `emulate: true` never breaks a test run even
        // when no terminal is attached, which is exactly the CI-safety
        // property `run_test` is supposed to guarantee.
        let src = format!(
            "{COUNTER}\ntest \"counts\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
        );
        let f = crate::parser::parse(crate::lexer::lex(&src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let outcome = run_test(std::slice::from_ref(&f), &src, decl, true).expect("runs");
        assert!(passes(&outcome));
    }
}
