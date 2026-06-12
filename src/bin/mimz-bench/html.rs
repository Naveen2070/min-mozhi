//! Renders one self-describing `bench-report.html`: the whole
//! [`BenchReport`] (plus run history) is embedded as a JSON blob and a
//! small vanilla-JS block builds the Chart.js charts from it. Chart.js
//! loads from the jsDelivr CDN (user decision 2026-06-12) — the report
//! needs internet to draw, but stays a single portable file.

use crate::metrics::{BenchReport, HistoryEntry, Rate};

/// Build the complete HTML document.
pub fn render(report: &BenchReport, history: &[HistoryEntry]) -> String {
    let data = serde_json::json!({
        "report": serde_json::to_value(report).expect("report serializes"),
        "history": serde_json::to_value(history).expect("history serializes"),
    });
    // `</` inside a <script> JSON blob can close the tag early — escape it.
    let data = data.to_string().replace("</", "<\\/");

    let verdict = if report.all_validations_pass() {
        ("pass", "ALL VALIDATIONS PASS")
    } else {
        ("fail", "VALIDATION FAILURES")
    };
    let llvm_card = match &report.coverage.llvm {
        Some(l) => format!("{:.1}% lines", l.line_percent),
        None => "skipped".to_string(),
    };
    let failures: Vec<&String> = report
        .accuracy
        .failures
        .iter()
        .chain(&report.safety.failures)
        .collect();
    let failures_html = if failures.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"failures\"><h2>Failures ({})</h2><ul>{}</ul></div>",
            failures.len(),
            failures
                .iter()
                .map(|f| format!("<li>{}</li>", escape(f)))
                .collect::<String>()
        )
    };
    let timing_rows: String = report
        .speed
        .per_example
        .iter()
        .map(|e| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td></tr>",
                escape(&e.name),
                e.loc,
                e.load_ms,
                e.check_ms,
                e.emit_ms,
                e.load_ms + e.check_ms + e.emit_ms,
            )
        })
        .collect();
    let rate_rows: String = rate_table_rows(report);

    TEMPLATE
        .replace("{{DATA}}", &data)
        .replace("{{VERDICT_CLASS}}", verdict.0)
        .replace("{{VERDICT}}", verdict.1)
        .replace("{{GIT}}", &escape(&report.meta.git_rev))
        .replace("{{RUSTC}}", &escape(&report.meta.rustc))
        .replace("{{ITERATIONS}}", &report.meta.iterations.to_string())
        .replace("{{TOTAL_MS}}", &format!("{:.1}", report.speed.total_ms))
        .replace(
            "{{LOC_PER_SEC}}",
            &format!("{:.0}", report.speed.loc_per_sec),
        )
        .replace(
            "{{GOLDEN_PCT}}",
            &format!("{:.0}%", report.accuracy.golden.percent()),
        )
        .replace(
            "{{FIXTURE_PCT}}",
            &format!("{:.0}%", report.safety.fixtures.percent()),
        )
        .replace("{{LLVM_CARD}}", &escape(&llvm_card))
        .replace("{{FAILURES}}", &failures_html)
        .replace("{{TIMING_ROWS}}", &timing_rows)
        .replace("{{RATE_ROWS}}", &rate_rows)
}

