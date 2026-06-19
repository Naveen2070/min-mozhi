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

        <div className="feat-tri-row feat-reveal feat-d3">
          <div>
            <div className="feat-tri-cap">Emitted Verilog · identical</div>
            <code className="feat-tri-code">module adder(…)</code>
          </div>
          <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true">
            <path className="feat-ok feat-draw" d="M5 13 l4 4 l10 -12" />
          </svg>
        </div>
      </div>
    </div>
  );
}
