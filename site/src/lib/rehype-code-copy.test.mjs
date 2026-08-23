// Run: node src/lib/rehype-code-copy.test.mjs   (from site/)
//
// The plugin wraps every <pre> in a <figure> with a copy button. Two rules are
// worth pinning: it must work on BOTH hast shapes (Astro's shiki transform may
// run before or after this plugin, and that ordering is not contractual), and
// it must never wrap the same <pre> twice — a double-wrap would put a second
// button in the DOM with no visible cause.
import assert from "node:assert/strict";
import rehypeCodeCopy from "./rehype-code-copy.mjs";

const el = (tagName, properties, children) => ({
  type: "element",
  tagName,
  properties,
  children,
});

function run(tree) {
  rehypeCodeCopy()(tree);
  return tree;
}

// --- pre-shiki shape: <pre><code class="language-mimz"> -------------------
{
  const tree = el("root", {}, [
    el("pre", {}, [
      el("code", { className: ["language-mimz"] }, [
        { type: "text", value: "module A {}" },
      ]),
    ]),
  ]);
  run(tree);
  const fig = tree.children[0];
  assert.equal(fig.tagName, "figure");
  assert.deepEqual(fig.properties.className, ["code-block"]);
  assert.equal(fig.children.length, 2);
  assert.equal(fig.children[0].tagName, "button");
  assert.equal(fig.children[0].properties["data-copy"], true);
  assert.equal(fig.children[0].properties.type, "button");
  assert.equal(fig.children[1].tagName, "pre");
}

// --- post-shiki shape: <pre class="astro-code"> ---------------------------
{
  const tree = el("root", {}, [
    el("pre", { className: ["astro-code"] }, [
      el("code", {}, [{ type: "text", value: "module A {}" }]),
    ]),
  ]);
  run(tree);
  assert.equal(tree.children[0].tagName, "figure");
  assert.equal(tree.children[0].children[1].properties.className[0], "astro-code");
}

// --- idempotent: running twice must not add a second button ---------------
{
  const tree = el("root", {}, [el("pre", {}, [])]);
  run(tree);
  run(tree);
  const fig = tree.children[0];
  assert.equal(fig.tagName, "figure");
  assert.equal(fig.children.filter((c) => c.tagName === "button").length, 1);
  // and it must not have nested a figure inside a figure
  assert.equal(fig.children[1].tagName, "pre");
}

// --- a <pre> that is not a code block is still wrapped, and that is fine --
// (no markdown in this repo emits a bare <pre>, but the plugin must not crash)
{
  const tree = el("root", {}, [
    el("div", {}, [el("pre", {}, [{ type: "text", value: "x" }])]),
  ]);
  run(tree);
  assert.equal(tree.children[0].children[0].tagName, "figure");
}

// --- a tree with no <pre> is left alone -----------------------------------
{
  const tree = el("root", {}, [el("p", {}, [{ type: "text", value: "hi" }])]);
  const before = JSON.stringify(tree);
  run(tree);
  assert.equal(JSON.stringify(tree), before);
}

console.log("rehype-code-copy.test.mjs: all assertions passed");
