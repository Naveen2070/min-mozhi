import { useEffect, useState, useCallback } from "react";
import { createPortal } from "react-dom";
import init, { runCommand } from "../lib/wasm/mimz_wasm.js";
import wasmUrl from "../lib/wasm/mimz_wasm_bg.wasm?url";
import WaveformViewer from "./WaveformViewer.tsx";

interface Props {
  sources: Record<string, string>;
}

// A module's interface, from the `ports` command
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

function LiveWaveform({
  source,
  initialCycles,
  initialInputs,
  initialSweep,
}: {
  source: string;
  initialCycles: number;
  initialInputs: Record<string, string>;
  initialSweep?: string;
}) {
  const [ready, setReady] = useState(false);
  const [vcd, setVcd] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ports, setPorts] = useState<Ports | null>(null);
  
  const [cycles, setCycles] = useState(initialCycles);
  const [held, setHeld] = useState<Record<string, string>>(initialInputs);

  useEffect(() => {
    let alive = true;
    init({ module_or_path: wasmUrl })
      .then(() => {
        if (!alive) return;
        setReady(true);
        try {
          const info = JSON.parse(runCommand(source, "ports", [])) as Ports;
          setPorts(info);
          // Only clocked designs have held inputs for now
        } catch (e) {
          setError(e instanceof Error ? e.message : String(e));
        }
      })
      .catch((e) => {
        if (!alive) return;
        setError("Failed to load compiler: " + String(e));
      });
    return () => {
      alive = false;
    };
  }, [source]);

  const runSim = useCallback(() => {
    if (!ports) return;
    const args: string[] = [];
    if (ports.clocked) {
      const inv = ports.inputs
        .map((p) => `${p.name}=${(held[p.name] ?? "0").trim() || "0"}`)
        .join(",");
      if (inv) args.push("--in", inv);
      args.push("--cycles", String(cycles));
    } else if (initialSweep) {
      args.push("--sweep", initialSweep);
    }
    args.push("--vcd");
    try {
      setVcd(runCommand(source, "sim", args));
      setError(null);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [ports, held, cycles, source, initialSweep]);

  // Run automatically once ready
  useEffect(() => {
    if (ready && ports) {
      runSim();
    }
  }, [ready, ports]);

  // Debounced live simulation on input changes
  useEffect(() => {
    if (!ready || !ports) return;
    const id = setTimeout(runSim, 120);
    return () => clearTimeout(id);
  }, [held, cycles, ready, ports, runSim]);

  function cellControl(p: Port) {
    const value = held[p.name] ?? "0";
    if (p.width === 1) {
      const on = value.trim() === "1";
      return (
        <button
          type="button"
          className="pg-bit"
          aria-label={`${p.name} value`}
          aria-pressed={on}
          onClick={() => setHeld((prev) => ({ ...prev, [p.name]: on ? "0" : "1" }))}
        >
          {on ? "1" : "0"}
        </button>
      );
    }
    return (
      <input
        className="pg-cell"
        aria-label={`${p.name} value`}
        value={value}
        inputMode="numeric"
        onChange={(e) => setHeld((prev) => ({ ...prev, [p.name]: e.currentTarget.value }))}
      />
    );
  }

  return (
    <div className="my-8 rounded-lg border border-zinc-200 bg-zinc-50/50 p-4 dark:border-zinc-800 dark:bg-zinc-900/50">
      <div className="mb-4 flex items-center justify-between">
        <span className="text-sm font-semibold tracking-wide text-zinc-900 dark:text-zinc-100">
          Interactive Waveform
        </span>
        <span className="text-xs text-zinc-500">Live WASM Compilation</span>
      </div>

      {ports && ports.clocked && (
        <div className="pg-stim mb-4 bg-white dark:bg-zinc-950">
          <div className="pg-stim-head">
            <span className="pg-label">
              Inputs · {ports.module}
            </span>
          </div>
          <div className="pg-held">
            {ports.inputs.map((p) => (
              <label key={p.name} className="pg-field">
                <span>{p.width > 1 ? `${p.name}[${p.width}]` : p.name}</span>
                {cellControl(p)}
              </label>
            ))}
            <label className="pg-field">
              <span>cycles</span>
              <input
                className="pg-cell"
                type="number"
                min={1}
                max={128}
                value={cycles}
                onChange={(e) =>
                  setCycles(Math.max(1, Math.min(128, Number(e.currentTarget.value) || 1)))
                }
              />
            </label>
          </div>
        </div>
      )}

      {error && <div className="rounded bg-red-50 p-3 text-sm text-red-600 dark:bg-red-900/20 dark:text-red-400 mb-4">{error}</div>}
      
      {vcd ? (
        <div className="overflow-hidden rounded-md border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950">
          <WaveformViewer vcd={vcd} />
        </div>
      ) : !error ? (
        <div className="py-8 text-center text-sm text-zinc-500">Compiling and simulating...</div>
      ) : null}
    </div>
  );
}

export default function LiveWaveformMount({ sources }: Props) {
  const [mounts, setMounts] = useState<Element[]>([]);

  useEffect(() => {
    // Find all <div class="live-waveform" ...> elements
    const els = document.querySelectorAll(".live-waveform");
    setMounts(Array.from(els));
  }, []);

  return (
    <>
      {mounts.map((el, i) => {
        const module = el.getAttribute("data-module");
        if (!module || !sources[module]) return null;
        
        const cycles = parseInt(el.getAttribute("data-cycles") || "16", 10);
        
        // Parse data-inputs="duty=10,rst=0"
        const inputsAttr = el.getAttribute("data-inputs") || "";
        const initialInputs: Record<string, string> = {};
        if (inputsAttr) {
          inputsAttr.split(",").forEach(pair => {
            const [k, v] = pair.split("=");
            if (k && v) initialInputs[k.trim()] = v.trim();
          });
        }

        const sweepAttr = el.getAttribute("data-sweep") || undefined;

        return createPortal(
          <LiveWaveform 
            source={sources[module]} 
            initialCycles={cycles} 
            initialInputs={initialInputs} 
            initialSweep={sweepAttr}
          />,
          el,
          `live-waveform-${i}`
        );
      })}
    </>
  );
}
