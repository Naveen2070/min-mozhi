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

---

## BUG-3 (HIGH) — `--emit-testbench` dropped module parameter defaults, breaking width resolution

**What.** `mimz compile --emit-testbench` generates a Verilog testbench from
inline `test` blocks. The `test_env` used to resolve width expressions (e.g.
`bits[W]`) for the DUT instance was built only from the test's own explicit
`(NAME: expr, …)` args — any module parameter with a declared `default` that
the test didn't re-pass was simply absent from `test_env`, so a width
expression referencing it failed to resolve.

**Cause.** The loop building `test_env` in `emit_testbench` only walked
`test.args`; it never consulted the DUT's `params` for ones the test omitted.
Every other parameter-resolution path in the compiler
(`sim::elaborate::elaborate_module`, `sim::harness::params`) merges in the
module's own defaults for anything not explicitly overridden — the testbench
emitter was the one place that didn't.

**How found.** 2026-06-21 review of the testbench emitter added by the
`--emit-testbench` feature (commit `a27b12c`).

**Severity.** HIGH — any test for a parameterized module that relies on a
default (the common case — that is what defaults are for) fails to emit a
testbench at all.

**Fix.** After resolving explicit `test.args` into `test_env`, walk
`dut.params` and fill in any parameter not already present from its
`default` expression (evaluated against the args already bound) — same
order/semantics as `elaborate_module` (`src/emit_verilog/testbench.rs`).

**Test.** `test_env_falls_back_to_module_param_defaults`
(`src/emit_verilog/testbench.rs`).

---

## BUG-4 (MEDIUM) — Test names sanitizing to the same Verilog identifier silently collided

**What.** `--emit-testbench` names each generated testbench module by
sanitizing the test's free-text name (`sanitize_verilog_ident`) and appending
`_tb`. Two differently-named tests can sanitize to the same identifier — e.g.
`"edge case"` and `"edge_case"` both become `edge_case_tb` — which silently
emitted two `module edge_case_tb` blocks into the same output file: invalid
Verilog (duplicate module definition), with no diagnostic pointing at the
cause.

**Cause.** Test names are free-text and were never checked for
post-sanitization uniqueness anywhere upstream (the checker validates
module/signal identifiers, not test-block names).

**How found.** 2026-06-21 review of the testbench emitter added by the
`--emit-testbench` feature (commit `a27b12c`).

**Severity.** MEDIUM — produces broken output rather than a crash, but fails
silently (no compiler error) until the generated file is fed to a Verilog
toolchain.

**Fix.** Track sanitized testbench names seen so far in a `HashMap`; on a
collision, push a diagnostic naming both colliding test names and the shared
identifier instead of emitting the second module
(`src/emit_verilog/testbench.rs`).

**Test.** `colliding_sanitized_test_names_are_rejected`
(`src/emit_verilog/testbench.rs`).
