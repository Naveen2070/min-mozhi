import { visit, SKIP } from "unist-util-visit";

// Wrap every rendered code block in a <figure class="code-block"> carrying a
// copy button. The button holds no code of its own — one delegated listener in
// Base.astro reads the sibling <pre>'s text — so this stays pure markup and
// adds no per-page JavaScript.
//
// Matches on `pre` alone, deliberately: Astro's shiki transform may run before
// or after user rehype plugins, and that ordering is not contractual. Both
// shapes are a <pre>, so both are handled.

function isCodeFigure(node) {
  return (
    node &&
    node.type === "element" &&
    node.tagName === "figure" &&
    Array.isArray(node.properties?.className) &&
    node.properties.className.includes("code-block")
  );
}

export default function rehypeCodeCopy() {
  return (tree) => {
    visit(tree, "element", (node, index, parent) => {
      if (node.tagName !== "pre") return;
      if (!parent || index === null || index === undefined) return;
      if (isCodeFigure(parent)) return SKIP; // already wrapped

      const button = {
        type: "element",
        tagName: "button",
        properties: {
          type: "button",
          "data-copy": true,
          "aria-label": "Copy code to clipboard",
        },
        // The label lives in a <span> so the "Copied" state can swap it via CSS
        // without JavaScript touching the text.
        children: [
          {
            type: "element",
            tagName: "span",
            properties: {},
            children: [{ type: "text", value: "Copy" }],
          },
        ],
      };

      parent.children[index] = {
        type: "element",
        tagName: "figure",
        properties: { className: ["code-block"] },
        children: [button, node],
      };

      // Skip past the figure we just created, so the <pre> now inside it is
      // not revisited.
      return [SKIP, index + 1];
    });
  };
}
