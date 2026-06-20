import { useReveal } from "./useReveal";

// "Modern & safe by default" — an 8-bit value driven into a 4-bit port. The wire
// draws in, then the compiler's truncation error snaps above it: unsafe by
// construction becomes a teaching error, caught at compile time. Reveals on
// scroll; holds the final frame under prefers-reduced-motion.
export default function SafeByDefault() {
  const { ref, shown } = useReveal<HTMLDivElement>();
  return (
    <div ref={ref} className={`feat-box ${shown ? "in" : ""}`}>
      <svg
        className="feat-svg"
        viewBox="0 0 360 220"
        role="img"
        aria-label="An 8-bit value driven into a 4-bit port is caught as a compile-time truncation error"
      >
        {/* The error diagnostic */}
        <g className="feat-reveal feat-d3">
          <rect
            className="feat-badge-box"
            x="60"
            y="20"
            width="240"
            height="36"
            rx="4"
          />
          <text className="feat-badge-text" x="180" y="42" textAnchor="middle">
            ⚠ E0203 — width mismatch
          </text>
        </g>

        <path className="feat-dashln feat-reveal feat-d4" d="M180 56 V100" />

        {/* Left component: 8-bit output */}
        <g className="feat-reveal feat-d1">
          <rect
            className="feat-chip"
            x="24"
            y="90"
            width="90"
            height="48"
            rx="4"
          />
          <text className="feat-label" x="69" y="119" textAnchor="middle">
            din
          </text>
          {/* pin */}
          <circle cx="114" cy="114" r="3" fill="var(--color-volt-400)" />
          <text className="feat-sub" x="104" y="106" textAnchor="end">[8]</text>
        </g>

        {/* Right component: 4-bit input */}
        <g className="feat-reveal feat-d2">
          <rect
            className="feat-chip"
            x="246"
            y="90"
            width="90"
            height="48"
            rx="4"
          />
          <text className="feat-label" x="291" y="119" textAnchor="middle">
            q
          </text>
          {/* pin */}
          <circle cx="246" cy="114" r="3" fill="var(--bg)" stroke="var(--color-volt-400)" strokeWidth="1.5" />
          <text className="feat-sub" x="256" y="106" textAnchor="start">[4]</text>
        </g>

        {/* Truncation wire */}
        <path className="feat-wire feat-draw" d="M114 114 H242" />
        {/* X mark for the truncation error point */}
        <g className="feat-reveal feat-d4">
          <path d="M232 108 L244 120 M232 120 L244 108" stroke="#ff6b6b" strokeWidth="2" strokeLinecap="round" />
        </g>

        <text className="feat-sub feat-reveal feat-d4" x="180" y="180" textAnchor="middle">
          8-bit value into a 4-bit port — caught at compile time
        </text>
      </svg>
    </div>
  );
}
