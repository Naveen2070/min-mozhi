# Hardening & checked-safe

Preventive measures added beyond the specific defects, and the items the audit
**checked and found already safe** (including corrections to over-rated initial
findings). See [`README.md`](README.md) for method.

---

## Hardening added

### HARD-1 — `#![forbid(unsafe_code)]`

`src/lib.rs` and `src/main.rs` now forbid `unsafe`. Zero cost today (none
exists); permanently locks the memory-safety guarantee so a future `unsafe` —
the only way a buffer overflow could enter — is a compile error. (See SEC-3 in
[`security.md`](security.md).)

### HARD-2 — Release `overflow-checks = true`

`Cargo.toml` `[profile.release]` now enables overflow checks. The evaluators use
checked arithmetic (a wrong value is never produced quietly), and this is a
defense-in-depth backstop: any integer overflow missed anywhere becomes a loud,
debuggable abort in release rather than a silent miscompile of hardware. Verified
the full test suite passes under `cargo test --release` with this on.

### HARD-3 — Source-size cap

`project::read_source` rejects files larger than `MAX_SOURCE_BYTES` (32 MB)
before reading them, bounding the lexer's `Vec<(usize, char)>` (several times the
file size). Generous enough that no legitimate file is refused; closes the
memory-proportional-to-input gap.

### HARD-4 — Per-process bench temp paths

`mimz-bench`'s `iverilog` output path was module-name-predictable in the shared
temp dir (TOCTOU / symlink-clobber on a multi-user host). It now includes the
process id (`src/bin/mimz-bench/metrics/`). Dev-tool only, inputs not
attacker-controlled — low severity, fixed while in the area.

### HARD-5 — Simulator count caps (`MAX_SIM_CYCLES` / `MAX_SWEEP_VECTORS`)

The Phase 1.5 simulator originally left its count-like inputs unbounded — the
`tick(clk, n)` loop, the `--sweep` cartesian product, and the `--cycles` run
length. `MAX_SIM_CYCLES = 1_000_000` and `MAX_SWEEP_VECTORS = 1_000_000`
(`src/sim/run.rs`) now bound them (test harness, `sweep_vectors`, `run()`, and a
clap range on `--cycles`), extending the parser-`MAX_DEPTH` / emitter-`REPEAT_BUDGET`
doctrine into the sim. Also bounded the `mimz.toml` walk-up
(`MAX_CONFIG_WALK_DEPTH = 256`). See SEC-5 in [`security.md`](security.md).

### HARD-6 — Simulator elaboration-time bounds (instance depth + repeat span)

The C1–C4 audit (2026) found SEC-5 bounded the simulator's _runtime_ counts but
not its _elaboration-time_ ones. `MAX_INSTANCE_DEPTH = 16` (`src/sim/elaborate.rs`)
now bounds instance-flattening recursion so a recursive/cyclic instantiation fails
cleanly instead of overflowing the stack (the checker guards this, but
`mimz sim`/`test` skip the checker); the `repeat` span now uses `checked_sub` so an
extreme `hi - lo` is an over-budget error, not an overflow panic; and a bit-index
drive is bounded to `0..128` before the `as u32` cast. A 2026-06-17 follow-up pass
also made `int_expr` (which lowers each flattened child const to a literal)
non-recursive and `unsigned_abs`-based, so a const evaluating to `i128::MIN`
lowers to `-2¹²⁷` instead of overflow-panicking the negation. See SEC-6 in
[`security.md`](security.md).

---

## Hardening recommended (open)

Preventive measures the 2026-08-02 CTO review
([`review-2026-08-02.md`](review-2026-08-02.md)) identified as missing. Not
defects — nothing here is broken today; each closes a class rather than an
instance. Listed with the same HARD numbering so they slot into "Hardening
added" once landed.

### HARD-7 (DONE) — `#![forbid(unsafe_code)]` on every crate, not just `mimz-core`

**Status update 2026-08-09** (round-3 class-closure plan Task 7): confirmed
done, not just partially — `src/lib.rs:52` and
`src/main.rs:9` both carry the attribute, closing the gap this entry's
2026-08-07 update said was the remaining action. Every crate in the
workspace now carries `#![forbid(unsafe_code)]`: `mimz-core`,
`mimz-sim`, `mimz-wasm`, and the shell crate (`src`, the one linking
`cpal`/`crossterm`/`notify` — the crate this entry singled out as
mattering most).

### HARD-8 (DONE) — Dependency-advisory scanning in CI

**Status update 2026-08-07** ([`review-2026-08-07.md`](review-2026-08-07.md)):
closed — `.github/workflows/ci.yml:245` runs a dedicated `audit` job
(`cargo install cargo-audit --locked` + `cargo audit`). `cargo-deny`'s `bans` /
`licenses` / `sources` checks are still not run; tracked as the remaining scope
below rather than as a separate ID.

