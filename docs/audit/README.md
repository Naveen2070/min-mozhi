# Security & Robustness Audit

A standing record of defects found by auditing the compiler against
**malicious or malformed input**, and exactly how each was fixed. Min-Mozhi
emits hardware-level logic, so the compiler must never crash, corrupt memory,
silently miscompute, or exhaust resources on a crafted `.mimz` file.

Each entry states: **what** was found, **how** it was found, its **severity and
reachability**, and the **fix** (with the file and the regression test that
locks it). New audits append here; nothing is deleted.

## Files

| Category                       | What it covers                                                                                   |
| ------------------------------ | ------------------------------------------------------------------------------------------------ |
| [`security.md`](security.md)   | Input-triggered crashes, overflow, memory safety — the threat-model defects                      |
| [`bugs.md`](bugs.md)           | **Index** of functional defects; the entries live in [`bugs/`](bugs/), ten per file              |
| [`hardening.md`](hardening.md) | Preventive measures added, recommended-but-open, and what was checked safe                       |
| [`gaps.md`](gaps.md)           | Correct-but-limited: architecture debt, absent language features, absent oracles                 |
| [`audit-log.md`](audit-log.md) | **Index** of the ledger; one row per audit-driven change, by month in [`audit-log/`](audit-log/) |

Full-project reviews (each one a dated snapshot that files its findings into the
four tables above):

| Review                                         | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`review-2026-07-03.md`](review-2026-07-03.md) | First full review — 14 findings                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| [`review-2026-07-17.md`](review-2026-07-17.md) | Full CTO review — filed BUG-11 (escalated with proof), corrected BUG-12                                                                                                                                                                                                                                                                                                                                                                                                               |
| [`review-2026-08-02.md`](review-2026-08-02.md) | Full CTO review — filed BUG-28…33, SEC-10, HARD-7…9, GAP-1…10; headline defect BUG-28                                                                                                                                                                                                                                                                                                                                                                                                 |
| [`review-2026-08-07.md`](review-2026-08-07.md) | v0.2 release-readiness re-verification — **do-not-ship**; filed BUG-41…44, GAP-11/12                                                                                                                                                                                                                                                                                                                                                                                                  |
| [`review-2026-08-09.md`](review-2026-08-09.md) | v0.2 release-readiness round 3 — **do-not-ship**; BUG-41…44/GAP-11/12 all verified fixed, filed BUG-48/49, GAP-13                                                                                                                                                                                                                                                                                                                                                                     |
| [`review-2026-08-10.md`](review-2026-08-10.md) | v0.2 release-readiness round 4 — **do-not-ship**; round-3 Tasks 1–9 all verified done (build enforcement triggered, fuzz acceptance re-run), filed BUG-52/53/54/55, GAP-14                                                                                                                                                                                                                                                                                                            |
| [`review-2026-08-13.md`](review-2026-08-13.md) | v0.2 release-readiness round 5 — **do-not-ship**; BUG-52/53/54/55/59 all verified fixed, classifier exhaustiveness triggered, gate 5 scored at **5000/5000** (clean), filed BUG-60/61, GAP-15                                                                                                                                                                                                                                                                                         |
| [`review-2026-08-15.md`](review-2026-08-15.md) | v0.2 release-readiness round 6 — **do-not-ship**, 5 of 8 gate items; BUG-60/61 verified fixed, `gate.sh`/`debug_assert!`/fuzz-bias acceptance all re-run from scratch; rule (a′) judged **mis-scoped** (it audits arms; 10 of 14 defects since round 3 are at call sites); filed BUG-62/63/64, GAP-16/17                                                                                                                                                                              |
| [`review-2026-08-17.md`](review-2026-08-17.md) | v0.2 release-readiness round 7 — **do-not-ship**, 7 of 10 gate items; BUG-62 (10/10), BUG-63 (4096/4096 vectors), BUG-64/65 (90 emitted testbench modules) all verified fixed; both fuzz acceptance criteria re-derived; **round-6 Task 1's `hoist_unresolved` invariant found two unknown defects on its own** — the first instrument in the series to do so — but a second, orthogonal scoping axis (the hoist buffer's flush point) has no cover; filed BUG-66/67/68/69, GAP-18/19 |

## Audit log

Moved to [`audit-log.md`](audit-log.md) — one row per audit-driven change,
newest last, filed a month per file under [`audit-log/`](audit-log/). It
outgrew this page (over 100 KB of table) and is now read on its own. **Append
new rows to the current month's file.**

## Release-gate scoring convention

v0.2's release-readiness gate 5 ("no new instance of the F-1/F-2 pattern") is
scored by running the differential fuzzer
(`MIMZ_DIFF_FUZZ_N`/`MIMZ_DIFF_FUZZ_CLOCKED_N`). Round 3 scored it green at the
per-PR depth (400/400) with two CRITICALs live at fresh indices 449 and 925 —
both inside the 5000-seed depth `ci.yml`'s `fuzz-nightly` already runs daily.
Round 4 found both, at 2000/2000, and neither round could tell a 400-seed
"clean" from a 5000-seed one apart just by reading the gate table.

**Standing rule, effective round 4 (GAP-14):**

- Gate 5 is scored at the **nightly depth (5000/5000)**, never the per-PR
  depth. A 400-seed run answers "does the generator's vocabulary reach the
  shape at all" — a different, cheaper question — not "is anything live."
- Every review's gate table records the **depth the run used**, next to the
  result, so "clean" is never ambiguous between rounds.
- Prefer dispatching the already-configured `fuzz-nightly` job
  (`workflow_dispatch`) and citing the run URL over a local run where
  possible — that job runs on Linux, where 5000/5000 is far cheaper than on
  Windows.

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