/// Every validation rate as a table row: label, passed/total, percent.
fn rate_table_rows(report: &BenchReport) -> String {
    let fmt = |label: &str, r: &Rate| {
        format!(
            "<tr><td>{label}</td><td>{}/{}</td><td>{:.1}%</td></tr>",
            r.passed,
            r.total,
            r.percent()
        )
    };
    let skipped = |label: &str| format!("<tr><td>{label}</td><td>—</td><td>skipped</td></tr>");
    let mut rows = String::new();
    rows.push_str(&fmt("Golden-file match", &report.accuracy.golden));
    rows.push_str(&fmt(
        "Flavor byte-identity",
        &report.accuracy.flavor_identity,
    ));
    match &report.accuracy.iverilog_syntax {
        Some(r) => rows.push_str(&fmt("Icarus syntax accept", r)),
        None => rows.push_str(&skipped("Icarus syntax accept")),
    }
    match &report.accuracy.testbenches {
        Some(r) => rows.push_str(&fmt("Self-checking testbenches", r)),
        None => rows.push_str(&skipped("Self-checking testbenches")),
    }
    rows.push_str(&fmt(
        "Error fixtures fire their code",
        &report.safety.fixtures,
    ));
    rows.push_str(&fmt(
        "Diagnostics carry a help line",
        &report.safety.help_lines,
    ));
    rows.push_str(&fmt(
        "Examples check clean (no false positives)",
        &report.safety.clean_examples,
    ));
    let c = &report.coverage.corpus;
    rows.push_str(&fmt("Checker codes with a fixture", &c.codes_with_fixture));
    rows.push_str(&fmt("Examples with a golden file", &c.examples_with_golden));
    rows.push_str(&fmt(
        "Examples with a testbench",
        &c.examples_with_testbench,
    ));
    rows.push_str(&fmt(
        "Flavor completeness (base × flavor)",
        &c.flavor_completeness,
    ));
    rows
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The page skeleton. `{{...}}` placeholders are replaced in [`render`];
/// the JS reads everything else from the embedded DATA blob.
const TEMPLATE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Min-Mozhi — Benchmark Report</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4"></script>
<style>
  :root { --pass: #1a7f37; --fail: #cf222e; --ink: #1f2328; --soft: #656d76; }
  body { font-family: system-ui, sans-serif; margin: 2rem auto; max-width: 1100px;
         padding: 0 1rem; color: var(--ink); }
  h1 { margin-bottom: 0.2rem; }
  .meta { color: var(--soft); margin-bottom: 1.5rem; }
  .verdict { display: inline-block; padding: 0.3rem 0.9rem; border-radius: 999px;
             color: white; font-weight: 600; }
  .verdict.pass { background: var(--pass); }
  .verdict.fail { background: var(--fail); }
  .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
           gap: 0.8rem; margin: 1.2rem 0 2rem; }
  .card { border: 1px solid #d0d7de; border-radius: 8px; padding: 0.8rem 1rem; }
  .card .label { color: var(--soft); font-size: 0.8rem; text-transform: uppercase; }
  .card .value { font-size: 1.5rem; font-weight: 600; }
  .charts { display: grid; grid-template-columns: 1fr 1fr; gap: 2rem; }
  .chart-box { min-height: 320px; }
  .chart-box.wide { grid-column: 1 / -1; }
  .failures { border: 1px solid var(--fail); border-radius: 8px;
              padding: 0.5rem 1.2rem; margin-bottom: 2rem; background: #fff5f5; }
  table { border-collapse: collapse; width: 100%; margin: 1rem 0 2rem; }
  th, td { border: 1px solid #d0d7de; padding: 0.4rem 0.7rem; text-align: left;
           font-size: 0.9rem; }
  th { background: #f6f8fa; }
  footer { color: var(--soft); margin-top: 2rem; font-size: 0.85rem; }
</style>
</head>
<body>
<h1>Min-Mozhi — Benchmark Report</h1>
<p class="meta">
  <span class="verdict {{VERDICT_CLASS}}">{{VERDICT}}</span>
  &nbsp; git {{GIT}} · {{RUSTC}} · median of {{ITERATIONS}} iteration(s) ·
  <span id="run-date"></span>
</p>

<div class="cards">
  <div class="card"><div class="label">Pipeline (all examples)</div>
    <div class="value">{{TOTAL_MS}} ms</div></div>
  <div class="card"><div class="label">Throughput</div>
    <div class="value">{{LOC_PER_SEC}} LOC/s</div></div>
  <div class="card"><div class="label">Golden match</div>
    <div class="value">{{GOLDEN_PCT}}</div></div>
  <div class="card"><div class="label">Error fixtures</div>
    <div class="value">{{FIXTURE_PCT}}</div></div>
  <div class="card"><div class="label">Code coverage</div>
    <div class="value">{{LLVM_CARD}}</div></div>
</div>

{{FAILURES}}

<div class="charts">
  <div class="chart-box wide"><canvas id="timing"></canvas></div>
  <div class="chart-box"><canvas id="rates"></canvas></div>
  <div class="chart-box"><canvas id="cov"></canvas></div>
  <div class="chart-box wide"><canvas id="trend"></canvas></div>
</div>

<h2>Per-example timing (median, ms)</h2>
<table>
  <tr><th>Example</th><th>LOC</th><th>Load + parse</th><th>Check</th><th>Emit</th><th>Total</th></tr>
  {{TIMING_ROWS}}
</table>

<h2>Validation rates</h2>
<table>
  <tr><th>Validation</th><th>Passed</th><th>Rate</th></tr>
  {{RATE_ROWS}}
</table>

<footer>Min-Mozhi — மின்மொழி — Speak in Circuits · generated by <code>mimz-bench</code></footer>

<script>
const DATA = {{DATA}};
const R = DATA.report;
document.getElementById("run-date").textContent =
  new Date(R.meta.timestamp_ms).toLocaleString();

const pct = (r) => r.total === 0 ? 100 : r.passed * 100 / r.total;

// Stacked bar: per-example phase timings.
new Chart(document.getElementById("timing"), {
  type: "bar",
  data: {
    labels: R.speed.per_example.map(e => e.name),
    datasets: [
      { label: "load + parse (ms)", data: R.speed.per_example.map(e => e.load_ms) },
      { label: "check (ms)", data: R.speed.per_example.map(e => e.check_ms) },
      { label: "emit (ms)", data: R.speed.per_example.map(e => e.emit_ms) },
    ],
  },
  options: {
    plugins: { title: { display: true, text: "Compilation speed by example (median)" } },
    scales: { x: { stacked: true }, y: { stacked: true, title: { display: true, text: "ms" } } },
  },
});

// Bar: every validation rate in one picture.
const rateBars = [
  ["Golden match", pct(R.accuracy.golden)],
  ["Flavor identity", pct(R.accuracy.flavor_identity)],
  ...(R.accuracy.iverilog_syntax ? [["Icarus syntax", pct(R.accuracy.iverilog_syntax)]] : []),
  ...(R.accuracy.testbenches ? [["Testbenches", pct(R.accuracy.testbenches)]] : []),
  ["Error fixtures", pct(R.safety.fixtures)],
  ["Help lines", pct(R.safety.help_lines)],
  ["No false positives", pct(R.safety.clean_examples)],
  ["Codes w/ fixture", pct(R.coverage.corpus.codes_with_fixture)],
];
new Chart(document.getElementById("rates"), {
  type: "bar",
  data: {
    labels: rateBars.map(r => r[0]),
    datasets: [{ label: "%", data: rateBars.map(r => r[1]),
      backgroundColor: rateBars.map(r => r[1] >= 100 ? "#1a7f37" : "#cf222e") }],
  },
  options: {
    indexAxis: "y",
    plugins: { title: { display: true, text: "Accuracy, safety & corpus rates (%)" },
               legend: { display: false } },
    scales: { x: { min: 0, max: 100 } },
  },
});

// Doughnut: real code coverage (or corpus coverage when llvm-cov skipped).
const covEl = document.getElementById("cov");
if (R.coverage.llvm) {
  new Chart(covEl, {
    type: "doughnut",
    data: {
      labels: ["lines covered", "lines uncovered"],
      datasets: [{ data: [R.coverage.llvm.line_percent, 100 - R.coverage.llvm.line_percent],
        backgroundColor: ["#1a7f37", "#d0d7de"] }],
    },
    options: { plugins: { title: { display: true,
      text: `Code coverage — ${R.coverage.llvm.line_percent.toFixed(1)}% lines, `
          + `${R.coverage.llvm.function_percent.toFixed(1)}% functions` } } },
  });
} else {
  const c = R.coverage.corpus.flavor_completeness;
  new Chart(covEl, {
    type: "doughnut",
    data: {
      labels: ["flavor files present", "missing"],
      datasets: [{ data: [c.passed, c.total - c.passed],
        backgroundColor: ["#1a7f37", "#d0d7de"] }],
    },
    options: { plugins: { title: { display: true,
      text: "Corpus coverage (code coverage " + (R.coverage.llvm_note || "skipped") + ")" } } },
  });
}

// Line: history trend — compile time and the two headline rates per run.
new Chart(document.getElementById("trend"), {
  type: "line",
  data: {
    labels: DATA.history.map(h =>
      new Date(h.timestamp_ms).toLocaleDateString() + " " + h.git_rev),
    datasets: [
      { label: "pipeline total (ms)", data: DATA.history.map(h => h.total_ms), yAxisID: "ms" },
      { label: "golden match (%)", data: DATA.history.map(h => h.golden_pct), yAxisID: "rate" },
      { label: "error fixtures (%)", data: DATA.history.map(h => h.fixture_pct), yAxisID: "rate" },
    ],
  },
  options: {
    plugins: { title: { display: true, text: "Trend across benchmark runs" } },
    scales: {
      ms: { type: "linear", position: "left", title: { display: true, text: "ms" } },
      rate: { type: "linear", position: "right", min: 0, max: 100,
              grid: { drawOnChartArea: false }, title: { display: true, text: "%" } },
    },
  },
});
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::*;

    fn tiny_report() -> BenchReport {
        BenchReport {
            meta: Meta {
                timestamp_ms: 0,
                git_rev: "abc1234".into(),
                rustc: "rustc 1.85.0".into(),
                iterations: 1,
                example_files: 1,
                fixture_files: 1,
            },
            speed: Speed {
                per_example: vec![ExampleTiming {
                    name: "counter".into(),
                    loc: 12,
                    load_ms: 1.0,
                    check_ms: 2.0,
                    emit_ms: 3.0,
                }],
                total_loc: 12,
                total_ms: 6.0,
                loc_per_sec: 2000.0,
            },
            accuracy: Accuracy {
                golden: Rate {
                    passed: 1,
                    total: 1,
                },
                flavor_identity: Rate {
                    passed: 3,
                    total: 3,
                },
                iverilog_syntax: None,
                testbenches: None,
                failures: vec![],
            },
            safety: Safety {
                fixtures: Rate {
                    passed: 1,
                    total: 1,
                },
                help_lines: Rate {
                    passed: 1,
                    total: 1,
                },
                clean_examples: Rate {
                    passed: 1,
                    total: 1,
                },
                failures: vec![],
            },
            coverage: Coverage {
                corpus: CorpusCoverage {
                    codes_with_fixture: Rate {
                        passed: 36,
                        total: 36,
                    },
                    examples_with_golden: Rate {
                        passed: 14,
                        total: 14,
                    },
                    examples_with_testbench: Rate {
                        passed: 13,
                        total: 14,
                    },
                    flavor_completeness: Rate {
                        passed: 56,
                        total: 56,
                    },
                },
                llvm: None,
                llvm_note: Some("skipped (--no-cov)".into()),
            },
        }
    }

    #[test]
    fn report_renders_a_complete_page() {
        let html = render(&tiny_report(), &[]);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("chart.js"), "Chart.js CDN script tag");
        assert!(html.contains("ALL VALIDATIONS PASS"));
        assert!(
            html.contains("\"git_rev\":\"abc1234\""),
            "embedded JSON blob"
        );
        assert!(html.contains("<td>counter</td>"), "timing table row");
        assert!(!html.contains("{{"), "no unreplaced placeholders");
    }

    #[test]
    fn failures_flip_the_verdict_and_are_listed() {
        let mut r = tiny_report();
        r.accuracy.failures.push("golden mismatch: counter".into());
        let html = render(&r, &[]);
        assert!(html.contains("VALIDATION FAILURES"));
        assert!(html.contains("golden mismatch: counter"));
    }
}
