# Phase 2 — Differential Fuzzing (T2)

> Structural defense against the "three semantic authorities" divergence
> class (checker / simulator / emitter each reimplement expression
> semantics independently — `docs/audit/review-2026-07-17.md` §3.1).
> Window: ongoing, grows with the language · Status: 🟡 v1+v2+v3 landed, v4+ not started

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

- [x] **v1 — unsigned combinational** (`tests/differential_fuzz.rs`):
      seeded deterministic PRNG, width-tracked bottom-up expression
      generator (`+ - +% -% & | ^` comparisons, shift, concat, slice-read),
      no signed/clocked/enum/bundle/fn/foreach. 20 programs per
      `cargo test`, `MIMZ_DIFF_FUZZ_N` env var for a deeper manual run.
      Gated by `REQUIRE_IVERILOG` like every other Icarus differential.
      **Landed 2026-07-19** — `tests/support/mod.rs` extraction +
      `tests/differential_fuzz.rs`, clean at both N=20 (CI default) and
      N=500 (manual confidence pass, after fixes). Found and fixed
      **BUG-18** (extend()-of-literal losing Verilog width in self-determined
      contexts) on the very first real run. The N=500 deeper pass then found
      **BUG-19** (subtraction-result-width divergence in a self-determined
      concat operand, same class, different construct) — initially filed OPEN
      and deferred, but later FIXED in Stage 4 Phase A1b. Both bugs are documented in docs/audit/bugs.md.
