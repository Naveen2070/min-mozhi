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
        <text className="feat-label feat-reveal feat-d1" x="24" y="44">
          q = en &amp; d
        </text>
        <path className="feat-wire feat-draw" d="M24 54 H126" />

        <g className="feat-reveal feat-d2">
          <rect
            className="feat-badge-box"
            x="232"
            y="26"
            width="104"
            height="30"
            rx="8"
          />
          <text className="feat-badge-text" x="284" y="46" textAnchor="middle">
            E0107
          </text>
        </g>

        <text className="feat-sub feat-reveal feat-d3" x="24" y="100">
          inferred latch — “q” isn’t assigned on every path
        </text>

        <path className="feat-dashln feat-reveal feat-d3" d="M40 112 V146" />
        <path
          className="feat-arrow feat-reveal feat-d3"
          d="M34 140 L40 150 L46 140 Z"
        />

        <text className="feat-label feat-reveal feat-d4" x="24" y="176">
          q = en ? d : q
        </text>
        <path className="feat-ok feat-draw" d="M150 170 l7 8 l14 -18" />
      </svg>
    </div>
  );
}
