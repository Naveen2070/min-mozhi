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
//!
//! A `sim { … }` block's peripheral binds talk only to [`EmulationHost`] — this
//! module has zero knowledge of ratatui/cpal. The caller (`mimz::emulate`, the
//! shell crate) implements the trait and decides `live`/`step` from real
//! terminal state; a headless/CI host just no-ops.

use std::collections::BTreeMap;

use mimz_core::ast::{self, BinOp, Expr, ExprKind, TestDecl, TestStmt};

use super::elaborate::{Signal, SimMode, Width, elaborate_project_with_mode};
use super::host::{Direction, EmulationHost};
use super::kernel::Sim;
use super::run::{Frame, MAX_SIM_CYCLES, Timeline};
use super::value::{self, Val};

/// Period between captured frames, in the timeline's time units (matches the
/// `mimz sim` clock period so a test trace lines up with a `sim` trace).
const PERIOD: u64 = 10;

/// Target dashboard refresh rate for frame-batched real-time pacing.
const DASHBOARD_FPS: u64 = 30;

/// Split `total` ticks into batches of at most `cycles_per_frame`, so a
/// `sim` block with `speed mhz(50)` doesn't try to sleep on every
/// individual 20ns cycle (physically impossible — OS sleep resolution is
/// ~1ms). Each batch runs instantly; the caller sleeps between batches to
/// match wall-clock time. Pure and terminal-free, so it's unit-testable
/// without a TTY.
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
    /// Per-tick capture, for `--trace` / `--trace=changes`. Empty (no
    /// frames) unless `run_test`'s `trace` parameter was `true`.
    pub timeline: Timeline,
    /// Default trace scope: the module's interface + state (inputs, outputs,
    /// registers), in that order.
    pub default_scope: Vec<String>,
    /// The user requested quit during this test — either by pressing `q`
    /// while paused mid-`--step` (surfaced by `EmulationHost::frame`), or at
    /// the final dismiss screen after a live run (surfaced by
    /// `EmulationHost::finish`). `false` for a headless/CI run. `mimz test`
    /// stops running further tests once this is `true`.
    pub quit: bool,
}

/// Whether a `test` block passed.
#[derive(Debug)]
pub enum TestResult {
    /// Every `expect` in the test block held.
    Pass,
    /// Failed with a teaching-quality message (already formatted, multi-line).
    Fail(String),
    /// Not run: the test has a `sim` block (only meaningful paced to
    /// real time under `--emulate`) but this run isn't live — running its
    /// body would just unbatched-tick the full simulated cycle count for
    /// no reason. Carries why it was skipped.
    Skipped(String),
}

/// Run one `test` block. `files` is the loaded project (`files[0]` the entry,
/// the rest its imports) so a module-under-test that instantiates a sub-module
/// from another file flattens; `src` is the entry source (for rendering
/// expressions in failure messages). `host` is the caller's view of
/// hardware-emulation peripherals (a headless/CI caller passes a no-op impl).
/// `live` is whether ticks should pace/redraw in real time (the caller has
/// already decided this from `--emulate` + a real terminal being attached);
/// `step` additionally forces single-cycle batches for interactive stepping.
/// `trace` is whether the caller will actually use `Outcome.timeline` (e.g.
/// `--trace` was passed) — skip it and every per-cycle full-signal snapshot
/// is skipped too, since capturing is real per-cycle overhead paid whether
/// or not anything ever reads the result.
/// `Err` is a setup/semantic error; a normal `expect` failure is
/// `Ok(Outcome { result: Fail(..), .. })`.
///
/// Defaults to [`SimMode::Warn`] for any `extern module` instance the test
/// reaches; see [`run_test_with_mode`] for an explicit mode (`mimz test
/// --extern-sim strict`).
pub fn run_test(
    files: &[ast::File],
    src: &str,
    decl: &TestDecl,
    host: Box<dyn EmulationHost>,
    live: bool,
    step: bool,
    trace: bool,
) -> Result<Outcome, String> {
    run_test_with_mode(files, src, decl, host, live, step, trace, SimMode::Warn)
}

