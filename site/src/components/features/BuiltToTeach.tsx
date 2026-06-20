import { useReveal } from "./useReveal";

// "Built to teach" — a diagnostic that explains and points to the fix. The buggy
// assignment is underlined, the error names the cause in plain words, then the
// corrected line resolves with a check. Reveals on scroll; static under
// prefers-reduced-motion.
export default function BuiltToTeach() {
  const { ref, shown } = useReveal<HTMLDivElement>();
  return (
    <div ref={ref} className={`feat-box ${shown ? "in" : ""}`}>
      <svg
        className="feat-svg"
        viewBox="0 0 360 220"
        role="img"
        aria-label="A diagnostic names an inferred latch and shows the fix"
      >
        {/* IDE-like Code */}
        <text className="feat-label feat-reveal feat-d1" x="32" y="44">
          q = en &amp; d;
        </text>
        {/* IDE squiggle underline */}
        <path className="feat-wire feat-draw" d="M32 52 q 4 3 8 0 t 8 0 t 8 0 t 8 0 t 8 0 t 8 0 t 8 0 t 8 0" style={{ strokeWidth: 1.5 }} />

        {/* Diagnostic popup */}
        <g className="feat-reveal feat-d2">
          {/* Popover pointer */}
          <path className="feat-badge-box" d="M48 60 l6 -6 l6 6 Z" />
          {/* Popover box */}
          <rect
            className="feat-badge-box"
            x="32"
            y="60"
            width="296"
            height="44"
            rx="4"
          />
          <text className="feat-badge-text" x="44" y="78">
            E0107: inferred latch
          </text>
          <text className="feat-sub" x="44" y="94">
            “q” isn’t assigned on every path
          </text>
        </g>

        {/* Arrow pointing to fix */}
        <path className="feat-dashln feat-reveal feat-d3" d="M48 112 V142" />
        <path className="feat-arrow feat-reveal feat-d3" d="M44 136 L48 144 L52 136 Z" />

        {/* Fixed code */}
        <text className="feat-label feat-reveal feat-d4" x="32" y="176">
          q = en ? d : q;
        </text>
        <path className="feat-ok feat-draw" d="M160 166 l5 6 l10 -12" />
      </svg>
    </div>
  );
}
