# LSP (`src/lsp.rs` unit + `tests/lsp.rs` smoke, 8 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

| Test                                                        | Locks in                                                                                                                                     |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `positions_are_utf16_lines_and_columns`                     | byte span → LSP Position math (0-based lines)                                                                                                |
| `offset_inverts_position_utf16`                             | `offset` is the exact inverse of `position` (UTF-16 units, incl. a Tamil line) - the cursor→byte mapping the feature handlers depend on      |
| `tamil_text_counts_utf16_units_not_bytes`                   | LSP columns are UTF-16 code units - a Tamil identifier before the error must not skew the squiggle                                           |
| `analyze_reports_checker_errors_with_codes`                 | the in-memory pipeline (didOpen text, never on disk) produces coded checker diagnostics                                                      |
| `diagnostics_localize_to_the_chosen_flavor`                 | the LSP renders E0501 in Tamil (`y-க்கு` via `morph`) and English verbatim - same plumbing as `check`/`compile`                              |
| `uncovered_code_is_not_localized_in_lsp`                    | an uncovered code (E0401) is byte-identical across flavors in the LSP (the English-fallback invariant)                                       |
| `mixed_flavor_lint_publishes_as_a_warning`                  | W0001 reaches the editor as a WARNING (yellow squiggle), not an error - a mixed-flavor file still builds                                     |
| `opening_a_broken_file_publishes_coded_diagnostics` (smoke) | the REAL binary over the real wire protocol: framed JSON-RPC initialize → didOpen → publishDiagnostics with code, source, help, and position |
