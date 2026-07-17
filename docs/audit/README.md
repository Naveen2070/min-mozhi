# Security & Robustness Audit

A standing record of defects found by auditing the compiler against
**malicious or malformed input**, and exactly how each was fixed. Min-Mozhi
emits hardware-level logic, so the compiler must never crash, corrupt memory,
silently miscompute, or exhaust resources on a crafted `.mimz` file.

Each entry states: **what** was found, **how** it was found, its **severity and
reachability**, and the **fix** (with the file and the regression test that
locks it). New audits append here; nothing is deleted.

## Files

| Category                       | What it covers                                                              |
| ------------------------------ | --------------------------------------------------------------------------- |
| [`security.md`](security.md)   | Input-triggered crashes, overflow, memory safety — the threat-model defects |
| [`bugs.md`](bugs.md)           | Functional defects (wrong behavior, hangs) found along the way              |
| [`hardening.md`](hardening.md) | Preventive measures + what was checked and found already-safe               |

## Audit log

| Date       | Scope                                                                                                                                        | Result                                                                                                                                                                                                                                                                                                                                                                           |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-06-14 | Full `src/` sweep: overflow, panics, recursion, resources, memory safety                                                                     | 2 real defects fixed (1 CRITICAL, 1 HIGH) + 1 hang + hardening                                                                                                                                                                                                                                                                                                                   |
| 2026-06-14 | Continuous fuzzing of the untrusted-input path                                                                                               | `cargo-fuzz` target `fuzz/` + CI smoke job landed                                                                                                                                                                                                                                                                                                                                |
| 2026-06-15 | Since-2026-06-14 changes (config, romanization + name-map, morph)                                                                            | No overflow/unsafe/crash; F1 map-version check + F4 reskin boundary guard ([`bugs.md`](bugs.md) BUG-2); `translate_roundtrip` fuzz target added                                                                                                                                                                                                                                  |
| 2026-06-20 | Full re-audit `src/sim/value.rs` + cross-codebase `as u32` scan + crates                                                                     | Finding A (MEDIUM): `Shl` `r.bits as u32` truncation, plus same semantic error in `Shr`'s `.min(127)` guard (shift-by-128 became shift-by-127). Fixed both. 8 regression tests. Fuzzer strengthened with edge-case values. WASM/bench crates: safe (thin FFI wrappers). No other truncation bugs found.                                                                          |
| 2026-06-21 | Testbench emitter `src/emit_verilog/testbench.rs` — `--emit-testbench` test_env + name sanitization                                          | `test_env` didn't merge module parameter defaults, breaking width resolution for tests omitting a defaulted param ([`bugs.md`](bugs.md) BUG-3, HIGH). Two test blocks with different names could sanitize to the same Verilog module identifier, silently emitting duplicate modules instead of diagnosing ([`bugs.md`](bugs.md) BUG-4, MEDIUM). Both fixed, 3 regression tests. |
| 2026-07-17 | Full CTO review: language, architecture, Rust, HDL correctness, perf, testing, DX, security ([`review-2026-07-17.md`](review-2026-07-17.md)) | BUG-11 escalated to proven CRITICAL (sim=255 vs Icarus=63 on the same checked-clean source); BUG-12 **corrected** — checker rejects module consts in `fn` bodies too (E0101), so it is a consistent design limitation, not divergence; root cause named (three semantic authorities + checker-skipping sim paths); ratings + top-10 priority list. 861/861 tests at HEAD.        |

## Threat model

`mimz compile | check | eval <file>` is run on an untrusted `.mimz` file (which
may `import` others). The requirements:

1. **No crash.** No panic, no stack overflow, no abort on any input.
2. **No memory unsafety.** No buffer overflow / out-of-bounds write — ever.
3. **No silent miscompute.** Integer overflow must error, never wrap quietly to
   a wrong value (a wrong width is wrong hardware).
4. **No resource exhaustion.** Bounded memory and CPU for bounded input.

## Method

Three independent read-only review passes (arithmetic/overflow, panics/recursion,
resources/imports), each finding cross-checked **against the actual code** before
acceptance — several initially-reported "criticals" did not survive that check
and were downgraded (see [`hardening.md`](hardening.md), "Checked and safe").
Every fix ships with a regression test run in **both debug and release** (the two
builds fail differently on overflow). Standard gate after: `cargo fmt`,
`clippy -D warnings`, `cargo test` (+`--release`), rustdoc, prettier,
markdownlint.
