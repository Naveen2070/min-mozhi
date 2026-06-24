import { useCallback, useEffect, useRef } from "react";

// A small, self-contained waveform renderer. The `vcd` string is the stable
// contract (a 2-state IEEE-1364 doc from `mimz sim --vcd`); swap this component
// for Surfer later without touching the playground. Parses the VCD, then draws
// each signal as a canvas row — square waves for 1-bit, value-labelled buses for
// wider signals. Hovering reads each signal's value at that time (a cursor line
// + per-signal value chips, drawn on the same canvas — no DOM overlay).

interface Sig {
  id: string;
  name: string;
  width: number;
}
interface Parsed {
  signals: Sig[];
  times: number[];
  // id -> value at each time index (carry-forward); null = x/z/unknown.
  values: Map<string, (bigint | null)[]>;
}
// Geometry shared between draw() and the hover handler (mouse-x -> time index).
interface Layout {
  labelW: number;
  step: number;
  n: number;
}

function parseVcd(text: string): Parsed {
  const signals: Sig[] = [];
  const values = new Map<string, (bigint | null)[]>();
  const times: number[] = [];
  let inDefs = true;
  let t = -1;

  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line) continue;

    if (inDefs) {
      if (line.startsWith("$var")) {
        // $var wire WIDTH ID NAME $end
        const p = line.split(/\s+/);
        const width = parseInt(p[2], 10) || 1;
        const id = p[3];
        const name = p[4];
        signals.push({ id, name, width });
        values.set(id, []);
      } else if (line.startsWith("$enddefinitions")) {
        inDefs = false;
      }
      continue;
    }

    if (line.startsWith("#")) {
      t = times.length;
      times.push(parseInt(line.slice(1), 10) || 0);
      // Carry each signal's previous value forward into the new column.
      for (const s of signals) {
        const arr = values.get(s.id)!;
        arr[t] = t > 0 ? arr[t - 1] : null;
      }
      continue;
    }
    if (line.startsWith("$")) continue; // $dumpvars / $end markers
    if (t < 0) continue;

    if (line[0] === "b") {
      // bVALUE ID
      const sp = line.indexOf(" ");
      const bits = line.slice(1, sp);
      const id = line.slice(sp + 1);
      const arr = values.get(id);
      if (arr) arr[t] = /^[01]+$/.test(bits) ? BigInt("0b" + bits) : null;
    } else {
      // scalar: 0ID / 1ID / xID / zID
      const c = line[0];
      const id = line.slice(1);
      const arr = values.get(id);
      if (arr) arr[t] = c === "1" ? 1n : c === "0" ? 0n : null;
    }
  }
  return { signals, times, values };
}

function cssVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return v || fallback;
}

// Format a signal value the way both the bus labels and the hover chips show it.
function fmtValue(width: number, v: bigint | null): string {
  if (v === null) return "x";
  return width > 16 ? "0x" + v.toString(16) : v.toString();
}

