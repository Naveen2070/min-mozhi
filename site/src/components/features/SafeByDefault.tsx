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
        <g className="feat-reveal feat-d3">
          <rect
            className="feat-badge-box"
            x="78"
            y="20"
            width="204"
            height="38"
            rx="9"
          />
          <text className="feat-badge-text" x="180" y="44" textAnchor="middle">
            ⚠ E0203 — truncation caught
          </text>
        </g>

        <path className="feat-dashln feat-reveal feat-d4" d="M180 58 V104" />

        <g className="feat-reveal feat-d1">
          <rect
            className="feat-chip"
            x="24"
            y="92"
            width="96"
            height="52"
            rx="10"
          />
          <text className="feat-label" x="72" y="124" textAnchor="middle">
            din [8]
          </text>
        </g>

        <g className="feat-reveal feat-d2">
          <rect
            className="feat-chip"
            x="240"
            y="92"
            width="96"
            height="52"
            rx="10"
          />
          <text className="feat-label" x="288" y="124" textAnchor="middle">
            q [4]
          </text>
        </g>

        <path className="feat-wire feat-draw" d="M120 118 H236" />
        <path
          className="feat-arrow feat-reveal feat-d2"
          d="M236 112 L246 118 L236 124 Z"
        />

        <text className="feat-sub feat-reveal feat-d4" x="180" y="190" textAnchor="middle">
          8-bit value into a 4-bit port — flagged before it ships
        </text>
      </svg>
    </div>
  );
}
