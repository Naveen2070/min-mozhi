// Run: node src/lib/keywords.test.mjs   (from site/)
//
// One test, for the one genuinely tricky rule in keywords.mjs: a block comment
// describes the table BELOW it. Attributing it to the table above is the obvious
// reading and it is wrong — that mistake has already produced two different wrong
// PROVISIONAL counts in this project's own notes.
import assert from "node:assert/strict";
import { parseKeywords, loadKeywords } from "./keywords.mjs";

const FIXTURE = `
version = 1

# A comment block about the reserved list, mentioning nothing special.
reserved = [
  "alpha",
  "beta", # a trailing comment that must not become an entry
]

[keywords.plain]
en = "plain"
tanglish = "plain_t"
tamil = "ப்"

# This block describes the table BELOW and says PROVISIONAL.
# The naive read attaches it to [keywords.plain] above. That is the trap.
[keywords.marked_by_block]
en = "mb"
tanglish = "mb_t"
tamil = "ம்"

[keywords.marked_by_trailing]
en = "mt"
tanglish = "mt_t" # PROVISIONAL — pending native review
tamil = "ம்2"

# A decoy block that follows a marked table and mentions nothing.
[keywords.decoy]
en = "decoy"
en_aliases = ["decoy2", "decoy3"]
tanglish = "decoy_t"
tamil = "ட்"
`;

const t = parseKeywords(FIXTURE);
const by = Object.fromEntries(t.keywords.map((k) => [k.name, k]));

// --- structure -------------------------------------------------------------
assert.equal(t.version, 1);
assert.deepEqual(t.reserved, ["alpha", "beta"]);
assert.equal(t.keywords.length, 4);
assert.equal(by.plain.en, "plain");
assert.equal(by.plain.tanglish, "plain_t");
assert.deepEqual(by.decoy.aliases.en, ["decoy2", "decoy3"]);

// --- the attribution rule --------------------------------------------------
assert.equal(
  by.plain.provisional,
  false,
  "the block comment BELOW `plain` must not mark `plain` (the trap)",
);
assert.equal(
  by.marked_by_block.provisional,
  true,
  "a block comment marks the table it sits above",
);
assert.equal(
  by.marked_by_trailing.provisional,
  true,
  "a trailing # PROVISIONAL on a value line marks its own table",
);
assert.equal(
  by.decoy.provisional,
  false,
  "a marked table must not bleed into the next one",
);

// Trailing comments must not leak into values.
assert.equal(by.marked_by_trailing.tanglish, "mt_t");

// --- the real file ---------------------------------------------------------
// Structural only. The exact PROVISIONAL set is expected to shrink as
// native-speaker review lands, so it is not pinned here.
const real = loadKeywords(); // throws if the 44/12 counts drift
assert.equal(real.version, 1);
for (const k of real.keywords) {
  assert.ok(k.name && !k.name.includes("]"), `bad table name: ${k.name}`);
  for (const col of ["en", "tanglish", "tamil"]) {
    assert.ok(k[col], `keyword "${k.name}" has an empty ${col} spelling`);
  }
}

console.log(
  `ok — fixture rules hold; real file: ${real.keywords.length} keywords, ` +
    `${real.reserved.length} reserved, ` +
    `${real.keywords.filter((k) => k.provisional).length} provisional`,
);
