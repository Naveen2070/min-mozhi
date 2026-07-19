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

## BUG-10 (MEDIUM, params FIXED 2026-07-16 / returns diagnostic FIXED 2026-07-18, real fix still pending) — Bundle-typed `fn` params/returns never flatten in emitted Verilog

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

**Workaround.** None at the language level for the still-open return case —
avoid bundle-typed `fn` returns until the real fix (call-site inlining)
lands; pass individual fields back instead. A bundle-typed return is now at
least a clean compile-time diagnostic instead of invalid output (see "Fix —
returns, diagnostic" below). (The param case needs no workaround anymore —
fixed below.)

**Fix — params (2026-07-16, this fix).** `render_fn_decl`'s param loop
(`crates/mimz-core/src/emit_verilog/module.rs`) now flattens a
bundle-typed (`Type::Bundle` or `Type::Named` resolving to a bundle)
parameter to one `input` per field, resolved via the existing
`resolve_bundle_fields` — same convention module ports/wires already use.
The `ExprKind::FnCall` call-site arg-expansion (`crates/mimz-core/src/
emit_verilog/expr.rs`) now expands a bundle-typed argument the same way: by
the **callee's declared param field names** (not the argument's own bundle
type), which is what makes this correct under structural matching (feature
2.9) — a differently-named-but-structurally-compatible argument still
resolves to the right `<arg>_<field>` wires, since flattened signal names
are always keyed by field name, never by a bundle's internal declaration
order. No change was needed in the function body's own codegen — `expr.rs`'s
generic `Field` arm (`x.y` → `x_y`) already assumed flattened names existed;
only the port declaration and the call-site argument list were missing the
flatten step. Verified against the exact repro in "What" above: `pick_tx`
(bit-returning, bundle-typed param only) now emits fully correct Verilog
end-to-end.

**Fix — returns, real fix (still pending).** NOT the same kind of fix as
params. A Verilog `function` can only return **one** value — there is no
Verilog syntax for a function to return multiple named outputs, so
"flatten the return type" the way params/ports do isn't applicable here at
all. Supporting a bundle-typed `fn` return for real needs a different
codegen strategy (inlining the function body at each call site instead of
emitting a real Verilog `function` call) — filed as a separate, larger
feature idea, not a bug-fix continuation. Already tracked in
`docs/plan/phase-2-ir-synthesis.md`'s language-features backlog
("Bundle-typed `fn` return via inlining") — confirmed present there
2026-07-18, not duplicated.

**Fix — returns, diagnostic (2026-07-18, interim — per
`docs/plan/phase-2-correctness-consolidation.local.md` Stage 1).** Until
the real fix above lands, a bundle-typed `fn` return is now a clean
compile-time diagnostic instead of either the bare form's confusing
"not a declared enum" hard error or the parametric form's silent invalid
Verilog. Fixed at the EMITTER level, not the checker: an earlier attempt to
reject this in the checker (`check_func_body_widths`) was reverted before
landing — feature 2.9's structural interface matching already has full,
deliberately-built, tested support for bundle-typed `fn` returns at the
checker level (`check_return_ty`'s `BundleShapeMatch` handling, E0910/E0804
etc.); rejecting there would have broken that legitimate, independently
valuable validation, not fixed BUG-10. The real gap is narrower: only
`width_subst` (`crates/mimz-core/src/emit_verilog/module.rs`) — reached
**exclusively** via `render_fn_decl`'s `let ret_w = self.width(&decl.ret)`,
since every other caller (module ports/wires, `fn` params) flattens a
bundle to per-field signals before ever calling `width()`/`width_subst()`
— doesn't know what to do with a bundle-typed return. Its `Type::Bundle`
arm now reports a real diagnostic (was: silent `String::new()`); its
`Type::Named` arm's bundle-resolving branch does too, replacing the
misleading "not a declared enum" message the bare form used to fall
through to. `mimz check` still accepts a bundle-typed `fn` return cleanly
(the checker's own view is unchanged and correct); `mimz compile` now
rejects it with a clear message — the same check-vs-compile split that
already existed for the bare form, now consistent for both forms.

**Test.** `bundle_typed_fn_param_flattens_to_per_field_inputs`
(`crates/mimz-core/src/emit_verilog/mod.rs`) — asserts the exact flattened
port declarations, body reference, and call-site expansion for both a bare
and a parametric bundle-typed param on the same `fn`.

**Test.** `bare_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog`,
`parametric_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog`
(`crates/mimz-core/src/emit_verilog/mod.rs`) — both forms now assert a
`Diag` mentioning "cannot return a bundle-typed value", not successful
emission.
`structurally_matched_fn_return_is_a_diagnostic_same_as_nominal_match`
(same file, repurposed from `..._emits_same_as_nominal_match`, which used a
dummy `W: int = 1` param to sidestep the old hard-error path and compare
byte-identical-but-invalid Verilog between nominal/structural bundle
returns — that workaround no longer works now that BOTH forms are rejected,
so there's no output left to compare; repurposed to the still-meaningful
invariant it was really pinning: nominal and structurally-matched bundle
returns get the IDENTICAL diagnostic, neither dodges it).

## BUG-11 (CRITICAL, FIXED 2026-07-18) — Simulation vs. Synthesis Mismatch on Left Shift (`<<`)

**What.** The simulator evaluates left shifts by dynamically expanding the width of the result based on the shift amount. The expression `a << 2` is evaluated with `w = (a.width + 2).min(128)`, carrying extra bits into subsequent operations.

**Cause.** `sim/value.rs` (`BinOp::Shl`) intentionally grows the width of the `Val` returned. The originally-filed cause statement above ("the checker correctly specifies that shifts preserve the left operand's width, matching standard Verilog") turned out to be **only half right** — see the Fix note below for what the CTO review's own prescribed fix got wrong, and why.

**How found.** CTO Architectural Review (July 2026) inspecting the fix for shift-amount truncation (cbcefd0).

**Severity.** CRITICAL — Causes simulation to behave differently than synthesized hardware. Intermediate calculations will silently carry overflow bits in simulation that will be truncated in the actual synthesized netlist.

**Fix (2026-07-18).** The review's own prescribed fix ("truncate/wrap the result to `l.width` unconditionally") was tried first and **empirically disproven** against `iverilog` before landing: `din << 2` for `din: bits[4]` assigned to an 8-bit target computes **28** in real Verilog (context-extends `din` to 8 bits before shifting), not **12** (truncating to `din`'s own 4-bit width first). Ground truth:

