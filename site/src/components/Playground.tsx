import { useCallback, useEffect, useRef, useState } from "react";
import init, { runCommand } from "../lib/wasm/mimz_wasm.js";
import wasmUrl from "../lib/wasm/mimz_wasm_bg.wasm?url";
import WaveformViewer from "./WaveformViewer.tsx";

interface Example {
  name: string;
  // keyword-flavor key (english/tanglish/tamil/mixed) -> source for that flavor.
  flavors: Record<string, string>;
}
interface Flavor {
  key: string;
  label: string;
}
interface Props {
  examples: Example[];
  flavors: Flavor[];
  initialExample?: string;
  hideEditor?: boolean;
}

// Pick an example's source in the requested flavor, falling back to the first
// flavor it has if that one is missing.
function sourceOf(ex: Example | undefined, flavor: string): string {
  if (!ex) return "";
  return ex.flavors[flavor] ?? Object.values(ex.flavors)[0] ?? "";
}

type LineKind = "cmd" | "out" | "err" | "note";
interface Line {
  kind: LineKind;
  text: string;
}

// A module's interface, from the `ports` command — drives the stimulus controls.
interface Port {
  name: string;
  width: number;
  signed: boolean;
}
interface Ports {
  module: string;
  clocked: boolean;
  inputs: Port[];
  outputs: Port[];
}
// One input name -> value (kept as a string so hex/0b are allowed).
type Vec = Record<string, string>;

const SUGGESTIONS = [
  "compile",
  "check",
  "eval --in a=3,b=5",
  "sim --cycles 8 --trace",
];
// Combinational designs seed with a short ramp so the wave moves on load (a
// single fixed vector would draw flat — the thing users get stuck on).
const SEED_STEPS = 3;

function seedSteps(inputs: Port[]): Vec[] {
  return Array.from({ length: SEED_STEPS }, (_, t) => {
    const v: Vec = {};
    for (const p of inputs) v[p.name] = String(p.width === 1 ? t & 1 : t);
    return v;
  });
}

function zeroVec(inputs: Port[]): Vec {
  const v: Vec = {};
  for (const p of inputs) v[p.name] = "0";
  return v;
}

