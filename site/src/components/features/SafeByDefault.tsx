import { useEffect, useRef, useState } from "react";
import { useReveal } from "./useReveal";

// "Modern & safe by default" — cycles through all six refusals from the list
// beside it, each redrawn as the same wire-and-badge diagram: a chip, a
// connection, and the compiler's diagnostic catching it. One box, six proofs,
// instead of one box repeating the width-mismatch example forever.
//
// Auto-advances every 4.2s, pauses on hover/focus, and the dots let a visitor
// jump straight to any one. Reveals on scroll; under prefers-reduced-motion the
// timer never starts (still browsable via the dots) and the first frame holds.

interface Chip {
  label: string;
  sub: string;
}

interface Scene {
  /** Real E-code from crates/mimz-core/src/explain.rs — kept honest on purpose. */
  code: string;
  badge: string;
  left: Chip;
  right: Chip;
  /** A dashed wire reads as "missing", a solid one as "present but wrong". */
  dashed: boolean;
  /** Multiple drivers gets a second source chip stacked above the left one. */
  extraLeft?: Chip;
  caption: string;
}

const SCENES: Scene[] = [
  {
    code: "E0601",
    badge: "match not exhaustive",
    left: { label: "sel", sub: "match" },
    right: { label: "q", sub: "reg" },
    dashed: true,
    caption: "A match missing a catch-all arm — the uncovered state latches.",
  },
  {
    code: "E0401",
    badge: "width mismatch",
    left: { label: "din", sub: "[8]" },
    right: { label: "q", sub: "[4]" },
    dashed: false,
    caption: "An 8-bit value into a 4-bit port — caught at compile time.",
  },
  {
    code: "E0501",
    badge: "more than one driver",
    left: { label: "on #1", sub: "clk" },
    extraLeft: { label: "on #2", sub: "clk" },
    right: { label: "led", sub: "reg" },
    dashed: false,
    caption: "Two on blocks write one reg — caught at compile time.",
  },
  {
    code: "E0301",
    badge: "regs but no reset",
    left: { label: "—", sub: "reset" },
    right: { label: "cnt", sub: "reg" },
    dashed: true,
    caption: "A reg declared with no reset — caught at compile time.",
  },
  {
    code: "E0403",
    badge: "kind mixing",
    left: { label: "a", sub: "signed" },
    right: { label: "b", sub: "bits" },
    dashed: false,
    caption: "Comparing signed and unsigned without a cast — caught at compile time.",
  },
  {
    code: "E0502",
    badge: "output never driven",
    left: { label: "—", sub: "driver" },
    right: { label: "q", sub: "out" },
    dashed: true,
    caption: "An out port nothing ever assigns — caught at compile time.",
  },
];

const INTERVAL_MS = 4200;

export default function SafeByDefault() {
  const { ref, shown, reduce } = useReveal<HTMLDivElement>();
  const [active, setActive] = useState(0);
  const [paused, setPaused] = useState(false);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (reduce || paused || !shown) return;
    timer.current = setInterval(() => {
      setActive((i) => (i + 1) % SCENES.length);
    }, INTERVAL_MS);
    return () => {
      if (timer.current) clearInterval(timer.current);
    };
  }, [reduce, paused, shown]);

  const scene = SCENES[active];
  const hasSecondDriver = scene.extraLeft !== undefined;

  return (
    <div
      ref={ref}
      className={`feat-box ${shown ? "in" : ""}`}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <svg
        className="feat-svg"
        viewBox="0 0 360 220"
        role="img"
        aria-label={`${scene.left.label} to ${scene.right.label}: ${scene.caption}`}
      >
        {/* `key` forces a full remount on every scene change, and every child
            below uses the *-cycle animation classes (not feat-reveal/feat-draw,
            which are transitions that only ever fire once) so the whole
            drawing — chips, wire, badge — replays each time, not just on the
            first scroll-into-view. Nothing renders before `shown` so it can't
            play off-screen at page load. */}
        {shown && (
        <g key={active}>
        {/* The error diagnostic */}
        <g className="feat-cycle feat-cycle-d3">
          <rect className="feat-badge-box" x="60" y="20" width="240" height="36" rx="4" />
          <text className="feat-badge-text" x="180" y="42" textAnchor="middle">
            {"⚠"} {scene.code} — {scene.badge}
          </text>
        </g>

        <path className="feat-dashln feat-cycle feat-cycle-d4" d="M180 56 V100" />

        {/* Left component(s) */}
        <g className="feat-cycle feat-cycle-d1">
          <rect
            className="feat-chip"
            x="24"
            y={hasSecondDriver ? "68" : "90"}
            width="90"
            height="48"
            rx="4"
          />
          <text
            className="feat-label"
            x="69"
            y={hasSecondDriver ? "97" : "119"}
            textAnchor="middle"
          >
            {scene.left.label}
          </text>
          <circle
            cx="114"
            cy={hasSecondDriver ? "92" : "114"}
            r="3"
            fill="var(--color-volt-400)"
          />
          <text
            className="feat-sub"
            x="104"
            y={hasSecondDriver ? "84" : "106"}
            textAnchor="end"
          >
            {scene.left.sub}
          </text>

          {hasSecondDriver && scene.extraLeft && (
            <>
              <rect className="feat-chip" x="24" y="132" width="90" height="48" rx="4" />
              <text className="feat-label" x="69" y="161" textAnchor="middle">
                {scene.extraLeft.label}
              </text>
              <circle cx="114" cy="156" r="3" fill="var(--color-volt-400)" />
              <text className="feat-sub" x="104" y="148" textAnchor="end">
                {scene.extraLeft.sub}
              </text>
            </>
          )}
        </g>

        {/* Right component */}
        <g className="feat-cycle feat-cycle-d2">
          <rect className="feat-chip" x="246" y="90" width="90" height="48" rx="4" />
          <text className="feat-label" x="291" y="119" textAnchor="middle">
            {scene.right.label}
          </text>
          <circle
            cx="246"
            cy="114"
            r="3"
            fill="var(--bg)"
            stroke="var(--color-volt-400)"
            strokeWidth="1.5"
          />
          <text className="feat-sub" x="256" y="106" textAnchor="start">
            {scene.right.sub}
          </text>
        </g>

        {/* Connecting wire(s) — solid ones draw in, a dashed ("missing") one
            just fades, since a dash pattern reads as a stub, not a stroke. */}
        <path
          className={
            scene.dashed
              ? "feat-wire-dashed feat-cycle feat-cycle-d4"
              : "feat-wire feat-draw-cycle"
          }
          d="M114 114 H242"
        />
        {hasSecondDriver && (
          <path className="feat-wire feat-draw-cycle" d="M114 156 H230 V118" />
        )}
        <g className="feat-cycle feat-cycle-d4">
          <path
            d="M232 108 L244 120 M232 120 L244 108"
            stroke="#ff6b6b"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </g>

        <text className="feat-sub feat-cycle feat-cycle-d4" x="180" y="196" textAnchor="middle">
          {scene.caption}
        </text>
        </g>
        )}
      </svg>

      <div className="feat-dots" role="tablist" aria-label="Refusal example">
        {SCENES.map((s, i) => (
          <button
            key={s.code}
            type="button"
            role="tab"
            aria-selected={i === active}
            aria-label={`${s.code} — ${s.badge}`}
            className={`feat-dot${i === active ? " is-on" : ""}`}
            onClick={() => setActive(i)}
          />
        ))}
      </div>
    </div>
  );
}
