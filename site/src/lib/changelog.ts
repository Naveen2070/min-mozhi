// Per-version sections of the repo's CHANGELOG.md, read at build time.
//
// The Downloads pages show "what changed" inline rather than bouncing the reader
// to a GitHub diff, so the changelog is the source and the diff link is the
// fallback for people who actually want commits.
//
// The file is sectioned as:  ## [0.2.0] - 2026-08-20 · Language edition: …
// Everything up to the next `## [` heading belongs to that version.
import { readFileSync } from "node:fs";
import path from "node:path";

export interface ChangelogSection {
  /** Bare version as written in the heading, e.g. "0.2.0". */
  version: string;
  /** The heading's trailing context, e.g. "2026-08-20 · Language edition: …". */
  subtitle: string;
  /** Markdown body, headings demoted so they nest under the page's own h1/h2. */
  body: string;
}

const HEADING = /^##\s+\[([^\]]+)\]\s*(?:[-–]\s*)?(.*)$/;

/** Release tags are "v0.2.0"; changelog headings are "[0.2.0]". */
function bareVersion(tag: string): string {
  return tag.replace(/^v/, "");
}

let cache: Map<string, ChangelogSection> | null = null;

function load(): Map<string, ChangelogSection> {
  if (cache) return cache;

  const file = path.resolve(process.cwd(), "../CHANGELOG.md");
  let text: string;
  try {
    text = readFileSync(file, "utf-8");
  } catch {
    // The site must still build if the changelog moves; callers fall back to
    // the GitHub links.
    cache = new Map();
    return cache;
  }

  const sections = new Map<string, ChangelogSection>();
  const lines = text.split(/\r?\n/);

  let current: { version: string; subtitle: string; body: string[] } | null =
    null;
  const flush = () => {
    if (!current) return;
    sections.set(current.version, {
      version: current.version,
      subtitle: current.subtitle,
      body: demote(stripLeadingNote(current.body.join("\n").trim())),
    });
    current = null;
  };

  for (const line of lines) {
    const m = line.match(HEADING);
    if (m) {
      flush();
      current = { version: m[1].trim(), subtitle: m[2].trim(), body: [] };
      continue;
    }
    // A `---` rule between sections is a separator, not content.
    if (current && line.trim() !== "---") current.body.push(line);
  }
  flush();

  cache = sections;
  return sections;
}

/**
 * Drop a blockquote sitting at the very top of an entry.
 *
 * Those are maintainer asides aimed at whoever cuts the release ("tag pending",
 * "do X before publishing") rather than release content, and they read as
 * confusing internal chatter on a public downloads page. CHANGELOG.md keeps
 * them — this only affects what the site renders.
 */
function stripLeadingNote(md: string): string {
  const lines = md.split("\n");
  let i = 0;
  while (i < lines.length && lines[i].trim() === "") i++;
  if (i >= lines.length || !lines[i].startsWith(">")) return md;
  // Consume the blockquote and its lazy-continuation lines, then the blank run.
  while (i < lines.length && lines[i].trim() !== "") i++;
  while (i < lines.length && lines[i].trim() === "") i++;
  return lines.slice(i).join("\n");
}

/**
 * Push every heading down one level. The changelog's `###` becomes `####` so it
 * sits under the "What changed" `h2` the page supplies, keeping one `h1` per
 * page and the outline honest.
 */
function demote(md: string): string {
  return md.replace(/^(#{1,5})\s/gm, (_, hashes) => `${hashes}# `);
}

export function changelogFor(tag: string): ChangelogSection | undefined {
  return load().get(bareVersion(tag));
}

export function hasChangelog(tag: string): boolean {
  return load().has(bareVersion(tag));
}