function draw(
  canvas: HTMLCanvasElement,
  wrap: HTMLDivElement,
  p: Parsed,
  cursorIdx: number | null,
): Layout {
  const ctx = canvas.getContext("2d");
  const labelW = 120;
  const rowH = 30;
  const topPad = 22;
  const n = Math.max(1, p.times.length);
  if (!ctx) return { labelW, step: 1, n };

  const col = {
    text: cssVar("--text", "#e8eef7"),
    muted: cssVar("--text-muted", "#93a7c4"),
    accent: cssVar("--accent", "#ffd60a"),
    border: cssVar("--border", "#1e3658"),
    soft: cssVar("--bg-soft", "#0d1b2e"),
  };

  const cssW = Math.max(360, wrap.clientWidth);
  const height = topPad + p.signals.length * rowH + 8;
  const dpr = window.devicePixelRatio || 1;

  // Size the backing store via attributes only (no inline styles); the display
  // size is handled in CSS (`.pg-wave canvas { width:100%; height:auto }`), which
  // scales this device-pixel buffer down to one CSS pixel per logical unit.
  canvas.width = Math.floor(cssW * dpr);
  canvas.height = Math.floor(height * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, cssW, height);
  ctx.font = '11px "Noto Sans Tamil", ui-monospace, monospace';
  ctx.textBaseline = "middle";

  const plotW = cssW - labelW - 8;
  const step = plotW / n;
  const x = (i: number) => labelW + i * step;

  // Faint column gridlines + a few time ticks.
  ctx.strokeStyle = col.border;
  ctx.fillStyle = col.muted;
  ctx.lineWidth = 1;
  ctx.textAlign = "center";
  const tickEvery = Math.ceil(n / 12);
  for (let i = 0; i <= n; i++) {
    ctx.globalAlpha = 0.35;
    ctx.beginPath();
    ctx.moveTo(x(i), topPad - 4);
    ctx.lineTo(x(i), height - 4);
    ctx.stroke();
    ctx.globalAlpha = 1;
    if (i < n && i % tickEvery === 0)
      ctx.fillText(String(p.times[i] ?? i), x(i) + step / 2, 10);
  }

  p.signals.forEach((s, r) => {
    const arr = p.values.get(s.id) ?? [];
    const rowTop = topPad + r * rowH;
    const hi = rowTop + 6;
    const lo = rowTop + rowH - 8;
    const mid = (hi + lo) / 2;

    // Signal name (+ width) in the gutter.
    ctx.fillStyle = col.muted;
    ctx.textAlign = "left";
    const label = s.width > 1 ? `${s.name}[${s.width}]` : s.name;
    ctx.fillText(label.length > 16 ? label.slice(0, 15) + "…" : label, 6, mid);

    ctx.strokeStyle = col.accent;
    ctx.fillStyle = col.text;
    ctx.lineWidth = 1.5;

    for (let i = 0; i < n; i++) {
      const x0 = x(i);
      const x1 = x(i + 1);
      const v = arr[i] ?? null;

      if (s.width <= 1) {
        const y = v === 1n ? hi : lo;
        // vertical transition from the previous level
        if (i > 0) {
          const pv = arr[i - 1] ?? null;
          if (pv !== v) {
            const py = pv === 1n ? hi : lo;
            ctx.beginPath();
            ctx.moveTo(x0, py);
            ctx.lineTo(x0, y);
            ctx.stroke();
          }
        }
        ctx.beginPath();
        ctx.moveTo(x0, y);
        ctx.lineTo(x1, y);
        ctx.stroke();
      } else {
        // bus: two rails + a crossing at each change, value label centered
        const pv = i > 0 ? (arr[i - 1] ?? null) : null;
        const changed = i === 0 || pv !== v;
        const inset = changed ? 3 : 0;
        ctx.beginPath();
        ctx.moveTo(x0 + inset, hi);
        ctx.lineTo(x1, hi);
        ctx.moveTo(x0 + inset, lo);
        ctx.lineTo(x1, lo);
        if (changed && i > 0) {
          ctx.moveTo(x0, mid);
          ctx.lineTo(x0 + inset, hi);
          ctx.moveTo(x0, mid);
          ctx.lineTo(x0 + inset, lo);
        }
        ctx.stroke();
        if (changed && step > 22 && v !== null) {
          ctx.textAlign = "left";
          ctx.fillText(fmtValue(s.width, v), x0 + inset + 3, mid);
        }
      }
    }
  });

  // Hover cursor: a vertical accent line at the column plus a value chip per
  // signal, so you can read every signal at one time point (the fix for "a flat
  // wave tells me nothing").
  if (cursorIdx !== null && cursorIdx >= 0 && cursorIdx < n) {
    const cx = x(cursorIdx + 0.5);
    ctx.strokeStyle = col.accent;
    ctx.lineWidth = 1;
    ctx.globalAlpha = 0.9;
    ctx.beginPath();
    ctx.moveTo(cx, topPad - 6);
    ctx.lineTo(cx, height - 4);
    ctx.stroke();
    ctx.globalAlpha = 1;

    ctx.fillStyle = col.accent;
    ctx.textAlign = "center";
    ctx.font = '11px "Noto Sans Tamil", ui-monospace, monospace';
    ctx.fillText("t" + (p.times[cursorIdx] ?? cursorIdx), cx, 10);

    p.signals.forEach((s, r) => {
      const v = (p.values.get(s.id) ?? [])[cursorIdx] ?? null;
      const mid = topPad + r * rowH + (rowH - 2) / 2;
      const txt = fmtValue(s.width, v);
      const w = ctx.measureText(txt).width + 6;
      let bx = cx + 4;
      if (bx + w > cssW) bx = cx - 4 - w;
      ctx.fillStyle = col.soft;
      ctx.globalAlpha = 0.95;
      ctx.fillRect(bx, mid - 8, w, 16);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = col.border;
      ctx.strokeRect(bx, mid - 8, w, 16);
      ctx.fillStyle = col.text;
      ctx.textAlign = "left";
      ctx.fillText(txt, bx + 3, mid);
    });
  }

  return { labelW, step, n };
}

export default function WaveformViewer({ vcd }: { vcd: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const parsedRef = useRef<Parsed | null>(null);
  const layoutRef = useRef<Layout | null>(null);

  const redraw = useCallback((cursorIdx: number | null) => {
    const c = canvasRef.current;
    const w = wrapRef.current;
    const p = parsedRef.current;
    if (c && w && p) layoutRef.current = draw(c, w, p, cursorIdx);
  }, []);

  useEffect(() => {
    parsedRef.current = parseVcd(vcd);
    redraw(null);
    const onResize = () => redraw(null);
    const ro = new ResizeObserver(onResize);
    if (wrapRef.current) ro.observe(wrapRef.current);
    // Redraw on theme toggle (the <html> class flips).
    const mo = new MutationObserver(onResize);
    mo.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  }, [vcd, redraw]);

  function onMove(e: React.MouseEvent<HTMLDivElement>) {
    const wrap = wrapRef.current;
    const lay = layoutRef.current;
    if (!wrap || !lay) return;
    const rect = wrap.getBoundingClientRect();
    const px = e.clientX - rect.left + wrap.scrollLeft;
    if (px < lay.labelW) {
      redraw(null);
      return;
    }
    const idx = Math.min(
      lay.n - 1,
      Math.max(0, Math.floor((px - lay.labelW) / lay.step)),
    );
    redraw(idx);
  }

  return (
    <div
      ref={wrapRef}
      className="pg-wave"
      onMouseMove={onMove}
      onMouseLeave={() => redraw(null)}
    >
      <canvas ref={canvasRef} />
    </div>
  );
}
