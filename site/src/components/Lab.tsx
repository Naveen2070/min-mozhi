// The lab island (plan W3/D3-D7): one editor, one console, one grader.
//
// The lesson's prose ships in the page (all of it, so Pagefind sees it and
// #step-N anchors work); this component reads the exercise machinery from the
// server-rendered #lab-data JSON and owns step state.
//
// Grading is D3 exactly: append the step's `verify` block and run `test` —
// pass means the command did not throw. The three guards are load-bearing:
//   1. append, never prepend (line numbers stay in the learner's region);
//   2. "no tests found." is a FAIL (the module was renamed/deleted);
//   3. no verify block → graded on `check` alone, labelled as such.
// wasm init is lazy (D5): first interaction or idle callback, whichever first.
// Progress and drafts live in localStorage under versioned keys (D6).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import WaveformViewer from "./WaveformViewer.tsx";
import { errMsg, parsePorts, useMimz, type Port } from "../lib/useMimz.ts";

interface StepData {
  n: number;
  title: string;
  starter: string | null;
  solution: string | null;
  verify: string | null;
  hint: string | null;
}
interface LabData {
  id: string;
  module: string | null;
  chapter: string | null;
  steps: StepData[];
}

interface Result {
  kind: "pass" | "fail" | "checked";
  text: string;
}

// ---- localStorage, versioned (D6) ----------------------------------------
// The v1 segment is bumped whenever lesson content changes in a way that
// invalidates old drafts; every lab draft dies at once. Never reuse a version.

const LS_VERSION = "v1";
const keyStep = (id: string) => `mimz:lab:${LS_VERSION}:${id}:step`;
const keySource = (id: string, n: number) =>
  `mimz:lab:${LS_VERSION}:${id}:${n}`;

function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null; // private windows throw on access; they do not return null
  }
}
function lsSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Quota/blocked storage — the session still works, it just forgets.
  }
}
function lsDel(key: string) {
  try {
    localStorage.removeItem(key);
  } catch {
    // ignore
  }
}

// ---- F3: diagnostics must not point at invisible code ---------------------
// The verify block is appended to the learner's source, so a diagnostic inside
// it cites a line past what the editor shows. If any cited line is beyond the
// buffer, swap the whole caret block for a message that names the contract.

function softenDiagnostic(
  msg: string,
  visibleLines: number,
  moduleName: string,
): string {
  const refs = [...msg.matchAll(/-->\s*\S+:(\d+)/g)].map((m) =>
    parseInt(m[1], 10),
  );
  if (!refs.length || Math.max(...refs) <= visibleLines) return msg;
  return (
    `The step's own check could not run against your module.\n\n` +
    `Check that the module is still named \`${moduleName}\`, with its ports intact.`
  );
}

const NO_TESTS = /no tests found/i;

function idleLoad(cb: () => void): () => void {
  const w = window as unknown as {
    requestIdleCallback?: (cb: () => void) => number;
    cancelIdleCallback?: (h: number) => void;
  };
  if (typeof w.requestIdleCallback === "function") {
    const h = w.requestIdleCallback(cb); // Safari < 18 has no idle callbacks
    return () => w.cancelIdleCallback?.(h);
  }
  const h = setTimeout(cb, 0);
  return () => clearTimeout(h);
}

