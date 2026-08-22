# Integration: config (`tests/config.rs`, 7 tests - run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The CLI merge (CLI › config › default) and name-map auto-discovery, end to end.

| Test                                               | Locks in                                                                                   |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `auto_name_map_restores_without_a_flag`            | reverse translate auto-loads `<input>.names.json` and restores Tamil - no `--names-map`    |
| `no_names_map_keeps_latin_names`                   | `--no-names-map` opts out of auto-discovery; the romanized Latin decl stays                |
| `config_default_flavor_is_overridden_by_the_cli`   | `[translate] to` supplies the default; an explicit `--to` overrides it                     |
| `malformed_config_is_a_clean_error`                | a broken `mimz.toml` fails with `invalid config`, not a panic                              |
| `name_map_with_unknown_version_is_rejected`        | a `--names-map` with an unknown `version` fails closed (`version 999`), never mis-restores |
| `std_override_inside_workspace_root_is_allowed`    | a `[lib] std` path inside the project is honored (vendored stdlib via `mimz eject std`)    |
| `std_override_escaping_workspace_root_is_rejected` | SEC: a `[lib] std` path escaping the project root is refused - no arbitrary-path read      |
