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

**Follow-up note (2026-06-23).** Building the pure-Tamil stdlib twins hit this
guard from the other direction: `sanitize_verilog_ident` replaces every
**non-ASCII** char with `_`, so an all-Tamil test name collapses to a run of
underscores — two equal-_length_ Tamil names collide regardless of content
(seen on `varisai`/`anuppi`; worked around by rewording to distinct lengths).
The rejection is correct (no broken Verilog), but the failure mode is awkward
for pure-Tamil authors. **Possible improvement (not done):** romanize test
names via the emitter's `romanize` (the same scheme used for identifiers)
instead of underscoring non-ASCII, so a Tamil name yields a readable, content-
distinct module name (`விரியும்` → `viriyum_tb`) rather than `_______tb`.

---

## BUG-5 (LOW) — `translate` romanize glued `0b…?` (MaskedInt) onto a romanized identifier, breaking re-lex

**What.** `mimz translate --romanize-names` could emit source that no longer
lexes. A `0b…?` don't-care binary literal directly abutting a Tamil identifier —
e.g. `match 0b1?ற்றம்` written with no space — romanized to `0b1?rrrram (clk)`,
which the lexer greedily consumed as a single number token: `0b1?rrrram` is not a
valid don't-care pattern → E1004. The same bug affected plain keyword reskin
(e.g. `0b1?மற்றும்` → `0b1?and`).

**Cause.** The `push_guarded` boundary guard in `translate::reskin` uses
`is_word_byte` to decide when to insert a separating space. `is_word_byte`
covered ASCII alphanumeric and `_`, but NOT `?`, which is the don't-care
character in `0b…?` patterns (MaskedInt tokens). When the preceding token ended
with `?` and the replacement identifier started with an ASCII letter, no guard
space was inserted — and the re-lexer's number loop consumes ASCII letters as
part of the number.

**How found.** The cargo-fuzz `translate_roundtrip` target (CI fuzz job)
produced a crash input whose romanized output failed the "must re-lex"
postcondition. Logged as CI fuzz crash for `crash-365775e3…`.

**Severity.** LOW — only affects non-idiomatic input with no whitespace between a
`0b…?` literal and an adjacent token; no memory/security impact; all examples
and real code use spacing. The `translate` round-trip would return `Err` for
affected files.

**Fix.** Added `|| b == b'?'` to `is_word_byte` in `src/translate.rs`, so the
guard fires for `?` as it already does for digits, letters, and `_`.

**Test.** `masked_int_q_does_not_glue_onto_romanized_identifier` and
`masked_int_q_does_not_glue_onto_english_keyword`
(`src/translate.rs`).

---

## BUG-6 (FIXED) — Simulator left-shift truncates the result to the left operand's width, so `1 << n` evaluates to 0

**What.** In the event-driven simulator / interpreter (`mimz sim`, `mimz eval`,
`mimz test`), a left-shift evaluates to the wrong value — usually `0` — whenever
the shifted bits move past the **left operand's** bit width. Minimal repro:

```mimz
module Shl {
  out a: bits[8]
  out b: bits[8]
  a = 1 << 2   // sim says 0; correct is 4
  b = 8 << 1   // sim says 0; correct is 16
}
```

`mimz eval` reports `a = 0`, `b = 0`. The **emitted Verilog** (`assign a =
(1 << 2)`) computes `4`/`16` correctly, and the **checker's** const-evaluator
also folds correctly (it rejects `255 << 2` as `1020` overflowing `bits[8]`). So
the same expression has **three interpretations**, and only the simulator is
wrong — a kernel/Verilog/checker divergence.

**Cause.** `binary()` in `src/sim/value.rs` lowers `BinOp::Shl` (and `Shr`) with
the result width set to **`l.width`** — the left operand's width:

```rust
BinOp::Shl => Val::new(l.bits.checked_shl(r.bits as u32)…, l.width, l.signed),
```

`Val::new` masks `bits & mask(width)`. An unsized integer literal carries its
**minimal** width (`1` is 1 bit, `8` is 4 bits), so `1 << 2 = 4` is masked by
`mask(1) = 1` → `4 & 1 = 0`; `8 << 1 = 16` is masked by `mask(4)` → `16 & 15 =
0`. The shifted-in high bits are discarded before the value is ever used in a
wider context (e.g. an 8-bit assignment). This is **distinct** from the
2026-06-20 fix, which only guarded the shift _amount_ (`r.bits >= 128`); the
_result-width_ truncation remains.

