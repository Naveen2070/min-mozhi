import { useEffect, useRef, useState } from "react";

// The hero oscilloscope: a faint blue circuit grid with a scrolling lightning-
// yellow signal — the waveform an HDL describes. Pure 2D canvas, no deps. Now
// interactive: play/pause, a speed (clock-frequency) slider, and a signal switch
// (clock / counter bus / random) drive the live trace. Honours prefers-reduced-
// motion by holding a single static frame (and starting paused).

type Signal = "clock" | "counter" | "random";

export default function Hero() {
  const ref = useRef<HTMLCanvasElement>(null);
  const reduceRef = useRef(false);

  const [playing, setPlaying] = useState(true);
  const [speed, setSpeed] = useState(1);
  const [signal, setSignal] = useState<Signal>("clock");

  // Latest control values for the rAF loop, without restarting it on change.
  const playingRef = useRef(playing);
  playingRef.current = playing;
  const speedRef = useRef(speed);
  speedRef.current = speed;
  const signalRef = useRef<Signal>(signal);
  signalRef.current = signal;

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const reduce = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    reduceRef.current = reduce;
    if (reduce) setPlaying(false);

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const css = getComputedStyle(document.documentElement);
    const blue = css.getPropertyValue("--color-volt-400").trim() || "#3b9eff";
    const yellow = css.getPropertyValue("--color-bolt-400").trim() || "#ffd60a";

    let w = 0;
    let h = 0;
    let raf = 0;
    let phase = 0;
    let last = 0;

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      w = rect.width;
      h = rect.height;
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    // Signal level in [-1, 1] for a given x (in cells already offset by phase).
    const level = (sig: Signal, cell: number) => {
      if (sig === "clock") return cell % 2 === 0 ? 1 : -1;
      if (sig === "counter") {
        const lvl = (((cell % 8) + 8) % 8) / 7;
        return lvl * 2 - 1;
      }
      const hsh = Math.abs(Math.sin(cell * 12.9898) * 43758.5453) % 1;
      return hsh > 0.5 ? 1 : -1;
    };

    const draw = (t: number) => {
      const dt = last ? (t - last) / 1000 : 0;
      last = t;
      if (playingRef.current && !reduceRef.current) {
        phase += dt * speedRef.current * 60;
      }

      ctx.clearRect(0, 0, w, h);

      // Faint blue grid (drifts only while playing).
      ctx.strokeStyle = blue + "20";
      ctx.lineWidth = 1;
      const step = 28;
      ctx.beginPath();
      for (let x = -(phase / 1.5) % step; x < w; x += step) {
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
      }
      for (let y = 0; y < h; y += step) {
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
      }
      ctx.stroke();

      // Scrolling signal trace.
      const cellW = signalRef.current === "counter" ? 30 : 46;
      const mid = h / 2;
      const amp = Math.min(h * 0.2, 54);
      ctx.beginPath();
      let started = false;
      for (let x = -2; x <= w + 2; x += 1) {
        const cell = Math.floor((x + phase) / cellW);
        const y = mid - level(signalRef.current, cell) * amp;
        if (!started) {
          ctx.moveTo(x, y);
          started = true;
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.strokeStyle = yellow;
      ctx.lineWidth = 2.5;
      ctx.lineJoin = "miter";
      ctx.shadowColor = yellow;
      ctx.shadowBlur = 18;
      ctx.stroke();
      ctx.shadowBlur = 0;

      raf = requestAnimationFrame(draw);
    };

    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    raf = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, []);

  return (
    <div className="hero-scene">
      <div className="hero-stage">
        <canvas ref={ref} className="h-full w-full" aria-hidden="true" />
      </div>
      <div className="hero-controls">
        <button
          type="button"
          className="hero-btn"
          aria-pressed={playing}
          aria-label={playing ? "Pause waveform" : "Play waveform"}
          onClick={() => setPlaying((p) => !p)}
        >
          {playing ? "❚❚ Pause" : "▶ Play"}
        </button>
        <label className="hero-ctl">
          <span>Speed</span>
          <input
            className="hero-range"
            type="range"
            min="0.2"
            max="2.4"
            step="0.1"
            value={speed}
            aria-label="Clock speed"
            onChange={(e) => setSpeed(parseFloat(e.target.value))}
          />
        </label>
        <label className="hero-ctl">
          <span>Signal</span>
          <select
            className="hero-select"
            value={signal}
            aria-label="Signal type"
            onChange={(e) => setSignal(e.target.value as Signal)}
          >
            <option value="clock">Clock</option>
            <option value="counter">Counter bus</option>
            <option value="random">Random</option>
          </select>
        </label>
      </div>
    </div>
  );
}
