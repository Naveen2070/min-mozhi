# Unit: widths pass internals (`crates/mimz-core/src/checker/widths/mod.rs`, `checker::widths::tests`, 7 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

A second, smaller unit-test pocket living inside the width pass itself
(distinct from the sibling `checker::tests` module above) - these pin
`Ty`/`Wcx` internals and dispatch paths too fine-grained for the
one-test-per-error-code table, added 2026-07-11 alongside the `Ty::Bundle`
model (see this file's changelog entry for that date).

| Test                                                              | Locks in                                                                                                        |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `sync_loop_result_init_width_checked`                             | a `sync loop`'s `-> result: ty = init` initializer is width-checked against `ty`                                |
| `sync_loop_var_width_is_clog2_hi_not_clog2_range_when_lo_nonzero` | the loop variable's width is `clog2(hi)`, not `clog2(hi - lo)`, when `lo != 0`                                  |
| `bundle_typed_fn_param_supports_field_access`                     | a bundle-typed fn parameter's field access resolves via `cx.sigs`, not a false E0105                            |
| `module_param_field_access_is_rejected`                           | `W.foo` on an int/bool module parameter errors instead of silently passing                                      |
| `mem_field_access_reports_exactly_one_diagnostic`                 | mem/clock/reset field access reports pass-3's diagnostic once, not doubled with `field_ty`'s                    |
| `enum_variant_from_wrong_enum_is_rejected`                        | assigning a variant from the wrong enum into an enum-typed reg/wire is caught (was a silent miss)               |
| `bundle_literal_tail_return_is_shape_checked`                     | a bundle-literal fn-tail return goes through `check_return_expr`, not the old `infer_ty`+`check_return_ty` path |