**How found.** Writing the stdlib FIFO (`examples/.../std/fifo.mimz`, 2026-06-23)
with `mem data: bits[W][1 << AW]` and `full = count == (1 << AW)`. The guard
`count != (1 << AW)` was always false (`1 << AW` evaluated to 0), so pushes never
fired; `mimz test` _passed_ its empty/full assertions only trivially (`full =
count == 0` with `count` stuck at 0). Reduced to the literal-only repro above,
which removes the parameter and still fails — so it is not parameter-specific.

**Severity.** HIGH — silent miscompute in the simulator. Any design that
left-shifts a small/unsized value into a wider result simulates wrong, and
because `mimz test` shares this evaluator a buggy assertion can pass _trivially_
(false green). The Icarus differential (`tests/icarus.rs` layer 3) would catch
it, but only for examples explicitly listed there, and no shift-heavy example is
in that hardcoded list.

**Workaround removed.** The FIFO (`examples/.../std/fifo.mimz`) was reverted from
the 3-param design (`WIDTH` + `AW` + `DEPTH`) back to a clean 2-param design
(`WIDTH`, `AW`) using `1 << AW` for the mem depth and the full comparison — the
fix makes the `<<` expression evaluate correctly so the workaround is unnecessary.

**Fix.** `Shl` was given the lossless-growth treatment (`(l.width + shift).min(128)`)
so the high bits survive into the mask, then the normal assignment-width check
applies (`src/sim/value.rs`).

**Test.** A new shift example (`examples/english/shift.mimz`) was added to the
`tests/icarus.rs` differential list (and registered in `BASE_EXAMPLES`/`PURE_TAMIL`
with its pure-Tamil twin `tamil-pure/nakartthi.mimz`), and a unit test
(`shl_does_not_truncate_to_left_operand_width`) was added to `src/sim/value.rs`.

---

## BUG-7 (FIXED) — Simulator `eval_fn_call` masks arguments without sign-extending

**What.** When passing a negative signed value to a function, the simulator loses the sign extension if the parameter width is wider. For example, passing `-128` (as `signed[8]`) to a function expecting `signed[16]` evaluates to `+128` rather than `-128`.

**Cause.** In `src/sim/value.rs`, `eval_fn_call` binds arguments using `Val::new(val.bits, w, s)`. This function applies the bit-mask of the parameter's width, but it fails to sign-extend the caller's value first based on its original signedness and width.

**How found.** User encountered it while implementing PID saturation where a `fn clamp` evaluated incorrectly for negative numbers.

**Severity.** HIGH — Silent miscompute in the simulator for negative numbers passed to functions.

**Workaround (no longer needed).** Inline the `min`/`max` logic or use built-ins (which handle sign-extension correctly) instead of using a user-defined function.

**Fix.** Factored the `Builtin::Extend` arm's sign-extension logic (replicate
the sign bit into the new high bits when widening a negative signed value)
into a shared `extend_bits` helper, and applied it in `eval_fn_call`'s two
argument-binding sites (scalar and array-element params) in place of the
naive `Val::new(val.bits, w, s)` (`crates/mimz-sim/src/sim/value.rs`).

**Test.** `fn_call_sign_extends_narrower_signed_arg_to_wider_param`
(`crates/mimz-sim/src/sim/value.rs`).

---

## BUG-8 (FIXED) — Simulator errors on bit-indexed register assignment

**What.** The parser and AST support bit-indexed register assignment (e.g., `shift[bit_idx] <- rx`), but the simulator rejects it.

**Cause.** In `src/sim/kernel.rs`, the `SeqStmt::Assign` evaluation explicitly returns an error: `"assigning a slice/bit of <name> is not supported by the simulator yet"`.

**How found.** User tried to implement a UART receiver echo shift register and encountered the simulator error.

**Severity.** MEDIUM — Missing simulator feature.

**Workaround (no longer needed).** Use a full-register assignment with bitwise shifts and masks, e.g., `shift <- (shift >> 1) | (rx << 7)`.

