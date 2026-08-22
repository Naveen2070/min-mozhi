# Test Map — Split Layout

This folder is the split of `../10-test-map.md` (which is the **index/overview**).

- `../10-test-map.md` — master table (1318 tests), legend, and changelog. The single place that names the current total. `tests/docs_sync.rs` asserts the table's count matches the badge in `README.md` and the `**1315 tests**` line here.
- `index.md` — same content as `../10-test-map.md` but with links relative to _this_ folder (useful when browsing `test-map/` directly).
- `lib-unit/` — lib-unit tables (keyword table, lexer, parser, checker, emitter, etc.)
- `crate-integration/` — crate-level integration suites (`sim_errors.rs`, `width_rules_conformance.rs`)
- `workspace-integration/` — workspace integration suites (examples, icarus, morph, …)
- `simulator/` — simulator unit + simulator integration suites
- `test-map-changelog.md` — full changelog (extracted from the old monolithic file)

When you add tests:

1. Run `cargo test-summary --workspace` and update the master table in **both** `../10-test-map.md` and `index.md`.
2. Update the per-unit table in the file under the matching subfolder (the `## Unit:` / `## Integration:` header in that file).
3. Add a changelog entry to `test-map-changelog.md` (and the summary line in `../10-test-map.md`'s changelog link).
4. `cargo test --test docs_sync` must stay green — it checks the master count.

Do not edit `../10-test-map.md`'s detailed tables in place — they no longer live there. Edit the file in `test-map/` and keep the index's `Detailed Breakdown` links in sync.
