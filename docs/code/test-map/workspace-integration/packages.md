# Integration: packages (`tests/packages.rs`, 2 tests — run the real binary)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Proves qualified references (`a.b.Name`) disambiguate two different files'
same-named module through the real `project.rs` loader, not a hand-wired
`resolved_file` like the unit tests use.

| Test                                                         | Locks in                                                                                    |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| `qualified_references_check_clean_with_zero_diagnostics`     | `mimz check` on a qualified-reference fixture reports zero diagnostics                      |
| `qualified_instances_compile_with_their_own_distinct_bodies` | two same-named modules from different files each keep their own body in the emitted Verilog |
