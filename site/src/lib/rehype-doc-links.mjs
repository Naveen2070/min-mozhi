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

const REPO = "https://github.com/Naveen2070/min-mozhi";
const GH_BLOB = `${REPO}/blob/master/`;
const GH_TREE = `${REPO}/tree/master/`;

export default function rehypeDocLinks() {
  return (tree, file) => {
    const filePath = String(file.path || file.history?.[0] || "").replace(
      /\\/g,
      "/",
    );
    const fileDir = path.posix.dirname(filePath);
    const cut = filePath.search(/\/(docs|spec)\//);
    const repoRoot = cut >= 0 ? filePath.slice(0, cut) : "";

    visit(tree, "element", (node) => {
      if (node.tagName !== "a") return;
      const href = node.properties?.href;
      if (typeof href !== "string" || href === "") return;
      // external, mailto/tel, in-page anchors, and site-absolute links are left alone
      if (/^(https?:|mailto:|tel:|#|\/)/.test(href)) return;

      const [rel, anchor] = href.split("#");
      if (!rel) return;

      const abs = path.posix.resolve(fileDir, rel);
      const withAnchor = (u) => (anchor ? `${u}#${anchor}` : u);

      // 1. A published markdown page -> its site route (README -> section root).
      const m = abs.match(/\/(docs\/guide|spec)\/([^/]+)\.md$/);
      if (m) {
        const section = m[1] === "spec" ? "spec" : "guide";
        const out =
          m[2].toLowerCase() === "readme" ? `/${section}` : `/${section}/${m[2]}`;
        node.properties.href = withAnchor(out);
        return;
      }

      // 2. A published section directory -> its landing route.
      if (repoRoot && abs === `${repoRoot}/spec`) {
        node.properties.href = withAnchor("/spec");
        return;
      }
      if (repoRoot && abs === `${repoRoot}/docs/guide`) {
        node.properties.href = withAnchor("/guide");
        return;
      }

      // 3. Anything else that lives in the repo -> GitHub (the site doesn't ship it).
      if (repoRoot && abs.startsWith(`${repoRoot}/`)) {
        const relFromRoot = abs.slice(repoRoot.length + 1);
        // A trailing slash or a basename with no extension reads as a directory.
        const isDir = rel.endsWith("/") || !path.posix.basename(abs).includes(".");
        node.properties.href = withAnchor(
          (isDir ? GH_TREE : GH_BLOB) + relFromRoot,
        );
        return;
      }

      // 4. Couldn't resolve within the repo: best-effort GitHub blob by basename.
      const base = path.posix.basename(abs);
      if (base) node.properties.href = withAnchor(GH_BLOB + base);
    });
  };
}