export default function Lab() {
  const data = useMemo<LabData | null>(() => {
    try {
      const el = document.getElementById("lab-data");
      return el ? (JSON.parse(el.textContent ?? "") as LabData) : null;
    } catch {
      return null;
    }
  }, []);

  const { ready, load, run, log, append } = useMimz();

  const [current, setCurrent] = useState(1);
  const [sources, setSources] = useState<Record<number, string>>({});
  const [result, setResult] = useState<Result | null>(null);
  const [revealed, setRevealed] = useState<Set<number>>(new Set());
  const [solved, setSolved] = useState<Set<number>>(new Set());
  const [vcd, setVcd] = useState<string | null>(null);
  const [cmd, setCmd] = useState("");
  const [booting, setBooting] = useState(false);

  const logRef = useRef<HTMLDivElement>(null);

  const steps = data?.steps ?? [];
  const maxStep = steps.length;
  const step = steps.find((s) => s.n === current) ?? steps[0];
  const moduleName = data?.module ?? "the lesson module";

  const sourceOf = useCallback(
    (n: number) => sources[n] ?? steps.find((s) => s.n === n)?.starter ?? "",
    [sources, steps],
  );
  const setSource = useCallback(
    (n: number, src: string) => {
      setSources((prev) => ({ ...prev, [n]: src }));
      lsSet(keySource(data?.id ?? "", n), src);
    },
    [data?.id],
  );

  // Restore saved progress once (furthest step + per-step drafts).
  useEffect(() => {
    if (!data) return;
    const drafts: Record<number, string> = {};
    for (const s of data.steps) {
      const d = lsGet(keySource(data.id, s.n));
      if (d !== null && d !== s.starter) drafts[s.n] = d;
    }
    if (Object.keys(drafts).length) setSources(drafts);
    const saved = parseInt(lsGet(keyStep(data.id)) ?? "1", 10);
    if (saved >= 1 && saved <= data.steps.length) setCurrent(saved);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- runs on mount only
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  // D5: init on the first interaction OR when the browser is idle.
  const ensureReady = useCallback(async () => {
    if (ready) return true;
    setBooting(true);
    try {
      await load();
      return true;
    } catch {
      return false;
    } finally {
      setBooting(false);
    }
  }, [ready, load]);

  useEffect(() => idleLoad(() => void load().catch(() => {})), [load]);

  function goto(n: number) {
    setCurrent(n);
    setResult(null);
    lsSet(keyStep(data?.id ?? ""), String(n));
  }

  /** Advance one step and move focus to its server-rendered heading. */
  function advance() {
    const next = steps.find((s) => s.n > current);
    if (!next) return;
    goto(next.n);
    requestAnimationFrame(() => {
      document.getElementById(`step-${next.n}-heading`)?.focus();
    });
  }

  /** Free play: run a command against the current buffer. No grading. */
  async function doRun(command: string, args: string[]) {
    if (!(await ensureReady())) return;
    try {
      run(sourceOf(current), command, args);
    } catch {
      // The diagnostic is already narrated into the log by useMimz.run.
    }
  }

  /** The grader (D3). */
  async function doVerify() {
    if (!step) return;
    if (!(await ensureReady())) return;
    const src = sourceOf(step.n);

    // Guard 3: an exploration step grades on `check` alone — say so honestly.
    if (!step.verify) {
      try {
        run(src, "check", []);
        setResult({ kind: "checked", text: "compiles ✓" });
        markSolved(step.n);
      } catch {
        setResult({ kind: "fail", text: "does not compile yet." });
      }
      return;
    }

    let out: string;
    try {
      out = run(src + "\n\n" + step.verify, "test", []); // guard 1: append
    } catch (e) {
      const msg = softenDiagnostic(
        errMsg(e),
        src.split("\n").length,
        moduleName,
      );
      setResult({ kind: "fail", text: msg });
      return;
    }
    // Guard 2: an empty test set returns Ok — that is a FAIL here, because it
    // means the learner deleted/renamed the module the check binds to.
    if (NO_TESTS.test(out)) {
      setResult({
        kind: "fail",
        text:
          "The check found nothing to test — the module this step targets was renamed or deleted.\n" +
          `This step expects a module named \`${moduleName}\`.`,
      });
      return;
    }
    setResult({ kind: "pass", text: "passed ✓" });
    markSolved(step.n);
  }

  function markSolved(n: number) {
    setSolved((prev) => new Set(prev).add(n));
  }

  /** Load a step's solution into the editor and mark it solved-by-reveal. */
  function showSolution(n: number) {
    const s = steps.find((x) => x.n === n);
    if (!s?.solution) return;
    setSource(n, s.solution);
    setRevealed((prev) => new Set(prev).add(n));
    markSolved(n);
    goto(n); // the editor switches to the revealed step
  }

  // The solution buttons are server-rendered beside each step's prose (left
  // column, per the plan's §4 mock), while the editor lives in this island —
  // one delegated listener bridges them without a second React root.
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      const btn = (e.target as HTMLElement).closest<HTMLElement>(
        "[data-lab-solution]",
      );
      if (!btn) return;
      const n = parseInt(btn.dataset.labSolution ?? "", 10);
      if (!Number.isNaN(n)) showSolution(n);
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  });

  // D7: steps have no routes; #step-N is the deep link. A hashchange listener
  // keeps the IDE on the anchored step (and honours the hash on first load).
  useEffect(() => {
    if (!data) return;
    const fromHash = () => {
      const m = location.hash.match(/^#step-(\d+)$/);
      if (!m) return;
      const n = parseInt(m[1], 10);
      if (data.steps.some((s) => s.n === n)) goto(n);
    };
    window.addEventListener("hashchange", fromHash);
    fromHash();
    return () => window.removeEventListener("hashchange", fromHash);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- data is stable post-mount
  }, [data]);

  // The lesson pane is server-rendered; this island owns step state. Reveal
  // the active step's prose and arm/disarm the pager buttons beside it.
  useEffect(() => {
    document.querySelectorAll<HTMLElement>("[data-lab-step]").forEach((el) => {
      el.hidden = el.dataset.labStep !== String(current);
    });
    document
      .querySelectorAll<HTMLButtonElement>("[data-lab-goto]")
      .forEach((b) => {
        const first = steps[0]?.n ?? 1;
        b.disabled =
          b.dataset.labGoto === "prev" ? current <= first : current >= maxStep;
      });
  }, [current, steps, maxStep]);

  // Clicks on those server-rendered pager buttons land here.
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      const btn = (e.target as HTMLElement).closest<HTMLElement>(
        "[data-lab-goto]",
      );
      if (!btn || (btn as HTMLButtonElement).disabled) return;
      const dir = btn.dataset.labGoto;
      const target =
        dir === "prev"
          ? [...steps].reverse().find((s) => s.n < current)
          : steps.find((s) => s.n > current);
      if (target) goto(target.n);
    };
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  });

  function resetLesson() {
    if (!data) return;
    for (const s of data.steps) lsDel(keySource(data.id, s.n));
    lsDel(keyStep(data.id));
    setSources({});
    setSolved(new Set());
    setRevealed(new Set());
    setResult(null);
    setVcd(null);
    setCurrent(data.steps[0]?.n ?? 1);
  }

  /** Waveform: seed a few input vectors from `ports`, sim for a VCD. */
  async function doWave() {
    if (!(await ensureReady())) return;
    const src = sourceOf(current);
    const ports = parsePorts(src);
    if (!ports) {
      append([
        { kind: "err", text: "Fix the module first — it does not elaborate." },
      ]);
      return;
    }
    const args: string[] = [];
    if (ports.clocked) {
      const inv = ports.inputs.map((p) => `${p.name}=0`).join(",");
      if (inv) args.push("--in", inv);
      args.push("--cycles", "16");
    } else if (ports.inputs.length) {
      const spec = [0, 1, 2, 3]
        .map((t) =>
          ports.inputs
            .map((p: Port) => `${p.name}=${String(p.width === 1 ? t & 1 : t)}`)
            .join(","),
        )
        .join(";");
      args.push("--steps", spec);
    }
    args.push("--vcd");
    try {
      setVcd(run(src, "sim", args));
    } catch {
      // narrated already
    }
  }

  if (!data || !step) return null;

  return (
    <div className="lab">
      {/* Compact head: where the learner is, and the way to forget everything.
          Step navigation itself lives beside the prose (data-lab-goto). */}
      <div className="lab-head">
        <span className="lab-where">
          Step {current} of {maxStep}
        </span>
        <button type="button" className="lab-reset" onClick={resetLesson}>
          reset lesson
        </button>
      </div>

      <label className="pg-label" htmlFor="lab-editor">
        Lesson editor — step {current}
        {revealed.has(current) && (
          <span className="pg-hint"> · solution shown</span>
        )}
      </label>
      <textarea
        id="lab-editor"
        className="pg-editor"
        aria-label={`Min-Mozhi source editor, step ${current}`}
        value={sourceOf(current)}
        onChange={(e) => setSource(current, e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key !== "Tab") return;
          e.preventDefault();
          const t = e.currentTarget;
          const s = t.selectionStart;
          const en = t.selectionEnd;
          setSource(
            current,
            sourceOf(current).slice(0, s) + "  " + sourceOf(current).slice(en),
          );
          requestAnimationFrame(() => {
            t.selectionStart = t.selectionEnd = s + 2;
          });
        }}
        spellCheck={false}
      />

      <div className="lab-actions">
        <button
          type="button"
          className="pg-btn"
          disabled={!ready}
          onClick={() => doRun("check", [])}
        >
          Run
        </button>
        <button
          type="button"
          className="pg-btn lab-verify"
          disabled={booting}
          onClick={doVerify}
        >
          Verify
        </button>
        <button
          type="button"
          className="pg-btn-ghost"
          onClick={doWave}
          disabled={!ready}
        >
          Wave ▸
        </button>
        <span className="pg-hint">
          {booting
            ? "starting the compiler…"
            : ready
              ? "in-browser · no install"
              : ""}
        </span>
      </div>

      {/* The one result region: announced politely (results land after the
          click, when attention has moved on), echoed visually in the banner.
          next → is ALWAYS enabled — a stuck learner is a failed lesson. */}
      <div
        role="status"
        aria-live="polite"
        className={"lab-result " + (result?.kind ?? "")}
      >
        {result && (
          <strong>{result.kind === "fail" ? "✗ not yet" : "✓"}</strong>
        )}
        {result && <pre>{result.text}</pre>}
        <button
          type="button"
          className="pg-btn-ghost lab-next"
          onClick={advance}
          disabled={current >= maxStep}
        >
          next →
        </button>
      </div>

      <form
        className="lab-cmd"
        onSubmit={(e) => {
          e.preventDefault();
          const parts = cmd.trim().split(/\s+/).filter(Boolean);
          if (parts.length) void doRun(parts[0], parts.slice(1));
        }}
      >
        <span className="pg-prompt" aria-hidden="true">
          $
        </span>
        <input
          aria-label="mimz command"
          className="pg-input"
          value={cmd}
          placeholder="compile · eval --in a=1,b=0 · sim --cycles 8"
          onChange={(e) => setCmd(e.currentTarget.value)}
          disabled={!ready}
        />
        <button type="submit" className="pg-btn-ghost" disabled={!ready}>
          run
        </button>
      </form>

      <div
        ref={logRef}
        className="pg-log lab-log"
        aria-live="polite"
        aria-label="Console output"
      >
        {log.map((l, i) => (
          <pre key={i} className={"pg-" + l.kind}>
            {l.text}
          </pre>
        ))}
      </div>

      {vcd && (
        <details className="lab-wave" open>
          <summary>Waveform</summary>
          <WaveformViewer vcd={vcd} />
        </details>
      )}
    </div>
  );
}
