import { visit } from "unist-util-visit";
import path from "node:path";

// The source docs (docs/guide/*.md, spec/*.md) link to each other and to other
// repo files with relative paths — fine on GitHub, broken on the site. This rehype
// plugin rewrites those links at render time, leaving the source markdown untouched:
//   - a published doc under docs/guide/ or spec/  -> /guide/<slug> | /spec/<slug>
//     (a section's README is its hub -> the section root /guide | /spec)
//   - the published section directories themselves -> their landing routes
//   - anything else in the repo we don't publish (keywords.toml, docs/*.md, demo/,
//     examples/, src/, …) -> GitHub (files -> blob/, directories -> tree/)
// Anchors (#...) are preserved; external / mailto / in-page / site-absolute links
// are left untouched.
//
// Paths are resolved to repo-relative form with posix `join` (never `resolve`),
// because `path.posix.resolve` treats a Windows "D:/…" path as relative and falls
// back to the build cwd, corrupting the result.

const REPO = "https://github.com/Naveen2070/min-mozhi";
const GH_BLOB = `${REPO}/blob/master/`;
const GH_TREE = `${REPO}/tree/master/`;

// Common extension-less files, so e.g. `LICENSE-MIT` routes to blob/ (a file), not
// tree/ (a directory). Anything else extension-less is treated as a directory.
const EXTLESS_FILE =
  /^(LICEN[CS]E|COPYING|NOTICE|AUTHORS|CONTRIBUTING|CHANGELOG|README|CODEOWNERS|Makefile|Dockerfile|Justfile)(-[\w.]+)?$/i;

export default function rehypeDocLinks() {
  return (tree, file) => {
    const filePath = String(file.path || file.history?.[0] || "").replace(
      /\\/g,
      "/",
    );
    const cut = filePath.search(/\/(docs|spec)\//);
    if (cut < 0) return; // not a repo doc we recognize; leave its links alone
    const repoRoot = filePath.slice(0, cut);
    const relDir = path.posix.dirname(filePath).slice(repoRoot.length + 1); // "docs/guide" | "spec"

    visit(tree, "element", (node) => {
      if (node.tagName !== "a") return;
      const href = node.properties?.href;
      if (typeof href !== "string" || href === "") return;
      // external, mailto/tel, in-page anchors, and site-absolute links are left alone
      if (/^(https?:|mailto:|tel:|#|\/)/.test(href)) return;

      const [rel, anchor] = href.split("#");
      if (!rel) return;

      // Repo-relative target, e.g. "spec/01-x.md", "keywords.toml", "spec/".
      const target = path.posix.join(relDir, rel);
      if (target.startsWith("..") || path.posix.isAbsolute(target)) return; // escapes the repo

      const isDir = rel.endsWith("/") || target.endsWith("/");
      const clean = target.replace(/\/+$/, "");
      const withAnchor = (u) => (anchor ? `${u}#${anchor}` : u);

      // 1. A published markdown page -> its site route (README -> section root).
      const m = clean.match(/^(docs\/guide|spec)\/([^/]+)\.md$/);
      if (m) {
        const section = m[1] === "spec" ? "spec" : "guide";
        node.properties.href = withAnchor(
          m[2].toLowerCase() === "readme" ? `/${section}` : `/${section}/${m[2]}`,
        );
        return;
      }

      // 2. A published section directory -> its landing route.
      if (clean === "spec") {
        node.properties.href = withAnchor("/spec");
        return;
      }
      if (clean === "docs/guide") {
        node.properties.href = withAnchor("/guide");
        return;
      }

      // 3. Anything else in the repo -> GitHub (files -> blob, directories -> tree).
      const base = path.posix.basename(clean);
      let dir;
      if (isDir) dir = true; // explicit trailing slash
      else if (base.includes(".")) dir = false; // has an extension
      else dir = !EXTLESS_FILE.test(base); // extension-less: file if known, else dir
      node.properties.href = withAnchor((dir ? GH_TREE : GH_BLOB) + clean);
    });
  };
}