**Fix.** A plain (non-array, non-mem) bit/slice index or slice bound must
already be a compile-time constant on the READ path (`value::eval`'s
`Index`/`Slice` arms use `const_eval`), so the write path needs no
runtime-index handling either — it reads the base register value (chained
through `next` first, so two disjoint-bit writes to the same register in
one `on` block combine instead of the second clobbering the first), patches
the constant-indexed bit or slice, and writes the merged whole value back
(`crates/mimz-sim/src/sim/kernel.rs`). `checked_index` was widened from
private to `pub(super)` to share it with the read path's existing helper.

**Test.** `bit_indexed_register_write_sets_one_bit`,
`slice_indexed_register_write_sets_a_range`, and
`disjoint_bit_indexed_writes_in_one_on_block_combine`
(`crates/mimz-sim/src/sim/kernel.rs`).

---

## BUG-9 (FIXED) — Two `fn`-body `let` bindings with the same name emit two conflicting Verilog `reg` declarations

**What.** A `fn` body that binds the same name twice via `let` at different
points (e.g. `let acc = 0` followed later by a shadowing `let acc = acc +% 1`,
including one inside a `loop`/`foreach` body re-binding a name declared
outside it) emits **two** `reg <width> <name>;` declarations for the same
identifier. Real Verilog rejects this outright (`iverilog`: `'<name>' has
already been declared in this scope.`).

**Cause.** `crates/mimz-core/src/emit_verilog/module.rs`'s `fn_all_locals`
collects one `LocalLet` entry per source-level `FnStmt::Let` node with no
dedup/rename by name, and the `reg` emission loop blindly emits one line per
entry.

**How found.** While writing the `examples/*/foreach_sum.mimz` example
(`foreach`, 2026-07-12): a natural "seed then re-bind inside the loop"
accumulator idiom (`let acc = 0` before `foreach`, `let acc = acc +% v`
inside it) hit this. Reproduced minimally, with no `foreach`/loop involved
at all: `fn bump(a: bits[8]) -> bits[8] { let x = a; let x = x +% 1; x }`
produces the same double-`reg x` output. Confirmed pre-existing (predates
`foreach`) and unrelated to the `foreach` feature itself.

**Severity.** MEDIUM — silently produces Verilog that a real toolchain
rejects; no compiler-side diagnostic warns the user before emit.

**Workaround (no longer needed for the same-width case).** Avoid re-binding
a `let` name inside a nested scope in a `fn` body; thread an accumulator
through as an extra parameter instead (fold-style) — this is what
`foreach_sum.mimz` does (and continues to do — same-width shadowing is the
supported pattern, not a workaround, after this fix).

**Fix.** Two-part, since a shadow at a genuinely DIFFERENT width can't
safely share one Verilog `reg` declaration (only same-width shadowing can):
(1) a new checker rule in `check_fn_stmt_widths`'s `FnStmt::Let` arm
(`crates/mimz-core/src/checker/widths/mod.rs`) rejects re-binding a name —
an earlier `let` in the same straight-line body, or a `fn` parameter — at a
different width, as new code **E0813**; (2) `render_fn_decl`'s reg-emission
loop (`crates/mimz-core/src/emit_verilog/module.rs`) now dedupes by name
(seeded with the scalar param names), skipping a second `reg` declaration
for a name it already declared — safe now that E0813 guarantees any
surviving shadow keeps the same width. `ALL_CHECKER_CODES` (`src/diag.rs`)
and the long-form explanation (`src/explain.rs`) were updated to match, and
the two goldens carrying the old duplicate-`reg` output for the workaround's
own variant (`tests/golden/foreach_sum.v`, `tests/golden/tamil_pure_kootu.v`)
were regenerated — the duplicate `reg [10:0] acc;`/`reg [10:0] thokai;`
lines are gone, since `acc`/`thokai` are now recognized as already declared
via their `input` param.

**Test.** `e0813_fn_let_shadow_width_mismatch`,
`fn_let_shadow_same_width_stays_clean`, and
`fn_let_shadowing_a_param_at_a_different_width_is_e0813`
(`crates/mimz-core/src/checker/tests.rs`), plus fixture
`tests/fixtures/errors/e0813_fn_let_shadow_width_mismatch.mimz`.

