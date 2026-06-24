import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";

// Source the existing repo docs directly — single source of truth, never copied.
// Paths are relative to the site project root (this file lives in site/).
const guide = defineCollection({
  // The README is the hub index; the sidebar is built from the numbered chapters,
  // so skip it here and render it separately as the /guide landing page.
  loader: glob({ pattern: ["*.md", "!README.md"], base: "../docs/guide" }),
});

const guideIndex = defineCollection({
  // The guide hub (docs/guide/README.md), rendered as the /guide landing page.
  loader: glob({ pattern: "README.md", base: "../docs/guide" }),
});

const spec = defineCollection({
  // README is the hub index; rendered separately as the /spec landing page.
  loader: glob({ pattern: ["*.md", "!README.md"], base: "../spec" }),
});

const specIndex = defineCollection({
  // The spec hub (spec/README.md), rendered as the /spec landing page.
  loader: glob({ pattern: "README.md", base: "../spec" }),
});

export const collections = { guide, guideIndex, spec, specIndex };
