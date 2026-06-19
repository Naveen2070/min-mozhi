import { useEffect, useRef, useState } from "react";
import init, { runCommand } from "../lib/wasm/mimz_wasm.js";
import wasmUrl from "../lib/wasm/mimz_wasm_bg.wasm?url";

interface Example {
  name: string;
  code: string;
}
interface Props {
  examples: Example[];
}

type LineKind = "cmd" | "out" | "err" | "note";
interface Line {
  kind: LineKind;
  text: string;
}

const SUGGESTIONS = [
  "compile",
  "check",
  "eval --in a=3,b=5",
  "sim --cycles 8 --trace",
  "sim --sweep a=0|1|2 --trace",
];

export default function Playground({ examples }: Props) {
  const [source, setSource] = useState(examples[0]?.code ?? "");
  const [exampleName, setExampleName] = useState(examples[0]?.name ?? "");
  const [log, setLog] = useState<Line[]>([{ kind: "note", text: "Loading the in-browser compiler…" }]);
  const [ready, setReady] = useState(false);
  const [cmd, setCmd] = useState("compile");
  const logRef = useRef<HTMLDivElement>(null);

  // Load the wasm module once. `{ module_or_path }` is the non-deprecated form.
  useEffect(() => {
    let alive = true;
    init({ module_or_path: wasmUrl })
      .then(() => {
        if (!alive) return;
        setReady(true);
        setLog([
          { kind: "note", text: "mimz ready — runs entirely in your browser. Press Run or Enter." },
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

  function run(line: string) {
    const trimmed = line.trim();
    if (!trimmed || !ready) return;
    const parts = trimmed.split(/\s+/);
    const command = parts[0];
    const args = parts.slice(1);
    append([{ kind: "cmd", text: "$ mimz " + trimmed }]);
    try {
      const out = runCommand(source, command, args);
      append([{ kind: "out", text: out.replace(/\s+$/, "") || "(no output)" }]);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      append([{ kind: "err", text: msg.replace(/\s+$/, "") }]);
    }
  }

  function loadExample(name: string) {
    const ex = examples.find((e) => e.name === name);
    if (ex) {
      setSource(ex.code);
      setExampleName(name);
    }
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

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      {/* Editor */}
      <div className="flex min-w-0 flex-col">
        <div className="mb-2 flex items-center gap-2">
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
            <button key={c} type="button" onClick={() => run(c)} disabled={!ready} className="pg-btn">
              {c}
            </button>
          ))}
          <span className="pg-hint">{ready ? "in-browser · no install" : "loading…"}</span>
        </div>

        <div ref={logRef} className="pg-log" aria-live="polite" aria-label="Console output">
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
  );
}
