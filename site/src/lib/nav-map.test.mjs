// Run: node src/lib/nav-map.test.mjs   (from site/)
//
// The one rule worth a test: every section belongs to exactly one track. The
// sidebar, the top nav and the breadcrumb all read that mapping, so a section
// missing from TRACKS would silently vanish from navigation while its pages
// still build and still resolve.
import assert from "node:assert/strict";
import {
  SECTION_NAMES,
  TRACKS,
  sectionOf,
  sectionName,
  trackOf,
  trackName,
  trackHome,
  docLabel,
  docOrder,
  sortDocs,
} from "./nav-map.mjs";

// --- every section is in exactly one track -------------------------------
const sections = Object.keys(SECTION_NAMES);
for (const section of sections) {
  const owners = Object.entries(TRACKS).filter(([, t]) =>
    t.sections.includes(section),
  );
  assert.equal(
    owners.length,
    1,
    `section "${section}" is in ${owners.length} tracks, expected exactly 1`,
  );
  assert.equal(trackOf(section), owners[0][0]);
}

// --- and no track lists a section that does not exist ---------------------
for (const [track, { sections: listed }] of Object.entries(TRACKS)) {
  for (const s of listed) {
    assert.ok(SECTION_NAMES[s], `track "${track}" lists unknown section "${s}"`);
  }
}

// --- the agreed grouping ---------------------------------------------------
assert.deepEqual(TRACKS.learn.sections, ["learn", "lab"]);
assert.deepEqual(TRACKS.reference.sections, ["guide"]);
assert.deepEqual(TRACKS.spec.sections, ["spec"]);
assert.deepEqual(TRACKS.handbook.sections, ["handbook"]);
assert.equal(trackHome("reference"), "/guide");
assert.equal(trackHome("handbook"), "/handbook");

// --- sectionOf ------------------------------------------------------------
assert.equal(sectionOf("guide/08-sequential-logic"), "guide");
// stdlib chapters live under the guide collection, so they are guide pages.
assert.equal(sectionOf("guide/stdlib/fifo"), "guide");
assert.equal(sectionOf("spec"), "spec");
assert.equal(sectionOf("lab/03-counter"), "lab");
// Unknown prefixes fall back to the guide, matching the old behaviour.
assert.equal(sectionOf("nonsense/x"), "guide");

assert.equal(sectionName("handbook"), "Handbook");
assert.equal(trackName("learn"), "Learn");

// --- label / order derivation (moved verbatim from docs.ts) ---------------
assert.equal(docLabel("08-sequential-logic"), "Sequential logic");
assert.equal(docLabel("README"), "README");
assert.equal(docOrder("10-word-order-thamizh"), 10);
assert.equal(docOrder("stdlib/fifo"), 999);
assert.deepEqual(
  sortDocs([{ id: "10-b" }, { id: "02-a" }]).map((e) => e.id),
  ["02-a", "10-b"],
);

console.log("nav-map.test.mjs: all assertions passed");
