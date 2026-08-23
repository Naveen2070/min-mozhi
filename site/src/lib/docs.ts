// Helpers for the docs collections.
//
// The pure parts (section/track mapping, label + order derivation) live in
// nav-map.mjs so plain `node` can test them. This file adds only what needs
// `astro:content`.
//
// Two flavors of source sit behind these helpers:
//   - guide/spec      — repo markdown with NO frontmatter, so nav labels and
//                       order come from the filename id ("08-sequential-logic").
//   - learn/handbook/lab — site-local originals WITH frontmatter, so they use
//                       their own `title` / `order`.
import { getCollection } from "astro:content";
import {
  SECTION_NAMES,
  TRACKS,
  sectionOf,
  sectionName,
  trackOf,
  trackName,
  trackHome,
  docLabel,
  docOrder,
  sortDocs,
} from "./nav-map.mjs";

export {
  SECTION_NAMES,
  TRACKS,
  sectionOf,
  sectionName,
  trackOf,
  trackName,
  trackHome,
  docLabel,
  docOrder,
  sortDocs,
};

export type SectionBase = "guide" | "spec" | "learn" | "handbook" | "lab";
export type Track = "learn" | "reference" | "spec";

export interface NavItem {
  /** Route without a leading slash, e.g. "guide/05-operators". */
  href: string;
  label: string;
}

export interface NavSection {
  title: string;
  items: NavItem[];
  /** Rendered as a "beta" pill beside the section title in the sidebar. */
  beta?: boolean;
}

/** Site-local collections: frontmatter `order` sorts, frontmatter `title` labels. */
function localItems(entries: readonly any[], base: string): NavItem[] {
  return [...entries]
    .sort((a, b) => a.data.order - b.data.order)
    .map((e) => ({ href: `${base}/${e.id}`, label: e.data.title }));
}

/** Repo-sourced collections: the filename sorts and labels. */
function repoItems(
  entries: readonly any[],
  base: string,
  strip = "",
): NavItem[] {
  return sortDocs(entries).map((e: any) => ({
    href: `${base}/${e.id}`,
    label: docLabel(strip ? e.id.replace(strip, "") : e.id),
  }));
}

/** A section's items, hub first, exactly as the sidebar shows them. */
async function sectionItems(section: SectionBase): Promise<NavSection[]> {
  if (section === "guide") {
    const all = await getCollection("guide");
    const main = repoItems(
      all.filter((d: any) => !d.id.startsWith("stdlib/")),
      "guide",
    );
    const stdlib = repoItems(
      all.filter(
        (d: any) =>
          d.id.startsWith("stdlib/") && d.id.toLowerCase() !== "stdlib/readme",
      ),
      "guide",
      "stdlib/",
    );
    return [
      { title: "Guide", items: [{ href: "guide", label: "Overview" }, ...main] },
      {
        title: "Standard Library",
        items: [
          { href: "guide/stdlib/readme", label: "Overview" },
          ...stdlib,
        ],
      },
    ];
  }

  if (section === "spec") {
    const spec = repoItems(await getCollection("spec"), "spec");
    return [
      { title: "Spec", items: [{ href: "spec", label: "Overview" }, ...spec] },
    ];
  }

  // learn / handbook / lab — site-local, frontmatter-driven, all beta.
  const entries = await getCollection(section);
  return [
    {
      title: SECTION_NAMES[section],
      beta: true,
      items: [
        { href: section, label: "Overview" },
        ...localItems(entries, section),
      ],
    },
  ];
}

/**
 * The sidebar's sections AND the linear sequence the pager walks.
 *
 * The SECTIONS are grouped by track — a Learn page shows Learn and Lab, a
 * Reference page shows Guide, Standard Library and Handbook. The SEQUENCE is
 * unchanged from before tracks existed: it stays scoped to the reader's own
 * section, so "next" never jumps between books.
 *
 * Each sequence deliberately starts at the section hub, so chapter 01's
 * "previous" is the overview and the hub itself gets a "next".
 */
export async function getNav(current: string): Promise<{
  sections: NavSection[];
  sequence: NavItem[];
  track: Track;
}> {
  const section = sectionOf(current) as SectionBase;
  const track = trackOf(section) as Track;

  const grouped = await Promise.all(
    TRACKS[track].sections.map((s: string) => sectionItems(s as SectionBase)),
  );
  const sections = grouped.flat();

  // The stdlib chapters are their own book: without this split the pager would
  // walk off the end of "13-hardware-emulation" straight into "stdlib/…".
  const key = current.toLowerCase();
  let sequence: NavItem[];
  if (key.startsWith("guide/stdlib")) {
    sequence = sections.find((s) => s.title === "Standard Library")!.items;
  } else if (section === "guide") {
    sequence = sections.find((s) => s.title === "Guide")!.items;
  } else {
    sequence = sections.find((s) => s.title === SECTION_NAMES[section])!.items;
  }

  return { sections, sequence, track };
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
