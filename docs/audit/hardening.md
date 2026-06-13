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
process id (`src/bin/mimz-bench/metrics.rs`). Dev-tool only, inputs not
attacker-controlled — low severity, fixed while in the area.

---

## Checked and found safe (no change needed)

The audit produced several initial "critical" claims that **did not survive
verification against the code**. Recording them so they are not re-investigated:

| Area                         | Why it is safe                                                                                                                                             |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Checker width arithmetic     | `MAX_WIDTH = 1_000_000` is enforced (`checker/widths/mod.rs`) before any width `+`/`*`/concat-sum, so u128 cannot overflow in practice.                    |
| `repeat` unrolling           | Capped at `REPEAT_BUDGET = 4096` in **both** the checker (`drivers.rs`) and the emitter (`emit_verilog/mod.rs`). Not a bomb.                               |
| Import cycles                | Detected via a canonicalized `visited` set (`project.rs`) — no infinite loop. (It silently skips a cycle rather than emitting an error — cosmetic.)        |
| Import path traversal        | Import segments are XID identifiers; `..` and `/` are not expressible, so `import ../../etc/x` cannot be written. Symlink escape needs local write access. |
| Import file count            | Each import must resolve to a real on-disk file, so loading is **linear in attacker-created files** — no amplification (no zip-bomb analogue).             |
| Panics / `unwrap` on input   | No `unwrap`/`expect`/`panic!` reachable from input in `src/` (outside tests).                                                                              |
| Checker cycle walk           | Combinational-cycle detection uses an explicit stack (`drivers.rs`), not recursion — cannot stack-overflow.                                                |
| `comb::mask`, runtime shifts | Already shift-guarded (`if w >= 128`, `checked_shl`, `.min(127)`) before this audit — only `const_eval`'s shifts were raw (fixed, SEC-2).                  |

---

## Ongoing assurance (recommended, not yet done)

- **Fuzz target:** a `cargo-fuzz` harness over `lex → parse → eval` (the full
  untrusted-input path) would catch the next SEC-1-class crash automatically;
  run nightly in CI.
- **CI** already enforces `clippy -D warnings` + full tests; `#![forbid(unsafe_code)]`
  now makes memory-unsafe code a hard error.

## Scope boundary

This audit hardens the compiler against **malicious input** (crashes, overflow,
exhaustion). The **correctness** of emitted hardware — that _valid_ input
produces correct, safe Verilog — is the job of the checker's six passes (E0xxx)
and the golden / Icarus differential tests, which already exist and are
unchanged by this work.
