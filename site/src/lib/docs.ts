// Helpers for the docs collections.
//
// Two flavors of source live behind these helpers:
//   - guide/spec  — repo markdown with NO frontmatter, so nav labels and order are
//                   derived from the filename id (e.g. "08-sequential-logic").
//   - learn/handbook — site-local originals WITH frontmatter, so they use their
//                   own `title` / `order` instead of the filename.
import { getCollection } from "astro:content";

export function docLabel(id: string): string {
  const words = id
    .replace(/^\d+(?:\.\d+)?[-_]?/, "") // drop a leading chapter number
    .replace(/[-_]/g, " ")
    .trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : id;
}

export function docOrder(id: string): number {
  const m = id.match(/^(\d+(?:\.\d+)?)/);
  return m ? parseFloat(m[1]) : 999;
}

export function sortDocs<T extends { id: string }>(entries: readonly T[]): T[] {
  return [...entries].sort((a, b) => docOrder(a.id) - docOrder(b.id));
}

// ---- Section navigation --------------------------------------------------

export interface NavItem {
  /** Route without a leading slash, e.g. "guide/05-operators". */
  href: string;
  label: string;
}

export interface NavSection {
  title: string;
  items: NavItem[];
}

const SECTION_NAMES = {
  guide: "Guide",
  spec: "Spec",
  learn: "Learn",
  handbook: "Handbook",
} as const;

export type SectionBase = keyof typeof SECTION_NAMES;

/** Which top-level section a `current` string ("guide/05-operators") belongs to. */
export function sectionOf(current: string): SectionBase {
  const base = current.split("/")[0];
  return base in SECTION_NAMES ? (base as SectionBase) : "guide";
}

export function sectionName(section: SectionBase): string {
  return SECTION_NAMES[section];
}

/** Site-local collections: frontmatter `order` sorts, frontmatter `title` labels. */
function localItems(entries: readonly any[], base: string): NavItem[] {
  return [...entries]
    .sort((a, b) => a.data.order - b.data.order)
    .map((e) => ({ href: `${base}/${e.id}`, label: e.data.title }));
}

/** Repo-sourced collections: filename sorts and labels. */
function repoItems(
  entries: readonly any[],
  base: string,
  strip = "",
): NavItem[] {
  return sortDocs(entries).map((e) => ({
    href: `${base}/${e.id}`,
    label: docLabel(strip ? e.id.replace(strip, "") : e.id),
  }));
}

/**
 * The sidebar's sections AND the linear sequence the pager walks, from one place
 * — so the two can never disagree about ordering.
 *
 * The sequence deliberately starts at the section hub, so chapter 01's "previous"
 * is the overview and the hub itself gets a "next".
 */
export async function getNav(
  current: string,
): Promise<{ sections: NavSection[]; sequence: NavItem[] }> {
  const section = sectionOf(current);

  // Learn / Handbook: contextual — show only this section (plus a cross-link,
  // which the Sidebar renders), never a five-section wall.
  if (section === "learn" || section === "handbook") {
    const entries = await getCollection(section);
    const hub: NavItem = { href: section, label: SECTION_NAMES[section] };
    const items = localItems(entries, section);
    return {
      sections: [
        {
          title: SECTION_NAMES[section],
          items: [{ href: hub.href, label: "Overview" }, ...items],
        },
      ],
      sequence: [hub, ...items],
    };
  }

  const allGuide = await getCollection("guide");
  const guideMain = repoItems(
    allGuide.filter((d: any) => !d.id.startsWith("stdlib/")),
    "guide",
  );
  const stdlib = repoItems(
    allGuide.filter(
      (d: any) =>
        d.id.startsWith("stdlib/") && d.id.toLowerCase() !== "stdlib/readme",
    ),
    "guide",
    "stdlib/",
  );
  const spec = repoItems(await getCollection("spec"), "spec");

  const guideHub: NavItem = { href: "guide", label: "Guide" };
  const stdlibHub: NavItem = {
    href: "guide/stdlib/readme",
    label: "Standard Library",
  };
  const specHub: NavItem = { href: "spec", label: "Spec" };

  const sections: NavSection[] = [
    { title: "Guide", items: [{ href: "guide", label: "Overview" }, ...guideMain] },
    {
      title: "Standard Library",
      items: [{ href: stdlibHub.href, label: "Overview" }, ...stdlib],
    },
    { title: "Spec", items: [{ href: "spec", label: "Overview" }, ...spec] },
  ];

  // The stdlib chapters are their own sequence: without this split the pager
  // would walk off the end of the main guide ("13-hardware-emulation") straight
  // into "stdlib/…", which is a different book.
  const sequence = current.toLowerCase().startsWith("guide/stdlib")
    ? [stdlibHub, ...stdlib]
    : section === "spec"
      ? [specHub, ...spec]
      : [guideHub, ...guideMain];

  return { sections, sequence };
}

/** Neighbours of `current` within an already-ordered sequence. */
export function getPager(
  sequence: readonly NavItem[],
  current: string,
): { prev?: NavItem; next?: NavItem } {
  // Case-insensitive: some repo ids keep their source casing ("stdlib/README")
  // while the hub hrefs are written lowercase.
  const key = current.toLowerCase();
  const i = sequence.findIndex((e) => e.href.toLowerCase() === key);
  if (i === -1) return {};
  return { prev: sequence[i - 1], next: sequence[i + 1] };
}