**Note — the workaround's own variant, closed by this fix too.**
`foreach_sum.mimz` reuses the param name itself (`let acc = acc +%
extend(v, 11)` inside the loop, rebinding the `acc` parameter), so its
golden used to emit both `input [10:0] acc;` and `reg [10:0] acc;` for the
same name — `fn_all_locals` didn't dedupe a synthesized `Let` against an
existing `FnParam` name either, only against other `Let`s. `iverilog`
tolerated this specific shape (input-then-reg-redeclaration, same width)
rather than rejecting it, so the example was never actually broken — but
the fix's param-seeded dedup set closes this variant too: the golden
(`tests/golden/foreach_sum.v`, and its pure-Tamil twin
`tests/golden/tamil_pure_kootu.v`) no longer emits the redundant `reg` line.

---

## BUG-10 (MEDIUM) — Bundle-typed `fn` params/returns never flatten in emitted Verilog

**What.** A bundle-typed `fn` parameter or return type is not flattened to
one Verilog port per field the way module ports and wires are. A bare
(non-parametric) bundle name used as a `fn` param/return type **hard-errors**
at emit time (`"unknown type 'X' — not a built-in and not a declared enum"`).
The parametric form (`Bundle(W: N)`) doesn't hard-error, but silently emits
**invalid Verilog**: the function is declared with one unflattened
`input u;` instead of `input u_tx; input u_rx;`, a call site passes a single
argument (`pick_tx(a)`) instead of the flattened fields, and a bundle-typed
`fn` call used as a wire initializer emits the syntactically invalid
`assign b_tx = as_uart(a)_tx;` (a field suffix appended directly to a
function-call expression).

**Cause.** `render_fn_decl` (`crates/mimz-core/src/emit_verilog/module.rs`)
calls `self.width(&decl.ret)` / `self.width(&param.ty)` directly with no
bundle-flatten check beforehand. `width()`'s `Type::Named` arm only
recognizes enums (hence the hard error for the bare form); its `Type::Bundle`
arm silently returns an empty width string instead of flattening (hence the
invalid-but-non-erroring output for the parametric form). Module
ports/wires avoid this because their own emission paths
(`module.rs:60-70`, `130-140`) check bundle-ness and flatten _before_ ever
calling `width()` — `render_fn_decl` has no equivalent check. This
contradicts `spec/02-syntax-and-grammar.md`'s claim that bundle flattening
"applies uniformly to ... `fn` bundle-typed args/returns."

**How found.** While writing emission-equality tests for feature 2.9
(structural interface matching)'s final whole-branch review fix pass
(2026-07-16) — the first tests to exercise a bundle-typed `fn` signature at
the emitter level at all. Unrelated to structural matching itself: nominal
and structurally-matched bundles hit the identical bug identically (both
compile "successfully" via the parametric-form workaround and produce
byte-identical, though invalid, Verilog) — pre-existing, not a regression
introduced by feature 2.9.

**Severity.** MEDIUM — silently produces Verilog a real toolchain rejects
(or a hard compiler error for the bare form) for a scenario the spec
documents as supported; no example or golden currently exercises a
bundle-typed `fn` param/return, so nothing else was silently broken by it.

**Workaround.** None at the language level — avoid bundle-typed `fn`
params/returns until fixed; pass individual fields instead.

**Fix.** Not yet fixed — filed as a follow-up. Likely shape: give
`render_fn_decl` the same bundle-flatten-before-`width()` check the module
port/wire paths already have, plus a `FnCall` RHS case in the "RHS is a
signal" branch (`module.rs:762-770`) so a bundle-returning call's per-field
uses resolve to `<call>_<field>` correctly instead of literal string
concatenation.

**Test.** None yet (bug is open). The two emission-equality tests that
surfaced it (`structurally_matched_fn_arg_emits_same_as_nominal_match`,
`structurally_matched_fn_return_emits_same_as_nominal_match`,
`crates/mimz-core/src/emit_verilog/mod.rs`) route around the hard-error path
via a dummy `W: int = 1` bundle param and assert only that structural vs.
nominal bundle naming doesn't change the (still-invalid) emitted output —
they do not assert the output is _correct_ Verilog, since it isn't yet.
