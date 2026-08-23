// Render build-time markdown strings to HTML with the SAME pipeline the site
// config uses - dual-theme Shiki and the `mimz` TextMate grammar - so a lab
// step's display fences highlight exactly like a docs page's.
//
// createMarkdownProcessor does NOT inherit astro.config.mjs, so the shiki
// settings are repeated here (the changelog gets away with `false` because it
// has no code; lab prose does). One cached processor for every call on the
// page: constructing Shiki per step would be pure waste.
import { createMarkdownProcessor } from "@astrojs/markdown-remark";
import { readFileSync } from "node:fs";
import path from "node:path";

type Processor = Awaited<ReturnType<typeof createMarkdownProcessor>>;

let processor: Processor | null = null;

async function get(): Promise<Processor> {
  if (processor) return processor;
  const grammarPath = path.resolve(
    process.cwd(),
    "../editors/vscode/syntaxes/mimz.tmLanguage.json",
  );
  const mimzGrammar = JSON.parse(readFileSync(grammarPath, "utf-8"));
  processor = await createMarkdownProcessor({
    syntaxHighlight: { type: "shiki", excludeLangs: ["ebnf"] },
    shikiConfig: {
      themes: { light: "github-light", dark: "github-dark" },
      langs: [{ ...mimzGrammar, name: "mimz" }],
      wrap: false,
    },
  });
  return processor;
}

/** Markdown source -> HTML string, safe to drop into `set:html`. */
export async function mdToHtml(md: string): Promise<string> {
  const { code } = await (await get()).render(md);
  return code;
}
