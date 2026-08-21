# Integration: compile_string (`tests/compile_string.rs`, 14 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Tests the in-memory `mimz::compile_string` entry point — the embedding API
behind the WASM playground — asserting the same pipeline behavior a browser
sees, with no filesystem access. Covers valid compilation, flavor identity,
rendered diagnostics on error (width mismatch, syntax error, rejected
import), bundle port flattening/literals, tagged-packet golden output,
guard-clause return ordering, and array-parameter/array-index expansion
(literal call args, `let` bindings, ports, constant vs. runtime indexing).
