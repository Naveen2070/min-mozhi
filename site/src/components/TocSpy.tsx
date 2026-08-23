import { useEffect } from "react";

// Marks the TOC link for the heading you are currently reading. Renders no DOM
// of its own — the rail is server-rendered by Toc.astro and works without this;
// the island only adds the highlight.
export default function TocSpy() {
  useEffect(() => {
    const links = Array.from(
      document.querySelectorAll<HTMLAnchorElement>("[data-toc] a[href^='#']"),
    );
    if (links.length === 0) return;

    const byId = new Map<string, HTMLAnchorElement>();
    for (const a of links) {
      const id = decodeURIComponent(a.hash.slice(1));
      if (id) byId.set(id, a);
    }
    const targets = [...byId.keys()]
      .map((id) => document.getElementById(id))
      .filter((el): el is HTMLElement => el !== null);
    if (targets.length === 0) return;

    const visible = new Set<string>();

    const mark = () => {
      let active: string | null = null;
      // The first heading intersecting the reading band wins.
      for (const el of targets) {
        if (visible.has(el.id)) {
          active = el.id;
          break;
        }
      }
      // Mid-section with no heading on screen: the last heading scrolled past.
      if (active === null) {
        for (const el of targets) {
          if (el.getBoundingClientRect().top < 120) active = el.id;
        }
      }
      for (const [id, a] of byId) {
        a.toggleAttribute("data-active", id === active);
      }
    };

    // The band is the top slice of the viewport: a heading counts as "current"
    // from just under the sticky nav down to 30% of the screen.
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) visible.add(e.target.id);
          else visible.delete(e.target.id);
        }
        mark();
      },
      { rootMargin: "-80px 0px -70% 0px" },
    );

    targets.forEach((t) => io.observe(t));
    mark();
    return () => io.disconnect();
  }, []);

  return null;
}
