# Fuzzing: `fuzz/fuzz_targets/` (CI-only, not `cargo test` units)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

Four `cargo-fuzz` harnesses over the untrusted-input path, asserting the audit's
core guarantee (any byte string yields a value/Verilog or a clean `Diag`/`Err`,
never a panic / abort / hang):

- `lex_parse_eval` - NFC → `lex` → `parse` → `sim::comb::eval_outputs`, run twice
  (empty inputs for the const path, then AST-derived per-port values for the
  runtime datapath). After the random pass, 8 fixed edge-case evaluation passes
  (0, 1, u128::MAX, 1<<32, 1<<63, 1<<127, (1<<126)-1, (1<<64)-1 as all-port
  values) ensure truncation-prone boundaries are always exercised regardless of
  the random byte stream.
- `lex_parse_compile` - NFC → `lex` → `parse` → `checker::check` →
  `transliterate` → `Project::from_files` → `emit` (the Verilog backend).
- `pretty_roundtrip` - NFC → `lex` → `parse` → `pretty::pretty_print` → re-`lex`
  → re-`parse` (the printed source MUST re-parse), and for an emittable program
  the re-parsed AST must lower to byte-identical Verilog. Exercises the
  `translate --order` printer on arbitrary input (the unit suite only covers the
  fixed example corpus).
- `translate_roundtrip` - NFC → `lex` → `parse` → `translate` (keyword reskin,
  `--romanize-names`, and name-map restore): every reskin/romanize output must
  re-lex, and `romanize → restore` must be token-equivalent to the plain reskin.
  Added 2026-06-15 after a deterministic stress audit found the numeric-literal
  abutment bug (`42தொகுதி`, fixed by the `push_guarded` boundary guard).

**Not** part of the test count above: they need a nightly toolchain + libFuzzer
(Linux/macOS), live in a standalone `fuzz/` crate the root gate never builds, and
run as the CI `fuzz` job (60 s smoke per target on push/PR, corpus seeded from
`examples/`) plus a weekly `fuzz-nightly` job (10 min per target). Run locally
under WSL2/Linux with `cargo +nightly fuzz run <target>`. See
[`../audit/hardening.md`](../audit/hardening.md) "Ongoing assurance".