```
din=7 (bits[4]) << 2  →  8-bit target: 28 (0001_1100)   4-bit target: 12 (1100)
```

Verilog's `<<`/`>>` are **context-determined** on their left operand (the shift amount is always self-determined) — the operand widens to the ENCLOSING width (an assignment target, `extend`'s target width) BEFORE the shift, not truncated-then-extended after. The checker's own `shift_ty` rule ("width preserved") is not wrong — it's a static TYPE-system invariant (the shift's declared type for downstream compatibility checks), separate from the runtime VALUE Verilog actually computes when that type flows into a wider context via an explicit `extend()`. Neither "grow by the shift amount" (the original BUG-6 fix, wrong on BUG-11's own `(a << 2) >> 2` chain) nor "always truncate to `l.width`" (the review's fix, wrong on the `din << 2` case above) match Verilog in general — only threading the real context width through does.

Implemented in `crates/mimz-sim/src/sim/value.rs`: `eval`/`binary` gained a context-aware sibling (`eval_ctx`/`binary_ctx`) taking an `expected_width: Option<u32>`, used only by `Shl`/`Shr` (every other operator's width rule is unchanged — deliberately scoped, see `docs/plan/phase-2-correctness-consolidation.local.md` Stage 1). `if`/`match` propagate the same `expected_width` into every branch (Verilog's ternary/case are likewise context-determined), so a shift nested in a branch still sees the real target width. Callers with a known target width now pass it in: `comb.rs`'s combinational driver resolution, `kernel.rs`'s register/default/memory-cell writes, `Builtin::Extend`'s argument (using `extend`'s own target width — the exact site that exposed the `din << 2` case), and `FnStmt::Let` (reusing the checker's existing `inferred_width`, the same mechanism that already re-masked post-hoc). Callers with no meaningful target width (conditions, indices, loop bounds) pass `None`, matching Verilog's self-determined rule for those positions.

**Test.** `shl_self_determined_preserves_left_operand_width`, `shl_widens_to_context_like_verilog`, `shl_chain_stays_at_shared_context_width` (`crates/mimz-sim/src/sim/value.rs`) — the middle one pins the `din << 2` ground-truth case above, the last pins the review's own `(a << 2) >> 2` reproduction (63, not 255). The pre-existing Icarus differential (`tests/icarus.rs`, `english/shift.mimz`'s `var_shift`) also now agrees with real Icarus end-to-end (it did not before this fix — the differential sweep hits `din` values the example's own static `test` block, using `din = 3`, never exercised).

## BUG-12 (MEDIUM, re-filed 2026-07-18) — `fn` cannot be parameterized by module scope (consistent design limitation, not a divergence)

**Re-filing note (2026-07-18).** Originally filed HIGH as "broken
parameterization... breaking standard Verilog lexical scoping," implying an
emitter-only bug. The 2026-07-17 CTO review (§4.2) verified this is wrong:
**the checker rejects the same construct too** — a module-const reference
inside a `fn` body fails `mimz check` with **E0101** ("unknown name"),
reproduced and re-confirmed 2026-07-18. Checker and emitter **agree**: `fn`
is a file-scoped construct that sees file-level consts and its own params,
never module scope. That is a consistent language-design limitation, not a
checker/emitter divergence and not "broken" scoping — downgraded HIGH → MEDIUM
and reframed accordingly. The original **What**/**Cause** below are kept for
history; the corrected framing is in **Severity** and **Fix**.

**What.** Functions (`fn`) cannot access the enclosing module's
constants/parameters — by design, consistently enforced by both the checker
and the emitter (not an emitter-only gap).

**Cause.** In `emit_verilog/module.rs` (`render_fn_decl`), the emitter
replaces the environment with `file_env` (stripping module consts) to
prevent shadowing function parameters — and the checker's own name
resolution (E0101 on a module-const reference from inside a `fn` body)
enforces the identical file-scoped-only rule ahead of emission. File-level
consts in `fn` bodies work fine
(`examples/english/fn_with_const.mimz` demonstrates exactly this) — only
module-scoped consts/params are unreachable from a `fn` body, in both
passes.

**How found.** CTO Architectural Review (July 2026); severity/framing
corrected by the same review's own §4.2 after checking the checker's actual
behavior, not just the emitter's.

**Severity.** MEDIUM — a real, workaroundable language-design gap (pass the
value as a `fn` parameter, or hoist the const to file level), not a
divergence bug and not broken lexical scoping. Module-parameterized helper
functions are inexpressible today; that is a feature gap, not a defect two
passes disagree on.

**Fix (Deferred — open, tracked as a feature, not a bug to close).** Not a
symbol-table bugfix — a language-design decision: either bless file-scoping
explicitly in `spec/02-syntax-and-grammar.md` (document the limitation as
intentional), or design deliberate module-scope capture for `fn` (a real
feature, needs its own spec section covering how a `fn`'s width/const
resolution would interact with the module's own parametric instantiation).
The emitter's current `file_env` swap (`emit_verilog/module.rs`) is correct
as-is either way — it already matches the checker; nothing to fix there
until the spec decision is made. **2026-07-18 decision:** left open and
deliberately deferred (not folded into the current correctness-consolidation
work) — tracked as a feature idea in
[`docs/Ideas/language_plan.md`](../Ideas/language_plan.md) §12, revisit once
that work lands.

## BUG-13 (MEDIUM) — 128-bit Simulator Ceiling

**What.** The simulator cannot handle vectors larger than 128 bits.

**Cause.** The simulator's `Val` struct is hardcoded around Rust's `u128`. Operations like shift yield `0` if `shift >= 128`.

**How found.** CTO Architectural Review (July 2026).

**Severity.** MEDIUM — Modern digital design routinely utilizes buses of 256, 512, or 1024 bits. This hard limit causes silent data corruption for wider memory buses.

**Fix (Pending).** Transition `Val` to a dynamic arbitrary-precision integer backend (e.g. `BigUint`).

## BUG-14 (MEDIUM, FIXED) — `mimz-sim` never registered the `__Valid`/`__ValidSigned` builtin bundles

**What.** Any `bit?`/`bits[N]?`/`signed[N]?`-typed wire or reg was
completely broken in the simulator: elaboration failed with an "unknown
bundle `__Valid`" error the moment a `?`-sugar-typed signal was touched,
even though the same code checked cleanly and emitted correct Verilog.

**Cause.** `bit?`/`bits[N]?`/`signed[N]?` desugar at parse time to a
reference to one of two compiler-synthesized bundle declarations
(`ast::builtin_valid_bundles`, `__Valid`/`__ValidSigned` — never present in
source, so both the checker and the emitter register them into their own
bundle tables at startup: `checker/symbols.rs` and `emit_verilog/mod.rs`
both call `ast::builtin_valid_bundles()` and insert the result into their
bundle registry alongside every real `bundle` declaration in the project).
`mimz-sim`'s elaborator (`sim/elaborate.rs`, `build_bundle_registry`) builds
its own, separate bundle registry from the parsed AST — it never got the
equivalent call, so it only ever knew about user-declared bundles. This
predates the `?`/`??` feature; it was a latent gap from whenever
`builtin_valid_bundles` was first introduced, invisible until something
actually tried to simulate a `?`-sugar-typed signal.

**How found.** `?`-sugar valid-bundle feature (this feature's) Task 9 —
the first work item to exercise a `?`-sugar-typed signal through the
simulator at all.

**Severity.** MEDIUM — total, unconditional failure for the affected
signal shape in the simulator only (checker and emitter were always fine);
no example/golden exercised a `?`-sugar-typed signal in `mimz test`/`mimz
eval` before this feature, so nothing else was silently broken by it.

**Fix (2026-07-17, Task 9 of the `?`/`??` feature).**
`build_bundle_registry` (`crates/mimz-sim/src/sim/elaborate.rs`) now also
registers `ast::builtin_valid_bundles()`, under a synthetic file index
(`files.len()`, one past every real file) — mirroring the existing
checker/emitter convention exactly. No other elaborator change was needed;
once registered, `__Valid`/`__ValidSigned` resolve through the same
bundle-lookup path any user bundle already used.

**Test.** Task 9's simulator unwrap-form tests (`crates/mimz-sim/src/sim/
elaborate.rs`) exercise a `bit?`/`bits[N]?`-typed wire end-to-end through
the simulator; they would fail with the pre-fix "unknown bundle" error.

## BUG-15 (MEDIUM, OPEN) — `mimz-sim` has no bundle-field-expansion baseline for instance ports or `fn` call arguments

**What.** A bundle-typed module-instantiation port connection or a
bundle-typed `fn` call argument is completely unsupported in the
simulator — `mimz-sim`'s `flatten_instance` and its `fn`-call argument
handling (`crates/mimz-sim/src/sim/elaborate.rs`) have no bundle-field
expansion at all for these two sites, unlike `mimz-core`'s emitter, which
already flattens a bundle-typed value at both (plus wire-init and
`Drive`) as a pre-existing baseline.

**Cause.** The simulator's bundle support grew incrementally, site by
site, and never reached instance-port connection or `fn`-argument passing
— those two sites still expect a plain scalar value where a bundle-typed
one is given.

**How found.** `?`-sugar valid-bundle feature's Task 10 (simulator `??`
OR-mux form): OR-mux needed a per-field extraction helper at every site a
bundle-typed value can reach. `mimz-core`'s emitter has that baseline at
four sites (wire-init, `Drive`, port connection, `fn`-call argument) and
Task 8 extended all four; `mimz-sim` only had wire-init and `Drive` to
extend — probing an instance port or `fn` argument with a plain (non-`??`)
bundle-typed value confirmed both are unsupported today, independent of
`??` entirely.

**Severity.** MEDIUM — a real capability gap (not a regression, and not
introduced by `??`), but narrow: no example/golden passes a bundle-typed
value to an instance port or `fn` call in the simulator today, so nothing
currently relies on it. `??`'s OR-mux form does not support these two
sites in the simulator as a direct, scoped-out consequence (`§1.12a`
correctly does not list them as supported combinations); `mimz-core`'s
emitter is unaffected and supports OR-mux at all four sites.

**Fix (Pending).** Give `mimz-sim` the same foundational bundle-field-
expansion baseline `mimz-core`'s emitter already has for instance ports
and `fn` call arguments, then `??`'s OR-mux form (or any other bundle-
typed value) can reach those two sites the same way it already reaches
wire-init and `Drive`. Filed as a follow-up, not part of this feature's
scope.

**Test.** None yet (gap is open, pre-existing, and out of this feature's
scope) — Task 10's probe tests confirmed the gap empirically but were not
committed as permanent regression coverage for a known-unsupported path.

## BUG-16 (MEDIUM, FIXED 2026-07-18) — `mimz-sim` never resolved file-scoped `enum` declarations

**What.** A file-scoped `enum Name { ... }` declared _alongside_ a module
(spec/02 §1.5b — the same tier as `bundle`/module declarations, not nested
inside the module body) crashed `mimz sim`/`mimz eval`/`mimz test` with
`unknown enum type` the moment any signal of that type was touched, even
though the same file checked cleanly with `mimz check` and compiled to
correct Verilog. `examples/english/enum_construct.mimz` — a shipped
example — hit this on every `mimz sim`/`eval` invocation.

**Cause.** `elaborate_module` (`crates/mimz-sim/src/sim/elaborate.rs`)
built its `enums: HashMap<String, &EnumDecl>` lookup **only** from
`ModuleItem::Enum` — enum declarations nested inside the current module's
own body (as `examples/english/traffic_light.mimz`'s `enum State { ... }`
does). It never scanned `ast::TopItem::Enum` — a file-scoped enum
declared as a sibling of the module, not inside it — across the loaded
project, unlike `func_reg`/`bundle_reg` (both already built project-wide
via `build_func_registry`/`build_bundle_registry` and threaded through
`elaborate_module`/`flatten_instance`). The checker's own enum table
(`checker/mod.rs`, `HashMap<String, Vec<(usize, &EnumDecl)>>`) already
covers both declaration positions correctly — this was a simulator-only
gap, invisible until an example used the file-scoped form instead of the
module-nested one (every enum-using example prior to this audit happened
to nest its enum inside the module).

**How found.** Stage 3 (T1, differential-testing consolidation,
`docs/plan/phase-2-correctness-consolidation.local.md`) — adding a layer-3
Icarus differential test for `enum_construct.mimz` (previously uncovered
by any semantic differential, only layer-1 validity) hit `unknown enum
type Packet` on the very first `mimz sim` run, despite `mimz check`
passing clean. Exactly the "checker accepts it, simulator can't run it"
divergence class BUG-6/BUG-11/BUG-14 are all instances of.

**Severity.** MEDIUM — total, unconditional failure for the affected
declaration shape (module-nested enums were always fine; checker and
emitter were always fine), but every enum-using example prior to this
audit happened to avoid it by nesting the enum inside the module, so
nothing else was silently broken by it.

**Fix (2026-07-18).** Added `EnumRegistry`/`build_enum_registry`
(`crates/mimz-sim/src/sim/elaborate.rs`), mirroring `FuncRegistry`/
`build_func_registry` exactly: scans `ast::TopItem::Enum` across every
loaded file, built once in `elaborate_project_with_mode` and threaded
through `elaborate_module`/`flatten_instance` (the same plumbing path
`func_reg`/`bundle_reg` already use). `elaborate_module`'s local `enums`
map now seeds from this project-wide registry, then overlays any
module-nested `ModuleItem::Enum` (module-local wins on a name clash).
Not a full per-file multimap with `a.b.Name` qualifier resolution like
the checker's own enum table — a checker-clean program (gated before
every sim path since A2) never reaches sim with a genuine cross-file
enum-name ambiguity, so a flat name→decl map is sufficient in practice.

**Test.** `tests/icarus.rs`'s `our_simulator_matches_icarus_bit_for_bit`
now differentials `english/enum_construct.mimz` (layer 3 — kernel == VCD
== Icarus, bit-for-bit); would fail with the pre-fix "unknown enum type"
error. Also surfaced (and fixed in the same pass) that
`differential_m`/the test harness itself never ran `checker::check` before
`elaborate_project` — needed for `Packet`'s `inferred_total_width` Cell
(a genuinely payload-bearing tagged enum) to be populated, matching what
every real `mimz sim`/`test` invocation does since A2.

## BUG-17 (MEDIUM, FIXED 2026-07-19) — Simulator rejects a combinational slice-indexed drive (`sig[hi:lo] = expr`)

**What.** Driving a **slice** of a wire/output combinationally —
`lamps[i*8+7 : i*8] = i*2`, `examples/english/foreach_fill.mimz`'s actual
line — is rejected by both simulator entry points: `mimz sim`/`test`
(`crates/mimz-sim/src/sim/elaborate.rs`) with "driving a slice of `lamps`
is not supported by the simulator yet", and `mimz eval`
(`crates/mimz-sim/src/sim/comb.rs`) with "driving a slice of `lamps` is
not supported by the evaluator yet". The parser, checker, and Verilog
emitter all fully support it — `mimz compile` emits a correct, valid
indexed part-select assignment; only Min-Mozhi's own simulator/evaluator
can't run a design that uses it. **Not the same gap as BUG-8** (FIXED):
BUG-8 covers a **sequential** (`<-`, inside `on rise`/`fall`) slice write
to a register, which works fine
(`slice_indexed_register_write_sets_a_range`,
`crates/mimz-sim/src/sim/kernel.rs`). This is specifically a
**combinational** (`=`) slice drive on a wire/output/port.

**Cause.** `elaborate.rs::record_drive` (the elaborator behind `mimz
sim`/`test`) handles a whole-signal drive (`lhs.index == None`) and a
single-bit-indexed drive (`Some((idx, None))`, collected per-bit into
`bit_drives` and reassembled as a `Concat`), but its third arm,
`Some((_, Some(_)))` — an actual range/slice — just returns an error;
nothing assembles a partial-slice `Concat` the way the bit-indexed arm
does. `comb.rs`'s lightweight single-file evaluator (behind `mimz eval`)
is even more restrictive: its `ModuleItem::Drive` handling rejects **any**
indexed drive at all via a blanket `lhs.index.is_some()` check — so it
also rejects the single-bit-indexed case `elaborate.rs` already supports,
not just slices (its error message says "a slice", which is the common
case but not the literal condition it checks).

**How found.** Stage 3 (T1, differential-testing consolidation,
`docs/plan/phase-2-correctness-consolidation.local.md`) — adding layer-3
Icarus differential coverage for `foreach_fill.mimz` (previously
layer-1-only) hit this immediately; excluded from that pass rather than
folded in, since fixing it is a simulator-kernel change, not a test
addition.

**Severity.** MEDIUM — a real capability gap in both simulator entry
points (not a crash, not silent miscompute — errors cleanly), but narrow:
only one shipped example (`foreach_fill.mimz`) currently uses a
combinational slice drive, so nothing else is silently affected. Blocks
`mimz sim`/`mimz eval`/`mimz test` on any design using this otherwise
fully-supported (parser/checker/emitter) construct.

**Fix (2026-07-19).** `record_drive` (`elaborate.rs`) now handles
`Some((hi, Some(lo)))` by expanding it to one `bit_drives` entry per bit
position — `sig[hi:lo] = rhs` behaves exactly like writing
`sig[hi] = rhs[hi-lo]; …; sig[lo] = rhs[0];` one bit at a time — reusing
the existing `bit_drives`-then-`Concat` assembly path unchanged, no
parallel range-aware structure needed. `comb.rs`'s `eval_outputs` got the
same treatment (its `ModuleItem::Drive` handling now matches on
`lhs.index` the same three ways instead of a blanket
`lhs.index.is_some()` rejection), plus its own `bit_drives`/`drivers`
plumbing to assemble the Concat — `drivers` had to change from
`BTreeMap<String, &Expr>` to an owning `BTreeMap<String, Expr>` since the
synthesized per-bit `Index`/`Concat` nodes are new, not borrowed from the
original AST.

One real subtlety found while fixing it: naively reconstructing each
target bit as `rhs[bit_position]` (indexing into the RAW rhs expression)
is wrong whenever `rhs` is compile-time-constant — `foreach_fill.mimz`'s
own `lamps[i*8+7:i*8] = i*2` becomes, after the `foreach` unroll
substitutes a literal for `i`, a constant-foldable RHS (e.g. `3 * 2`) —
and a constant RHS _adapts_ to the slice's declared width the same way
any other `Ty::CtInt` assignment does (the checker's `fit` check only
verifies the folded VALUE fits, never that the expression's own "natural"
width already equals `hi - lo + 1`). Indexing bit 7 of a value the
evaluator infers as only 4 bits wide panicked-clean with "bit index 7 is
out of range for a 4-bit value" — a real bug in the fix's first draft,
caught by testing against the actual repro before landing. Fixed by
special-casing a constant-foldable RHS: pull each target bit straight
from the folded `i128` value (`(v >> (b - lo)) & 1`, arithmetic shift so
a negative constant sign-extends correctly into a wider slice) instead of
indexing into the expression; a genuine runtime RHS (the checker already
guarantees it's exactly `hi - lo + 1` bits wide in that case) still uses
the original per-bit `Index` reconstruction.

Verified end-to-end: `mimz sim --trace` on `foreach_fill.mimz` now
produces `lamps = 100925952`, matching a standalone `iverilog` run of the
same compiled Verilog bit-for-bit. `mimz eval` on the same file still
errors — but with the ACCURATE "signal `lamps` is never driven" instead
of the slice-specific message this entry originally claimed: `comb.rs`
has never unrolled module-level `foreach`/`repeat` at all (its own header
comment says so — "deliberately a SLICE of the Phase 1.5 simulator...no
repeat" — `ModuleItem::ForEach`/`Repeat` just fall through its item loop's
wildcard arm), so a foreach-wrapped drive is invisible to it regardless
of this fix. That's a separate, pre-existing, deliberately-scoped
limitation of `mimz eval` specifically (elaborate.rs, behind `mimz
sim`/`test`, DOES lower `ForEach` to `Repeat` and unroll it — that's the
path this fix actually lands on) — not part of this bug, and the
inaccurate claim in this entry's original filing is corrected here rather
than left standing.

**Test.** `tests/icarus.rs`'s `our_simulator_matches_icarus_bit_for_bit`
now differentials `english/foreach_fill.mimz` (layer 3 — kernel == Icarus,
bit-for-bit) — previously excluded pending this fix, per T1's own note.

## BUG-18 (MEDIUM, FIXED 2026-07-18) — `extend()` of a literal lost its width in self-determined Verilog contexts

**What.** A checker-valid, kernel-correct program whose emitted Verilog
`iverilog` **refuses to elaborate**. The generated design

```
module Fuzz {
  in p0: bits[3]
  in p1: bits[12]
  out y: bits[13]
  y = {(extend(extend(3, 3), 12) | p1), (extend(p0, 5) < extend(29, 5))}
}
```

passes `mimz check` and `mimz eval` computes the correct `y = 6367`, but the
emitted Verilog was `assign y = {(((3)) | p1), ((p0) < (29))};` and Icarus
rejected it: `Concatenation operand "('sd3)|(p1)" has indefinite width.`

**Cause.** `emit_verilog/expr.rs`'s `Builtin::Extend` arm was a pure
passthrough — `format!("({})", self.expr_subst(&args[0], ...))` — that emitted
its argument unchanged and relied entirely on Verilog's **context-determined**
implicit widening to size it at the point of use. That rule only fires in
context-determined positions (e.g. directly on an `assign` RHS). A
concatenation `{...}` operand is **self-determined** (Verilog-2005 LRM §5.4.1):
each operand's width must be knowable from the operand alone. A named signal
already carries a definite width from its declaration, so `extend()` over a
signal is fine anywhere — but an **unsized integer literal** (rendered by
`verilog_literal` in `emit_verilog/mod.rs` as a bare `3`, never with a width
prefix) has no self-determined width, and the passthrough did nothing to give
it one. The failing case nests: `extend(extend(3, 3), 12)` — the outer
`extend`'s `args[0]` is itself an `extend` wrapping the literal — so a fix
matching only a bare `ExprKind::Int` would patch the reported seed while
leaving the general (routinely generated) nested shape broken.

**How found.** T2 v1 of the differential fuzzer
(`tests/differential_fuzz.rs`'s `differential_fuzz_matches_icarus`,
`docs/plan/phase-2-differential-fuzzing.md`) on its **first real run** — seed
`12648432` (iteration i=2, not a rare corner). The random-program generator's
`widen()` helper produces the nested-`extend`-of-literal shape routinely.
Same "checker accepts it, simulator/emitter disagree" divergence class as
BUG-6/BUG-11/BUG-16/BUG-17 — here specifically the emitter's Verilog output
being rejected by a real Verilog compiler, not a value disagreement.

**Severity.** MEDIUM — an emitter miscompile (invalid Verilog) for an
otherwise-valid, kernel-correct construct: any `extend()` over a literal
(directly or through nested `extend`/`trunc`) placed inside a concatenation
operand or other self-determined position. Not a crash and not a silent wrong
value — `iverilog` errors cleanly at elaboration — but it blocks synthesis of
affected designs. No shipped example triggered it (found only by fuzzing).

**Fix (2026-07-18).** Approach A — an emitter-local recursive resolver, chosen
over widening the shared `checker::consteval::eval` (approach B) to keep the
blast radius minimal: `consteval::eval` is used by the checker for array sizes,
`repeat` bounds, parameter defaults, etc., and folding `extend`/`trunc` there
could shift which programs const-fold (e.g. the E0407 "`trunc` of a bare
literal does nothing useful" path). The new `resolve_const_value`
(`emit_verilog/expr.rs`) walks `ExprKind::Int` directly and recurses through
`Builtin::Extend` (value unchanged) and `Builtin::Trunc` (value masked to its
low N bits, using `consteval::eval` only on the width argument, which is always
separately constant-foldable). The `Builtin::Extend` arm now renders a
constant-resolved argument as an explicitly-sized `W'd{v}` literal at this
extend's own target width (`args[1]`) — extending a fully-resolved constant to
a larger width doesn't change its value, only the bits available to hold it —
and falls back to the original passthrough for genuine runtime operands (which
already carry a definite width). Scoped to the **unsigned** case: `extend()` of
a `CtInt` literal always yields unsigned `bits(n)` per the checker
(`checker/widths/ops.rs`'s `Builtin::Extend | Builtin::Trunc` arm), and source
literals are unsized and non-negative, so `W'd{v}` is always correct here; a
negative/signed literal renders as `Unary(Neg, Int)`, which `resolve_const_value`
does not match and so falls through to the (safe) passthrough. After the fix the
repro emits `assign y = {(12'd3 | p1), ((p0) < 5'd29)};`, which Icarus accepts.

**Test.** `tests/differential_fuzz.rs`'s `differential_fuzz_matches_icarus`
(T2 v1) — green at N=20 (was failing on seed 12648432) and confirmed against
Icarus. No golden file or emitter unit test asserted the previous (broken)
`extend` rendering, so changing its format regressed nothing (full workspace
suite: 886 passed).

## BUG-19 (MEDIUM, OPEN) — A width-growing/-changing operator's result silently gets the WRONG value once `extend()` crosses back into a wider Verilog context

**What.** A checker-valid, kernel-correct program whose emitted Verilog gives
a **different value** under real Icarus — not an elaboration error like
BUG-18, a genuine value mismatch. The generated design:

```
module Fuzz {
  in p0: bits[6]
  in p1: bits[15]
  in p2: bits[8]
  out y: bits[31]
  y = {(p1 ^ extend(extend(1, 1), 15)), (extend(p2, 15) - p1)}
}
```

Vector `p0=55, p1=15470, p2=165`: our kernel computes `y=1013957687`; Icarus
computes `y=506971191`. Emitted Verilog (post-BUG-18 fix):
`assign y = {(p1 ^ 15'd1), ((p2) - p1)};`

**Cause.** `extend(p2, 15) - p1` is `bits[15] - bits[15]`, spec's lossless
`-` (always grows the result by exactly one bit — spec/02 §1.2 — so the
checker and our kernel both model this subexpression as **16 bits**, giving
`{15-bit hi, 16-bit lo} = 31 bits = y`). But this subtraction is a
**self-determined** concat operand (same Verilog-2005 LRM §5.4.1 rule
BUG-18 hit): Verilog sizes a self-determined `-` to the **max operand
width** (15 bits here) with zero awareness of mimz's own "lossless, grows
by one" semantics — there is no surrounding assignment target to borrow a
wider width from, the way a plain `y = a - b` (context-determined position)
would correctly get. So Icarus computes `{15-bit hi, 15-bit lo} = 30 bits`,
zero-padded to the declared 31-bit `y` — silently misaligning the high
field by one bit position and dropping the carry the checker's own width
model assumed was preserved. Verified arithmetically: kernel
`15471*65536 + 50231 = 1013957687`; Icarus `15471*32768 + 17463 =
506971191`.

**How found.** T2 v1's `differential_fuzz_matches_icarus`, a deeper manual
confidence pass (`MIMZ_DIFF_FUZZ_N=500`) — seed `12648451` (iteration i=21).
Masked until BUG-18 was fixed (BUG-18 panicked the loop at i=2, before the
fuzzer ever reached this seed) — always latent, not a regression from the
BUG-18 fix. Same "checker/kernel and emitted Verilog disagree" divergence
class as BUG-6/BUG-11/BUG-16/BUG-18, but a distinct construct (a lossless
arithmetic operator's result width, not extend-of-literal) and a distinct
symptom (wrong value, not a build failure) — genuinely deeper than BUG-18,
since it means the emitter cannot in general assume a mimz-computed width
survives into a self-determined Verilog position, for ANY width-growing
operator, not just `extend()`.

**Update (2026-07-19, T2 v2 — the class is bigger than "lossless `+`/`-`").**
While extending the differential fuzzer's generator to signed values (v2),
a **second, distinct manifestation** turned up in the same `MIMZ_DIFF_FUZZ_N=500`
deep pass, seed `12648524`, an entirely unsigned program — no signed values
involved, so not a v2-specific regression, the same pre-existing gap v1 has
always had, just reached by a different generated shape:

```
module Fuzz {
  in p0: bits[15]
  in p2: bits[3]
  out y: bits[18]
  y = ({p0, p2} & extend((extend(3, 3) -% p2), 18))
}
```

Vector `p0=7735, p2=5`: our kernel computes `y=4` (matching the spec: `-%`
wraps modulo 2^3 at its own 3-bit operand width — `3 -% 5 = 6` — THEN
`extend(6, 18)` zero-extends the already-wrapped value, giving `4` after
the `&`). Real Icarus computes `y=61884`. Emitted Verilog:
`assign y = ({p0, p2} & ((3'd3 - p2)));` (the `extend(..., 18)` call is,
again, an invisible passthrough for a non-literal argument, exactly BUG-18's
pattern). Confirmed by hand and against a standalone `iverilog` run: Icarus
does **not** compute "wrap at 3 bits, then zero-extend" — because `&` is a
context-determined operator, Icarus widens BOTH of its operands to the
context width (18 bits, borrowed from the `{p0,p2}` concat sibling) **before**
performing the subtraction, i.e. it computes `(18'd3 - 18'd5) = 262142` (the
wraparound now happens modulo 2^18, not 2^3) and ANDs that with the 18-bit
concat, landing on `61884`. This is the same root cause as the original
lossless-`+`/`-` case (`extend()`'s passthrough codegen trusts a
mimz-computed width that real Verilog silently recomputes on its own terms)
but hits **wrapping** `+%`/`-%` too, not just lossless `+`/`-` — and here the
divergence isn't a lost carry bit, it's an entirely different modulus, so
the wrong value is not even close. This means the class is: **any operator
whose spec-defined result depends on the width it was originally evaluated
at** (lossless growth for `+`/`-`, or the wrap modulus for `+%`/`-%`) is
unsound the moment its result gets `widen()`-ed (via `extend()`) to fit a
wider sibling and that combination is not itself the assignment's own
top-level context-determined position. Bitwise `&`/`|`/`^` and comparisons
are NOT in this class (confirmed by construction, not just observation):
zero/sign-extension commutes with bitwise ops and with order comparison
regardless of when Verilog performs it, so re-deriving them at a wider
context always gives the same answer either way.

**Severity.** MEDIUM — silent wrong value (no crash, no compile error) for
lossless `+`/`-` or wrapping `+%`/`-%` whenever the result is `extend()`-ed
to match a wider sibling and that combination is not itself the top-level
assignment RHS (a concat member, a comparison operand, or an operand of a
further bitwise/shift op that later escapes into one of those). Does not
block T2 v2's default `cargo test`/CI gate — the fuzzer's generator now
deliberately excludes all four operators (`+ - +% -%`) from its
same-width-family combine step precisely to avoid resurfacing this already-
filed, already-deferred bug on every run (`tests/differential_fuzz.rs`'s
`WidthEffect`/`SAME_WIDTH_OPS` doc comment has the full reasoning) — but it
remains a real synthesis correctness gap for any hand-written design
combining either operator family with concatenation/comparison the same
way, and the project's own constitution rates silent miscompute above
elaboration failure (BUG-18) in severity, being harder for a user to
notice.

**Fix (Pending).** Broader than BUG-18's narrow literal-rendering patch:
needs the emitter to make a width-growing/-changing operator's result
self-determined-safe wherever it can land in a non-context-determined
position — e.g. explicitly wrapping the operator's Verilog rendering in a
width-forcing idiom (a `{1'b0, (a - b)}`-style explicit pad for lossless
growth, or forcing the wrap operator's own operands to a temporary at their
ORIGINAL width — e.g. a `$unsigned'()`-style size cast at the pre-widen
width before the wrap, then extending the RESULT — contingent on
Icarus-version compatibility) whenever the surrounding position is
self-determined, not just when the operand happens to be
`extend()`-of-literal. Likely belongs alongside BUG-17 and the A1 "one
shared width/const-eval module" item
(`docs/plan/phase-2-correctness-consolidation.local.md` stage 4) rather than
as another narrow one-off patch — deferred by explicit decision (not
overlooked) pending that broader design pass.

**Test.** None yet — filed as open. Both repros above are standalone,
reproducible `.mimz` snippets (not currently exercised by any committed
test — `tests/differential_fuzz.rs`'s generator deliberately avoids
generating either shape, per the "Severity" note above); re-adding a
regression test for this is part of the Stage-4 fix, not before.

## BUG-21 (MEDIUM, FIXED 2026-07-19) — Simulator's slice-read incorrectly inherited the base's signedness

**What.** `mimz sim`/`test`'s evaluator computed the WRONG value for
`extend(<a slice of a signed value>, N)` whenever the slice's top bit was
set. Minimal repro:

```
module Fuzz {
  clock clk
  reset rst
  in p1: signed[10]
  reg r1: bits[9] = 0
  out y: bit
  on rise(clk) {
    r1 <- extend(p1[9:7], 9)
  }
  y = r1[8:8]
}
```

With `p1 = 1012` (bit pattern `1111110100`, so `p1[9:7] = 0b111 = 7`): real
Icarus computes `r1 = 7` (`y = 0`) — correct, since a slice is always
`bits`-typed regardless of the base's kind (spec/02, and `mimz check`
agrees). Our kernel computed `r1 = 511` (`y = 1`): it treated the 3-bit
slice `0b111` as `signed`, saw its top bit set, and sign-extended it to 9
bits instead of zero-extending.

**Cause.** `ExprKind::Slice`'s evaluator (`crates/mimz-sim/src/sim/value.rs`)
built its result as `Val::new((b.bits >> lo) & mask(w), w, b.signed)` —
propagating the SLICED BASE's `signed` flag to the slice result. The
checker's own `slice_ty` (`crates/mimz-core/src/checker/widths/expr.rs`)
returns `bits(...)` unconditionally, never `Signed`, regardless of the
base's kind — and the sibling single-bit `ExprKind::Index` arm one case
above already got this right (`Val::new(..., 1, false)`, hardcoded
unsigned). The multi-bit `Slice` arm was the one place that copied the
base's signedness instead.

**How found.** T2 v3 (`tests/differential_fuzz.rs`'s clocked generator,
`docs/plan/phase-2-differential-fuzzing.md`), a `MIMZ_DIFF_FUZZ_CLOCKED_N=2000`
deep confidence pass — seed `202428133`. `extend()`'s own passthrough
codegen for a non-literal argument (the same mechanism BUG-18/BUG-19 hit)
was the first suspect, but an isolated standalone repro against real
`iverilog` matched the checker's OWN model exactly (zero-extend), proving
the emitter was innocent this time — the divergence was entirely inside
our own kernel's evaluator, not the emitted Verilog.

**Severity.** MEDIUM — silent wrong value (no crash, no compile error),
narrow trigger: only a slice of a `signed`-typed value whose sliced bits
happen to have their own top bit set, subsequently widened via `extend()`
or bound to a wider context. No shipped example exercises this shape
(found only by fuzzing, same story as BUG-16/BUG-18).

**Fix.** `ExprKind::Slice`'s two `Val`-construction sites (the `unknown`
early-return and the normal case) now hardcode `signed: false`, matching
its sibling `Index` arm and the checker's own `slice_ty`
(`crates/mimz-sim/src/sim/value.rs`).

**Test.** Confirmed by `tests/differential_fuzz.rs`'s
`differential_fuzz_clocked_matches_icarus` at `MIMZ_DIFF_FUZZ_CLOCKED_N=2000`
(seed 202428133 no longer reproduces); no standalone unit test added
(the fuzzer itself is the regression guard, same as BUG-18/BUG-19's
discovery path).