- [x] **v2 — signed values** (`tests/differential_fuzz.rs`): `signed[N]`
      ports and literal leaves (`signed(extend(v, w))`, since `extend()` of
      a bare literal is always unsigned — `call_ty`'s `CtInt` arm), real
      kind unification (`cast_to`, `signed(x)`/`unsigned(x)`) before every
      same-width combine so `+%/-%/&/|/^`/comparisons never mix kinds
      (E0403), concat casting a signed operand to unsigned first (E0403
      again — concat never accepts `signed` directly), shift preserving
      the LHS's own kind. **Landed 2026-07-19.** Found and fixed a second,
      independent gap while implementing it: `clamp()` (used to keep any
      fragment under the 32-bit generator cap) was slicing ARBITRARY
      composite fragments — legal per `checker::check`, but `iverilog`
      rejects a bare (non-identifier) base before `[hi:lo]` as a syntax
      error (confirmed live: `(a & b)[2:0]` and `{a, b}[3:0]` both fail to
      compile). `clamp` now only slices a genuine port identifier,
      matching the restriction `gen_slice` already had; an over-cap
      composite is discarded and replaced with a fresh leaf instead. Also
      hit **BUG-19**'s class twice more during the `MIMZ_DIFF_FUZZ_N=500`
      deep pass — once confirming lossless `+`/`-` (already filed), once
      finding wrapping `+%`/`-%` ALSO belongs to the same class (a
      genuinely different symptom: Verilog redoes the wrap at the widened
      context, not just drops a carry bit) — documented in BUG-19's own
      entry (`docs/audit/bugs.md`) rather than filed separately. All four
      operators (`+ - +% -%`) were initially excluded from the generator
      to keep CI green, but as of Stage 4 Phase A1b (which fixed BUG-19 and BUG-23),
      they have been fully re-enabled. The generator now natively produces them,
      and `&`/`|`/`^`/comparisons stay in scope as before. Clean at both N=20 (CI default)
      and N=500 (manual confidence pass) after both fixes.
- [x] **v3 — clocked designs** (`tests/differential_fuzz.rs`,
      `gen_clocked_module`): `clock clk` + `reset rst` + 1-3 `reg`s each
      driven by one `on rise(clk)` block, `out y` combinational over
      port/register values, run for a fixed number of cycles with held
      (constant) inputs and one reset cycle — reusing `elaborate_project` + `run` (the exact engine behind `mimz sim`/`test`) for our kernel
      side and a new `support::clocked_testbench` (extracted from
      `tests/icarus.rs`, pure refactor, that suite unaffected) for Icarus.
      Reuses v1/v2's SAME expression generator for both a register's
      next-state expression and the output expression, over a combined
      leaf pool (`ports ++ regs` — a register's current value reads just
      like an input port). No dual-edge, no multiple clocks, no
      multi-cycle (time-varying) stimulus — `SimOpts` only supports
      inputs held constant for the whole run, which is enough to exercise
      reset + state evolution across cycles; a genuinely bigger v3.1 if
      varying stimulus is ever needed. **Landed 2026-07-19.** Found and
      fixed three real gaps getting it to a clean N=2000 deep pass — all
      pre-existing, kind/clock-independent, just never reachable until a
      register's next-state expression needed to be forced onto an exact
      pre-declared type. First, `widen()` was unsound for ANY composite
      fragment, not just the cases already scoped out: added an `atomic`
      field to `Frag` (true only for a plain identifier/slice or an
      already-explicit-width literal); `widen()` now discards and
      regenerates a fresh exactly-sized literal instead of trusting
      `extend()` on a computed expression it can't verify — generalizes
      the same "discard what can't be made safe" move `clamp()` already
      used. Second, `signed(x)`/`unsigned(x)` have a self-determined
      argument in real Verilog — `signed(extend(x, W))` never actually
      widens `x` before reinterpreting it, atomic or not, since
      `extend()` contributes nothing syntactically for a non-literal
      argument; `force_width` (built to land a register's next-state
      expression on its exact declared type) had this backwards, fixed
      by casting FIRST then widening (`force_width`, in the source) —
      matching the order `combine_same_width` already happened to use
      correctly. Third,
      **BUG-21** (new, filed FIXED in `docs/audit/bugs.md`): the
      simulator's `ExprKind::Slice` evaluator inherited the SLICED BASE's
      signedness instead of always being unsigned (per the checker's own
      `slice_ty`) — a genuine kernel bug, unrelated to the generator,
      found because v3 was the first thing to combine a signed port's
      slice with `extend()` into a register write, fixed in
      `crates/mimz-sim/src/sim/value.rs`. Clean at N=20 (CI default),
      N=500, and a N=2000 extra-deep pass after all three fixes; the
      combinational v1/v2 tests were re-verified clean at N=1000 too (the
      `atomic`/`widen` refactor touches shared code).

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

## Milestone (v1) — achieved 2026-07-19

`tests/differential_fuzz.rs` landed, green in CI with `REQUIRE_IVERILOG=1`.
Original framing here predicted "finds zero new bugs on the first clean
run, else it's a generator defect" — that was wrong in the way that
matters least: it found two real, previously-unknown _product_ bugs
(BUG-18, BUG-19) on its first meaningful runs, not a generator defect (the
generator's own checker-validity held at 1000/1000 seeds throughout,
verified separately and continuously). That outcome is the actual proof
the milestone was reaching for — a curated example list (T1) had already
missed both.

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
- v2 (signed) landed 2026-07-19 — BUG-11's own class is unsigned, but
  signed/unsigned boundary bugs are a historically common HDL defect class
  v1 didn't reach. No NEW signed-specific divergence turned up in the
  N=500 deep pass (both real findings — the `clamp()` slice-syntax gap and
  BUG-19's `+%`/`-%` extension — were kind-independent, latent since v1).
- v3 (clocked) landed 2026-07-19 — same pattern held again: no divergence
  specific to CLOCKING itself turned up (reset/multi-cycle state evolution
  worked first try); all three real findings (`widen()`'s composite
  unsoundness, `signed()`'s self-determined argument, BUG-21's slice-sign
  bug) were latent, kind/clock-independent gaps only reachable once a
  register's next-state expression needed forcing onto an exact
  pre-declared type — something neither v1 nor v2 ever needed to do.
- v4 (libFuzzer) is explicitly deferred, not because it's low-value, but
  because it needs new plumbing (Icarus shelling out from inside a
  libFuzzer harness) that doesn't exist yet and shouldn't block v1 landing.