CI gates `fmt`, `clippy -D warnings`, `rustdoc -D warnings`, full workspace tests
with `REQUIRE_IVERILOG=1`, and pins every GitHub Action to a full commit SHA —
good supply-chain hygiene on the _workflow_ side. But there is **no
`cargo-deny` / `cargo-audit` step**, so a published RUSTSEC advisory against the
dependency tree would not be surfaced.

The tree is not trivial: `tokio`, `tower-lsp`, `ratatui`, `crossterm`, `cpal`,
`notify`, `clap`, `serde`, `toml` and their transitives.

**Action.** Add a `cargo-deny check advisories bans licenses sources` step to the
`check` job. Start non-gating if the initial run is noisy, then gate. `bans` also
catches duplicate-version drift, and `licenses` protects the MIT/Apache-2.0 dual
license the Constitution commits to forever.

### HARD-9 (OPEN) — Fuzz target for the checker → emitter path

The four existing `cargo-fuzz` targets (`lex_parse_compile`, `lex_parse_eval`,
`pretty_roundtrip`, `translate_roundtrip`) cover lex/parse/eval and two
round-trip properties. The **checker's width rules and the emitter's
self-determined-position logic have no fuzz target** — only hand-written tests
and the random-program differential fuzzer, whose generator gap is exactly what
let BUG-28/BUG-29 through (see [`gaps.md`](gaps.md) GAP-5).

**Action.** Add a fifth target driving `checker::check` → `emit_verilog::emit`
over generated-but-plausible ASTs, asserting the two invariants that must always
hold:

1. Checker-accepted input **never panics** the emitter (the SEC-10 class).
2. Emitted Verilog **always elaborates** under `iverilog -t null`.

Wire it into both the per-PR and weekly CI fuzz jobs, same as the existing four.

---

## Checked and found safe (no change needed)

The audit produced several initial "critical" claims that **did not survive
verification against the code**. Recording them so they are not re-investigated:

| Area                                   | Why it is safe                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Checker width arithmetic               | `MAX_WIDTH = 1_000_000` is enforced (`checker/widths/mod.rs`) before any width `+`/`*`/concat-sum, so u128 cannot overflow in practice.                                                                                                                                                                                                                                                                                                                                                                                                         |
| `repeat` unrolling                     | Capped at `REPEAT_BUDGET = 4096` in **both** the checker (`drivers.rs`) and the emitter (`emit_verilog/mod.rs`). Not a bomb.                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Import cycles                          | Detected via a canonicalized `visited` set (`project.rs`) — no infinite loop. (It silently skips a cycle rather than emitting an error — cosmetic.)                                                                                                                                                                                                                                                                                                                                                                                             |
| Import path traversal                  | Import segments are XID identifiers; `..` and `/` are not expressible, so `import ../../etc/x` cannot be written. Symlink escape needs local write access.                                                                                                                                                                                                                                                                                                                                                                                      |
| Import file count                      | Each import must resolve to a real on-disk file, so loading is **linear in attacker-created files** — no amplification (no zip-bomb analogue).                                                                                                                                                                                                                                                                                                                                                                                                  |
| Panics / `unwrap` on input             | No `unwrap`/`expect`/`panic!` reachable from input in `src/` (outside tests). **Partial correction (2026-08-02):** still true of `src/`, but the workspace split moved the emitter into `crates/mimz-core/`, where `emit_verilog/kinds.rs:52` carries a `panic!` guarded only by a cross-file `kind_is_inferrable` convention. No reproducer found in ~20 targeted probes (latent, not live) — filed as SEC-10 in [`security.md`](security.md) with the durable fix (`Option<Kind>`). Re-scope this row to the whole workspace once that lands. |
| Checker cycle walk                     | Combinational-cycle detection uses an explicit stack (`drivers.rs`), not recursion — cannot stack-overflow.                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `comb::mask`, runtime shifts (initial) | Already shift-guarded (`if w >= 128`, `checked_shl`, `.min(127)`) before first pass — only `const_eval`'s shifts were raw (fixed, SEC-2). Second pass (2026-06-20) found `Shl` used bare `r.bits as u32` (silently truncating when bit ≥ 32 was set, admitted by the initial review's too-broad claim). **Re-fixed:** both `Shl` and `Shr` now guard with `if r.bits >= 128 { 0 } else { … }` — correct semantics (shift-by-128 → 0, not shift-by-127).                                                                                         |
| Sim frame-time `cycle * PERIOD`        | Flagged as a u64-overflow "high", but **unreachable**: the cap (HARD-5) bounds `cycle ≤ 1_000_000`, so the product is ≤ 10^7. The loop could never complete ~10^18 iterations anyway — bounding the loop subsumes it. No checked-mul added (would be dead code).                                                                                                                                                                                                                                                                                |
| Sim concat `as u32` cast (`value.rs`)  | Guarded by the `total > 128` check immediately above it — the cast is never reached with an out-of-range value.                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Thamizh-order flip recursion           | All five clause flips (incl. `seq_if_thamizh`, `if_expr_thamizh`) route through the SEC-1 `enter()`/`leave()` depth guard — no new unguarded neck.                                                                                                                                                                                                                                                                                                                                                                                              |

