//! Turns a panic inside `ir::lower` or `ir::opt::optimize` into a
//! classified [`Failure`] a caller can report instead of crashing. See
//! `docs/superpowers/specs/2026-10-01-ir-pipeline-cli-design.local.md`.
//!
//! One panic hook is installed per process, on first use, and chains to the
//! hook it replaced: it records only for a thread that is inside [`catch`].
//! Swapping hooks per call would race when two threads catch at once.

use crate::span::Span;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

/// Which pipeline step panicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Lower,
    Optimize,
}

/// Root-cause class of a [`Failure`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The IR cannot lower this construct yet (`unimplemented!`, or the
    /// `S0227` loop budget).
    Limitation,
    /// A broken invariant: a compiler bug.
    Internal,
}

/// A caught panic from [`catch`].
pub struct Failure {
    pub stage: Stage,
    pub kind: FailureKind,
    /// The panic message.
    pub message: String,
    /// `file:line:col` of the `panic!` itself.
    pub location: Option<String>,
    /// Innermost non-synthesized span being lowered when it panicked.
    pub span: Option<Span>,
    /// Only when `catch` was asked for one.
    pub backtrace: Option<String>,
    payload: Box<dyn Any + Send>,
}

impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failure")
            .field("stage", &self.stage)
            .field("kind", &self.kind)
            .field("message", &self.message)
            .field("location", &self.location)
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

impl Failure {
    /// Re-raises the original panic.
    pub fn resume(self) -> ! {
        panic::resume_unwind(self.payload)
    }
}

#[derive(Default)]
struct Captured {
    location: Option<String>,
    span: Option<Span>,
    backtrace: Option<String>,
}

thread_local! {
    static SPANS: RefCell<Vec<Span>> = const { RefCell::new(Vec::new()) };
    /// `Some(want_backtrace)` while this thread is inside `catch`.
    static ARMED: Cell<Option<bool>> = const { Cell::new(None) };
    static CAPTURED: RefCell<Option<Captured>> = const { RefCell::new(None) };
}

static HOOK: Once = Once::new();

fn install_hook() {
    HOOK.call_once(|| {
        let prev = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let Some(want_backtrace) = ARMED.with(Cell::get) else {
                return prev(info);
            };
            let span = SPANS.with(|s| {
                s.try_borrow()
                    .ok()
                    .and_then(|v| v.iter().rev().find(|s| **s != Span::default()).copied())
            });
            let captured = Captured {
                location: info
                    .location()
                    .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
                span,
                backtrace: want_backtrace
                    .then(|| std::backtrace::Backtrace::force_capture().to_string()),
            };
            CAPTURED.with(|c| *c.borrow_mut() = Some(captured));
        }));
    });
}

/// Pops the span [`enter_span`] pushed.
pub(crate) struct SpanGuard(());

impl Drop for SpanGuard {
    fn drop(&mut self) {
        SPANS.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

/// Marks `span` as being lowered until the guard drops.
#[must_use]
pub(crate) fn enter_span(span: Span) -> SpanGuard {
    SPANS.with(|s| s.borrow_mut().push(span));
    SpanGuard(())
}

#[cfg(test)]
pub(crate) fn span_depth() -> usize {
    SPANS.with(|s| s.borrow().len())
}

/// Runs `f`; a panic inside it comes back as a classified [`Failure`]
/// instead of unwinding further. Prints nothing for a caught panic.
pub fn catch<T>(stage: Stage, want_backtrace: bool, f: impl FnOnce() -> T) -> Result<T, Failure> {
    install_hook();
    let depth = SPANS.with(|s| s.borrow().len());
    let outer = ARMED.with(|a| a.replace(Some(want_backtrace)));
    CAPTURED.with(|c| c.borrow_mut().take());
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    ARMED.with(|a| a.set(outer));
    // Unwinding already popped the guards; truncating guards against a leak.
    SPANS.with(|s| s.borrow_mut().truncate(depth));
    let payload = match result {
        Ok(v) => return Ok(v),
        Err(p) => p,
    };
    let captured = CAPTURED.with(|c| c.borrow_mut().take()).unwrap_or_default();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic payload>".to_string());
    Err(Failure {
        stage,
        kind: classify(stage, &message),
        message,
        location: captured.location,
        span: captured.span,
        backtrace: captured.backtrace,
        payload,
    })
}

fn classify(stage: Stage, message: &str) -> FailureKind {
    // ponytail: classified by message text; a typed error from `lower` would
    // replace this if lowering ever returns `Result`.
    if stage == Stage::Lower
        && (message.starts_with("not implemented") || message.contains("(S0227)"))
    {
        FailureKind::Limitation
    } else {
        FailureKind::Internal
    }
}
