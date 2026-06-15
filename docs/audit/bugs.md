# Functional bugs found

Non-security defects (wrong behavior, hangs) surfaced during the audit and
related work. See [`README.md`](README.md) for method.

---

## BUG-1 (HIGH) — Stray top-level `}` spun the parser into an OOM hang

**What.** A stray `}` at file level made `Parser::file()` loop forever, pushing
the same diagnostic until memory was exhausted (observed: a ~6 GiB allocation
abort). It was not a stack overflow but an **unbounded heap-growing loop** — the
process hung and then died.

**Cause.** `file()`'s recovery for an unexpected token called
`sync_to_newline()`, which **returns at `}` without consuming it** (`}` is a
block terminator inside items, not skippable trivia). A `}` is never valid at
file level, so `file()` re-read the same token every iteration and never made
progress. Unbalanced braces left by error recovery inside a module (e.g. a
malformed block whose `{` got skipped) orphan a `}` and trigger this.

**How found.** A new parser test triggered it while developing the grammar
engine; the backtrace showed `file()` repeatedly calling `Parser::error`
(growing the `Vec<Diag>`). An integration test had been masking it — the binary
OOM-crashed with a non-zero exit, which the test misread as "errored cleanly".

**Severity.** HIGH — denial of service (hang + OOM) reachable from malformed
input.

**Fix.** `file()` now bumps a stray `}` directly (rather than relying on
`sync_to_newline`), guaranteeing forward progress every iteration
(`src/parser/items/file.rs`; was `items.rs` before the 2026-06-15 split).

**Test.** `stray_top_level_brace_does_not_hang` (`src/parser/tests.rs`) asserts
a stray `}` yields E1102 and terminates.

**Note.** Found and fixed during the Phase 1.8 grammar-engine work (commit
`e519690`); recorded here because it is the same input-robustness class that
motivated the full audit, and it shares the "must always make progress" lesson
with the `MAX_DEPTH` and overflow fixes in [`security.md`](security.md).

---

## BUG-2 (LOW) — `translate` reskin glued a number onto a Tamil token into unlexable output

**What.** `mimz translate` (the keyword reskin, and `--romanize-names`) could emit
source that no longer lexes. A numeric literal directly abutting a Tamil
keyword/identifier — e.g. `42தொகுதி` or `42கணக்கி`, written with no space —
reskinned to ASCII as `42module` / `42kannakki`, which the lexer rejects (a digit
run followed by letters is an invalid numeric literal). For a Tamil-keyword case
the two tokens silently merged; for the romanize case the output failed to re-lex
outright, breaking the name-map round-trip.

**Cause.** The lexer treats a Latin↔Tamil script change as an implicit token
boundary, so `42தொகுதி` lexes as `42` + `தொகுதி` with no separator between them.
Reskinning the Tamil token to an ASCII spelling erases that script change, and
nothing put a separator back.

**How found.** The 2026-06-15 fuzz/security audit: a deterministic LCG stress
harness over adversarial Tamil + keyword + ASCII input (libFuzzer doesn't build on
Windows) hit it within ~60 cases; reduced to the minimal `42<Tamil>` trigger.

**Severity.** LOW — non-idiomatic input only (real code separates a number from a
following token), no memory/security impact; `translate` returned wrong/`Err`
output for the user's own file. The whole `examples/tamil-pure/` corpus and all
288 tests were unaffected.

**Fix.** A byte-level boundary guard `push_guarded` in `src/translate.rs`, applied
to both the keyword and identifier arms of `reskin`: when a re-emitted,
script-changing token would touch an adjacent ASCII word byte, insert one
separating space. Output stays lexable; such input now round-trips
token-equivalent (gains the space), not byte-identical.

**Test.** `number_abutting_tamil_keeps_a_separator_when_reskinned`
(`tests/translate.rs`); the path is also covered by the new `translate_roundtrip`
cargo-fuzz target (see [`hardening.md`](hardening.md)).
