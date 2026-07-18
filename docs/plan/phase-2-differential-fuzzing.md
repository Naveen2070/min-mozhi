# Phase 2 — Differential Fuzzing (T2)

> Structural defense against the "three semantic authorities" divergence
> class (checker / simulator / emitter each reimplement expression
> semantics independently — `docs/audit/review-2026-07-17.md` §3.1).
> Window: ongoing, grows with the language · Status: 🟡 v1 in progress

## Goal

Random-program differential testing: generate valid `.mimz` programs, run
each through our own kernel AND real Icarus Verilog, assert they agree.
Every divergence bug found in this codebase so far (BUG-6, BUG-11, BUG-16)
is a checker-accepts-it-simulator-can't-run-it-or-runs-it-differently
defect that a curated example list only catches if a human happens to
write the right example. Random generation removes that dependency.

This is deliberately a **phased, growing** effort — v1 is intentionally
narrow (see design doc,
`docs/superpowers/specs/2026-07-18-differential-fuzzing-design.local.md`),
not the final shape. Each version below extends the generator's reach as
the generator itself proves out and as the language gains surface area.

## Work items

- [ ] **v1 — unsigned combinational** (`tests/differential_fuzz.rs`):
      seeded deterministic PRNG, width-tracked bottom-up expression
      generator (`+ - +% -% & | ^` comparisons, shift, concat, slice-read),
      no signed/clocked/enum/bundle/fn/foreach. 20 programs per
      `cargo test`, `MIMZ_DIFF_FUZZ_N` env var for a deeper manual run.
      Gated by `REQUIRE_IVERILOG` like every other Icarus differential.
- [ ] **v2 — signed values**: `signed[N]` generation with real
      signed/unsigned-mixing avoidance (E0403), not the "always one
      concrete type" shortcut v1 uses.
- [ ] **v3 — clocked designs**: `clock`/`reset`/`reg`/`on rise`/`fall` —
      state-aware generation (reset values, multi-cycle stimulus).
      Meaningfully bigger than v1/v2; needs its own design pass.
- [ ] **v4 — real libFuzzer integration** (`fuzz/fuzz_targets/`):
      coverage-guided fuzzing reusing this generator's expression-tree
      logic, once the deterministic version is proven out. Needs its own
      Icarus-shellout plumbing (doesn't exist in `fuzz/` today) and is
      CI-only (libFuzzer needs nightly + Linux/macOS, same constraint the
      existing 4 fuzz targets already have). Natural point to also wire
      into the existing weekly fuzz cron (`ci.yml`'s `0 4 * * 1` schedule).
- [ ] **v5 — broader language surface**: enums, bundles, functions,
      `foreach`/`repeat`, cross-file instances — tracking the language's
      own growth, matching T1's finding that language surface needs
      differential coverage to keep pace with it, not trail behind it.

## Milestone (v1)

`tests/differential_fuzz.rs` lands, green in CI with `REQUIRE_IVERILOG=1`,
and finds zero new bugs on the first clean run (a bug found immediately
would mean v1 itself has a generator defect, not a product one — the
generator's own checker-validity is asserted separately and continuously).

## Exit criteria (v1)

1. `differential_fuzz_generates_checker_valid_programs` (fast,
   Icarus-independent, ~1000 seeds) green.
2. `differential_fuzz_matches_icarus` (real differential, 20 programs)
   green under `REQUIRE_IVERILOG=1`.
3. `tests/icarus.rs`'s existing suite unaffected by the `tests/support/`
   extraction refactor (same tests, same behavior, shared helpers only).
4. Full workspace `cargo test` + `cargo clippy -D warnings` + `cargo fmt
--check` still clean.

## Risks / notes

- The generator being _wrong_ (emitting something that isn't actually
  spec-legal) would manufacture false-positive bug reports — mitigated by
  the fast checker-validity unit test running continuously, independent of
  Icarus availability.
- v2 (signed) is the highest-value next step once v1 is stable — BUG-11's
  own class is unsigned, but signed/unsigned boundary bugs are a
  historically common HDL defect class this doesn't reach yet.
- v4 (libFuzzer) is explicitly deferred, not because it's low-value, but
  because it needs new plumbing (Icarus shelling out from inside a
  libFuzzer harness) that doesn't exist yet and shouldn't block v1 landing.
