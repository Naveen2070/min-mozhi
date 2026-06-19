import { useEffect, useRef } from "react";

// A lightweight on-theme hero: a faint blue circuit grid with a scrolling
// lightning-yellow clock waveform (the signal an HDL describes). Pure canvas, no
// deps; honours prefers-reduced-motion by drawing a single static frame.
export default function Hero() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const reduce = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const css = getComputedStyle(document.documentElement);
    const blue = css.getPropertyValue("--color-volt-400").trim() || "#3b9eff";
    const yellow = css.getPropertyValue("--color-bolt-400").trim() || "#ffd60a";

    let w = 0;
    let h = 0;
    let raf = 0;

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      w = rect.width;
      h = rect.height;
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    const draw = (t: number) => {
      ctx.clearRect(0, 0, w, h);

      // Faint blue grid.
      ctx.strokeStyle = blue + "20";
      ctx.lineWidth = 1;
      const step = 28;
      ctx.beginPath();
      for (let x = (-t / 40) % step; x < w; x += step) {
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
      }
      for (let y = 0; y < h; y += step) {
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
      }
      ctx.stroke();

      // Scrolling clock waveform in lightning yellow.
      const halfPeriod = 46;
      const offset = reduce ? 0 : (t / 14) % (halfPeriod * 2);
      const mid = h / 2;
      const amp = Math.min(h * 0.2, 54);
      ctx.beginPath();
      let started = false;
      for (let x = -halfPeriod * 2; x <= w + halfPeriod; x += 1) {
        const high = Math.floor((x + offset) / halfPeriod) % 2 === 0;
        const y = mid - (high ? amp : -amp);
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

      if (!reduce) raf = requestAnimationFrame(draw);
    };

    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    if (reduce) draw(0);
    else raf = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, []);

  return <canvas ref={ref} className="h-full w-full" aria-hidden="true" />;
}
