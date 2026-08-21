# Unit: config (`src/config.rs`, 8 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

`mimz.toml` parsing + discovery (the precedence merge lives in `main.rs` and is
exercised by the integration tests below).

| Test                                                           | Locks in                                                                                 |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `empty_config_is_all_defaults`                                 | an empty/missing config is all `None` — pure built-in defaults                           |
| `parses_every_section`                                         | `lang` + `[translate]` + `[fmt]` keys deserialize to the right fields                    |
| `unknown_key_is_rejected`                                      | a typo'd key (`too`, `flavour`) errors via `deny_unknown_fields`, never silently dropped |
| `discover_walks_up_to_the_nearest_config`                      | discovery climbs from a nested file to the ancestor `mimz.toml`                          |
| `parses_lib_std_section`                                       | the `[lib] std = "…"` override (vendored standard library) parses                        |
| `unknown_lib_key_is_rejected`                                  | …and a typo inside `[lib]` is rejected the same way                                      |
| `resolve_with_path_returns_config_location`                    | resolution reports WHICH `mimz.toml` won, so a `std` override resolves relative to it    |
| `config_parses_top_level_extern_sim_and_compile_verilog_files` | the top-level `extern_sim` mode and `verilog_files` list parse                           |
