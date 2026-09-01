//! Teaching diagnostics: every error says WHAT is wrong and, where possible,
//! HOW to fix it (spec/01 G1). Rendered with source excerpt and carets.
//! (English catalog first; Tanglish/Tamil catalogs land with Phase 1.8.)

use crate::lexer::token::Flavor;
use crate::span::Span;

/// Every stable checker error code (catalog: docs/code/11-checker.md).
/// THE machine-readable list — `tests/errors.rs` guards it against the
/// human catalog and demands an end-to-end fixture per code, and
/// `mimz-bench` measures fixture coverage against it. Append-only by
/// the E-code stability contract (codes are never renumbered).
pub const ALL_CHECKER_CODES: [&str; 76] = [
    "E0001", "E0002", "E0003", "E0004", "E0101", "E0102", "E0103", "E0104", "E0105", "E0106",
    "E0107", "E0108", "E0109", "E0110", "E0111", "E0201", "E0202", "E0301", "E0302", "E0303",
    "E0401", "E0402", "E0403", "E0404", "E0405", "E0406", "E0407", "E0408", "E0409", "E0410",
    "E0411", "E0412", "E0413", "E0414", "E0415", "E0416", "E0417", "E0418", "E0419", "E0420",
    "E0501", "E0502", "E0503", "E0504", "E0505", "E0601", "E0602", "E0701", "E0702", "E0703",
    "E0704", "E0705", "E0801", "E0802", "E0803", "E0804", "E0805", "E0806", "E0807", "E0808",
    "E0809", "E0810", "E0811", "E0812", "E0813", "E0901", "E0902", "E0903", "E0906", "E0907",
    "E0909", "E0910", "E0911", "E0912", "E1301", "E1302",
];

/// How loud a diagnostic is. `Error` fails the build; `Warning` is advisory —
/// it is printed but the command still succeeds (exit 0) and still produces
/// output. Almost every `Diag` is an `Error`; warnings are opt-in via
/// [`Diag::as_warning`] (e.g. the mixed-flavor lint, W0001).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Fails the build; exit code reflects it.
    Error,
    /// Advisory only — printed, but the command still exits 0.
    Warning,
}

/// One compiler diagnostic. Diagnostics are plain values — passes collect
/// `Vec<Diag>` and keep going (multi-error reporting), they never panic
/// or print directly. Rendering happens once, in [`render`].
#[derive(Clone, Debug)]
pub struct Diag {
    /// Where in the source the problem is (drives the caret underline).
    pub span: Span,
    /// WHAT is wrong — one sentence, names the construct.
    pub msg: String,
    /// HOW to fix it — the teaching line, ideally with a spec reference.
    pub help: Option<String>,
    /// Which project file the span points into (index into the loaded
    /// file list). `None` in single-file passes (lexer, parser), where
    /// the caller already knows the file. Project-wide passes
    /// (`Project::from_files`, the checker, the emitter) MUST set this —
    /// see `project::render_diags`.
    pub file: Option<usize>,
    /// Stable code (`E0101` error, `W0001` warning), rendered as
    /// `error[E0101]: ...` / `warning[W0001]: ...`. Catalog lives in
    /// docs/code/11-checker.md + 06-diagnostics.md. Checker errors always
    /// carry one; lexer/parser errors will be retrofitted (Phase 1).
    pub code: Option<&'static str>,
    /// Error (fails the build) or Warning (advisory; exit 0). Defaults to
    /// `Error` in [`Diag::new`]; flip with [`Diag::as_warning`].
    pub severity: Severity,
    /// Structured interpolation args for the localized catalog, `(token, value)`
    /// — e.g. `("expected", "bits[8]")`. The English `msg` already bakes these
    /// in via `format!`; this carries the SAME values to `morph::fill` so a
    /// localized template can interpolate `{expected}` etc. Empty for most
    /// diagnostics. The `--json` and English paths ignore it.
    pub args: Vec<(&'static str, String)>,
}

impl Diag {
    /// A plain `Error`-severity diagnostic with no help text, code, or file
    /// index yet — attach those with the builder methods below.
    pub fn new(span: Span, msg: impl Into<String>) -> Self {
        Diag {
            span,
            msg: msg.into(),
            help: None,
            file: None,
            code: None,
            severity: Severity::Error,
            args: Vec::new(),
        }
    }

