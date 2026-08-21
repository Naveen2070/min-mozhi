# Unit: standard-library routing (`crates/mimz-core/src/stdlib.rs`, 5 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

`import std.fifo` resolves to a module EMBEDDED in the binary
(`include_str!` over the already-tested `examples/` sources), so there is
no install path and it works in WASM. Routing keys on the written alias:
an English stem picks the canonical module, a Tamil twin name or its
romanization picks the pure-Tamil twin.

| Test                                               | Locks in                                                                       |
| -------------------------------------------------- | ------------------------------------------------------------------------------ |
| `english_stem_selects_canonical`                   | `std.fifo` → the canonical module                                              |
| `twin_name_and_roman_select_twin`                  | `nuulagam.varisai` and its romanization → the pure-Tamil twin                  |
| `namespace_aliases_match_all_three_flavors`        | the `std`/`nuulagam`/`நூலகம்` namespace spellings all resolve                  |
| `unknown_module_is_none_and_available_lists_stems` | an unknown module is `None` and the error can list what IS available           |
| `every_embedded_module_has_no_imports`             | INVARIANT: no embedded module imports another, so resolution can never recurse |
