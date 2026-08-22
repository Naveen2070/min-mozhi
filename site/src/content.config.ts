import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

// Source the existing repo docs directly — single source of truth, never copied.
// Paths are relative to the site project root (this file lives in site/).
const guide = defineCollection({
  // The README is the hub index; the sidebar is built from the numbered chapters,
  // so skip it here and render it separately as the /guide landing page.
  loader: glob({ pattern: ["**/*.md", "!README.md"], base: "../docs/guide" }),
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

// ---- Site-local sections (beta): Learn + Handbook ------------------------
// Unlike the four above, these are ORIGINAL site content, not a mirror of repo
// docs — so they live under site/ and, unlike the repo markdown, they carry
// frontmatter. That kills the filename-derived title mangling docLabel() would
// otherwise do ("02-history-of-hdls" -> "History of hdls") and gives each page a
// real meta description.

const chapterSchema = z.object({
  title: z.string(),
  description: z.string(),
  order: z.number(),
  // Opt a chapter into a build-time generated block appended after its prose.
  // Markdown cannot import components, so the page renders it instead.
  generated: z.enum(["keywords"]).optional(),
});

const hubSchema = z.object({
  title: z.string(),
  description: z.string(),
});

const learn = defineCollection({
  loader: glob({ pattern: ["*.md", "!README.md"], base: "./content/learn" }),
  schema: chapterSchema,
});

const learnIndex = defineCollection({
  loader: glob({ pattern: "README.md", base: "./content/learn" }),
  schema: hubSchema,
});

const handbook = defineCollection({
  loader: glob({ pattern: ["*.md", "!README.md"], base: "./content/handbook" }),
  schema: chapterSchema,
});

const handbookIndex = defineCollection({
  loader: glob({ pattern: "README.md", base: "./content/handbook" }),
  schema: hubSchema,
});

export const collections = {
  guide,
  guideIndex,
  spec,
  specIndex,
  learn,
  learnIndex,
  handbook,
  handbookIndex,
};
