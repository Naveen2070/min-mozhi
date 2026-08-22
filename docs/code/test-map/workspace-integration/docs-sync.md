# Docs-sync (`tests/docs_sync.rs`, 5 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The mechanical staleness guard for `docs/code/` — these verify the
structural facts the docs state, so doc drift fails CI. When one fails,
**fix the named doc page, don't weaken the test.**

| Test                                                | Locks in                                                                                                                                                                                             |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crate_map_lists_every_module`                      | both crate-map copies (`src/main.rs` `//!` table, `docs/code/README.md`) name every top-level module                                                                                                 |
| `module_pages_list_every_source_file`               | each module page's file-layout table lists every `.rs` file actually in that `src/` directory                                                                                                        |
| `every_module_is_documented_somewhere_in_docs_code` | a new pipeline stage (e.g. `crates/mimz-core/src/checker/`) cannot land without a docs mention                                                                                                       |
| `code_docs_have_a_sync_stamp`                       | the "Last synced" tripwire line survives                                                                                                                                                             |
| `test_count_matches_docs_and_badge`                 | the total test count in `docs/code/10-test-map.md`, the `README.md` badge, and `ROADMAP.md`'s v0.2.0 reference all agree with the live count (re-counted from source each run; currently 1318 tests) |
