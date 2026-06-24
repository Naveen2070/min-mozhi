import { useEffect, useRef, useState } from "react";

// Reveal-on-scroll for the feature illustrations. Returns a ref to attach to the
// wrapper and `shown`, which flips true the first time the element scrolls into
// view (driving CSS entrance transitions). Under prefers-reduced-motion it flips
// true immediately and the CSS disables the transitions, so the final state shows
// at once with no animation. `reduce` is exposed for components that also drive
// JS-timed motion (e.g. cycling text) and must hold still when motion is reduced.
export function useReveal<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [shown, setShown] = useState(false);
  const [reduce, setReduce] = useState(false);

  useEffect(() => {
    const reduced = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    setReduce(reduced);
    if (reduced) {
      setShown(true);
      return;
    }
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setShown(true);
          io.disconnect();
        }
      },
      { threshold: 0.35 },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  return { ref, shown, reduce };
}