/// Like [`run_test`], but takes an explicit [`SimMode`] for how an `extern
/// module` instance reached during elaboration is handled.
#[allow(clippy::too_many_arguments)]
pub fn run_test_with_mode(
    files: &[ast::File],
    src: &str,
    decl: &TestDecl,
    host: Box<dyn EmulationHost>,
    live: bool,
    step: bool,
    trace: bool,
    mode: SimMode,
) -> Result<Outcome, String> {
    let params = params(decl)?;
    let design = elaborate_project_with_mode(files, Some(&decl.module.name.name), &params, mode)?;

    let module = design.module.clone();
    let clocks = design.clocks.clone();
    let default_scope: Vec<String> = design
        .inputs
        .iter()
        .chain(&design.outputs)
        .map(|s| s.name.clone())
        .chain(design.regs.iter().map(|r| r.name.clone()))
        .collect();

    let outputs = design.outputs.clone();
    let inputs = design.inputs.clone();
    let sim = Sim::new(design);
    let signals: Vec<Signal> = sim
        .snapshot()?
        .into_iter()
        .map(|(name, _, width)| Signal { name, width })
        .collect();

    // A `sim` block's binds still get constructed/validated regardless of
    // `live` (so a bad bind still errors), but once a non-live run reaches a
    // `tick` with that `sim` block active, `TestStmt::Tick` bails out with
    // `Stop::Skip` instead of falling through to the unbatched path: that
    // path has no real-time pacing, so a `sim` block's `tick(clk, <large
    // N>)` — sized for real-time audio/UART, not a quick sanity check —
    // would just unbatched-tick N cycles for no reason.
    let stepping = step && live;
    let skip_reason = if live || !has_sim_block(&decl.body) {
        None
    } else {
        Some("needs --emulate to run its `sim` block".to_string())
    };

    let mut run = Run {
        sim,
        clocks,
        module: module.clone(),
        src,
        cycle: 0,
        frames: Vec::new(),
        checks: 0,
        outputs,
        inputs,
        active_sim: None,
        host,
        live,
        stepping,
        skip_reason,
        trace,
    };
    if run.trace {
        run.capture()?; // the initial (pre-tick) state
    }

    let (result, quit_from_exec) = match run.exec(&decl.body) {
        Ok(()) => (TestResult::Pass, false),
        Err(Stop::Fail(m)) => (TestResult::Fail(m), false),
        Err(Stop::Skip(m)) => (TestResult::Skipped(m), false),
        Err(Stop::Quit) => (TestResult::Skipped("aborted by user (q)".to_string()), true),
        Err(Stop::Err(e)) => return Err(e),
    };

    // Flush any peripheral that defers work to the end (`speaker`'s
    // offline-rendered playback) — a no-op host impl for everything else,
    // and for `speaker` itself when it never ticked (skipped, or no `sim`
    // block). For a live host this also runs the final "press Enter to
    // continue, q to quit" dismiss screen (unless the run already quit
    // mid-step), returning whether the user quit there.
    let quit_from_finish = run.host.finish()?;
    let quit = quit_from_exec || quit_from_finish;

    Ok(Outcome {
        name: decl.name.clone(),
        checks: run.checks,
        result,
        quit,
        timeline: Timeline {
            module,
            signals,
            frames: run.frames,
        },
        default_scope,
    })
}