export default function Playground({ examples, flavors, initialExample, hideEditor }: Props) {
  const [flavor, setFlavor] = useState(flavors[0]?.key ?? "english");
  
  const startingEx = initialExample ? examples.find(e => e.name === initialExample) || examples[0] : examples[0];
  const [source, setSource] = useState(
    sourceOf(startingEx, flavors[0]?.key ?? "english"),
  );
  const [exampleName, setExampleName] = useState(startingEx?.name ?? "");
  const [log, setLog] = useState<Line[]>([
    { kind: "note", text: "Loading the in-browser compiler…" },
  ]);
  const [ready, setReady] = useState(false);
  const [cmd, setCmd] = useState("compile");
  const [vcd, setVcd] = useState<string | null>(null);

  // Stimulus state, rebuilt from `ports` whenever the source changes.
  const [ports, setPorts] = useState<Ports | null>(null);
  const [steps, setSteps] = useState<Vec[]>([]); // combinational: per-step vectors
  const [held, setHeld] = useState<Vec>({}); // clocked: held input values
  const [cycles, setCycles] = useState(8);
  const [stimErr, setStimErr] = useState<string | null>(null);
  // Once the user clicks Run, the sim goes "live": further input edits re-run it
  // automatically. Switching example/flavor turns it back off (click to start).
  const [live, setLive] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);

  // Load the wasm module once. `{ module_or_path }` is the non-deprecated form.
  useEffect(() => {
    let alive = true;
    init({ module_or_path: wasmUrl })
      .then(() => {
        if (!alive) return;
        setReady(true);
        setLog([
          {
            kind: "note",
            text: "mimz ready — runs entirely in your browser. Press Run or Enter.",
          },
          { kind: "note", text: "Try:  " + SUGGESTIONS.join("   ·   ") },
        ]);
      })
      .catch((e: unknown) => {
        if (!alive) return;
        const msg = e instanceof Error ? e.message : String(e);
        setLog([{ kind: "err", text: "Failed to load the compiler: " + msg }]);
      });
    return () => {
      alive = false;
    };
  }, []);

  // Keep the log scrolled to the newest line.
  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  function append(lines: Line[]) {
    setLog((prev) => [...prev, ...lines]);
  }

  // Re-read the module interface and reseed the stimulus. On a parse/elaborate
  // error the panel hides (the console still shows the diagnostic on compile).
  const loadPorts = useCallback(() => {
    try {
      const info = JSON.parse(runCommand(source, "ports", [])) as Ports;
      setPorts(info);
      if (info.clocked) setHeld(zeroVec(info.inputs));
      else setSteps(info.inputs.length ? seedSteps(info.inputs) : []);
      setStimErr(null);
    } catch {
      // Source no longer elaborates — hide the panel and stop live updates.
      setPorts(null);
      setVcd(null);
      setLive(false);
    }
  }, [source]);

  // Build the `sim` flags from the current stimulus and run for a VCD.
  const runSim = useCallback(() => {
    if (!ports) return;
    const args: string[] = [];
    if (ports.clocked) {
      const inv = ports.inputs
        .map((p) => `${p.name}=${(held[p.name] ?? "0").trim() || "0"}`)
        .join(",");
      if (inv) args.push("--in", inv);
      args.push("--cycles", String(cycles));
    } else if (ports.inputs.length) {
      const spec = steps
        .map((s) =>
          ports.inputs
            .map((p) => `${p.name}=${(s[p.name] ?? "0").trim() || "0"}`)
            .join(","),
        )
        .join(";");
      args.push("--steps", spec);
    }
    args.push("--vcd");
    try {
      setVcd(runCommand(source, "sim", args));
      setStimErr(null);
    } catch (e: unknown) {
      setStimErr(e instanceof Error ? e.message : String(e));
    }
  }, [ports, steps, held, cycles, source]);

  // Reload ports when the source changes (debounced — the user is typing). This
  // only rebuilds the input controls; it never simulates on its own.
  useEffect(() => {
    if (!ready) return;
    const id = setTimeout(loadPorts, 200);
    return () => clearTimeout(id);
  }, [ready, loadPorts]);

  // While live (after the first Run click), re-simulate whenever the stimulus
  // changes — `runSim`'s identity tracks steps/held/cycles/source, so editing an
  // input redraws the wave automatically. Debounced for rapid edits.
  useEffect(() => {
    if (!ready || !ports || !live) return;
    const id = setTimeout(runSim, 120);
    return () => clearTimeout(id);
  }, [ready, ports, live, runSim]);

  function run(line: string) {
    const trimmed = line.trim();
    if (!trimmed || !ready) return;
    const parts = trimmed.split(/\s+/);
    append([{ kind: "cmd", text: "$ mimz " + trimmed }]);
    try {
      const out = runCommand(source, parts[0], parts.slice(1));
      append([{ kind: "out", text: out.replace(/\s+$/, "") || "(no output)" }]);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      append([{ kind: "err", text: msg.replace(/\s+$/, "") }]);
    }
  }

  // A fresh design starts paused: clear the wave and stop live updates until the
  // user clicks Run again.
  function resetRun() {
    setLive(false);
    setVcd(null);
  }

  function loadExample(name: string) {
    const ex = examples.find((e) => e.name === name);
    if (ex) {
      setSource(sourceOf(ex, flavor));
      setExampleName(name);
      resetRun();
    }
  }

  // Re-skin the current example into another keyword flavor (same design, same
  // grammar — only the keywords change). Leaves a hand-edited buffer alone only
  // if it still matches a known example; otherwise the picked example wins.
  function changeFlavor(next: string) {
    setFlavor(next);
    const ex = examples.find((e) => e.name === exampleName);
    if (ex) setSource(sourceOf(ex, next));
    resetRun();
  }

  function setCell(t: number, name: string, val: string) {
    setSteps((prev) =>
      prev.map((s, i) => (i === t ? { ...s, [name]: val } : s)),
    );
  }
  function addStep() {
    setSteps((prev) => [...prev, ports ? zeroVec(ports.inputs) : {}]);
  }
  function removeStep(t: number) {
    setSteps((prev) =>
      prev.length > 1 ? prev.filter((_, i) => i !== t) : prev,
    );
  }

  // Insert two spaces on Tab instead of leaving the textarea.
  function onEditorKey(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key !== "Tab") return;
    e.preventDefault();
    const t = e.currentTarget;
    const s = t.selectionStart;
    const en = t.selectionEnd;
    setSource(source.slice(0, s) + "  " + source.slice(en));
    requestAnimationFrame(() => {
      t.selectionStart = t.selectionEnd = s + 2;
    });
  }

  // A single input cell: a 0/1 toggle for 1-bit ports, a value field otherwise.
  function cellControl(
    p: Port,
    value: string,
    onChange: (v: string) => void,
    label: string,
  ) {
    if (p.width === 1) {
      const on = value.trim() === "1";
      return (
        <button
          type="button"
          className="pg-bit"
          aria-label={label}
          aria-pressed={on}
          onClick={() => onChange(on ? "0" : "1")}
        >
          {on ? "1" : "0"}
        </button>
      );
    }
    return (
      <input
        className="pg-cell"
        aria-label={label}
        value={value}
        inputMode="numeric"
        onChange={(e) => onChange(e.currentTarget.value)}
      />
    );
  }

  return (
    <>
      <div className="grid gap-4 lg:grid-cols-2">
        {/* Editor */}
        <div className="flex min-w-0 flex-col">
          <div className="mb-2 flex flex-wrap items-center gap-2">
            <label className="pg-label" htmlFor="pg-example">
              Example
            </label>
            <select
              id="pg-example"
              aria-label="Load an example program"
              value={exampleName}
              onChange={(e) => loadExample(e.currentTarget.value)}
              className="pg-select"
            >
              {examples.map((e) => (
                <option key={e.name} value={e.name}>
                  {e.name}
                </option>
              ))}
            </select>
            <label className="pg-label" htmlFor="pg-flavor">
              Flavor
            </label>
            <select
              id="pg-flavor"
              aria-label="Keyword flavor"
              value={flavor}
              onChange={(e) => changeFlavor(e.currentTarget.value)}
              className="pg-select"
            >
              {flavors.map((f) => (
                <option key={f.key} value={f.key}>
                  {f.label}
                </option>
              ))}
            </select>
          </div>
          <textarea
            aria-label="Min-Mozhi source editor"
            value={source}
            onChange={(e) => setSource(e.currentTarget.value)}
            onKeyDown={onEditorKey}
            spellCheck={false}
            className="pg-editor"
          />
        </div>

        {/* Console */}
        <div className="flex min-w-0 flex-col">
          <div className="mb-2 flex flex-wrap items-center gap-2">
            {["compile", "check"].map((c) => (
              <button
                key={c}
                type="button"
                onClick={() => run(c)}
                disabled={!ready}
                className="pg-btn"
              >
                {c}
              </button>
            ))}
            <span className="pg-hint">
              {ready ? "in-browser · no install" : "loading…"}
            </span>
          </div>

          <div
            ref={logRef}
            className="pg-log"
            aria-live="polite"
            aria-label="Console output"
          >
            {log.map((l, i) => (
              <pre key={i} className={"pg-" + l.kind}>
                {l.text}
              </pre>
            ))}
          </div>

          <form
            className="mt-2 flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              run(cmd);
            }}
          >
            <span className="pg-prompt" aria-hidden="true">
              $
            </span>
            <input
              aria-label="mimz command"
              value={cmd}
              onChange={(e) => setCmd(e.currentTarget.value)}
              disabled={!ready}
              placeholder="sim --in a=1 --cycles 8 --trace"
              className="pg-input"
            />
            <button type="submit" disabled={!ready} className="pg-btn-ghost">
              Run
            </button>
          </form>
        </div>
      </div>

      {/* Stimulus — drives the waveform. Adapts to the module type. Simulation
          runs only when the user clicks Simulate (never on edit). */}
      {ports && (ports.inputs.length > 0 || ports.clocked) && (
        <div className="mt-6 pg-stim">
          {ports.clocked ? (
            <>
              <div className="pg-stim-head">
                <span className="pg-label">
                  Inputs · {ports.module} (clocked) — held over {cycles} cycles
                </span>
                <button
                  type="button"
                  className="pg-btn"
                  onClick={() => {
                    setLive(true);
                    runSim();
                  }}
                  disabled={!ready}
                >
                  {live ? "Re-run ▸" : "Run simulation ▸"}
                </button>
              </div>
              <div className="pg-held">
                {ports.inputs.map((p) => (
                  <label key={p.name} className="pg-field">
                    <span>
                      {p.width > 1 ? `${p.name}[${p.width}]` : p.name}
                    </span>
                    {cellControl(
                      p,
                      held[p.name] ?? "0",
                      (v) => setHeld((prev) => ({ ...prev, [p.name]: v })),
                      `${p.name} value`,
                    )}
                  </label>
                ))}
                <label className="pg-field">
                  <span>cycles</span>
                  <input
                    className="pg-cell"
                    aria-label="cycle count"
                    type="number"
                    min={1}
                    max={1024}
                    value={cycles}
                    onChange={(e) =>
                      setCycles(
                        Math.max(
                          1,
                          Math.min(1024, Number(e.currentTarget.value) || 1),
                        ),
                      )
                    }
                  />
                </label>
              </div>
            </>
          ) : (
            <>
              <div className="pg-stim-head">
                <span className="pg-label">
                  Inputs · {ports.module} — one column per step
                </span>
                <span className="flex items-center gap-2">
                  <button
                    type="button"
                    className="pg-btn-ghost"
                    onClick={addStep}
                  >
                    + step
                  </button>
                  <button
                    type="button"
                    className="pg-btn"
                    onClick={() => {
                      setLive(true);
                      runSim();
                    }}
                    disabled={!ready}
                  >
                    {live ? "Re-run ▸" : "Run simulation ▸"}
                  </button>
                </span>
              </div>
              <div className="overflow-x-auto">
                <table className="pg-steps">
                  <thead>
                    <tr>
                      <th scope="col" aria-label="input" />
                      {steps.map((_, t) => (
                        <th key={t} scope="col">
                          <span className="pg-step-head">
                            t{t}
                            {steps.length > 1 && (
                              <button
                                type="button"
                                className="pg-step-x"
                                aria-label={`remove step ${t}`}
                                onClick={() => removeStep(t)}
                              >
                                ×
                              </button>
                            )}
                          </span>
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {ports.inputs.map((p) => (
                      <tr key={p.name}>
                        <th scope="row">
                          {p.width > 1 ? `${p.name}[${p.width}]` : p.name}
                        </th>
                        {steps.map((s, t) => (
                          <td key={t}>
                            {cellControl(
                              p,
                              s[p.name] ?? "0",
                              (v) => setCell(t, p.name, v),
                              `${p.name} at step ${t}`,
                            )}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
          {stimErr && <div className="pg-stim-note">{stimErr}</div>}
          {live && !stimErr && (
            <div className="pg-hint mt-2">
              Live — editing an input updates the wave automatically.
            </div>
          )}
          {!vcd && !stimErr && !live && (
            <div className="pg-hint mt-2">
              Press Run simulation ▸ — then editing inputs updates the wave
              live.
            </div>
          )}
        </div>
      )}

      {vcd && (
        <div className="mt-4">
          <div className="pg-label mb-2">Waveform — hover to read values</div>
          <WaveformViewer vcd={vcd} />
        </div>
      )}
    </>
  );
}
