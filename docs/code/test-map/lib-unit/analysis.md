# Editor analysis (`crates/mimz-core/src/analysis.rs`, 7 lib unit tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The pure, async-free symbol index and resolution behind the LSP's hover /
go-to-definition / completion (the `src/lsp.rs` handlers are a thin adapter).

| Test                                                     | Locks in                                                                                                                                                  |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `index_collects_each_definition_kind`                    | `build_index` emits a `Symbol` for every def kind (module, param, port, clock, reg, const, enum + variant, inst) with the right `SymKind` + hover render  |
| `resolve_at_use_returns_definition`                      | a use site resolves to its **declaration** span, not the use                                                                                              |
| `resolve_at_works_on_partial_tree`                       | `parse_recover` `Error` node between good ports — names around it still resolve                                                                           |
| `resolve_at_inside_test_block`                           | inside `test "…" for M { … }`: the module-under-test name + driven inputs + `expect` signals resolve to M's ports (cross-file via `same_module_any_file`) |
| `resolve_at_cross_file_instance`                         | an instantiated imported module name resolves into the imported file (`file_idx` differs)                                                                 |
| `completions_include_scope_idents_and_majority_keywords` | in-scope module members + majority-flavor keywords offered, with the right `CandKind`                                                                     |
| `completions_exclude_other_flavor_keywords`              | a Tamil-flavored file offers Tamil keywords, never the English spellings (no cross-flavor leak)                                                           |