/// Whether a test body has a `sim` block anywhere — including nested inside
/// an `if`/`else`, which the grammar allows since both branches reuse the
/// same `test_block` parser. Drives whether the `--emulate` degrade note
/// fires.
fn has_sim_block(body: &[TestStmt]) -> bool {
    body.iter().any(|s| match s {
        TestStmt::Sim(_) => true,
        TestStmt::If { then, els, .. } => {
            has_sim_block(then) || els.as_deref().is_some_and(has_sim_block)
        }
        _ => false,
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

/// How a test body stops early: a real `expect` failure (the test fails), a
/// setup/semantic error (the whole command errors), a `sim`-block tick that
/// isn't live and so isn't run (the test is skipped), or the user pressing
/// `q` mid-`--step` (the whole run is aborted).
enum Stop {
    Fail(String),
    Err(String),
    Skip(String),
    Quit,
}

/// The registered peripherals' real-world clock rate for the test's `sim`
/// block, if it has one, plus the bound port names — direction and dispatch
/// for each is looked up from `host` on demand
/// (`EmulationHost::direction_of`), so this crate never needs its own
/// peripheral registry.
struct ActiveSim {
    /// Real-world clock rate in Hz, if a `speed` clause was given.
    speed_hz: Option<u64>,
    /// Port names bound in this `sim` block, in bind order.
    bound: Vec<String>,
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
    /// `design` moved into `Sim::new` — used to validate `bind` targets
    /// against `Direction::Output` peripherals (`led`, `uart_tx`).
    outputs: Vec<Signal>,
    /// The module's input ports, same reasoning — used for
    /// `Direction::Input` peripherals (`uart_rx`).
    inputs: Vec<Signal>,
    active_sim: Option<ActiveSim>,
    /// The caller's view of hardware-emulation peripherals — a headless/CI
    /// caller passes a no-op impl; the shell passes one backed by
    /// ratatui/cpal.
    host: Box<dyn EmulationHost>,
    /// `--emulate` requested AND (decided by the caller) a real terminal is
    /// attached. Gates `TestStmt::Tick`'s batched pacing: when true and a
    /// `sim` block is active, ticks run in speed-sized batches, calling
    /// `host.frame()` once per batch instead of unthrottled.
    live: bool,
    /// `--step` requested AND live. Forces single-cycle batches — one
    /// `host.frame()` call per cycle — instead of speed-sized batches, for
    /// interactive stepping. Any actual pause-for-keypress lives entirely
    /// inside the host's `frame()` impl; this crate just drives it one
    /// cycle at a time.
    stepping: bool,
    /// Why this test would be skipped if a `tick` runs while a `sim` block
    /// is active and the run isn't live — `None` iff `live` (or the test
    /// has no `sim` block at all).
    skip_reason: Option<String>,
    /// Whether the caller will use `Outcome.timeline` at all (`--trace` was
    /// requested). `false` skips every per-cycle `capture()` — real
    /// overhead (a full-signal snapshot) paid on every simulated cycle
    /// whether or not anything ever reads the result.
    trace: bool,
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
                    // A `sim` block's binds are still constructed/validated
                    // above regardless of `live` — only the tick itself,
                    // sized for real-time pacing, is skipped when there's no
                    // real time to pace against.
                    if self.active_sim.is_some() && !self.live {
                        return Err(Stop::Skip(self.skip_reason.clone().unwrap_or_default()));
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
                    let limit = if self.live { u64::MAX } else { MAX_SIM_CYCLES };

                    if self.cycle.saturating_add(n) > limit {
                        let limit_str = if limit == u64::MAX {
                            "unlimited".to_string()
                        } else {
                            limit.to_string()
                        };
                        return Err(Stop::Err(format!(
                            "test exceeds the {limit_str}-cycle simulation limit"
                        )));
                    }
                    let batched = self.live && self.active_sim.is_some();
                    if batched {
                        let batch_size = if self.stepping {
                            1
                        } else {
                            self.batch_cycles()
                        };
                        for batch in batch_sizes(n, batch_size) {
                            let started = std::time::Instant::now();
                            for _ in 0..batch {
                                self.drive_peripherals().map_err(Stop::Err)?;
                                self.sim.tick(&clock.name).map_err(Stop::Err)?;
                                self.cycle += 1;
                                if self.trace && self.cycle <= 1_000_000 {
                                    self.capture().map_err(Stop::Err)?;
                                }
                                self.notify_on_tick().map_err(Stop::Err)?;
                            }
                            self.notify_peripherals().map_err(Stop::Err)?;
                            // `frame` returns `true` only when the user quit
                            // at a `--step` pause — abort the whole run, as
                            // the pre-refactor `wait_for_step` path did.
                            if self.host.frame(self.cycle).map_err(Stop::Err)? {
                                return Err(Stop::Quit);
                            }
                            if !self.stepping
                                && let Some(remaining) =
                                    Self::frame_budget().checked_sub(started.elapsed())
                                && self.active_sim.as_ref().and_then(|a| a.speed_hz).is_some()
                            {
                                std::thread::sleep(remaining);
                            }
                        }
                    } else {
                        for _ in 0..n {
                            self.sim.tick(&clock.name).map_err(Stop::Err)?;
                            self.cycle += 1;
                            if self.trace && self.cycle <= 1_000_000 {
                                self.capture().map_err(Stop::Err)?;
                            }
                        }
                    }
                }
                TestStmt::Expect(e) => {
                    self.checks += 1;
                    let v = self.sim.eval(e).map_err(Stop::Err)?;
                    if v.unknown || v.bits & 1 != 1 {
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
                    let mut bound = Vec::new();
                    for b in &block.binds {
                        let width = match self.host.direction_of(b.peripheral.name.as_str()) {
                            Some(Direction::Output) => match self.port_width(&b.port.name) {
                                Some(w) => w,
                                None => {
                                    let msg = if self.input_width(&b.port.name).is_some() {
                                        format!(
                                            "`{}` binds to an output port, but `{}` is an input",
                                            b.peripheral.name, b.port.name
                                        )
                                    } else {
                                        format!(
                                            "`{}` has no output port `{}` to bind",
                                            self.module, b.port.name
                                        )
                                    };
                                    return Err(Stop::Err(msg));
                                }
                            },
                            Some(Direction::Input) => match self.input_width(&b.port.name) {
                                Some(w) => w,
                                None => {
                                    let msg = if self.port_width(&b.port.name).is_some() {
                                        format!(
                                            "`{}` binds to an input port, but `{}` is an output",
                                            b.peripheral.name, b.port.name
                                        )
                                    } else {
                                        format!(
                                            "`{}` has no input port `{}` to bind",
                                            self.module, b.port.name
                                        )
                                    };
                                    return Err(Stop::Err(msg));
                                }
                            },
                            None => {
                                return Err(Stop::Err(format!(
                                    "unknown peripheral `{}`",
                                    b.peripheral.name
                                )));
                            }
                        };
                        self.host
                            .bind(
                                b.port.name.as_str(),
                                b.peripheral.name.as_str(),
                                width,
                                &b.args,
                                speed_hz,
                            )
                            .map_err(Stop::Err)?;
                        bound.push(b.port.name.clone());
                    }
                    self.active_sim = Some(ActiveSim { speed_hz, bound });
                }
                // Unreachable: the sim runs on a strict-parsed tree, which
                // carries no `Error` placeholder.
                TestStmt::Error(_) => {}
            }
        }
        Ok(())
    }

    /// The folded `Width` of an output port, or `None` if `name` isn't
    /// one — used to validate `Direction::Output` binds (`led`,
    /// `uart_tx`). See `input_width` for `Direction::Input` binds.
    fn port_width(&self, name: &str) -> Option<Width> {
        self.outputs
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.width)
    }

    /// The folded `Width` of an input port, or `None` if `name` isn't one
    /// — used to validate `uart_rx`-style binds, which drive an INPUT
    /// rather than observe an OUTPUT (`port_width`'s job).
    fn input_width(&self, name: &str) -> Option<Width> {
        self.inputs.iter().find(|s| s.name == name).map(|s| s.width)
    }

    /// How many ticks make up one batch: the declared `speed`'s
    /// cycles-per-frame, or `u64::MAX` (one batch, no pacing) if the `sim`
    /// block gave no `speed`.
    fn batch_cycles(&self) -> u64 {
        self.active_sim
            .as_ref()
            .and_then(|a| a.speed_hz)
            .map(cycles_per_frame)
            .unwrap_or(u64::MAX)
    }

    /// Call `on_change` on the host for every bound OUTPUT port — reads the
    /// signal's live value straight from `self.sim`, NOT from a captured
    /// frame: capture stops past `MAX_SIM_CYCLES`'s 1M-frame cap (see
    /// `capture`), so a frame is stale forever after that point while the
    /// sim keeps ticking underneath it.
    fn notify_peripherals(&mut self) -> Result<(), String> {
        let Some(active) = &self.active_sim else {
            return Ok(());
        };
        let bound = active.bound.clone();
        for port in &bound {
            let Some(width) = self
                .outputs
                .iter()
                .find(|s| &s.name == port)
                .map(|s| s.width)
            else {
                continue;
            };
            let raw = self.sim.peek(port)?;
            self.host
                .on_change(port, &Val::new(raw, width.bits, width.signed));
        }
        Ok(())
    }

    /// Call `on_tick` on the host for every bound peripheral, every
    /// individual cycle (not just batch-end) — the hook `uart_tx`'s decoder
    /// needs; a cheap no-op host-side for peripherals that don't care
    /// (`led`). Searches both `outputs` and `inputs` for the port's width
    /// since a peripheral may be bound to either. Reads the live value via
    /// `self.sim.peek`, same staleness reasoning as `notify_peripherals`.
    fn notify_on_tick(&mut self) -> Result<(), String> {
        let Some(active) = &self.active_sim else {
            return Ok(());
        };
        let bound = active.bound.clone();
        for port in &bound {
            let width = self
                .outputs
                .iter()
                .chain(self.inputs.iter())
                .find(|s| &s.name == port)
                .map(|s| s.width);
            let Some(width) = width else { continue };
            let raw = self.sim.peek(port)?;
            self.host
                .on_tick(port, &Val::new(raw, width.bits, width.signed))?;
        }
        Ok(())
    }

    /// Call `drive` on the host for every bound peripheral before the
    /// cycle's tick, applying any returned bit to that peripheral's port.
    /// Collects all (port, bit) pairs before applying them so the loop
    /// never holds a borrow of `self.active_sim`/`self.host` at the same
    /// time `self.sim.set` needs one of `self.sim`.
    fn drive_peripherals(&mut self) -> Result<(), String> {
        let Some(active) = &self.active_sim else {
            return Ok(());
        };
        let bound = active.bound.clone();
        let mut sets = Vec::new();
        for port in &bound {
            if let Some(bit) = self.host.drive(port) {
                sets.push((port.clone(), bit));
            }
        }
        for (port, bit) in sets {
            self.sim.set(&port, bit as u128)?;
        }
        Ok(())
    }

    /// One dashboard frame's wall-clock budget.
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
        if let ExprKind::Binary { op, lhs, rhs } = &e.kind
            && is_cmp(*op)
        {
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

    /// Minimal `EmulationHost` test double — only `led`/`speaker`/`uart_tx`
    /// (Output) and `uart_rx` (Input) are known peripherals, mirroring the
    /// real registry closely enough that the bind-validation tests below
    /// exercise the same "unknown peripheral" / direction-mismatch paths.
    struct NullHost;
    impl EmulationHost for NullHost {
        fn bind(
            &mut self,
            _port: &str,
            peripheral: &str,
            _width: Width,
            _args: &[ast::BindArg],
            _speed_hz: Option<u64>,
        ) -> Result<(), String> {
            match peripheral {
                "led" | "speaker" | "uart_tx" | "uart_rx" => Ok(()),
                other => Err(format!("unknown peripheral `{other}`")),
            }
        }
        fn direction_of(&self, name: &str) -> Option<Direction> {
            match name {
                "led" | "speaker" | "uart_tx" => Some(Direction::Output),
                "uart_rx" => Some(Direction::Input),
                _ => None,
            }
        }
        fn on_change(&mut self, _name: &str, _val: &Val) {}
        fn on_tick(&mut self, _name: &str, _val: &Val) -> Result<(), String> {
            Ok(())
        }
        fn drive(&mut self, _name: &str) -> Option<u64> {
            None
        }
        fn frame(&mut self, _cycle: u64) -> Result<bool, String> {
            Ok(false)
        }
        fn finish(&mut self) -> Result<bool, String> {
            Ok(false)
        }
    }

    fn run_test_headless(
        files: &[ast::File],
        src: &str,
        decl: &TestDecl,
    ) -> Result<Outcome, String> {
        run_test(files, src, decl, Box::new(NullHost), false, false, true)
    }

    fn run(src: &str) -> Vec<Outcome> {
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        f.items
            .iter()
            .filter_map(|i| match i {
                ast::TopItem::Test(t) => {
                    Some(run_test_headless(std::slice::from_ref(&f), src, t).expect("runs"))
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
            TestResult::Skipped(reason) => panic!("should have failed, was skipped: {reason}"),
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
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(&src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let err = run_test_headless(std::slice::from_ref(&f), &src, decl).unwrap_err();
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

    #[test]
    fn trace_false_skips_every_capture() {
        // A caller that never reads `Outcome.timeline` (e.g. `mimz test`
        // without `--trace`) shouldn't pay for a full-signal snapshot on
        // every simulated cycle.
        let src = format!(
            "{COUNTER}\ntest \"notrace\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
        );
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(&src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let outcome = run_test(
            std::slice::from_ref(&f),
            &src,
            decl,
            Box::new(NullHost),
            false,
            false,
            false,
        )
        .expect("runs");
        assert!(passes(&outcome));
        assert_eq!(
            outcome.timeline.frames.len(),
            0,
            "trace: false must skip every capture(), including the initial one"
        );
    }

    #[test]
    fn sim_block_with_unknown_peripheral_errors() {
        let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\n\
                    test \"t\" for M {\n  sim {\n    bind playing -> microphone()\n  }\n  tick(clk)\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
        assert!(err.contains("unknown peripheral"), "got: {err}");
    }

    #[test]
    fn sim_block_with_unknown_port_errors() {
        let src = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\n\
                    test \"t\" for M {\n  sim {\n    bind nope -> led()\n  }\n  tick(clk)\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
        assert!(err.contains("nope"), "got: {err}");
    }

    #[test]
    fn sim_block_binding_an_input_to_an_output_peripheral_errors() {
        let src = "module M {\n  clock clk\n  in start: bit\n  out playing: bit\n  playing = start\n}\n\
                    test \"t\" for M {\n  sim {\n    bind start -> led()\n  }\n  tick(clk)\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
        // `start` genuinely exists as an input — this must produce the
        // direction-aware message, not the generic "no such port" one
        // (which would also happen to contain "output port" and "start",
        // so asserting on the specific phrase is what proves the
        // mismatch was actually detected, not coincidental).
        assert!(err.contains("binds to an output port, but"), "got: {err}");
    }

    #[test]
    fn sim_block_binding_an_output_to_an_input_peripheral_errors() {
        let src = "module M {\n  clock clk\n  in start: bit\n  out playing: bit\n  playing = start\n}\n\
                    test \"t\" for M {\n  sim {\n    bind playing -> uart_rx()\n  }\n  tick(clk)\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let err = run_test_headless(std::slice::from_ref(&f), src, decl).unwrap_err();
        // Mirror of the test above: `playing` genuinely exists as an output
        // — this must produce the direction-aware message, not the generic
        // "no such port" one.
        assert!(err.contains("binds to an input port, but"), "got: {err}");
    }

    #[test]
    fn sim_block_with_speaker_bound_runs_fine_without_emulate() {
        let src = "module M {\n  clock clk\n  in start: bit\n  out tone: bit\n  tone = start\n}\n\
                    test \"t\" for M {\n  start = 1\n  sim {\n    speed mhz(1)\n    bind tone -> speaker()\n  }\n  tick(clk, 4)\n}\n";
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        // `live: false` (second-to-last arg) — `on_tick` never runs in this
        // mode, so `speaker`'s real audio device is never touched even
        // though it's bound. This is the proof that a headless/CI run is
        // safe.
        run_test_headless(std::slice::from_ref(&f), src, decl)
            .expect("test passes without touching audio hardware");
    }

    #[test]
    fn batch_sizes_splits_evenly() {
        assert_eq!(batch_sizes(100, 30), vec![30, 30, 30, 10]);
        assert_eq!(batch_sizes(0, 30), Vec::<u64>::new());
        assert_eq!(batch_sizes(5, 30), vec![5]);
    }

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

    #[test]
    fn has_sim_block_only_true_when_a_sim_block_is_present() {
        // A body with no `sim` block (the common case — most tests never
        // touch emulation) must not trigger the degrade note.
        let no_sim = format!(
            "{COUNTER}\ntest \"t\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  expect count == 1\n}}\n"
        );
        assert!(!has_sim_block(&test_body(&no_sim)));

        // A top-level `sim` block is detected.
        let with_sim = "module M {\n  clock clk\n  out playing: bit\n  playing = 1\n}\n\
                         test \"t\" for M {\n  sim {\n    bind playing -> led()\n  }\n  tick(clk)\n}\n";
        assert!(has_sim_block(&test_body(with_sim)));

        // A `sim` block nested inside an `if`/`else` branch is also detected
        // (the grammar allows it — `if`'s then/else reuse `test_block`).
        let nested = format!(
            "{COUNTER}\ntest \"t\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk)\n  \
             if count == 1 {{ expect count == 1 }} else {{ sim {{ bind count -> led() }} }}\n}}\n"
        );
        assert!(has_sim_block(&test_body(&nested)));
    }

    fn test_body(src: &str) -> Vec<TestStmt> {
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(src).expect("lexes")).expect("parses");
        f.items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t.body.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn live_true_without_a_dashboard_still_passes() {
        // Proves passing `live: true` never breaks a headless test run even
        // with a no-op host — the CI-safety property `run_test` is supposed
        // to guarantee now that dashboard interactivity lives entirely in
        // the caller's `EmulationHost` impl.
        let src = format!(
            "{COUNTER}\ntest \"counts\" for Counter(WIDTH: 4) {{\n  \
             rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
        );
        let f =
            mimz_core::parser::parse(mimz_core::lexer::lex(&src).expect("lexes")).expect("parses");
        let decl = f
            .items
            .iter()
            .find_map(|i| match i {
                ast::TopItem::Test(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let outcome = run_test(
            std::slice::from_ref(&f),
            &src,
            decl,
            Box::new(NullHost),
            true,
            false,
            true,
        )
        .expect("runs");
        assert!(passes(&outcome));
    }
}