    /// Builder-style: attach a `(token, value)` interpolation arg for the
    /// localized catalog (the localizer fills `{token}`; see `morph::fill`).
    pub fn with_arg(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.args.push((key, value.into()));
        self
    }

    /// Builder-style: attach the `= help:` line.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Builder-style: record which project file the span points into.
    pub fn with_file(mut self, file: usize) -> Self {
        self.file = Some(file);
        self
    }

    /// Builder-style: attach the stable error code.
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    /// Builder-style: mark this diagnostic as a non-fatal warning (advisory;
    /// the command still succeeds and still produces output).
    pub fn as_warning(mut self) -> Self {
        self.severity = Severity::Warning;
        self
    }

    /// Whether this diagnostic should fail the build.
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Render diagnostics against the (NFC-normalized) source they refer to.
/// rustc-style shape:
///
/// ```text
/// error: register `value` has no reset value
///   --> examples/english/counter.mimz:7:3
///    |
///   7|   reg value: bits[WIDTH]
///    |   ^^^
///    = help: every reg declares its reset value ...
/// ```
///
/// Messages render in English. For another error `flavor` see [`render_lang`].
pub fn render(diags: &[Diag], src: &str, path: &str) -> String {
    render_lang(diags, src, path, Flavor::English)
}

use std::sync::atomic::{AtomicBool, Ordering};

static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable colorized diagnostics output globally.
pub fn set_color_enabled(enabled: bool) {
    COLOR_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check if colorized diagnostics output is globally enabled.
pub fn is_color_enabled() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

/// Like [`render`], but emits each message in `flavor` when the localized
/// catalog covers its E-code (`morph::localized_msg`); otherwise the English
/// `msg` is used verbatim. The caret/location/help layout is identical — only
/// the WHAT line is localized (Phase 1.8, spec/04 section 5).
pub fn render_lang(diags: &[Diag], src: &str, path: &str, flavor: Flavor) -> String {
    use owo_colors::OwoColorize;
    let mut out = String::new();
    let use_color = is_color_enabled();
    for d in diags {
        let (line_no, col, line_text, line_start) = locate(src, d.span.start);
        let msg = crate::morph::localized_msg(d, src, flavor).unwrap_or_else(|| d.msg.clone());

        let label = match d.severity {
            Severity::Error => {
                if use_color {
                    "error".red().bold().to_string()
                } else {
                    "error".to_string()
                }
            }
            Severity::Warning => {
                if use_color {
                    "warning".yellow().bold().to_string()
                } else {
                    "warning".to_string()
                }
            }
        };

        let msg_styled = if use_color {
            msg.bold().to_string()
        } else {
            msg.clone()
        };

        match d.code {
            Some(c) => {
                let code_styled = if use_color {
                    format!("[{c}]").bold().to_string()
                } else {
                    format!("[{c}]")
                };
                let code_colored = match d.severity {
                    Severity::Error => {
                        if use_color {
                            code_styled.red().to_string()
                        } else {
                            code_styled
                        }
                    }
                    Severity::Warning => {
                        if use_color {
                            code_styled.yellow().to_string()
                        } else {
                            code_styled
                        }
                    }
                };
                out.push_str(&format!("{label}{code_colored}: {msg_styled}\n"));
            }
            None => out.push_str(&format!("{label}: {msg_styled}\n")),
        }

        let arrow = if use_color {
            "-->".bright_blue().bold().to_string()
        } else {
            "-->".to_string()
        };
        out.push_str(&format!("  {arrow} {path}:{line_no}:{col}\n"));

        let pipe = if use_color {
            "|".bright_blue().bold().to_string()
        } else {
            "|".to_string()
        };
        out.push_str(&format!("   {pipe}\n"));

        let line_no_styled = if use_color {
            format!("{line_no:>3}").bright_blue().bold().to_string()
        } else {
            format!("{line_no:>3}")
        };
        out.push_str(&format!("{line_no_styled} {pipe} {line_text}\n"));

        // Caret underline: from span start to span end, clamped to the line.
        let in_line_start = d.span.start - line_start;
        let len = (d.span.end.saturating_sub(d.span.start)).max(1);
        let len = len.min(line_text.len().saturating_sub(in_line_start).max(1));
        let pad = line_text[..in_line_start.min(line_text.len())]
            .chars()
            .count();

        let carets = "^".repeat(len);
        let carets_styled = match d.severity {
            Severity::Error => {
                if use_color {
                    carets.red().bold().to_string()
                } else {
                    carets
                }
            }
            Severity::Warning => {
                if use_color {
                    carets.yellow().bold().to_string()
                } else {
                    carets
                }
            }
        };
        out.push_str(&format!("   {pipe} {}{}\n", " ".repeat(pad), carets_styled));

        if let Some(h) = &d.help {
            let help_label = if use_color {
                "= help:".bright_blue().bold().to_string()
            } else {
                "= help:".to_string()
            };
            out.push_str(&format!("   {help_label} {h}\n"));
        }
        out.push('\n');
    }
    out
}

/// One diagnostic in the `--json` wire format (docs/code/06): the stable
/// machine-readable contract for editors and the npm/PyPI wrappers.
/// Positions are 1-based line/column (columns count CHARS, matching the
/// human renderer); the byte span is included for exact tooling.
#[derive(serde::Serialize)]
pub struct JsonDiag {
    /// `"error"` or `"warning"` — the diagnostic's severity.
    pub severity: &'static str,
    /// Stable code (`"E0101"`/`"W0001"`), or `null` for pre-code diagnostics.
    pub code: Option<&'static str>,
    /// WHAT is wrong.
    pub message: String,
    /// HOW to fix it (the teaching line), when present.
    pub help: Option<String>,
    /// The file the span points into.
    pub path: String,
    /// 1-based line of the span start.
    pub line: usize,
    /// 1-based character column of the span start.
    pub col: usize,
    /// Byte offsets `[start, end)` into the NFC-normalized source.
    pub span: (usize, usize),
}

impl JsonDiag {
    /// Resolve a [`Diag`] against the source it points into.
    pub fn new(d: &Diag, path: &str, src: &str) -> Self {
        let (line, col, _, _) = locate(src, d.span.start);
        JsonDiag {
            severity: match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            code: d.code,
            message: d.msg.clone(),
            help: d.help.clone(),
            path: path.to_string(),
            line,
            col,
            span: (d.span.start, d.span.end),
        }
    }
}

/// Every stable `mimz-sim` runtime error code (R2:
/// `docs/audit/review-2026-07-17.md`). Moved here from `mimz-sim`'s own
/// `sim::diag` (Phase 2 IR Task 1) alongside [`bridge_code`]/
/// [`diag_from_bridged`] so `mimz-core`'s `value`/`comb` evaluators can
/// construct properly-coded diagnostics across the `Resolver` string
/// boundary without `mimz-core` depending on `mimz-sim`; `mimz-sim`'s
/// `sim::diag` re-exports this so every existing call site there keeps
/// compiling unchanged.
///
/// These are a DIFFERENT catalog from the checker's `E0xxx` codes
/// (`ALL_CHECKER_CODES`) — a checker code fires at compile time, before a
/// design ever runs; an `S0xxx` code fires at elaboration/execution time,
/// after the checker has already accepted the program. Four ranges by
/// category: `S01xx` elaboration/wiring, `S02xx` expression evaluation,
/// `S03xx` test-harness control flow, `S04xx` peripheral bind. Append-only,
/// like the checker's own catalog — a code is never renumbered or reused
/// once assigned.
///
/// Every code here must have a firing fixture in the sim-errors contract
/// test.
///
/// `S0101` was retired 2026-08-01 (BUG-26): `resolve_module`'s own "unknown
/// module" arm was dead code (its only caller always pre-checks the same
/// lookup first), so the append-only stability contract never applied to
/// it — nothing could have observed it firing, since it never did.
///
/// `S0125` was retired 2026-08-12 (round-4 plan Task 8, BUG-53's own
/// check/sim/emit split): "nested `repeat` is not supported" was a real,
/// reachable condition, but the checker's own `no_decls_in_repeat` (E0303)
/// already treats a nested `repeat` as legal hardware generation — the
/// simulator's repeat-body walk was a separate, hand-restricted loop that
/// hadn't caught up. That walk is now the same recursive one `run_worklist`'s
/// own `ConstIf` arm already used, so nested `repeat` elaborates for real and
/// this code's only emission site is gone — same "nothing could observe it
/// firing" reasoning as `S0101`, just because the FEATURE landed rather than
/// because the arm was always dead.
pub const ALL_SIM_CODES: [&str; 80] = [
    // S01xx — elaboration/wiring (sim/elaborate/registry.rs, instance.rs,
    // mod.rs, module.rs, rewrite.rs).
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
    "S0126", // a `repeat` body item is neither an instance, a drive, a nested `repeat`, nor `const if`
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
    "S0222", // shift growth exceeds MAX_WIDTH
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
    // S02xx — the event-driven kernel (sim/kernel.rs). S0238 is REUSED here
    // too (`CombEnv::signal`'s own cycle detection, BUG-27 fix) — the
    // structurally-identical condition, just checked in the real
    // multi-module simulator's per-cycle resolver instead of `mimz eval`'s
    // single-module one.
    "S0239", // `Sim::set`: name is not a drivable input/clock/reset
    "S0240", // `+`/`-`/`*` operands disagree on signedness
    "S0241", // `??` reached `binary_known` unlowered (comb.rs's lighter
    // pipeline skips `elaborate::Rw::expr`'s desugar-to-`IfExpr` pass, so a
    // raw `??` can reach here — was misfiled under S0222 until this split)
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
    // S05xx — assertion failures (sim/kernel.rs, sim/run.rs; GAP-6).
    "S0501", // `assert(cond)` / `assert(cond, "msg")` evaluated false
];

/// Delimiter used by [`bridge_code`]/[`diag_from_bridged`] to smuggle a
/// `Diag`'s own `code` through the `Resolver` trait's fixed `Result<_,
/// String>` boundary. `signal`/`mem_read` deliberately keep that plain
/// `&str`-in, `String`-error signature rather than threading a `Span`/
/// `Diag` through it — doing so would ripple through every `Resolver`
/// implementer (`Env`, `CombEnv`, ...) and call site for what's a
/// rarely-hit error path, so this marker-string trick preserves an
/// already-coded `Diag` across the boundary instead. BUG-27: without this,
/// a `Resolver::signal`/`mem_read` impl's own already-coded `Diag` (e.g.
/// `Env::resolve`'s
/// `S0238` combinational-cycle error) is unconditionally REPLACED by the
/// boundary's generic fallback code (`S0201`/`S0206`) the moment it's
/// bridged down to a plain `String` — a control character that can never
/// appear in a real diagnostic message, so a plain (non-bridged) string —
/// the common case, from a `Resolver` with no code of its own to preserve
/// — is never misread as carrying one.
const BRIDGE_MARKER: char = '\0';

/// Write side of the code-smuggling in `BRIDGE_MARKER`'s doc comment.
/// Call this INSTEAD OF bridging a `Resolver::signal`/`mem_read` impl's own
/// already-coded `Diag` down to a plain `.msg` string.
pub fn bridge_code(code: &'static str, msg: impl AsRef<str>) -> String {
    format!("{BRIDGE_MARKER}{code}{BRIDGE_MARKER}{}", msg.as_ref())
}

/// Read side: build a `Diag` from a bridged `Resolver::signal`/`mem_read`
/// error, recovering the ORIGINAL code [`bridge_code`] embedded (validated
/// against [`ALL_SIM_CODES`] — an unrecognized or absent marker never
/// fabricates a bogus code), else falling back to `default_code` (the
/// boundary's own generic code — `S0201` for `signal`, `S0206` for
/// `mem_read`).
pub fn diag_from_bridged(span: Span, msg: String, default_code: &'static str) -> Box<Diag> {
    if let Some(rest) = msg.strip_prefix(BRIDGE_MARKER)
        && let Some((code, real_msg)) = rest.split_once(BRIDGE_MARKER)
        && let Some(&known) = ALL_SIM_CODES.iter().find(|c| **c == code)
    {
        return Box::new(Diag {
            code: Some(known),
            ..Diag::new(span, real_msg)
        });
    }
    Box::new(Diag::new(span, msg).with_code(default_code))
}

/// (1-based line, 1-based column, line text, byte offset of line start)
pub(crate) fn locate(src: &str, offset: usize) -> (usize, usize, String, usize) {
    let offset = offset.min(src.len());
    let mut line_no = 1usize;
    let mut line_start = 0usize;
    for (i, b) in src.bytes().enumerate() {
        if i >= offset {
            break;
        }
        if b == b'\n' {
            line_no += 1;
            line_start = i + 1;
        }
    }
    let line_end = src[line_start..]
        .find('\n')
        .map(|i| line_start + i)
        .unwrap_or(src.len());
    let line_text = src[line_start..line_end].trim_end_matches('\r').to_string();
    let col = src[line_start..offset].chars().count() + 1;
    (line_no, col, line_text, line_start)
}
