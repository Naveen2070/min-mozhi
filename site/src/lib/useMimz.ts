// The shared wasm driver behind the interactive surfaces (plan D4).
//
// One hook owns what every island duplicates today: lazy module init, the
// runCommand wrapper with its console-log trail, and the `ports` shape. The
// lab consumes it from day one; the Playground and LiveWaveform still carry
// their own copies until the optional Phase-2 refactor.
//
// Init is LAZY on purpose (plan D5): a 2.8 MB fetch must not fire because a
// page mounted — only when the visitor actually runs something. `load()` is
// idempotent, so "first click OR requestIdleCallback, whichever fires first"
// is safe to call from both.
import { useCallback, useRef, useState } from "react";
import init, { runCommand } from "./wasm/mimz_wasm.js";
import wasmUrl from "./wasm/mimz_wasm_bg.wasm?url";

export type LineKind = "cmd" | "out" | "err" | "note";
export interface Line {
  kind: LineKind;
  text: string;
}

// A module's interface, from the `ports` command.
export interface Port {
  name: string;
  width: number;
  signed: boolean;
}
export interface Ports {
  module: string;
  clocked: boolean;
  inputs: Port[];
  outputs: Port[];
}

/** Errors arrive as Error/JsError/string depending on the throw site. */
export function errMsg(e: unknown): string {
  return (e instanceof Error ? e.message : String(e)).replace(/\s+$/, "");
}

/** Parse a source's module interface; null when it does not elaborate. */
export function parsePorts(source: string): Ports | null {
  try {
    return JSON.parse(runCommand(source, "ports", [])) as Ports;
  } catch {
    return null;
  }
}

const BOOT_NOTE = "Loading the in-browser compiler…";

export function useMimz() {
  const [log, setLog] = useState<Line[]>([{ kind: "note", text: BOOT_NOTE }]);
  const [ready, setReady] = useState(false);
  // The init promise lives in a ref so `load()` stays idempotent across
  // renders without re-triggering effects.
  const initRef = useRef<Promise<unknown> | null>(null);

  const append = useCallback((lines: Line[]) => {
    setLog((prev) => [...prev, ...lines]);
  }, []);

  /** Start loading the wasm module exactly once; resolves when ready. */
  const load = useCallback((): Promise<unknown> => {
    if (!initRef.current) {
      initRef.current = init({ module_or_path: wasmUrl })
        .then(() => {
          setReady(true);
          setLog((prev) =>
            prev.map((l) =>
              l.text === BOOT_NOTE ? { ...l, text: "mimz ready." } : l,
            ),
          );
        })
        .catch((e: unknown) => {
          initRef.current = null; // allow a retry after a failed fetch
          setLog([
            { kind: "err", text: "Failed to load the compiler: " + errMsg(e) },
          ]);
          throw e;
        });
    }
    return initRef.current;
  }, []);

  /**
   * Run one command against `source`, narrating into the log like a shell.
   * Returns the output on success, REJECTS on failure — callers that need to
   * distinguish pass/fail (grading, D3) wrap their own try/catch around it.
   */
  const run = useCallback(
    (source: string, command: string, args: string[]): string => {
      append([{ kind: "cmd", text: "$ mimz " + [command, ...args].join(" ") }]);
      try {
        const out = runCommand(source, command, args);
        append([
          { kind: "out", text: out.replace(/\s+$/, "") || "(no output)" },
        ]);
        return out;
      } catch (e: unknown) {
        append([{ kind: "err", text: errMsg(e) }]);
        throw e;
      }
    },
    [append],
  );

  return { ready, load, run, log, append };
}
