// The numbers on the landing page's proof band.
//
// Everything that CAN be derived from the repository is derived at build time —
// a hardcoded count is a count that goes stale, and this project has already
// proved that by carrying three different test counts in three files.
//
// The two constants below cannot be derived: the test count would require
// running cargo inside a site build, and the safety-pass count is a prose
// claim about the checker's structure rather than a countable thing.
import { readFileSync, readdirSync, existsSync } from "node:fs";
import path from "node:path";

/**
 * Passing tests in `cargo test --workspace`.
 *
 * Re-derive with, from the repo root (PowerShell):
 *   $env:CARGO_BUILD_JOBS = "2"; cargo test --workspace -- --test-threads=4
 * then sum the "test result: ok. N passed" lines. Update README.md and
 * AGENTS.md in the same edit — this constant is the source of truth and those
 * two files quote it.
 */
export const TEST_COUNT = 1320;

/** Safety passes in crates/mimz-core/src/checker/. Prose claim, not a file count. */
export const SAFETY_PASSES = 9;

/** The repo root, from site/ — the cwd during `astro build` and `astro dev`. */
function defaultRoot() {
  return path.resolve(process.cwd(), "..");
}

/**
 * The `version` under `[workspace.package]` in Cargo.toml.
 *
 * Cargo.toml contains several `version = "…"` lines (dependency pins among
 * them), so this anchors to the section rather than taking the first match.
 */
export function parseCargoVersion(text) {
  const section = text.split(/^\[workspace\.package\]\s*$/m)[1];
  const m = section && section.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!m) {
    throw new Error("Cargo.toml: no version under [workspace.package]");
  }
  return m[1];
}

/** Count *.md in a directory, ignoring README.md (every section's hub). */
function countChapters(dir) {
  if (!existsSync(dir)) return 0;
  return readdirSync(dir).filter(
    (f) => f.endsWith(".md") && f.toLowerCase() !== "readme.md",
  ).length;
}

/** Count *.mimz across every immediate subdirectory of examples/. */
function countExamples(examplesDir) {
  if (!existsSync(examplesDir)) return 0;
  let n = 0;
  for (const entry of readdirSync(examplesDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    n += readdirSync(path.join(examplesDir, entry.name)).filter((f) =>
      f.endsWith(".mimz"),
    ).length;
  }
  return n;
}

export function siteStats(repoRoot = defaultRoot()) {
  const version = parseCargoVersion(
    readFileSync(path.join(repoRoot, "Cargo.toml"), "utf-8"),
  );

  const examples = countExamples(path.join(repoRoot, "examples"));

  const chapters =
    countChapters(path.join(repoRoot, "docs/guide")) +
    countChapters(path.join(repoRoot, "docs/guide/stdlib")) +
    countChapters(path.join(repoRoot, "spec")) +
    countChapters(path.join(repoRoot, "site/content/learn")) +
    countChapters(path.join(repoRoot, "site/content/handbook")) +
    countChapters(path.join(repoRoot, "site/content/lab"));

  return {
    version,
    examples,
    chapters,
    tests: TEST_COUNT,
    safetyPasses: SAFETY_PASSES,
  };
}
