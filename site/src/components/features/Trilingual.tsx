import { useEffect, useState } from "react";
import { useReveal } from "./useReveal";

// "Trilingual, Tamil-first" — the same keyword cycles English → Tanglish → Tamil
// while the emitted Verilog stays byte-identical. Cycles only while in view and
// motion is allowed; under prefers-reduced-motion it lists all three flavors at
// once and the Verilog row resolves with a check.
const FLAVORS = [
  { cap: "English", word: "module", ta: false },
  { cap: "Tanglish", word: "thoguthi", ta: false },
  { cap: "Tamil", word: "தொகுதி", ta: true },
];

export default function Trilingual() {
  const { ref, shown, reduce } = useReveal<HTMLDivElement>();
  const [i, setI] = useState(0);

  useEffect(() => {
    if (!shown || reduce) return;
    const t = setInterval(() => setI((v) => (v + 1) % FLAVORS.length), 2600);
    return () => clearInterval(t);
  }, [shown, reduce]);

  const cur = FLAVORS[i];

  return (
    <div ref={ref} className={`feat-box ${shown ? "in" : ""}`}>
      <div className="feat-tri">
        <div className="feat-tri-row feat-reveal feat-d1">
          <div>
            <div className="feat-tri-cap">Source keyword</div>
            {reduce ? (
              <div className="feat-tri-list">
                <span>module</span>
                <span>thoguthi</span>
                <span className="feat-tri-ta">தொகுதி</span>
              </div>
            ) : (
              <span
                key={i}
                className={`feat-tri-word feat-cycle ${cur.ta ? "feat-tri-ta" : ""}`}
              >
                {cur.word}
              </span>
            )}
          </div>
          <span className="feat-tri-cap">{reduce ? "3 flavors" : cur.cap}</span>
        </div>

        {/* The card's other two rows are short, and the right column is tall
            — this fills the leftover height with the claim itself: one AST,
            three keyword spellings branching off it. Reuses .feat-chip, the
            same primitive SafeByDefault's diagrams use. */}
        <svg
          className="feat-tri-branch feat-reveal feat-d2"
          viewBox="0 0 320 92"
          role="img"
          aria-label="One grammar branches into three keyword spellings: module, thoguthi, தொகுதி"
        >
          <rect className="feat-badge-box" x="110" y="4" width="100" height="26" rx="4" />
          <text className="feat-badge-text" x="160" y="21" textAnchor="middle">
            one grammar
          </text>
          <path className="feat-dashln" d="M160 30 L58 64" />
          <path className="feat-dashln" d="M160 30 V64" />
          <path className="feat-dashln" d="M160 30 L262 64" />
          <rect className="feat-chip" x="14" y="64" width="88" height="26" rx="4" />
          <text className="feat-tri-pill-text" x="58" y="81" textAnchor="middle">
            module
          </text>
          <rect className="feat-chip" x="116" y="64" width="88" height="26" rx="4" />
          <text className="feat-tri-pill-text" x="160" y="81" textAnchor="middle">
            thoguthi
          </text>
          <rect className="feat-chip" x="218" y="64" width="88" height="26" rx="4" />
          <text className="feat-tri-pill-text feat-tri-ta" x="262" y="81" textAnchor="middle">
            தொகுதி
          </text>
        </svg>

        <div className="feat-tri-row feat-reveal feat-d3">
          <div>
            <div className="feat-tri-cap">Emitted Verilog · identical</div>
            <code className="feat-tri-code">module adder(…)</code>
          </div>
          <svg width="28" height="28" viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="10" fill="none" stroke="var(--border)" strokeWidth="1.5" />
            <path className="feat-ok feat-draw" d="M7 12.5 l3.5 3.5 l6.5 -7" />
          </svg>
        </div>
      </div>
    </div>
  );
}
