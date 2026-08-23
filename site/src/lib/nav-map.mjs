// Section and track mapping, plus label/order derivation.
//
// Deliberately plain .mjs with no Astro imports, so `node` can test it
// directly — the same reason keywords.mjs is not a .ts file. docs.ts imports
// from here and adds the parts that need `astro:content`.

/** Every documentation section, and its display name. */
export const SECTION_NAMES = {
  guide: "Guide",
  spec: "Spec",
  learn: "Learn",
  handbook: "Handbook",
  lab: "Lab",
};

/**
 * The four tracks the site presents. Purely a presentation grouping — no URL
 * anywhere changes because of it, and the pager still walks each section's own
 * sequence.
 *
 * `home` is where the top-nav link for the track points.
 */
export const TRACKS = {
  learn: { label: "Learn", home: "/learn", sections: ["learn", "lab"] },
  reference: {
    label: "Reference",
    home: "/guide",
    sections: ["guide"],
  },
  spec: { label: "Spec", home: "/spec", sections: ["spec"] },
  handbook: {
    label: "Handbook",
    home: "/handbook",
    sections: ["handbook"],
  },
};

/** Which section a `current` string ("guide/08-sequential-logic") belongs to. */
export function sectionOf(current) {
  const base = String(current).split("/")[0];
  return base in SECTION_NAMES ? base : "guide";
}

export function sectionName(section) {
  return SECTION_NAMES[section];
}

/** Which track owns a section. Guarded by nav-map.test.mjs. */
export function trackOf(section) {
  for (const [track, { sections }] of Object.entries(TRACKS)) {
    if (sections.includes(section)) return track;
  }
  return "reference";
}

export function trackName(track) {
  return TRACKS[track].label;
}

export function trackHome(track) {
  return TRACKS[track].home;
}

/** "08-sequential-logic" -> "Sequential logic". */
export function docLabel(id) {
  const words = id
    .replace(/^\d+(?:\.\d+)?[-_]?/, "") // drop a leading chapter number
    .replace(/[-_]/g, " ")
    .trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : id;
}

/** Leading chapter number, or 999 for anything unnumbered. */
export function docOrder(id) {
  const m = id.match(/^(\d+(?:\.\d+)?)/);
  return m ? parseFloat(m[1]) : 999;
}

export function sortDocs(entries) {
  return [...entries].sort((a, b) => docOrder(a.id) - docOrder(b.id));
}