---

## Ongoing assurance

- **Fuzz target (done).** A `cargo-fuzz` harness over `lex → parse → eval` (the
  full untrusted-input path) lives in `fuzz/fuzz_targets/lex_parse_eval.rs`. It
  NFC-normalizes the raw bytes (like `project::read_source`), lexes, parses, and
  constant-evaluates — any panic / abort / hang is a finding. The CI `fuzz` job
  (`.github/workflows/ci.yml`) runs a bounded 60 s smoke on every push/PR; a
  crash writes a reproducer to `fuzz/artifacts/` and fails the build. libFuzzer
  needs a nightly toolchain and runs on Linux/macOS only, so it is **not** built
  on the Windows dev box — run it locally under WSL2/Linux with
  `cargo +nightly fuzz run lex_parse_eval`. The `fuzz/` crate is standalone (own
  manifest + `[workspace]`), so the normal `cargo build`/`clippy`/`test` gate
  never sees it. **Extensions landed 2026-06-14:** the corpus is seeded from
  `examples/` (CI step, flattened names), the eval target now also feeds each
  input port an AST-derived value (runtime datapath, not just constant folding),
  a second target `lex_parse_compile` fuzzes the Verilog backend
  (`lex → parse → check → emit`), and a weekly `fuzz-nightly` job runs 10 min per
  target (vs the 60 s per-PR smoke). All CI-verified (nightly/Linux); the Windows
  dev box still cannot build the fuzz crate. **First finding (2026-06-14):** the
  `lex_parse_compile` target caught a subtract-overflow panic in the checker's
  zero-width output coverage check — fixed and regression-tested (SEC-4 in
  [`security.md`](security.md)). The reproducer is in the gitignored
  `fuzz/corpus/`; the durable guard is the checker unit test. **Third target
  (2026-06-15):** `pretty_roundtrip` fuzzes the `translate --order` AST
  pretty-printer — printed source must re-parse, and an emittable program must
  round-trip to byte-identical Verilog; wired into both the per-PR and weekly
  CI fuzz jobs. **Fourth target (2026-06-15):** `translate_roundtrip` fuzzes the
  `translate` byte-walk — keyword reskin, `--romanize-names`, and the name-map
  restore: every reskin/romanize output must re-lex, and `romanize → restore`
  must be token-equivalent to the plain reskin. Added after a deterministic
  stress audit found a numeric literal abutting a Tamil token (`42தொகுதி`) glued
  into an unlexable lexeme on reskin (fixed by the `push_guarded` boundary guard
  in `reskin`). Also wired into both CI fuzz jobs.
- **CI** also enforces `clippy -D warnings` + full tests; `#![forbid(unsafe_code)]`
  makes memory-unsafe code a hard error.

## Scope boundary

This audit hardens the compiler against **malicious input** (crashes, overflow,
exhaustion). The **correctness** of emitted hardware — that _valid_ input
produces correct, safe Verilog — is the job of the checker's nine passes (E0xxx)
and the golden / Icarus differential tests, which already exist and are
unchanged by this work.

**Correction (2026-08-02).** Two amendments to the sentence above, both from
[`review-2026-08-02.md`](review-2026-08-02.md):

1. **Pass count.** The checker runs **nine** passes, not seven
   (`crates/mimz-core/src/checker/mod.rs:36`): symbols, extern-module ports,
   `fn` cycles, `fn` unreachable-code, consts, names, widths, drivers, clocks.
   Corrected in place above.
2. **The boundary is not as clean as this paragraph implies.** BUG-28/BUG-29
   ([`bugs.md`](bugs.md)) are **invalid HDL generation from valid input** — a
   silent miscompile that the golden and Icarus differential tests do not catch,
   because every existing oracle compares simulator-vs-Verilog and none compares
   declared-type-vs-produced-value ([`gaps.md`](gaps.md) GAP-5). "Never silently
   miscompute" is listed in this audit's own opening threat statement, so that
   class belongs inside this scope, not outside it. HARD-9 above is the
   preventive measure for it.
