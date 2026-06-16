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
//! This is the shipped `tick`/`expect` form (spec/02 §1.10, spec/05 §3). The
//! `await clk.cycles(n)` sugar is a later increment: it needs the `await`
//! Tamil/Tanglish spelling from native review (R9/R11) before it can be parsed.
//!
//! Like `mimz sim` / `mimz eval`, this runs on the parsed AST without the
//! checker — semantic problems surface as runtime errors. Every run also
//! captures a [`Timeline`] (one frame per tick) so `--trace` works here too,
//! riding the same per-cycle snapshot the VCD writer consumes.

use std::collections::BTreeMap;

use crate::ast::{self, BinOp, Expr, ExprKind, TestDecl, TestStmt};

use super::elaborate::{Signal, elaborate};
use super::kernel::Sim;
use super::run::{Frame, MAX_SIM_CYCLES, Timeline};
use super::value::{self, Val};

/// Period between captured frames, in the timeline's time units (matches the
/// `mimz sim` clock period so a test trace lines up with a `sim` trace).
const PERIOD: u64 = 10;

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

/// Run one `test` block from `file` (whose source is `src`, for rendering
/// expressions in failure messages). `Err` is a setup/semantic error (bad
/// parameter, unknown module/clock/input, an unsupported construct); a normal
/// `expect` failure is `Ok(Outcome { result: Fail(..), .. })`.
pub fn run_test(file: &ast::File, src: &str, decl: &TestDecl) -> Result<Outcome, String> {
    let params = params(decl)?;
    let design = elaborate(file, Some(&decl.module.name), &params)?;

    let module = design.module.clone();
    let clocks = design.clocks.clone();
    let default_scope: Vec<String> = design
        .inputs
        .iter()
        .chain(&design.outputs)
        .map(|s| s.name.clone())
        .chain(design.regs.iter().map(|r| r.name.clone()))
        .collect();

    let sim = Sim::new(design);
    let signals: Vec<Signal> = sim
        .snapshot()?
        .into_iter()
        .map(|(name, _, width)| Signal { name, width })
        .collect();

    let mut run = Run {
        sim,
        clocks,
        module: module.clone(),
        src,
        cycle: 0,
        frames: Vec::new(),
        checks: 0,
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
                    for _ in 0..n {
                        self.sim.tick(&clock.name).map_err(Stop::Err)?;
                        self.cycle += 1;
                        self.capture().map_err(Stop::Err)?;
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
            }
        }
        Ok(())
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
                ast::TopItem::Test(t) => Some(run_test(&f, src, t).expect("runs")),
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
        let err = run_test(&f, &src, decl).unwrap_err();
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
}
