import { useRef, useState } from "react";
import { useMimz, errMsg } from "../lib/useMimz";

export interface HeroFlavor {
  key: string;
  label: string;
  source: string;
  /** Render the label in the Tamil face. */
  tamil?: boolean;
}

interface Props {
  flavors: HeroFlavor[];
  /** tests/golden/counter.v — what the counter really compiles to. */
  golden: string;
}

type Status =
  | { kind: "static" }
  | { kind: "compiling" }
  | { kind: "ok"; identical: boolean }
  | { kind: "error" }
  | { kind: "edited" };

// The emitted Verilog can carry internal audit notes (BUG-65: why `initial`
// register inits are simulation/FPGA-only). They are aimed at compiler
// maintainers reading tests/golden/, not site visitors — strip them wherever
// this pane shows Verilog, so the static golden and a live compile agree.
function stripAuditNotes(code: string): string {
  return code
    .split("\n")
    .filter((line) => !line.trim().startsWith("// NOTE (BUG"))
    .join("\n");
}

// The landing hero: the real compiler, not a picture of one.
//
// The static state is not a mock — it is `tests/golden/counter.v`, the exact
// Verilog this source compiles to, kept honest by the test suite. So the pane
// is truthful before the 2.8 MB wasm module has been fetched, and the module is
// only fetched when a visitor actually presses something (useMimz.load() is
// lazy and idempotent).
export default function HeroCompile({ flavors, golden: rawGolden }: Props) {
  const golden = stripAuditNotes(rawGolden);
  const { load, run } = useMimz();
  const [active, setActive] = useState(0);
  const [source, setSource] = useState(flavors[0].source);
  const [output, setOutput] = useState(golden);
  const [status, setStatus] = useState<Status>({ kind: "static" });
  // Verilog per flavor, so switching can prove the outputs match rather than
  // asserting it in a caption. A ref, not state: nothing renders from it, and
  // a ref cannot go stale inside an async callback.
  const seenRef = useRef<Record<string, string>>({});
  // Ticket number, bumped by every user action. An async completion whose
  // ticket is no longer current was superseded by a tab switch or an edit
  // while `await load()` was pending — it must paint nothing.
  const actionId = useRef(0);

  async function compile(src: string, flavorKey: string) {
    const id = ++actionId.current;
    setStatus({ kind: "compiling" });
    try {
      await load();
      const verilog = stripAuditNotes(run(src, "compile", []).trimEnd());
      if (id !== actionId.current) return; // superseded mid-await — drop it
      const prev = seenRef.current;
      const others = Object.entries(prev).filter(([k]) => k !== flavorKey);
      const identical =
        others.length > 0 && others.every(([, v]) => v === verilog);
      seenRef.current = { ...prev, [flavorKey]: verilog };
      setOutput(verilog);
      setStatus({ kind: "ok", identical });
    } catch (e) {
      if (id !== actionId.current) return;
      setOutput(errMsg(e));
      setStatus({ kind: "error" });
    }
  }

  function pick(i: number) {
    const f = flavors[i];
    if (i === active && status.kind !== "edited") return;
    actionId.current += 1; // cancel any in-flight compile
    setActive(i);
    setSource(f.source);
    // Before wasm has ever loaded, the golden output is still correct for every
    // flavor — that is the whole point — so do not fetch 2.8 MB just to switch
    // a tab. Once a result exists for this session, re-prove it live instead
    // of trusting the golden file.
    if (status.kind === "ok" || status.kind === "error") {
      void compile(f.source, f.key);
    } else {
      setStatus({ kind: "static" });
      setOutput(golden);
    }
  }

  const label =
    status.kind === "compiling"
      ? "compiling…"
      : status.kind === "edited"
        ? "edited — press Compile"
        : status.kind === "error"
          ? "does not compile"
          : status.kind === "ok" && status.identical
            ? "0 errors · identical Verilog"
            : "0 errors";

  return (
    <div className="hero-pane">
      <div className="hero-col">
        <div className="hero-bar" role="tablist" aria-label="Keyword flavor">
          {flavors.map((f, i) => (
            <button
              key={f.key}
              role="tab"
              type="button"
              aria-selected={i === active}
              className={`hero-tab${i === active ? " is-on" : ""}${f.tamil ? " ff-tamil" : ""}`}
              onClick={() => pick(i)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <textarea
          className="hero-editor"
          spellCheck={false}
          value={source}
          aria-label="Min-Mozhi source"
          onChange={(e) => {
            actionId.current += 1; // an edit invalidates any shown result
            setSource(e.target.value);
            setOutput("");
            setStatus({ kind: "edited" });
          }}
        />
        <div className="hero-actions">
          <button
            type="button"
            className="pg-btn"
            onClick={() => void compile(source, flavors[active].key)}
          >
            Compile
          </button>
          <span className="pg-hint">
            Runs in your browser. Nothing is uploaded.
          </span>
        </div>
      </div>

      <div className="hero-col">
        <div className="hero-bar">
          <span className="hero-bar-title">Verilog</span>
          <span
            className={`hero-status${status.kind === "error" ? " is-err" : ""}`}
          >
            {label}
          </span>
        </div>
        {/* While edited, `output` is empty on purpose: showing the previous
            Verilog next to code that no longer produces it would be a lie,
            and showing `golden` would claim unverified code is clean. */}
        <pre className="hero-out">
          {status.kind === "edited"
            ? "Source changed — press Compile."
            : output}
        </pre>
      </div>
    </div>
  );
}
