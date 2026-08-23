// Run: node src/lib/stats.test.mjs   (from site/)
//
// Two things worth testing. First the Cargo.toml version parse, because the
// file has TWO `version = "…"` lines (the workspace package and a dependency
// pin) and taking the wrong one would put a silently wrong version on the
// landing page. Second a smoke check that the counts come back sane, so a
// renamed directory fails the test instead of rendering "0 examples".
import assert from "node:assert/strict";
import { parseCargoVersion, siteStats, TEST_COUNT, SAFETY_PASSES } from "./stats.mjs";

// --- version parsing ------------------------------------------------------
const CARGO = `
[workspace]
members = ["crates/mimz-core"]

[workspace.package]
version = "0.2.0"
edition = "2021"

[workspace.dependencies]
serde = { version = "1.0.200" }
`;
assert.equal(parseCargoVersion(CARGO), "0.2.0");

// A file with no workspace.package version is a build error, not a silent "".
assert.throws(() => parseCargoVersion("[workspace]\nmembers = []\n"));

// --- smoke: the real repository -------------------------------------------
const s = siteStats();
assert.match(s.version, /^\d+\.\d+\.\d+$/);
assert.ok(s.examples >= 150, `expected >=150 examples, got ${s.examples}`);
assert.ok(s.chapters >= 40, `expected >=40 chapters, got ${s.chapters}`);
assert.equal(s.tests, TEST_COUNT);
assert.equal(TEST_COUNT, 1320);
assert.equal(s.safetyPasses, SAFETY_PASSES);

console.log("stats.test.mjs: all assertions passed");
