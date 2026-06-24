// @ts-check
import { defineConfig } from "astro/config";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import react from "@astrojs/react";
import sitemap from "@astrojs/sitemap";
import vercel from "@astrojs/vercel";
import tailwindcss from "@tailwindcss/vite";
import { unified } from "@astrojs/markdown-remark";

import rehypeDocLinks from "./src/lib/rehype-doc-links.mjs";

// Reuse the VS Code TextMate grammar (single source of truth, kept in sync with
// keywords.toml by tests/grammar_sync.rs) so `.mimz` highlights identically on the
// web. Read at config time to avoid JSON-import-outside-root friction.
const mimzGrammar = JSON.parse(
  readFileSync(
    fileURLToPath(
      new URL(
        "../editors/vscode/syntaxes/mimz.tmLanguage.json",
        import.meta.url,
      ),
    ),
    "utf-8",
  ),
);

// https://astro.build/config
export default defineConfig({
  // Public subdomain (DNS wired at deploy): only affects absolute URLs / sitemap.
  // Vercel preview URLs work regardless of this value.
  site: "https://mimz.naveenr.in",

  adapter: vercel(),
  integrations: [react(), sitemap()],

  vite: {
    plugins: [tailwindcss()],
  },

  markdown: {
    // Astro 6: remark/rehype plugins live on the unified() processor (the old
    // top-level `markdown.rehypePlugins` is deprecated). `shikiConfig` still
    // applies — it flows through the shared markdown config to the renderer.
    // rehypeDocLinks rewrites the docs' relative .md links to site routes (or
    // GitHub for files we don't publish); the source markdown stays untouched.
    processor: unified({ rehypePlugins: [rehypeDocLinks] }),
    // `ebnf` (used in spec/02 grammar blocks) has no Shiki grammar — skip it so it
    // renders as plain code instead of warning + falling back.
    syntaxHighlight: { type: "shiki", excludeLangs: ["ebnf"] },
    shikiConfig: {
      // Dual themes; global.css switches to the dark vars under `html.dark`.
      themes: { light: "github-light", dark: "github-dark" },
      langs: [{ ...mimzGrammar, name: "mimz" }],
      wrap: false,
    },
  },
});
