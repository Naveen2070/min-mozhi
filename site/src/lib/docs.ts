// Helpers for the docs collections. The source markdown has no frontmatter, so we
// derive nav labels + order from the filename id (e.g. "08-sequential-logic").

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
