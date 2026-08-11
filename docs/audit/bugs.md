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

## BUG-13 (MEDIUM, FIXED) — 128-bit Simulator Ceiling

**What.** The simulator could not handle vectors larger than 128 bits.
Correction to the original write-up: this was a CLEAN elaboration-time
error (`"width {n} exceeds the simulator's 128-bit limit"`,
`value::checked_width`), not silent data corruption — a >128-bit design
was rejected outright, never silently wrong.

**Cause.** The simulator's `Val` struct was hardcoded around Rust's
`u128`, and every runtime-value boundary in `mimz-sim` (`SimOpts.inputs`,
`Frame.values`, `comb::Output.value`, `kernel::Sim::set`/`peek`, the VCD
writer, the console tracer, `runner.rs`'s parsers) was typed on raw
`u128` too.

**How found.** CTO Architectural Review (July 2026).

**Severity.** MEDIUM — modern digital design routinely uses buses of
256/512/1024 bits; the 128-bit ceiling blocked those designs from
running in `mimz sim`/`mimz test`/`mimz eval`/the WASM playground
entirely (checker and emitter already supported them).

**Fix (2026-07-22, layer 1 — runtime values).**
`docs/superpowers/plans/2026-07-22-sim-wide-values.local.md`. `Val.bits`
became a `Small(u128)`/`Wide(Vec<u64>)` enum; a new `sim/wide.rs` holds
hand-rolled limb arithmetic (no division — the language has none); every
operator in `value.rs` dispatches on the operand/result-width
combination, with the narrow path byte-for-byte unchanged. The same
`Bits` type replaced raw `u128` at every runtime-value boundary in the
crate. `MAX_WIDTH` (1,000,000 bits) relocated from the checker into
`mimz_core::width_rules` as the one shared ceiling both checker and
simulator now consume. `tests/differential_fuzz.rs`'s generator width
raised from 32 to 512 to prove wide-value correctness against Icarus.

**Fix-forward (2026-07-24, final-review findings).** A full-branch review
before merge found `kernel.rs`'s register-lifecycle code was left on the
old width-unaware `Val::new` pattern — `Sim::new`'s leaf/reg init,
`tick_edge`'s synchronous reset, and `CombEnv::mem_read`'s unwritten-cell
fallback all built a `Val` straight from the signal's declared width
without routing through the `Small`/`Wide` dispatch, so a `bits[200]`
register reset to a small magnitude (the common case — most registers
start at/near 0) stayed `Bits::Small` despite its width. Every unary
operator (`~`, `-`, reduction) and comparison then dispatched on
`is_wide()` alone, silently truncating results to 128 bits for such a
register (confirmed: `~r` on a 200-bit register reset to 0 flipped only
the low 128 bits). Separately, the bit/slice-indexed register write path
(`reg[i] <- v` / `reg[hi:lo] <- v`) still did raw `1u128 << i` arithmetic,
panicking for any index/bound >= 128. Fixed by adding a width-aware
`value::from_u128_at_width` constructor (used at every kernel.rs site
above), rewriting the bit/slice-write arm to dispatch through
`wide.rs`'s limb primitives when `base.width > 128`, and hardening
`unary_known`/`cmp_lt`/`cmp_eq`'s dispatch to gate on `width <= 128`
rather than `is_wide()` alone (defense in depth, in case a future
construction site regresses the same way). 4 new regression tests in
`kernel.rs` cover a wide register through `~`, a wide bit-indexed write,
and a wide slice-indexed write.

**Remaining gap (layer 2, tracked separately, NOT part of this fix).**
`Reg.reset`/`Mem.init`'s compile-time-folded value stays `i128`-bounded,
and `ast::ExprKind::Int`'s literal value stays `u128`-bounded in the AST
itself (`mimz-core`'s lexer/parser) — a wide reg/mem that resets to `0`
is unaffected; a nonzero large literal reset, or a >128-bit literal
written directly in source, is not yet fixed. Its own spec+plan, same
split pattern as this project's Phase A1a/A1b or T1/T2/T3 staged efforts.

Separately (adjacent, `mimz-sim`-internal, pre-existing — not introduced
by this fix): `comb.rs`'s combinational bit-drive/slice-drive assignment
(`wire y: bits[200]; y[150] = x` style) still hard-errors on any bit
index/slice bound outside `0..128` regardless of the wire's declared
width (`comb.rs`'s `"... out of range (0..128)"` messages) — this path
was never migrated to `Bits`. Real designs almost always read a wide
signal rather than bit/slice-_drive_ one combinationally, so this is a
narrow, rare gap, but it means "compute, drive, and observe any
checker-legal width" isn't quite 100% true for this one combinational
assignment shape yet.

**Fix (2026-07-25, layer 2 — compile-time constants).**
`docs/superpowers/specs/2026-07-25-const-literal-wide-values-design.local.md`.
`Bits` and its limb arithmetic (`wide.rs`) relocated from `mimz-sim` into
`mimz-core` so the lexer, AST, and checker could share the same
representation. `ExprKind::Int`/`Pattern::Int`/`TokKind::Int`'s literal
value became `Bits` instead of `u128`; `checker::consteval`'s evaluator
now returns a `ConstVal` (an arbitrary-width two's-complement integer)
instead of a bare `i128`, computing each arithmetic op at a safe
upper-bound width before shrinking to natural width and checking against
`width_rules::MAX_WIDTH` — preserving the evaluator's "overflow is a
clean error, never a silent wrap" contract at a 1,000,000-bit ceiling
instead of 127. `Reg.reset`/`Mem.init` carry `ConstVal` end to end. A wide
register/memory can now reset/init to any nonzero literal the checker
would accept for its declared width — the gap layer 1 explicitly left
open is closed. `Pattern::IntMask`/`TokKind::MaskedInt` (the `0b1??`
don't-care match-pattern literal) were deliberately left untouched — no
real use case for a >128-bit don't-care pattern has come up.

Along the way, getting `cargo test --workspace --all-targets` to actually
run clean (rather than just `cargo build -p mimz-core`) surfaced a real,
pre-existing arithmetic bug in `consteval.rs`'s own sign-detection
headroom: `Add`/`Sub`'s `+1`-bit growth (and `Mul`/`Shl`'s original
no-extra-bit growth) is only safe when both operands are already
n-bit-_signed_ values; an unsigned magnitude sitting at its own tight
n-bit width (e.g. a literal, whose every bit is real magnitude, no
reserved sign bit) can produce a sum/product/shift whose own top bit is
legitimately set without the true result being negative — e.g.
`(2^127-1)+(2^127-1)` needs exactly 128 bits and was misread as negative.
Fixed by widening `Add`/`Sub` to `+2` and `Mul`/`Shl` to their tight
magnitude bound `+1`, verified against `examples/english/shift.mimz`
(`1 << 3` was failing E0405 before the fix) and a new consteval unit
test. `emit_verilog`'s enum-payload literal masking (also touched by this
layer) had the mirror bug: a negative payload value must be
sign-extended to its field width before masking, not zero-padded via a
raw limb reshape.

**Test.** `crates/mimz-sim/src/sim/wide.rs`'s own unit tests;
`crates/mimz-sim/src/sim/value.rs`'s `wide_*` tests; `differential_fuzz`
at `MAX_WIDTH = 512` (both default and deep `N`); `kernel.rs`'s 4
register-lifecycle regression tests (2026-07-24 fix-forward).

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

## BUG-15 (MEDIUM, FIXED 2026-08-01) — `mimz-sim` has no bundle-field-expansion baseline for instance ports or `fn` call arguments

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

**Fix — instance ports (2026-08-01).** `flatten_instance`
(`crates/mimz-sim/src/sim/elaborate/instance.rs`) rebuilds, from the
child's own declared ports (`cm.items`), a map from each flattened
`port_field` scalar name back to its ORIGINAL `(port, field)` pair —
`elaborate_module` had already flattened a bundle-typed port to its
per-field `Signal`s by the time `child.inputs` reaches this function, so
that mapping (which the user's single `port: signal` connection needs to
resolve at all) no longer existed anywhere. Each bundle-typed input's
connection is then expanded per field via `bundle_field_expr` (the same
helper the `??` OR-mux form and Wire/Drive bundle paths already use) and
rewritten through `Rw` — which needed its `bundle_sigs` set fixed from an
always-empty placeholder to the PARENT's real bundle-signal set,
since a connection expression lives in the parent's own (still
unflattened) scope, unlike the child body `Rw` also builds here.

**Fix — `fn` call arguments (2026-08-01).** Two-sided, since the
CALL SITE and the CALLEE's own body both reference the bundle value by
its single pre-flattening name. (1) `Rw::expr`'s `FnCall` arm
(`rewrite.rs`) now expands a bundle-typed argument into one sub-expression
per field — keyed by the CALLEE's _declared parameter type_, not the
argument's own inferred type (mirroring the emitter's identical BUG-10
call-site convention), via a new `expand_fn_call_args` method requiring
`Rw` to carry `func_reg`/`bundle_reg`/`imports` (threaded through all four
`Rw` construction sites: `module.rs`'s `build_rw`, `instance.rs`'s `prw`/
`crw`, and `rewrite.rs`'s own nested match-arm `ext_rw`). (2) A NEW
`flatten_bundle_params_in_func` (`bundle.rs`) pre-expands each `FuncDecl`
with a bundle-typed parameter into N flat scalar params (`<param>_<field>`,
folded to a concrete literal width — a `fn`'s own const environment is
file-scope only, so nothing may stay symbolic) and rewrites every
`param.field` body reference to the matching flat name, run ONCE when
`Elaboration::new` builds its `funcs` map from `func_reg` — so
`eval_fn_call`'s own runtime param-binding loop (`value/fn_eval.rs`) needed
NO bundle-awareness added at all: by the time it runs, both the expanded
call-site argument list and the callee's own expanded parameter list
already agree on field count, order, and flat names. This also means the
`Resolver` trait needed no new method — the entire fix lives at
elaboration time, never at runtime.

**Test.** `bundle_typed_instance_input_port_connection_flattens_per_field`,
`bundle_typed_fn_call_argument_expands_to_one_arg_per_field`
(`crates/mimz-sim/src/sim/elaborate/tests.rs`) — both drive a real
`kernel::Sim` end-to-end (not just structural `Design` assertions) through
a `bundle Handshake(W: int = 8) { valid: bit, data: bits[W] }` connected
to, respectively, a child instance's bundle-typed input port and a `fn`'s
bundle-typed parameter, confirming the correct field routes through in
both the `valid`-true and `valid`-false cases. Full workspace 1115/1115
(1113 pre-fix baseline + these 2), clippy/fmt clean.

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

## BUG-19 (MEDIUM, FIXED 2026-07-19) — A width-growing/-changing operator's result silently gets the WRONG value once `extend()` crosses back into a wider Verilog context

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

**Fix.** The emitter now computes both mimz's own `Kind` (`emit_verilog::kinds::infer_kind`) and Verilog's self-determined `Kind` (`emit_verilog::self_determined::verilog_self_determined_kind`) for every concat member, replication part, comparison operand, and `$signed`/`$unsigned` argument, hoisting to a named wire on a mismatch instead of trusting a passthrough.

**Test.** `bug_19_lossless_sub_in_a_concat_matches_icarus` and `bug_19_wrapping_sub_in_a_bitand_matches_icarus` (`tests/self_determined_regression.rs`) — both regression tests from Task 6, verifying the fix against real Icarus.

## BUG-20 (MEDIUM, FIXED 2026-07-19) — Emitter renders a slice of a non-identifier base ungrouped, invalid Verilog part-select syntax

**What.** mimz's own language semantics allow `[hi:lo]` on any expression,
not just a plain signal — e.g. `(p1 & p2)[3:0]` passes `mimz check` and
evaluates correctly under `mimz sim`/`eval`. The emitter, however, would
render this as invalid Verilog.

**Cause.** `emit_verilog/expr.rs`'s `ExprKind::Slice` arm:

```rust
ExprKind::Slice { base, hi, lo } => {
    let b = self.expr_subst(base, subst, arrays);
    let h = self.index_expr(hi, subst, arrays);
    let l = self.index_expr(lo, subst, arrays);
    format!("{b}[{h}:{l}]")
}
```

renders `base`'s text with no grouping. Verilog's part-select grammar
(`expr[hi:lo]`) only accepts an identifier or hierarchical reference as
the base — parenthesizing a composite expression does not help; Verilog
does not permit part-selecting an arbitrary expression at all, only a
net/reg reference. `(p1 & p2)[3:0]` is invalid Verilog regardless of how
`b`'s text is wrapped.

**How found.** Not by fuzzing directly — the differential fuzzer's own
generator (`tests/differential_fuzz.rs`) already works around this gap:
`clamp()` (its slice-producing helper) was restricted during T2 v2 to
only slice a real port identifier, discarding-and-regenerating
otherwise (see `docs/plan/phase-2-differential-fuzzing.md`'s v2 entry).
That restriction masked this bug from ever being exercised by the
fuzzer, leaving it undiscovered as a live gap until reviewed directly
against the emitter's source during Stage 4 Phase A1b's design pass
(2026-07-19) — referenced there as "BUG-20" in
`docs/superpowers/specs/2026-07-19-shared-width-semantics-design.local.md`'s
Non-goals section (deferred pending confirmation it shares machinery
with A1b), but never previously given its own tracked entry until now.

**Severity.** MEDIUM — an emitter miscompile (invalid Verilog, `iverilog`
would reject at parse/elaboration) for a checker-valid, kernel-correct
construct: any slice of a non-identifier expression. Not a crash, not a
silent wrong value — a clean compile-time rejection by real Verilog
tools — but blocks synthesis of affected designs. No shipped example
triggers it (the fuzzer's own workaround has kept it from surfacing in
CI so far).

**Fix.** The emitter now hoists a slice's base into a named wire whenever it isn't already a plain identifier (`Emitter::hoist_slice_base_if_needed`). A named signal is always a valid part-select base regardless of what it was assigned from.

**Test.** `bug_20_slice_of_a_composite_expression_matches_icarus` (`tests/self_determined_regression.rs`) — regression test from Task 6, verifying the fix against real Icarus.

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

## BUG-22 (MEDIUM, FIXED 2026-07-19) — Simulator's `-` result is always tagged `signed`, disagreeing with the checker's own lossless-growth rule

**What.** `mimz sim`/`test`/`eval`'s evaluator tags EVERY subtraction's
result value as `signed`, even when the checker types the same
expression as unsigned `bits[N]`. Minimal repro (checker-level, not yet
confirmed against a concrete value mismatch — filed from static code
reading during Stage 4 Phase A1b's scoping, not from fuzzing):

```
module Fuzz {
  in p0: bits[4]
  in p1: bits[4]
  out y: bits[5]
  y = p0 - p1
}
```

The checker's `lossless_ty` (`checker/widths/ops.rs`) types `p0 - p1` as
unsigned `bits[5]` (both operands are unsigned `Bits`, so the result
kind is unsigned per the lossless-growth rule: signed only when BOTH
operands are already `Signed`). The simulator's `binary_known`
(`crates/mimz-sim/src/sim/value.rs`)'s `BinOp::Sub` arm:

```rust
BinOp::Sub => Val::new(
    l.as_i128().wrapping_sub(r.as_i128()) as u128,
    wmax + 1,
    true,
),
```

hardcodes the result's `signed` field to `true` unconditionally —
unlike its sibling `Add`/`Mul` arms, which correctly propagate
`l.signed || r.signed`.

**Cause.** Appears to be a simple oversight: `Add`/`Mul` compute
`signed` from the operands (`let signed = l.signed || r.signed;` at the
top of `binary_known`), but `Sub`'s arm was written with a literal
`true` instead of using that same `signed` variable.

**Severity.** MEDIUM — the raw bit pattern computed for the subtraction
itself is correct (sign-extended arithmetic via `as_i128`), but the
resulting `Val`'s `signed` tag is wrong for any all-unsigned-operand
subtraction. Since `Val.signed` propagates into subsequent operations
(shift's sign extension in `extend_bits`, further lossless-op chaining,
etc.), a chained expression consuming an unsigned `-` result could
compute a different value than the checker's own type model — and thus
than the emitted Verilog — implies. Same divergence CLASS as BUG-21
(simulator's kind for an operator's result disagreeing with the
checker's own rule for that same operator), not yet confirmed with a
concrete wrong-output repro (found by code inspection during Stage 4
Phase A1b's scoping, not by fuzzing — `tests/differential_fuzz.rs`'s
generator currently excludes lossless `-` entirely per BUG-19, so it
has not had a chance to surface this independently).

**Fix.** `binary_known`'s `Add`/`Sub`/`Mul` arms now all route through
the shared `width_rules::lossless_result(Kind{l.width,l.signed},
Kind{r.width,r.signed}, is_mul)`, which derives `signed` from the
operands the same way the checker's `lossless_ty` does — `Sub`'s
hardcoded `true` disappeared as a natural consequence of sharing one
rule with `Add`/`Mul`, not a separate patch (`.expect(...)` guards the
call, since `checker::check`'s mandatory gating — Stage 2, A2 — already
rejects any mixed-signedness operand pair before this code can run).

**Test.** New regression tests in `crates/mimz-sim/src/sim/value.rs`'s
inline test module: `sub_of_two_unsigned_values_is_unsigned` (confirmed
it failed against the pre-fix hardcoded `true`) and
`sub_of_two_signed_values_is_signed` (a positive pin — this case
already passed by coincidence, kept as a regression guard going
forward).

## BUG-23 (MEDIUM, FIXED 2026-07-20) — A wrapping operator nested under a sibling context-determined operator loses its own-width truncation

**What.** A checker-valid, kernel-correct program whose emitted Verilog
gives a **different value** under real Icarus, whenever a wrapping
operator (`+%`/`-%`/`*%`) is used as a direct operand of a DIFFERENT,
context-determined arithmetic/bitwise operator (`+`/`-`/`*`/`&`/`|`/`^`)
— not inside one of the four Verilog self-determined positions Stage 4
Phase A1b's hoist mechanism actually checks (concat member, replication
part, comparison operand, `$signed`/`$unsigned` argument). Two repros,
both found by `tests/differential_fuzz.rs`'s generator on its very first
re-enabled run (Stage 4 Phase A1b, Task 8) — the fuzzer's `+`/`-`/`+%`/
`-%` combinators had been excluded from the generator entirely since
BUG-19 was filed; re-enabling them (the closing-the-loop proof for
BUG-19's own fix) surfaced this DIFFERENT, narrower member of the same
bug class immediately, at default `N`, no deep pass needed:

```
module Fuzz {
  in p0: signed[6]
  in p1: signed[8]
  out y: bits[18]
  y = (extend(63, 7) + ({unsigned((extend(signed(extend(1, 1)), 6) ^ p0)), {unsigned(p0), extend(21, 5)}} +% extend(63727, 17)))
}
```

Vector `p0=25, p1=208`: our kernel computes `y=11363`; Icarus computes
`y=142435`. The outer `+`'s right operand is a `+%` — not sitting in
any of the four checked self-determined positions, since it's a direct
operand of `+`, which is itself the top-level assignment's RHS (never
itself run through `hoist_if_needed`, only the four checked positions
are).

A second, clocked repro (seed `202427630`):

```
module Fuzz {
  clock clk
  reset rst
  in p0: bits[1]
  in p1: bits[3]
  in p2: bits[5]
  reg r0: bits[11] = 0
  reg r1: bits[13] = 0
  out y: bits[26]
  on rise(clk) {
    r0 <- extend(287, 11)
    r1 <- extend(5643, 13)
  }
  y = {(extend(1524, 14) | {r1, p0}), (p0[0:0] + (extend(extend(1, 1), 11) -% r0))}
}
```

held inputs `p0=0, p1=7, p2=24`: our kernel computes `y=48195298`;
Icarus computes `y=48197346`. Here the outer `+` (containing the `-%`)
IS itself a concat member, so it correctly gets hoisted by Task 6's own
mechanism — but the hoisted wire's own `assign` RHS still contains the
inner `-%` rendered as bare `-`, still directly connected (via that
outer `+`) to the new wire's declared width. The hoist protects the
OUTER growth bit but does not isolate the INNER wrap subexpression from
that same context.

**Cause.** mimz's own model (`emit_verilog::kinds::infer_binary`,
`width_rules::matched_result`) gives `AddWrap`/`SubWrap`/`MulWrap` the
same width rule as `BitAnd`/`BitOr`/`BitXor` — `max(l, r)`, no growth —
treating "wrap at the operand width, discard the carry" as a fact of
the operator itself, independent of where the expression sits.
`emit_verilog/expr.rs` renders `AddWrap`/`SubWrap`/`MulWrap` as the
EXACT same Verilog text as `Add`/`Sub`/`Mul` (bare `+`/`-`/`*`) —
nothing in the emitted text marks a width boundary there. Real
Verilog's arithmetic/bitwise operators are context-determined: a
connected tree of `+`/`-`/`*`/`&`/`|`/`^` computes ONE width for the
whole tree (the widest leaf, extended outward to whatever ends the
chain), and does not stop partway through to "wrap" an inner
subexpression at that subexpression's own declared width first — the
exact effect `tests/differential_fuzz.rs`'s own `SAME_WIDTH_OPS` doc
comment already described for a hand-found case before Stage 4 (seed
`12648524`: `extend((extend(3,3) -% p2), 18)` inside an `&`), now shown
to be reachable generally through the fuzzer, not only by hand
construction.

**How found.** `tests/differential_fuzz.rs`'s
`differential_fuzz_matches_icarus`/`differential_fuzz_clocked_matches_icarus`,
default N (`N=20` combinational, `N=20` clocked), Stage 4 Phase A1b's
Task 8 — the very first re-enablement run of the `combine_lossless`/
`combine_wrap` combinators (added specifically as BUG-19's own
closing-the-loop proof). Both failures are real, `iverilog`-confirmed
value mismatches (not a generator/parser/checker panic — the checker
accepted both generated programs as valid, and real Icarus disagreed
with the kernel's own computed value).

**Severity.** MEDIUM — silent wrong value (no crash, no compile error)
whenever a wrapping operator (`+%`/`-%`/`*%`) is a direct operand of
ANY other context-determined arithmetic/bitwise operator and that
combination doesn't happen to land in one of Task 6's four checked
self-determined positions (or does land there, as in the clocked repro,
but the hoisted wire's own contents still connect the inner wrap
operator to the same wider context). Distinct from BUG-19's two
original repros (both already fixed by Task 6) and from BUG-20 (an
unrelated slice-grammar issue) — this is a narrower, previously-
undetected member of the same "self-determined vs. context-determined"
divergence class, closer to the class's true boundary than either of
BUG-19's named cases.

**Fix.** `Emitter::hoist_width_effect_operand`
(`crates/mimz-core/src/emit_verilog/expr.rs`) — extracted from the
pre-existing hoist pattern already used by `Builtin::Extend`'s own
render arm (a pure refactor, no behavior change) — is now called at
every recursive-descent operand position `expr_subst` walks into: the
shared binary-operator arm's LHS/RHS (the primary gap this bug named),
`Unary`, `IfExpr`/`Match` branches, `Concat`/`Replicate` members, an
`Index`'s base, function-call/enum-construct arguments, and the
remaining builtins (`Trunc`/`Min`/`Max`/`Abs`/`Nand`/`Nor`/`Xnor`/
`SignedCast`/`UnsignedCast`). Any lossless (`+`/`-`/`*`) or wrap
(`+%`/`-%`/`*%`) operand sitting at any of these positions is hoisted
unconditionally into a fresh, definite-width wire — regardless of
whether that position also happens to be one of Stage 4 Phase A1b's
four self-determined-mismatch positions — closing the gap this entry's
two repros both exploited. Only the true top-level assignment RHS stays
exempt (it is never itself passed through the hoist), since the
assignment target's own declared width already pins it correctly.

Two sub-fixes landed alongside the wiring, both found while implementing
it: (1) `hoist_slice_base_if_needed` (`emit_verilog/module.rs`)
previously always declared its hoisted wire plain unsigned; it now
takes a `signed: bool` parameter (mirroring `hoist_if_needed`'s existing
correct pattern) so a signed wrap operand's hoisted wire is itself
declared signed — without this, Verilog's "any unsigned operand makes
the whole expression unsigned" rule (LRM 5.5.1) would zero-extend the
wire instead of sign-extending it once the surrounding operator is
evaluated at its own wider context, silently changing the value. (2) At
the four call sites where both the new unconditional hoist and the
existing self-determined-mismatch hoist (`hoist_if_needed`) run on the
same operand (`Concat`/`Replicate`/`SignedCast`/`UnsignedCast`), a
lossless operand could get hoisted twice, emitting a redundant
same-width alias wire; `hoist_if_needed` now early-returns its input
unchanged when it is already a plain identifier (`is_plain_identifier`)
— the same guard `hoist_slice_base_if_needed` already used.

See "Follow-on findings" below for two further regressions this same
effort's own full-workspace verification pass found and fixed before
landing.

**Test.** `tests/self_determined_regression.rs`:
`bug_23_wrap_under_sibling_add_matches_icarus` and
`bug_23_wrap_under_sibling_add_inside_a_concat_matches_icarus` (this
entry's two originally-filed repros, seeds `12648435`/`202427630`, both
checked against real Icarus), `bug_23_signed_wrap_operand_hoist_preserves_sign_extension`
(the signedness sub-fix, above), `bug_23_top_level_wrap_needs_no_hoist`
(the top-level-exemption case), and `bug_23_wrap_directly_inside_a_concat_matches_icarus`
together with `bug_19_lossless_sub_in_a_concat_hoists_exactly_one_wire`
(the composability/double-hoist case — the latter is the one that
actually proves no double-hoist occurs, since a wrap operand's own
hoist is provably a no-op for the mismatch-check path; a wrap needed a
LOSSLESS sibling operand to exercise the real double-hoist shape).
Also: `tests/differential_fuzz.rs`'s `differential_fuzz_matches_icarus`/
`differential_fuzz_clocked_matches_icarus` — the exact tests that
surfaced this bug via the `+`/`-`/`+%`/`-%` combinators re-enabled by
Stage 4 Phase A1b's Task 8 — now pass at default N, and a deep pass
(`MIMZ_DIFF_FUZZ_N=500`/`MIMZ_DIFF_FUZZ_CLOCKED_N=2000`) is clean too now
that BUG-24 (below), the one thing the deep pass still caught, is also
fixed.

**Follow-on findings (2026-07-20).** See BUG-24's own entry below for
the same paragraph — while landing this fix and BUG-24's, a
full-workspace verification pass (`cargo test --workspace --all-targets
--no-fail-fast`, not something Tasks 1-4 individually ran, each having
scoped its own verification to narrower suites) found two further
regressions, both fixed before either bug's status changed to FIXED: a
generic/parametric-width decls fallback bug in `resolved_kind`, and an
over-broad version of BUG-24's own fix. Neither ever shipped or was
independently filed as a numbered bug — both were found and fixed
within this same BUG-23/BUG-24 effort, before anything was committed.

## BUG-24 (MEDIUM, FIXED 2026-07-20) — A shift nested under a sibling operator was wrongly excluded from the width-effect hoist, letting Verilog re-widen its left operand in the wider context

**What.** A checker-valid, kernel-correct program whose emitted Verilog
gives a **different value** under real Icarus, whenever a width-growing
context-determined operator (e.g. lossless `+`) is the left operand of
a shift (`<<`/`>>`). Found during BUG-23's own fix (Tasks 1-3) by that
plan's Task 4 deep-confidence pass
(`MIMZ_DIFF_FUZZ_N=500 cargo test --test differential_fuzz
differential_fuzz_matches_icarus`) — seed `12648537`, the 108th
generated program (seed = `0xC0FFEE + i`, `i=107`), outside default
`N=20`'s range, so it does not manifest at the level `cargo test`
normally runs at:

```
module Fuzz {
  in p0: signed[12]
  in p1: signed[14]
  out y: signed[29]
  y = ((((p1 * p1) + (p1 << extend(3, 4))) >> extend(0, 4)) << extend(3, 2))
}
```

Vector `{"p0": 2024, "p1": 13855}`: our kernel computes `y=51135944`;
Icarus computes `y=51004872`.

**Cause (corrected 2026-07-20 — the original diagnosis below was wrong).**
The entry as originally filed claimed the root cause was a missing
"fifth self-determined position" (a shift's left operand) that
`emit_verilog/self_determined.rs` needed to be taught to check, mirroring
BUG-19's fix shape. Hands-on empirical verification against real Icarus
found a simpler, different, and ALREADY-FAMILIAR root cause: this is the
SAME bug class as BUG-23, just for two operators BUG-23's own fix
(Tasks 1-3) didn't cover.

`emit_verilog/expr.rs`'s `is_width_effect_binop` decides which nested
binary operators must be hoisted into their own definite-width wire
before a sibling operator's Verilog text can safely embed them (BUG-23's
mechanism: `hoist_width_effect_operand`, wired into all ~9 recursive-
descent call sites in `expr_subst`). Its doc comment claimed every
operator OTHER than lossless (`+`/`-`/`*`) and wrap (`+%`/`-%`/`*%`)
"gives the SAME value no matter what width Verilog happens to (re)compute
it at" — including `<<`/`>>`. **This is false for `Shl`, verified
directly against real Icarus in this task:** `p1 << 3` (`p1` = -2529 as a
signed 14-bit value) evaluated at its own natural 14-bit width gives
`-3848`; the SAME expression evaluated in a wider 29-bit context gives
`-20232` — genuinely different values. A shift's own RESULT width is
self-determined (fixed to the left operand's natural width, unaffected
by context — this part of the old diagnosis was correct), but the
shift's LEFT OPERAND is itself context-determined: real Verilog widens
it to whatever ambient context the whole shift expression sits in
BEFORE performing the shift. `crates/mimz-sim/src/sim/value.rs`'s
`eval_ctx` already models this correctly for the simulator (its own doc
comment: "Verilog's `<<`/`>>` are context-determined on their LEFT
operand … ground-truthed against `iverilog` (BUG-11's fix)") — the
emitter's hoist mechanism just never extended the same fact to
`is_width_effect_binop`'s match arm. **`Shr` is included on the strength
of that same simulator precedent** (the `eval_ctx` doc comment above
treats `Shl`/`Shr` identically, and BUG-11's own historical fix covered
both) **rather than a fresh, independent Icarus repro for `Shr`
specifically in this task** — no `Shr`-shaped differential test was run
here; the inclusion is a reasoned extension of an already-ground-truthed
fact, not a second freshly-confirmed repro. So a `Shl`/`Shr` nested as a
direct operand of ANOTHER operator has EXACTLY the same "context escape"
problem BUG-23 fixed for lossless/wrap arithmetic: it was never added to
the check, on the strength of that function's own (incorrect, for these
two operators) doc-comment claim.

This makes BUG-24 the SAME underlying bug class as BUG-23 (a
context-determined-family operator escaping the hoist due to a wrong
exclusion from `is_width_effect_binop`), just for `Shl`/`Shr` instead of
`AddWrap`/`SubWrap`/`MulWrap`. It is NOT a missing self-determined
position, and `self_determined.rs` (Stage 4 Phase A1b's four checked
positions: concat member, replication's repeated part/count, comparison
operand, `$signed`/`$unsigned` argument) needed no change — the original
entry's "fifth self-determined position" framing, and its secondary
observation about `verilog_self_determined_kind`'s generic `_ =>` arm
computing `l.max(r)` for `Shl`/`Shr`, do not apply to this repro and are
withdrawn along with the rest of the original diagnosis (that function is
only ever consulted for the four A1b positions, none of which this
repro's shift sits in — the shift here is a plain sibling operand of `+`
and `>>`, reached only through `hoist_width_effect_operand`).

**How found.** BUG-23's own fix plan (Tasks 1-3), Task 4's confidence
verification pass: `cargo test --test differential_fuzz` at default N
passes 4/4 (confirmed twice — BUG-23 itself is genuinely fixed at that
level), but the deep pass
(`MIMZ_DIFF_FUZZ_N=500 cargo test --test differential_fuzz
differential_fuzz_matches_icarus`) found this different, reproducible
failure (confirmed deterministic across two re-runs). Root-caused by
tracing, then confirmed by manually hoisting `(p1 << extend(3,4))` into
its own dedicated 14-bit wire and re-running through real Icarus — this
reproduced the kernel's `y=51135944` exactly, confirming the fix
direction before it was implemented.

**Severity.** MEDIUM — silent wrong value (no crash, no compile error),
requiring a specific nested shape (a shift as a direct operand of a
sibling operator) that deep-N fuzzing needed 108 iterations to generate —
same severity class as BUG-19/20/21/22/23.

**Fix (as first landed — superseded below, see "Follow-on findings").**
Added `BinOp::Shl | BinOp::Shr` to `is_width_effect_binop`'s match arm in
`crates/mimz-core/src/emit_verilog/expr.rs`, alongside the existing
`Add | Sub | Mul | AddWrap | SubWrap | MulWrap`, and corrected that
function's doc comment (it no longer claims shift is context-immune; it
now explains why a shift's left operand belongs in the same
context-escape family as lossless/wrap arithmetic, while confirming
`&`/`|`/`^`/comparisons/logical-and-or genuinely remain safe to exclude).
The existing Task 1-3 hoisting machinery
(`hoist_width_effect_operand`/`hoist_slice_base_if_needed`, already wired
into every recursive-descent call site) then automatically covers `Shl`/
`Shr` the same way it already covers the lossless/wrap family — no other
code changes were needed. A matching correction was made to a stale
comment in `Builtin::Extend`'s render arm (which had used `1 << 3` as an
example of a "context-immune" expression — no longer accurate).

**This was too broad** (regressed BUG-6, see "Follow-on findings" below)
— `Shl`/`Shr` were split back OUT of `is_width_effect_binop` into their
own `is_shift_binop` predicate, hoistable only where an `allow_shift`
parameter confirms the position is safe. `is_width_effect_binop`'s match
arm, in the code as it stands now, is back to exactly
`Add | Sub | Mul | AddWrap | SubWrap | MulWrap` — this paragraph
describes the fix's first (incomplete) shape, kept here for the fix's
own history, not the current state of `is_width_effect_binop` itself.

**Test.** `tests/self_determined_regression.rs`'s
`bug_24_shl_under_sibling_add_matches_icarus`, using BUG-24's own filed
repro and vector (`p0=2024, p1=13855`) verified against real Icarus.
Confirmed RED (kernel `51135944` vs. Icarus `51004872`, the exact
originally-filed mismatch) before the fix, GREEN after. Also verified:
`cargo test -p mimz-core emit_verilog::` (unchanged emitter suite, no
behavior change for any program without a nested shift),
`cargo test --test self_determined_regression` (all 10, including
BUG-19/20/22/23's regressions), `cargo test --test differential_fuzz`
at default N (4/4), and the scoped deep-N completeness check
`MIMZ_DIFF_FUZZ_N=500 REQUIRE_IVERILOG=1 cargo test --test
differential_fuzz differential_fuzz_matches_icarus` (the exact level
BUG-24 itself was found at) — all pass.

**Follow-on findings (2026-07-20).** Landing this fix (and BUG-23's,
above) together, a full-workspace verification pass
(`cargo test --workspace --all-targets --no-fail-fast`, not something
Tasks 1-4 individually ran, each having scoped its own verification to
narrower suites) found two further regressions, both fixed before
either bug's status changed to FIXED. Neither ever shipped or was
independently filed as a numbered bug — both were found and fixed
within this same BUG-23/BUG-24 effort, before anything was committed:

- **Generic-width decls fallback.** `resolved_kind`
  (`emit_verilog/module.rs`) silently defaulted an unresolved
  generic-parameter width (e.g. a module's own `WIDTH: int = 8` generic
  feeding a `bits[WIDTH]` port) to 1 bit. The new hoist call sites above
  — plus this bug's own `Shl`/`Shr` addition to `is_width_effect_binop`
  — started reaching that fallback, silently truncating real hardware
  (`alu.mimz`, `shift_register.mimz`, confirmed via Icarus). Fixed by
  changing `resolved_kind`'s return type to `Option<Kind>` and having
  `build_decls` skip inserting a decl entirely when a width can't be
  resolved, rather than substituting a wrong value — `kind_is_inferrable`'s
  existing check then naturally refuses to hoist there.
- **Shift-hoist over-broadening.** This bug's own fix (adding `Shl`/`Shr`
  to `is_width_effect_binop`, above) initially hoisted a shift at EVERY
  call site — but the reference simulator only treats a shift's left
  operand as self-determined at SOME positions (see `is_shift_binop`'s
  doc comment in `crates/mimz-core/src/emit_verilog/expr.rs` for the
  full per-site rationale). The over-broad version regressed
  `examples/english/shift.mimz` (BUG-6's own historical guard). Fixed by
  splitting `is_shift_binop` out from `is_width_effect_binop` and adding
  an `allow_shift: bool` parameter to `hoist_width_effect_operand`,
  individually classified per call site against the simulator's actual
  source — `false` at the 4 unsafe positions (`Builtin::Extend`'s
  argument, `IfExpr`/`Match` branches, a shift's LHS when the outer
  operator is itself a shift), `true` everywhere else. Two new
  regression tests: `bug_24_regression_shift_in_if_branch_stays_unhoisted`
  and `bug_24_regression_nested_shift_lhs_of_shift_stays_unhoisted`
  (`tests/self_determined_regression.rs`).

## BUG-25 (MEDIUM, FIXED 2026-07-24) — Emitter panics on a nested `+%`/`-%`/`*%`/bitwise op with a narrower bare-literal operand

**What.** `emit_verilog::kinds::infer_binary`'s `AddWrap`/`SubWrap`/
`MulWrap`/`BitAnd`/`BitOr`/`BitXor` arm PANICS ("checker already
validated this operator's operand kinds: `KindMismatch { .. }`") when
one of these operators appears NESTED as a hoist-candidate child (see
BUG-23) and one operand is a bare integer literal narrower than its
sibling — e.g. `cnt +% 1 +% 1` with `cnt: bits[26]`, where the inner
`cnt +% 1` becomes the hoistable child. This is an ordinary,
checker-VALID program: `cnt +% 1` alone (not nested) compiles and runs
fine today, and would too if written as the sole assignment. Only the
NESTED shape reaches the buggy code path.

**Cause.** The checker's own width-matching (`checker::widths::ops::
matched_ty`) treats a bare integer literal as `Ty::CtInt` — untyped,
adapting to a sized sibling operand's width with no equality
requirement (`(Ty::CtInt(v), t) | (t, Ty::CtInt(v)) => { fit(v, t); t }`).
`emit_verilog::kinds::infer_kind`'s `Int` arm has no such context: it
always computes a literal's own MINIMAL natural width
(`min_width_for`), independent of any sibling operand. `infer_binary`'s
matched-width family then calls `width_rules::matched_result(l, r)`,
which requires the two `Kind`s to be IDENTICAL — so `cnt`'s `Kind{26,
false}` against literal `1`'s `Kind{1, false}` mismatches, an outcome
the checker never considered an error, and the `.expect(...)` panics.

**How found.** `fuzz_targets/pretty_roundtrip.rs`'s libFuzzer run in CI
(mutated a real example — `examples/*/led_blinker.mimz`'s counter
increment — into a doubled `cnt +% 1 +% 1`).

**Severity.** MEDIUM — a real, if narrow, correctness bug: any design
with a matched-family operator against a narrower bare literal, nested
under another operator that triggers BUG-23's hoist, crashes the
compiler instead of emitting. Not data corruption (a hard crash, always
caught immediately), and the un-nested form (the overwhelmingly common
shape in practice — a plain `reg <- reg +% 1` at statement level never
reaches this code path at all, per `hoist_width_effect_operand`'s own
doc comment) is unaffected.

**Fix (2026-07-24).** `infer_binary`'s matched-width arm
(`crates/mimz-core/src/emit_verilog/kinds.rs`) now checks, before
calling `matched_result`, whether either operand is a bare `ExprKind::
Int` — if so, the literal side's `Kind` is DISCARDED and the sibling's
`Kind` used directly (mirroring `matched_ty`'s own unconditional
adapt-to-peer arms), falling back to the existing `matched_result` check
only when neither side is a bare literal (preserving existing behavior
for the two-declared-signal case). New `is_bare_int` helper. Deliberately
narrow: a literal wrapped in `Unary` (e.g. a negated constant) is not
recognized by this check — out of scope for this fix, filed only if a
real case surfaces (see the fix's own code comment).

**Test.** `emit_verilog::kinds::tests::
wrap_add_with_a_narrower_bare_literal_adapts_to_the_sized_operand`
(`crates/mimz-core/src/emit_verilog/kinds.rs`) — checks both literal-on-
left and literal-on-right against a `bits[26]` sibling. Manually
verified end-to-end against the exact fuzzer-found input (lex → parse →
pretty-print → re-parse → emit both sides → `assert_eq!`), matching
`fuzz_targets/pretty_roundtrip.rs`'s own property.

---

## BUG-26 (LOW, FIXED 2026-08-01) — `mimz-sim`'s `resolve_module`'s own "unknown module" branch is dead code

**What.** `sim/elaborate/registry.rs`'s `resolve_module` has its own
"uses unknown module `{name}`" error, coded `S0101` (the sim-runtime
diagnostics catalog, R2). It never fires: `resolve_module`'s only
caller, `resolve_target`, always checks `reg.contains_key(&q.name.name)`
before calling `resolve_module` at all — so by the time `resolve_module`
runs, the name is already confirmed present in the module registry, and
its own `reg.get(...).ok_or_else(S0101)` branch can never see a miss.

**Cause.** `resolve_target` (added when the extern-module registry was
introduced) needed to try the real-module registry FIRST, falling back
to the extern registry on a miss — the `contains_key` pre-check exists
for that dispatch, not for `resolve_module`'s own benefit, but it has
the side effect of making `resolve_module`'s own unknown-name arm
unreachable from this, its only call site.

**How found.** Writing `crates/mimz-sim/tests/sim_errors.rs` (the R2
design's Phase 5 `S0xxx` fixture-per-code contract test) — a fixture
built to fire `S0101` (`let x = Bogus() {}`, `Bogus` undeclared
anywhere) instead fired `S0105` (`resolve_target`'s own combined
module-or-extern lookup miss), every time.

**Severity.** LOW — a diagnostics-catalog quality gap, not a functional
bug: a genuinely unknown bare module reference still produces a correct,
well-spanned error (`S0105`, worded identically to `S0101`'s own
message), just under the "wrong" of two codes that were meant to mean
slightly different things. No wrong behavior, no missed rejection.

**Fix (2026-08-01).** Deleted the dead arm rather than manufacturing an
artificial reachable path for it: `resolve_module`'s `reg.get(...)` now
`.expect()`s the invariant its only caller already guarantees
(`resolve_target`'s `reg.contains_key` pre-check), and `S0101` was
retired from `ALL_SIM_CODES` entirely (79 → 78 entries) — the
append-only stability contract never applied to a code that could never
fire in the first place, so nothing observable changes for any real
caller (`resolve_target`'s own combined-lookup miss still reports
`S0105`, exactly as it always did).

**Test.** `sim_errors.rs`'s `every_sim_code_has_a_fixture_above` no
longer lists `S0101` anywhere (removed from `known_gaps`, not moved to
`covered`) — the coverage check now simply has one fewer code to
account for. Full workspace 1115/1115, clippy/fmt clean.

---

## BUG-27 (LOW, FIXED 2026-08-01) — `mimz-sim`'s combinational-cycle diagnostic always loses its own code to `S0201`

**What.** `sim/comb.rs`'s `Env::resolve` constructs a well-worded,
correctly-coded `S0238` ("combinational cycle through {name} — feedback
must pass through a register") when it detects a signal already on its
own in-progress resolution stack. That `Diag` never survives to a
caller with `S0238` intact — it always arrives as `S0201`
instead, with the SAME message text.

**Cause.** A cycle can only be DETECTED on a re-entrant `Env::resolve`
call (the in-progress check requires a name to already be mid-resolution
elsewhere on the stack) — and every re-entrant call is reached through
`Env::signal` (`comb.rs`'s `Resolver::signal` impl), never through
`Env::resolve` directly. `Resolver::signal`'s trait signature is fixed
at `Result<Val, String>` (Phase 2's deliberate "leave the trait alone"
design, so the boundary never threads a `Span`/`Diag` through every
implementer), so `Env::signal` bridges `Env::resolve`'s result down to a
flat `String` via `.map_err(|e| e.msg)` — discarding `S0238` — and the
OUTER `value::eval_ctx`'s `Ident` arm then re-wraps that string as a NEW
`Diag` coded `S0201` (the generic "Resolver::signal failed" code), since
it has no way to know the original error already had a more specific
code of its own.

**How found.** Same as BUG-26 — a `sim_errors.rs` fixture built to fire
`S0238` (two wires driving each other) consistently produced `S0201`
instead, with `S0238`'s own message text intact underneath.

**Severity.** LOW — again a diagnostics-catalog quality gap: a
combinational cycle is still correctly REJECTED with a clear, accurate
message (feedback must pass through a register) and a real span; it
just can't be filtered/matched on the more specific `S0238` code the
catalog documents for it, since that code never actually reaches a
caller.

**Fix (2026-08-01).** Exactly the mechanism this entry's own "Fix
(Pending)" note anticipated: a code-prefix marker
(`sim::diag::BRIDGE_MARKER`, `bridge_code`/`diag_from_bridged`,
`crates/mimz-sim/src/sim/diag.rs`) — `Env::signal` (`comb.rs`) now
smuggles `resolve`'s own `Diag.code` through the bridged `String` instead
of unconditionally discarding it (`.map_err(|e| e.msg)` → matches on
`e.code` first), and `eval_ctx`'s `Ident`/`Index`/mem-read arms
(`value/mod.rs`) recover it via `diag_from_bridged` (validated against
`ALL_SIM_CODES`, so a plain non-bridged string — the common case, from a
`Resolver` with no code to preserve — is never misread as carrying one)
instead of always re-coding to the generic `S0201`/`S0206`. The
`Resolver` trait's own `Result<_, String>` signature is untouched, per
Phase 2's original design constraint.

**Found and fixed the same class in `sim/kernel.rs` too** (not part of
this bug's original filed scope, but the identical condition): `CombEnv::
signal`'s own combinational-cycle detection (the REAL multi-module
simulator's per-cycle resolver, used by `mimz sim`/`mimz test` — distinct
from `comb.rs`'s single-file `mimz eval` evaluator this bug was filed
against) built its cycle message as a bare `String` with no `Diag`/code
at all, so it ALSO always surfaced as `S0201`. Now constructs the same
bridged message reusing `S0238` (same reuse precedent as every other
structurally-identical condition shared across `elaborate/module.rs` and
`comb.rs` in this catalog).

**Test.** `sim_errors.rs`'s `s0238_combinational_cycle_fires_with_its_own_code`
(renamed from `..._condition_fires_recoded_as_s0201`) now asserts the
CODE via `assert_code`, not just message text; moved from `known_gaps` to
`covered` in `every_sim_code_has_a_fixture_above`. Full workspace
1115/1115, clippy/fmt clean.

---

## BUG-28 (CRITICAL, FIXED 2026-08-02) — `extend()` in a Verilog self-determined position emits an unsized operand → silent miscompile

**What.** A checker-clean, simulator-green program whose emitted Verilog computes
a **different value** under real Icarus. `extend(x, N)` is rendered as the bare
`(x)`, relying entirely on the enclosing assignment's context width to
zero/sign-extend. A concat member and a replication body are
**self-determined** positions with no such context, so the padding bits are never
materialized and every field to the left shifts down.

**Repro A — concat.**

```mimz
module CC { in a: bits[4]  in b: bits[4]  out y: bits[12]
  y = { b, extend(a, 8) } }

test "concat with extend" for CC { a = 0b1111  b = 0b1010
  expect y == 0b1010_0000_1111 }
```

```text
mimz test  → ok (1 passed)
emitted    → assign y = {b, (a)};
iverilog   → y = 000010101111     ✗  (expected 101000001111)
```

**Repro B — replication.**

```mimz
module R { in a: bits[2]  out y: bits[8]
  y = {2{ extend(a, 4) }} }

test "rep" for R { a = 0b11  expect y == 0b0011_0011 }
```

```text
mimz test  → ok (1 passed)
emitted    → assign y = {2{(a)}};
iverilog   → y = 00001111         ✗  (expected 00110011)
```

**Cause.** `crates/mimz-core/src/emit_verilog/self_determined.rs:22`.
`verilog_self_determined_kind` models Verilog's self-determined width for
`ExprKind::Binary` and for `$signed`/`$unsigned`, then falls through with
`_ => None` — documented as _"no Verilog-specific rule differs from mimz's own
here."_ `Builtin::Extend` lands in that arm, but its rule **does** differ:
Verilog gives the rendered `(x)` the **argument's own width**, never `N`.

The hoisting machinery that fixes this already exists (`hoist_if_needed`,
`__mimz_sub_N` in `emit_verilog/module/ports.rs:266`) and works correctly for
binary operators and for `if`/`match` branches — verified during the review:

```text
y = { b, a + b }   →   wire [4:0] __mimz_sub_1;
                       assign __mimz_sub_1 = (a + b);
                       assign y = {b, __mimz_sub_1};      ✓ matches iverilog
```

**This is BUG-19's fix, landed for operators and never extended to the builtin
table.** Same LRM rule (Verilog-2005 §5.4.1), same mechanism, uncovered case.

**How found.** CTO Architectural Review 2026-08-02
([`review-2026-08-02.md`](review-2026-08-02.md) F-1), by hand-probing every
`Builtin` in each self-determined position against `iverilog`/`vvp`.

**Why the existing oracles missed it.** Every differential oracle in the suite
compares **simulator vs. Verilog on generated programs**. The random-program
fuzzer's generator is documented as checker-clean _"by construction — every
combine step unifies operand widths via `extend()`"_
(`tests/differential_fuzz.rs:8`), which keeps every `extend` in a
**context-determined** position. The broken case only appears in a
self-determined one, which the generator never produces. See
[`gaps.md`](gaps.md) GAP-5.

**Severity.** CRITICAL — the worst failure mode an HDL compiler has. The design
verifies green in simulation and is wrong in silicon, with no diagnostic. It
survives `mimz check`, `mimz test`, the Icarus example suite, and the fuzzer.

**Fix (landed 2026-08-02, branch `bug-28-29-self-determined-extend-abs`).**
The `Call` arm is now exhaustive over `Builtin`, exactly as proposed below —
`extend`'s own two upstream gates (`kind_is_inferrable`, `infer_call`) already
classified it, so this one file was sufficient for `extend` specifically
(BUG-29's writeup below covers why `abs` needed two more files):

```rust
// crates/mimz-core/src/emit_verilog/self_determined.rs
ExprKind::Call { func, args } => match func {
    // `extend(x, N)` renders as bare `(x)` — Verilog gives it the
    // ARGUMENT's width in a self-determined position, never N. Report
    // that so the caller sees the mismatch against mimz's Kind{N} and
    // hoists to `wire [N-1:0] __mimz_sub_k`.
    Builtin::Extend => Some(Kind {
        width: self_determined_operand_width(&args[0], decls),
        signed: infer_kind(expr, decls).signed,
    }),
    Builtin::Abs => Some(Kind {          // BUG-29
        width: self_determined_operand_width(&args[0], decls),
        signed: infer_kind(expr, decls).signed,
    }),
    // `trunc` renders as an explicit part-select `x[N-1:0]` — already
    // exactly N bits in Verilog. Min/Max render to a ternary whose
    // operands are same-width by the checker's own rule, so max() == N.
    // Reductions are 1-bit on both sides. No mismatch possible.
    Builtin::Trunc | Builtin::Min | Builtin::Max
    | Builtin::Nand | Builtin::Nor | Builtin::Xnor => None,
    Builtin::SignedCast | Builtin::UnsignedCast =>
        verilog_self_determined_kind(&args[0], decls),
    Builtin::Clog2 => None,  // const-folded before emit
    Builtin::SyncDoubleFlop | Builtin::SyncPulse => None, // lowered to items
},
```

`Min`/`Max` were empirically confirmed correct-as-is during the review
(`y = { b, min(a, b) }` matches Icarus exactly); they are listed explicitly
rather than left to a wildcard so the reasoning is recorded at the site.

**Test (landed).** `bug_28_extend_in_concat_matches_icarus` and
`bug_28_extend_in_replication_matches_icarus`
(`tests/self_determined_regression.rs`) — both repros above, exact vectors,
run against real `iverilog`/`vvp`. Watched both fail first (kernel 2575/51 vs
Icarus 175/15, matching this entry's own numbers) before the fix. The static
half of the `Builtin` × position matrix (GAP-5's second ask) landed in the
same branch, same day — see GAP-5's own entry for what it covers and what it
deliberately doesn't (the fuzzer's own generator extension).

---

## BUG-29 (CRITICAL, FIXED 2026-08-02) — `abs()` in a Verilog self-determined position emits an unsized ternary → silent miscompile

**What.** Same class and same root cause as BUG-28, different builtin. `abs(x)`
on a `signed[N]` has result type `signed[N+1]` in mimz, but renders to a Verilog
ternary, which Verilog self-determines at `max(operand widths)` = `N` — one bit
short.

**Repro.**

```mimz
module Q { in a: signed[4]  in b: bits[4]  out y: bits[9]
  y = { b, unsigned(abs(a)) } }

test "abs concat" for Q { a = -8  b = 0b1010
  expect y == 0b1010_01000 }
```

```text
mimz test  → ok (1 passed)
emitted    → assign y = {b, $unsigned(((a < 0) ? (-a) : (a)))};
iverilog   → y = 010101000        ✗  (expected 101001000)
```

**Cause.** `crates/mimz-core/src/emit_verilog/self_determined.rs:22`,
`Builtin::Abs` falls into the `_ => None` arm. Note the `$unsigned(...)` wrapper
does **not** save it: `verilog_self_determined_kind` recurses into the cast's
argument, but that argument is a `Call`, which returns `None` too.

**How found.** CTO Architectural Review 2026-08-02
([`review-2026-08-02.md`](review-2026-08-02.md) F-1 Repro C).

**Severity.** CRITICAL — same reasoning as BUG-28. Silent, simulation-green,
hardware-wrong.

**Fix (landed 2026-08-02, branch `bug-28-29-self-determined-extend-abs`).**
The review's proposed `Builtin::Abs` arm in `self_determined.rs` (BUG-28's
entry) turned out to be **necessary but not sufficient** — traced the call
chain by hand before touching code and found two more gates that never knew
`Abs` existed:

- `emit_verilog/expr.rs::kind_is_inferrable` — the pre-check every hoist call
  site runs _before_ ever calling `hoist_if_needed`. Its `Call` arm allowed
  only `Extend|Trunc|SignedCast|UnsignedCast`; `Abs` fell to `_ => false`. This
  means `hoist_if_needed` was **never invoked for `abs(...)` at all**,
  regardless of what `self_determined.rs` said — patching that file alone is
  dead code for this builtin. Fixed by adding `Builtin::Abs` to the arm.
- `emit_verilog/kinds.rs::infer_call` — computes mimz's own `Kind` for the
  whole `abs(...)` expression (needed as the `mimz_kind` side of the mismatch
  comparison inside `hoist_if_needed`). Its `other => panic!(...)` catchall
  didn't handle `Abs` either — would have panicked the moment the gate above
  opened. Fixed by adding
  `Builtin::Abs => Kind { width: infer_kind(&args[0], decls).width + 1, signed: true }`,
  matching the checker's own rule (`checker/widths/ops/builtins.rs`,
  `Ty::Signed(n) → Ty::Signed(n + 1)`).

With all three gates agreeing, `abs`'s own render arm (`expr.rs:922-927`)
needed no change — the hoist happens one level up, in whichever caller
(`Concat`, `Replicate`, the `$signed`/`$unsigned` arm) embeds the rendered
ternary text.

**Test (landed).** `bug_29_abs_in_concat_matches_icarus`
(`tests/self_determined_regression.rs`) — the repro above, exact vector, real
`iverilog`/`vvp`. Watched it fail first (kernel 328 vs Icarus 168, matching
this entry's own numbers). A planned 4th test (`abs` under a bare top-level
`$unsigned`, no concat) was written and dropped — it passed even _before_ the
fix, since `$unsigned`'s argument self-determines correctly on its own
regardless of the classification gap; the mismatch only bites once the result
is embedded in another self-determined position, which the concat repro
already covers. The static half of the `Builtin` × position matrix (GAP-5's
second ask) landed in the same branch, same day — see GAP-5's own entry for
what it covers and what it deliberately doesn't (the fuzzer's own generator
extension).

---

## BUG-30 (HIGH, FIXED 2026-08-02) — A shift's declared type does not bound its value; naming an intermediate changes the result

**What.** Two expressions with the **identical declared type** `bits[4]` produce
different values, and the simulator and real Icarus both agree on the
difference — so it is the _type_ that is wrong, not either evaluator.

```mimz
module Ref { in din: bits[4]
  out direct: bits[8]   out named: bits[8]
  wire w: bits[4] = din << 2
  direct = extend(din << 2, 8)
  named  = extend(w, 8) }
```

```text
mimz test  → FAIL: direct = 60, named = 12
iverilog   → direct = 60, named = 12      (simulator and hardware agree)
```

`din << 2` is typed `bits[4]`, which claims the value is at most 15. With
`din = 15` it is 60 — a 6-bit value.

**Cause.** This is BUG-11's residue. BUG-11's fix threaded a real context width
through `Shl`/`Shr` so the simulator matches Verilog's context-determined shift
semantics — correct as far as it goes — but left the checker's `shift_ty`
asserting a width the value provably exceeds.
`crates/mimz-core/src/width_rules.rs:64` documents the split as intentional:

> a STATIC type-system invariant, not a claim about the runtime value

**How found.** CTO Architectural Review 2026-08-02
([`review-2026-08-02.md`](review-2026-08-02.md) F-2).

**Severity.** HIGH — not a wrong-value bug on its own (both evaluators agree),
but:

- `spec/01` sells "safe by default" and `spec/02 §1.1` sells _"the type system
  catches the classic dropped-carry bug."_ A width type that does not bound the
  value is not a safety property — it is a naming convention.
- It **breaks referential transparency**: extracting a subexpression into a named
  wire silently changes behavior. That is exactly the Verilog wart the language
  exists to remove, and the single hardest thing to explain to a beginner.

**Fix (landed 2026-08-02, branch `bug-30-self-determined-shifts`).** Chose
**(b) growing shifts**, not (a)'s self-determined-truncating option — real
Verilog's own `<<` already grows via context (ground-truthed by BUG-11), and
every peer HDL built specifically to fix Verilog's footguns (Chisel,
SpinalHDL, Bluespec) also grows rather than truncates; truncating would mean
`extend(din << 2, 8)` silently drops bits, the exact "dropped-carry" class
spec/01's "safe by default" claim exists to prevent. `<<` now grows: a
constant shift amount grows by exactly that amount; a runtime signal grows by
its own worst case (`2^width(amount) - 1`, Chisel's own rule). `>>` is
unchanged (`grows: false`) — right-shifting only ever reduces magnitude, so
it was never wrong.

- `width_rules::shift_result` gained `const_amount`/`grows` parameters
  (`crates/mimz-core/src/width_rules.rs`).
- The checker's `shift_ty` needed no width-rule change (it already typed
  shifts as self-determined) — just wiring the new parameters through
  (`crates/mimz-core/src/checker/widths/ops/mod.rs`).
- The emitter's `infer_binary` (`emit_verilog/kinds.rs`) computes
  `const_amount` from a bare literal shift amount; everywhere else the
  EXISTING BUG-24 hoist scoping (`hoist_width_effect_operand`'s `allow_shift`)
  turned out to already classify every position correctly for growing
  semantics too — self-determined positions (concat/replicate members, a
  non-shift sibling operator) already hoisted, and the context-determined
  positions it deliberately left un-hoisted (`extend`'s argument, `if`/`match`
  branches, a shift's own LHS when the outer op is also a shift) are provably
  harmless under growing (extra ambient width beyond the safe minimum is pure
  zero/sign-extension of an already-lossless value — no truncation-then-
  extend ordering bug is reachable anymore).
- The simulator's BUG-11 context-threading (`eval_ctx`, `expected_width`
  through `binary_ctx`/`binary_known`/`shl`/`shr`) became entirely dead once
  growing removed the need for it — deleted; `eval_ctx` collapsed back into
  plain `eval` (`crates/mimz-sim/src/sim/value/{mod,binary}.rs`).
- Found along the way: a directly-compiled parametric module (`reg sr:
bits[WIDTH]`) was silently absent from the emitter's own `decls` map
  (`resolved_kind` needs a concrete width; `self.env` never had `WIDTH`
  bound outside an instantiation), so hoisting silently never fired for any
  parametric-width signal — invisible until growing-shift's new hoist need
  exposed it (`trunc(sr << 1, WIDTH)` rendered as an illegal Verilog
  part-select of a compound expression). Fixed by binding each parameter's
  own default value into `self.env` for the duration of `build_decls` only
  (`emit_verilog/module/mod.rs`) — real per-instance Verilog overrides still
  render symbolically everywhere else. Known residual gap: the hoisted
  wire's width is the parameter's DEFAULT, not a per-instantiation override,
  so a real Verilog-level `#(.WIDTH(16))` override on a module whose default
  differs would size the hoisted wire wrong — pre-existing (every earlier
  hoist fix had the same silent-no-op gap for parametric widths, just never
  exercised until now), not newly introduced, and out of this fix's scope
  (needs symbolic-width-aware hoisting, GAP-1-adjacent).
- The classic shift-register idiom (`sr <- (sr << 1) | extend(din, WIDTH)`)
  now needs an explicit `trunc`: `sr <- trunc(sr << 1, WIDTH) | extend(din,
WIDTH)` — updated in `examples/*/shift_register.mimz` (5 flavors) and
  `showcase/*/uart_echo.mimz` (5 flavors), goldens regenerated.

**Test.** `tests/self_determined_regression.rs`'s
`bug_30_extend_of_a_shift_matches_a_named_wire_of_it` — the `Ref` module
above (wire retyped `bits[6]`, matching the new grown type) as both a direct
kernel assertion (`direct == named == 60`) and an Icarus differential
fixture. `width_rules.rs`/`value/tests.rs` gained unit coverage for the
growth formula (constant vs. dynamic amount, signedness, `MAX_WIDTH`
overflow). The width-conformance property described in [`gaps.md`](gaps.md)
GAP-5 (assert every simulated `Val` fits its declared width) remains open —
this fix's own targeted tests cover BUG-30 specifically, not that broader
class.

---

## BUG-31 (MEDIUM, FIXED 2026-08-04) — `E0403` emits the clock/reset help line for an enum used as data

**What.** The inline `= help:` line is unrelated to the error being reported.

```text
error[E0403]: enum `S` is not data
  --> p.mimz:9:12
   |
  9 |   y = { a, s }
   |            ^
   = help: clocks and resets only appear in `on rise(clk)` and module connections
           — they never enter expressions (spec/02 section 1.2)
```

The user wrote an enum in a concat. The help talks about clocks and resets.

**Cause.** `Checker::not_data` (`crates/mimz-core/src/checker/widths/mod.rs:429`)
is documented as the shared _"clocks/resets are not data"_ helper and hardcodes
that one help string, but it is also reached for enum-typed operands.

`mimz explain E0403` gives the correct three-case text (signed/bits mixing, enum
as number, clock/reset as data) — but the inline line is what users actually
read.

**How found.** CTO Architectural Review 2026-08-02
([`review-2026-08-02.md`](review-2026-08-02.md) F-7).

**Severity.** MEDIUM — no wrong hardware, but it actively misdirects a learner,
which is a direct hit on G1 (teaching diagnostics). The affected path is also the
one users hit while looking for the missing enum→bits cast
([`gaps.md`](gaps.md) GAP-7), so the wrong help compounds a real expressiveness
gap.

**Fix.** Branched the help on the `Ty` variant inside `not_data`
(`crates/mimz-core/src/checker/widths/mod.rs`): `Enum` → "an enum is a
symbolic state, not a number — match on it, or add an explicit encoding if
you need its bits"; `Memory`/`Array` → "index it (`m[addr]`) to get one
element"; `Bundle` → "access one field (`bus.field`) to get data"; everything
else (`Clock`/`Reset`) keeps the original text, now correctly scoped to its
own case.

**Test.** `enum_in_concat_is_e0403_with_enum_specific_help`
(`crates/mimz-core/src/checker/tests/widths.rs`) — watched it fail against the
old hardcoded text, then pass; `clock_in_a_data_expression_is_e0403` extended
with its own help-content assertion so the clock/reset case can't silently
regress.

---

## BUG-32 (MEDIUM, OPEN) — `mem` lowers to an `initial` block: FPGA-only, not ASIC-synthesizable, and unresettable

**What.** `examples/english/regfile.mimz` emits:

```verilog
reg [(8)-1:0] m [0:(4)-1];
integer __mimz_m_i;
initial for (__mimz_m_i = 0; __mimz_m_i < (4); __mimz_m_i = __mimz_m_i + 1) m[__mimz_m_i] = 0;
```

`initial` is inferred by Vivado/Quartus into BRAM init contents. It is **ignored
by every ASIC synthesis flow** (Genus, Design Compiler) — the array powers up
undefined.

**Cause.** Memory initialization has exactly one lowering, chosen for FPGA
inference, with no target awareness and no alternative.

**How found.** CTO Architectural Review 2026-08-02
([`review-2026-08-02.md`](review-2026-08-02.md) F-5).

**Severity.** MEDIUM — correct on the primary target (FPGA, per `spec/01`
_"for digital circuits (FPGAs first)"_), silently wrong elsewhere. The severity
is driven by the **documentation claim**, not the lowering: the language's stated
"no uninitialized state" guarantee evaporates on ASIC, and
`examples/english/regfile.mimz`'s own comment — _"there is no reset line because a
memory initializes itself"_ — is true on FPGA and false on ASIC.

**Related gaps.** No way to express a memory reset; no memory-style attribute
(`(* ram_style = "block" *)` / `ramstyle`); no init-from-file (`$readmemh`); no
explicit dual-port or read-during-write policy. The inference outcome is
therefore entirely at the vendor's discretion and unspecifiable from source. See
[`gaps.md`](gaps.md) GAP-8b.

**Fix.**

1. Emit a synthesis-flow note in the generated header comment when any `mem` is
   present, and document the ASIC caveat in `docs/guide/` and in the `regfile`
   example's comment. **(Do this first — it is the honest-framing fix and it is
   nearly free.)**
2. Add an optional memory-attribute syntax that lowers to the vendor pragma.
3. Long-term: model memories as an IR node ([`gaps.md`](gaps.md) GAP-1) with an
   explicit read-during-write policy, so the emitter picks a correct template per
   target rather than hoping the vendor infers one.

**Test.** A golden test asserting the header note appears when a `mem` is
present; a docs-sync assertion tying the guide's memory chapter to the emitted
form.

---

## BUG-33 (LOW, FIXED 2026-08-03) — Perf test asserts an absolute throughput floor; the repo is red on slower machines and the failure masks 880 tests

**What.** `tests/sim.rs:221` asserts a hardcoded simulator throughput floor.
On the review machine:

```text
counter kernel: 641750 cycle-events/sec (best of 5, debug=false)

thread 'the_counter_kernel_clears_the_perf_baseline' panicked at tests\sim.rs:221:5:
counter kernel too slow: best 641750 cycle-events/sec < 1000000

test result: FAILED. 16 passed; 1 failed
```

**Cause.** An absolute constant (`1_000_000`) is used as a pass/fail gate. It
fails on any slower machine, any shared CI runner under load, any laptop on
battery, and every contributor whose box is slower than the author's.

**How found.** CTO Architectural Review 2026-08-02
([`review-2026-08-02.md`](review-2026-08-02.md) F-9) — first `cargo test` run on
a clean checkout of `bb79838`.

**Severity.** LOW on correctness, **MEDIUM on contributor experience**, because
of a secondary effect: `cargo test` stops after the first failing binary unless
`--no-fail-fast` is passed, so this one failure **masks 13 suites / 880 tests**
locally.

```text
cargo test --workspace --release                  → 22 suites,  234 passed, 1 failed
cargo test --workspace --release --no-fail-fast   → 35 suites, 1114 passed, 1 failed
```

A first-time contributor sees a red repo and an incomplete run.

**Note on precedent.** The project already made the opposite (correct) call in
CI, which runs `cargo bench --no-run` with the comment _"microbench timings on
shared runners are too noisy to gate on."_ This test contradicts that reasoning.

**Fix (2026-08-03).** The hard assert now only runs when `MIMZ_PERF_GATE=1` is
set (mirrors `REQUIRE_IVERILOG`'s opt-in-hard-fail convention); ungated, the
rate is still printed but a below-floor result is a warning, not a failure.
CI's PR-facing `check` job runs it ungated (matches `cargo bench --no-run`'s
existing "too noisy for a shared runner" stance); `nightly-bench` sets
`MIMZ_PERF_GATE=1` so the floor is still enforced somewhere
(`.github/workflows/ci.yml`). Trending the number in `bench-history.jsonl`
was not done — out of scope for the XS-effort fix; `mimz-bench`'s own
history mechanism is a separate harness from this test.

**Test.** No new test — the fix is to the gate itself; `tests/sim.rs`'s
existing perf test now demonstrates both branches (verified manually: ungated
passes on this machine's ~640k cycle-events/sec result; `MIMZ_PERF_GATE=1` in
release still hard-fails on the same machine, exactly reproducing this
entry's own repro number).

---

## BUG-34 (HIGH, FIXED 2026-08-03) — Chained shifts (`(x >> a) << b`) diverge from Verilog when the inner operand is signed

**What.** A right-shift immediately consumed by an outer left-shift, with no
`extend()` between them, computes a different value than real Verilog when
the shifted operand is `signed`. Minimal repro (verified directly against
`iverilog`/`vvp`, not just the fuzzer's own harness):

```mimz
module Fuzz {
  in p2: signed[16]
  out y: signed[23]
  y = ((p2 >> extend(4, 5)) << extend(7, 3))
}
```

```text
p2 = -9563 (raw 55973)
mimz eval  → y = 447744
iverilog   → y = -76544   (bit pattern 8312064)
```

**Cause (confirmed).** Matches the same class of divergence BUG-11 first
characterized: real Verilog context-extends a shift's left operand to the
_enclosing_ width **before** either shift executes — since `p2` is signed,
that context extension is a sign extension. BUG-30's "growing shifts" fix
(`width_rules::shift_result`) computes bottom-up instead: the inner
`p2 >> 4` resolves at `p2`'s own self-determined 16-bit width first (a
LOGICAL, zero-fill shift, since it's `>>` not `>>>` — confirmed correct in
isolation, see the single-shift check below), and only widens _after_,
once the outer `<< 7` consumes it. For a single shift this bottom-up order
is indistinguishable from Verilog's top-down context propagation — BUG-30's
own regression tests are single-shift and stayed green — but for a
**chained** shift-of-shift with no intervening `extend()`, the two orders
diverge: sign-extending `p2` to 23 bits _before_ the first shift (real
Verilog) is not the same value as shifting `p2` at 16 bits _then_
sign-extending the result to 23 bits (what growing-shifts did pre-fix).

Isolated single-shift sanity check (matches expectation, not itself broken):

```verilog
reg signed [7:0] x = -8;
x >> 1    // 124  (logical, zero-fill — correct, `>>` is never arithmetic)
x >>> 1   // -4   (arithmetic, sign-fill — correct, `>>>` on a signed operand)
```

**Confirmed NOT a referential-transparency loophole in Verilog's favor.**
Materializing the inner shift into a named wire first —
`wire w: signed[16] = p2 >> 4; y = w << 7` — was checked directly against
`iverilog` too: it gives `447744` (the OLD, pre-fix kernel value), not
`-76544`. So real Verilog itself treats a FUSED shift chain differently
from the same shift split across a named signal — an inherent Verilog
subtlety, not a mimz language-design question. BUG-34's fix only needed to
make the simulator replicate Verilog's own fused-expression sizing
faithfully; it does not touch (and should not touch) the named-wire case,
which both the simulator and real Verilog already agree on.

**How found.** `tests/differential_fuzz.rs`'s existing kernel-vs-Icarus
differential (`differential_fuzz_matches_icarus`), run at `MIMZ_DIFF_FUZZ_N
= 100` (seed 12648521, i=35) while validating GAP-5's new width-conformance
assertion — past the default `N=20` CI runs, so this was never caught by
any default `cargo test`. Not a false positive from the new width-
conformance oracle itself (that assertion never fired on any of the 35
prior iterations); this is the pre-existing kernel-vs-Icarus `assert_eq!`
catching a genuine divergence.

**Severity.** HIGH — silent miscompile of the same "simulator passes,
hardware disagrees" shape as BUG-11/BUG-28/BUG-29, on a construct
(chained shifts on a signed value with no `extend()` between them) that is
unremarkable, not adversarial input.

**Fix (2026-08-03, branch `bug-34-chained-signed-shifts`).** A scoped
revival of BUG-11's context-threading, deliberately narrow (not the full
general mechanism BUG-30 deleted — see `docs/audit/bugs.md` BUG-30's own
note on why that was removed): `crates/mimz-sim/src/sim/value/binary.rs`
gained `collect_shift_chain` (walks an expression's left spine through
consecutive `Shl`/`Shr` nodes, returning the non-shift BASE expression plus
the ordered `(op, amount)` chain, innermost-first) and `eval_shift_chain`
(resolves each step's `Kind` bottom-up first — identical rule the old
per-node dispatch already used, purely to learn the chain's FINAL width —
then extends the BASE operand to that final width ONCE, sign-extending iff
signed, and folds every step as a plain logical shift at that fixed width).
`crates/mimz-sim/src/sim/value/mod.rs`'s `eval()` routes every `Shl`/`Shr`
AST node through this new path instead of the old per-node
`eval(lhs)? then binary_ctx`; a lone shift (chain-of-one) is not a special
case — extending a value to its own unchanged width is a no-op, so it's
byte-identical to the old behavior. The low-level `shl`/`shr` primitives
themselves are unchanged (still directly callable, still correct — they're
what `eval_shift_chain` calls internally after the base is pre-extended),
so nothing outside the AST-driven evaluator's dispatch needed to change.

**Test.** `chained_signed_shift_context_extends_before_the_shift`
(`crates/mimz-sim/src/sim/comb.rs`) — an integration-level test through
`eval_outputs` on real parsed source (a unit test hand-chaining `binary_ctx`
calls, tried first, can't observe this bug at all: it exercises exactly the
per-node primitives that stay correct and unchanged). Watched fail
(`447744`) against the pre-fix code first, green (`8312064`) after. Full
`mimz-sim` unit suite (158 tests) and the full workspace
(`cargo test --workspace --all-targets --release`) both green; the
differential fuzzer re-run at `MIMZ_DIFF_FUZZ_N`/`MIMZ_DIFF_FUZZ_CLOCKED_N
= 200` (past BUG-34's own discovery seed at i=35) is clean. Verified
end-to-end via the CLI too: `mimz eval` on this entry's own repro now
prints `y = -76544`, matching `iverilog` exactly.

---

## BUG-35 (HIGH, FIXED 2026-08-04) — A shift whose left operand is a builtin call is not hoisted in a self-determined (concat) position

**What.** `nand(p1) << extend(5, 3)` — a shift whose LEFT OPERAND is a
builtin call, not a plain identifier or arithmetic expression — sitting
directly inside a concat member is not hoisted into its own
`__mimz_sub_N` wire, so the emitted Verilog computes it self-determined at
1 bit (the width of `nand(p1)` alone) instead of mimz's own declared
growth width. Minimal repro (verified directly against `iverilog`, not
just the fuzzer's own harness):

```mimz
module Fuzz {
  in p0: bits[7]
  in p1: bits[9]
  out y: bits[1]
  y = (extend(15736, 15) <= {((p0 *% p0) | extend(p1[8:7], 7)), (nand(p1) << extend(5, 3))})
}
```

```text
p0 = 55, p1 = 110
mimz eval  → y = 1
iverilog   → y = 0
```

Emitted Verilog for the offending member: `((~&(p1)) << 3'd5)` — bare,
un-hoisted, sitting directly in the concat literal. The sibling member
(`p0 *% p0`) DOES get hoisted (`__mimz_sub_2`), confirming the hoist
mechanism runs at this call site in general; it specifically fails for
this shift.

**Cause (hypothesis, not yet root-caused).** Same class as BUG-19/BUG-24
(a self-determined-position shift that should be hoisted but isn't) and
the same underlying mechanism BUG-28/29 fixed for `extend`/`abs` — but
those fixes were about the SHIFT/BUILTIN ITSELF being unclassified in
`self_determined.rs`'s exhaustive match. Here the shift operator IS
classified (an ordinary `<<`, already correctly hoisted in every prior
test case, including the differential fuzzer's own pre-existing
`combine_shift` outputs). The new element is specifically the LEFT
OPERAND being a builtin call (`nand(p1)`) rather than a plain identifier
or arithmetic expression — `kind_is_inferrable`/`infer_call`
(`crates/mimz-core/src/emit_verilog/{expr,kinds}.rs`) likely fail to
correctly propagate an inferable `Kind` through a `Nand`/`Nor`/`Xnor`
(or more generally, some subset of builtins) argument position when that
builtin's OWN result then feeds a shift's hoist-eligibility check,
causing `hoist_if_needed` to silently skip it.

**How found.** GAP-5 direction 2's fuzzer position-aware generation
(`tests/differential_fuzz.rs`'s new `wrap_builtin`, this session) — the
first random-generation pass to place a builtin call as a shift's own
left operand, inside a self-determined (concat) position. Not reachable
by the pre-existing generator (which only ever shifted a plain port
reference or a same-width-combinator result, never a builtin call).

**Severity.** HIGH — silent miscompile of the same "simulator passes,
hardware disagrees" shape as BUG-11/BUG-19/BUG-24/BUG-28/BUG-29.

**Fix (2026-08-04).** Confirmed the hypothesis exactly: `kind_is_inferrable`
(`crates/mimz-core/src/emit_verilog/expr.rs`) and `infer_call`
(`crates/mimz-core/src/emit_verilog/kinds.rs`) both had no arm for
`Nand`/`Nor`/`Xnor`/`Min`/`Max` — the former fell to `_ => false`, the
latter to `other => panic!(...)`. `self_determined.rs`'s own exhaustive
`Builtin` match already classified all five correctly (`None` — reductions
are 1-bit in both models, `min`/`max` render to a same-width ternary in
both), so no bug lived there; the gap was purely on the mimz-side
inference, which made the ENCLOSING expression (the shift, here)
"un-analyzable" and skip the hoist machinery entirely — not because the
existing hoist logic was wrong, but because it never ran. Added real
`infer_call` arms: reductions → `Kind{width:1,signed:false}` (matches the
checker's own `Ty::Bit` rule); `min`/`max` → the same "a literal adapts to
its sized sibling" rule `infer_binary`'s `AddWrap`/`BitAnd` arm already
uses, mirroring the checker's own `matched_ty` call for these two. Added
matching `kind_is_inferrable` arms (single-arg recursion for the three
reductions, both-args for `min`/`max`). This alone unblocks the
already-correct `hoist_width_effect_operand`/`hoist_if_needed` mechanism
for the shift — no change needed to either.

Fixing `min`/`max`'s literal-adapts check surfaced a second, real bug along
the way: reusing `infer_binary`'s existing `is_bare_int` helper (which
deliberately does NOT recognize a negated literal — see its own doc
comment, scoped to that one caller's own repro) broke a real, shipped
example — `showcase/pid_controller.mimz`'s `max(-128, min(total, 127))` —
panicking on a `Kind` mismatch (`Kind{8,false}` vs `Kind{16,true}`) the
checker never considered one (`-128` parses as `Unary{Neg, Int(128)}`, not
a bare `Int`). Added `is_ct_int_like` (recognizes a literal OR a negated
literal), scoped to the `Min`/`Max` arm only — `AddWrap`/`BitAnd`'s
narrower `is_bare_int` is untouched, per its own documented scoping.

**Test.** `bug_35_shift_with_a_builtin_call_left_operand_in_a_concat_matches_icarus`
(`tests/self_determined_regression.rs`) — BUG-35's own filed repro and
vector, watched fail against real Icarus first (kernel says 1, Icarus says
0, matching the filing) via a temporary `git stash` of the fix, then pass
after. The full workspace suite (`cargo test --workspace --all-targets
--no-fail-fast`, `REQUIRE_IVERILOG=1`) stayed green, including the
pre-existing `matrix_min_in_concat_matches_icarus`/`matrix_max_in_concat_matches_icarus`/
`matrix_nand_in_concat_matches_icarus`/`matrix_nor_in_concat_matches_icarus`/
`matrix_xnor_in_concat_matches_icarus` GAP-5 position-matrix tests, and
`showcase_emitted_verilog_matches_goldens`/`showcase_pure_tamil_match_goldens`
after regenerating the two goldens this fix legitimately changed (see
BUG-36's own entry below — same root fix, same side effect).

---

## BUG-36 (HIGH, FIXED 2026-08-04) — `trunc()` of a non-identifier expression emits an invalid Verilog part-select

**What.** `trunc({p0, extend(39, 7)}, 15)` — `Builtin::Trunc` applied to
a CONCAT expression rather than a bare identifier — is accepted by the
checker but emits `{p0, __mimz_sub_1}[(15)-1:0]`: a Verilog part-select
(`[hi:lo]`) applied directly to a `{...}` concatenation literal, which
`iverilog` rejects outright:

```text
error: syntax error in continuous assignment
```

(reproduced live: the full source is
`module Fuzz { clock clk  reset rst  in p0: bits[11]  in p1: bits[13]
in p2: bits[15]  reg r0: bits[5] = 0  out y: bits[15]  on rise(clk) {
r0 <- unsigned(signed(extend(6, 5))) }  y = trunc({p0, extend(39, 7)}, 15)
}` — `iverilog` fails to elaborate the emitted Verilog at all, a hard
compile failure, not a value mismatch).

**Cause.** This is **BUG-20's own class** (`docs/audit/bugs.md`,
FIXED for the raw-slice/`clamp()`-fallback case): Verilog's part-select
grammar only accepts an identifier before `[...]`, not an arbitrary
expression. `Builtin::Trunc`'s codegen renders as an explicit part-select
`x[N-1:0]` (`self_determined.rs`'s own doc comment: _"already exactly N
bits in Verilog"_) — correct when `x` is a plain identifier, but this
codegen path has no guard for `x` being a compound expression (a concat,
in this repro) needing hoisting into a named wire FIRST, the same way
`p0 *% p0` in BUG-35's own repro gets hoisted before use. BUG-20's
original fix covered the checker/emitter's generic slice (`ExprKind::
Slice`) path; `Builtin::Trunc`'s OWN separate codegen site was never
updated to match.

**How found.** GAP-5 direction 2's fuzzer position-aware generation
(`tests/differential_fuzz.rs`'s new `wrap_builtin`), same session as
BUG-35 — `wrap_builtin`'s `Trunc` candidate can pick ANY fragment from
the pool as its argument, including a `combine_concat` result, which the
pre-existing generator's own `clamp()` fallback was already careful to
avoid (`clamp`'s doc comment explicitly documents BUG-20 and only ever
slices a bare port identifier) — `wrap_builtin` reopened the same class
through a different call site (`trunc()` itself, not a raw slice).

**Severity.** HIGH — hard compile failure (not silent miscompile) for a
checker-accepted program; `mimz check`/`mimz test` pass clean, `mimz
compile`'s own output fails to elaborate under any real Verilog
toolchain.

**Fix (2026-08-04).** `Builtin::Trunc`'s codegen
(`crates/mimz-core/src/emit_verilog/expr.rs`) gained the exact same
hoist-non-identifier-bases-first treatment `ExprKind::Slice`'s BUG-20 fix
already applies: after `hoist_width_effect_operand`'s existing
width-effect-binop/shift check (BUG-23/24's own, unrelated concern — value
correctness, not part-select grammar), the base is additionally passed
through `hoist_slice_base_if_needed` (guarded by `kind_is_inferrable`, to
avoid panicking on a `fn`-body/testbench base outside `decls`) — the same
unconditional-on-shape check `ExprKind::Slice` uses, since a Verilog
part-select only ever accepts a plain identifier regardless of what Kind
mismatch analysis would say.

**Side effect, not a regression.** This fix also corrected a
previously-undetected instance of the SAME bug in two shipped showcase
examples, reached through a different shape than the filed repro:
`pid_controller.mimz`/`pid_kattu.mimz`'s
`control = trunc(max(-128, min(total, 127)), 8)` — a ternary base, not a
concat — was already emitting `(cond ? a : b)[(8)-1:0]` directly. Confirmed
with a standalone minimal repro that real `iverilog` already rejected this
exact shape (`syntax error in continuous assignment`) BEFORE this fix too —
it had simply never been caught, since neither example is in the
Icarus-differential-tested list (`tests/icarus.rs`), only the
checker-clean/compiles-without-erroring one (`tests/showcase.rs`'s
`showcase_every_example_compiles`, which never invokes `iverilog` at all).
This bug therefore didn't need `kind_is_inferrable`'s Min/Max-argument
change (BUG-35's own fix) to be _reachable_ — a ternary base was always a
non-identifier shape — but it DID need it to be _caught by this fix_: the
Trunc codegen's new `kind_is_inferrable(&args[0], &decls)` guard was
`false` for `max(...)` before BUG-35's fix (falling through `_ => false`),
so the hoist would have silently skipped it exactly as before. Fixing both
bugs together is what makes this example correct.
`tests/golden/showcase_pid_controller.v` and
`tests/golden/showcase_tamil_pure_pid_kattu.v` regenerated
(`MIMZ_UPDATE_GOLDENS=1`); the diff is exactly the expected extra
`__mimz_sub_N` hoist wire (renumbering every later one), confirmed to
elaborate clean under `iverilog -t null` afterward.

**Test.** `bug_36_trunc_of_a_concat_hoists_the_base_first`
(`tests/self_determined_regression.rs`) — a simplified concat-base repro
(the filed repro's clock/reset/reg machinery was incidental, not
load-bearing), watched fail against real `iverilog` first (the exact
"syntax error in continuous assignment" the filing reported) via a
temporary `git stash` of the fix, then pass after. Full workspace green
per BUG-35's entry above (same fix, same verification pass).

---

## BUG-38 (MEDIUM, OPEN) — `mimz-sim`'s combinational-only kernel rejects every enum-typed signal, port or wire

**What.** The checker fully accepts an enum-typed module port or wire (e.g.
`examples/english/tagged_packet.mimz`'s `in bus: Packet`, or a plain
`wire state: Light = ...`), and the CLOCKED simulator path already handles
an enum `reg` correctly (`examples/*/traffic_light.mimz`'s own
`reg state: State`, run daily via `mimz test`). But the STANDALONE
combinational-only kernel (`mimz_sim::sim::comb::eval_outputs`) rejects
**any** enum-typed signal outright:

```text
signal of enum type `Light` — the simulator does not model enum signals yet
```

This fires for a port AND for a plain internal `wire`, with no way around
it in the comb-only entry point.

**Cause.** `crates/mimz-sim/src/sim/value/mod.rs`'s `type_width` explicitly
errors on `Type::Named` (an enum) — a deliberate, documented stub, not an
accident. The general/clocked elaborator
(`crates/mimz-sim/src/sim/elaborate/module.rs`) never calls `type_width`
directly for a signal's type: it wraps every call through its own
`width_of()` (`module.rs:246-275`), which special-cases `Type::Named` FIRST
(resolving the enum's `inferred_total_width`, tag+payload, from the
checker) and only falls through to `type_width` for every other type.
`crates/mimz-sim/src/sim/comb.rs` — a separate, standalone comb-only
elaborator, built for fast differential testing
(`tests/self_determined_regression.rs`'s own doc comment: "there is no
existing single-call helper of this exact shape anywhere in the suite") —
calls `value::type_width` **directly** for both `Port` (`comb.rs:239`) and
`Wire` (`comb.rs:246`) declarations, missing the `width_of()`-style enum
wrapper entirely. This is an incomplete port of `elaborate/module.rs`'s own
enum-handling to the second, parallel elaboration path `comb.rs`
represents — the same "two implementations of one rule disagreed" shape as
GAP-1's own width/kind-duplication family, just for signal WIDTH
RESOLUTION rather than an operator rule.

**How found.** Writing GAP-7's (`encoding(e)` enum→bits cast,
`docs/audit/gaps.md`) Icarus differential tests in
`tests/self_determined_regression.rs`: an enum-typed port, then an
enum-typed `wire`, both driven through `differential()` (which calls
`comb::eval_outputs`), both rejected identically. Switching to
`differential_clocked()` (`elaborate_project`/`run`, the general path)
worked immediately with no other change beyond the entry point.

**Severity.** MEDIUM — no silent miscompile (a clean, named error, not a
crash or a wrong value), but it blocks a checker-accepted, spec-legal
program from running through half of `mimz-sim`'s own public API
surface. Anything built on `comb::eval_outputs` (this test file today;
potentially a future `mimz eval`-style comb-only tool) cannot exercise
enum signals at all.

**Fix.** Give `comb.rs` the same `width_of()`-style `Type::Named` wrapper
`elaborate/module.rs` already has — ideally by extracting `width_of()` into
a shared helper both elaborators call, rather than duplicating the
enum-resolution logic a second time (the GAP-1 lesson: a third copy of the
same rule is a third place for it to drift).

**Test.** A `comb::eval_outputs` unit/integration test driving an
enum-typed port and an enum-typed wire directly (no clock), asserting a
successful evaluation with the expected bit pattern — the comb-only
counterpart to `traffic_light.mimz`'s existing clocked coverage.

---

## BUG-39 (MEDIUM, OPEN) — A `reg`'s reset value cannot be a payload-carrying `EnumConstruct` expression

**What.** `reg p: Packet = Packet.Ctrl(0)` — a `reg` reset value calling a
payload-carrying enum variant's constructor, even with an all-literal
argument — fails elaboration:

```text
this expression is not a compile-time constant
```

A **tag-only** variant reset (`reg state: Light = Light.Red`,
`examples/*/traffic_light.mimz`'s own working pattern) is unaffected —
only a variant that takes arguments hits this.

**Cause.** `crates/mimz-sim/src/sim/elaborate/module.rs`'s `Reg` arm
(`module.rs:392-402`) first rewrites the reset expression through
`self.rw0().expr(reset)` — which lowers any `ExprKind::EnumConstruct` via
`rewrite.rs`'s `enum_construct()` (documented there: "Produces a plain
`ExprKind::Int` for a tag-only (zero-arg) variant, or an `ExprKind::Concat`
otherwise — both already fully evaluated by `crate::sim::value`, so no new
interpreter code is needed") — then passes the REWRITTEN expression to
`const_eval_wide`, which calls straight into
`mimz_core::checker::consteval::eval` (the compile-time-only constant
folder shared with the checker). That folder has no `ExprKind::Concat` arm
at all (confirmed: no match on it in `consteval.rs`), so it falls through
to the generic `_ => not_const(...)` catch-all — even though, in this
case, every part of the concat (the tag index, the argument) is itself a
literal. The doc comment on `enum_construct()`'s own rewrite is candid
about the mismatch: it was written assuming the RUNTIME evaluator
(`crate::sim::value::eval`, which DOES handle `Concat`) would consume the
result, not the narrower, reset-value-only `consteval::eval` path — reg
resets are the one place in the pipeline that specifically needs the
latter, and nothing routes an `EnumConstruct`'s rewritten `Concat` back
through it.

**How found.** Same GAP-7 differential-test work as BUG-38, same session
— `reg p: Packet = Packet.Ctrl(0)` was the first, more natural design for
a payload-enum clocked test (toggling `p` between two constructed variants
every cycle); worked around by making `p` a combinational `wire` instead
(no reset value needed) rather than fixing this.

**Severity.** MEDIUM — again a clean, named error rather than a silent
miscompile, but it blocks a checker-accepted, spec-legal declaration
(nothing in `spec/02`'s enum-construction rules requires the reset
argument to be anything other than a compile-time constant, which `0`
plainly is) from ever reaching the simulator. Any design using a
payload-carrying enum as register state — the natural "current
transaction" idiom a tagged union exists for — cannot declare a reset
value for it without this workaround.

**Fix.** Teach `consteval::eval` (or a `mimz-sim`-local wrapper around it,
scoped to reg/mem reset values specifically, if extending the shared
`mimz-core` function is out of bounds for its own "plain `int`/`bool`
values" contract) to fold an all-constant `ExprKind::Concat` — recursively
const-evaluating each part and bit-packing the results, mirroring exactly
what the runtime evaluator already does for the same node shape. This
would also transitively make a tag-only variant reset expressed with
explicit call syntax (`Light.Red()`) constant-foldable if it isn't
already, for the same reason.

**Test.** A reg-reset-with-payload-enum-constructor case
(`reg p: Packet = Packet.Ctrl(0)`) added to `mimz-sim`'s elaboration test
suite, asserting successful elaboration and the correct initial bit
pattern (tag + zero-padded/literal payload).

---

## BUG-40 (MEDIUM, FIXED 2026-08-07) — `pattern_matches`'s `unreachable!()` fires on a raw `Pattern::Variant`, crashing CI's fuzz job

**What.** The weekly/PR `lex_parse_eval` fuzz job crashed with a deadly
signal:

```text
internal error: entered unreachable code: Pattern::Variant is lowered to
IntMask during elaboration — raw variants should not reach pattern_matches
```

The reported artifact is a byte-mutated (`ShuffleBytes-CMP-CopyPart`)
version of `examples/english/priority.mimz` (the don't-care/`casez`
match-pattern showcase). The mutation garbled the match block's arm text
so badly that it no longer resembles the original `0b1??`/`_` patterns at
all — but it still **lexes and parses cleanly** (the fuzz harness returns
early on any lex/parse failure, so it must), because the parser only
checks pattern _syntax_, not whether a name after a dot resolves to
anything real.

**Cause.** The mangled text parsed into a single match arm whose pattern
is `Pattern::Variant { enum_name: "s", variant: "_" }` — i.e. `s._`,
referencing an enum `s` that is declared nowhere in the file. That is
exactly the kind of error only the **checker** (E0101/E0103, "unknown
name"/"unknown enum") is supposed to catch — and `mimz_sim::sim::
comb::eval_outputs`, the standalone combinational-only evaluator the fuzz
harness calls **directly**, deliberately never runs the checker (that is
the fuzz target's own stated purpose: prove raw, untrusted,
checker-unchecked input can never panic it).

Tracing the two elaboration paths side by side:

- The **clocked/general** path (`crates/mimz-sim/src/sim/elaborate/
module.rs` → `rewrite.rs`) always lowers every `Pattern::Variant` to a
  plain `Pattern::IntMask` before evaluation, and `rewrite.rs`'s own
  `pattern()` function validates the enum name exists first (`self.enums.
get(...)`, erroring `S0115` if not).
- `crates/mimz-sim/src/sim/comb.rs` — a second, standalone comb-only
  evaluator (its own doc-comment precedent: "there is no existing
  single-call helper of this exact shape anywhere in the suite") — has
  **no such lowering pass at all**. It assumes every pattern reaching it
  is already an `IntMask`, true for every hand-written or
  generator-produced input, but not for arbitrary fuzzed bytes.

So a `Pattern::Variant` reaches `pattern_matches`
(`crates/mimz-sim/src/sim/value/mod.rs`) completely unlowered, hitting an
`unreachable!()` whose invariant only actually holds on the clocked path
— the same "two implementations of one rule disagreed" shape as GAP-1's
own width/kind-duplication family (`docs/audit/gaps.md`), here for pattern
lowering rather than an operator rule. Related but distinct from
[BUG-38](#bug-38-medium-open--mimz-sims-combinational-only-kernel-rejects-every-enum-typed-signal-port-or-wire)/
[BUG-39](#bug-39-medium-open--a-regs-reset-value-cannot-be-a-payload-carrying-enumconstruct-expression)
(also `comb.rs`/enum gaps found the same day, but about enum _types_, not
match _patterns_) — filed separately since the crash mechanism and fix
site are unrelated.

**How found.** CI's scheduled/per-PR `fuzz-nightly`/`fuzz` job on
`lex_parse_eval` (`fuzz/fuzz_targets/lex_parse_eval.rs`). Reproduced
locally (Windows dev box has no nightly libFuzzer toolchain, so
`cargo fuzz run` isn't available here) by decoding the crash artifact's
byte dump and replaying the fuzz harness's exact call sequence (lex →
parse → `comb::eval_outputs` with empty/seeded/edge-case inputs) in a
throwaway `cargo test`, which hit the identical panic at the identical
line.

**Severity.** MEDIUM — a clean CI-blocking crash on malformed input, not a
silent miscompile or a reachable-by-a-real-user path (the CLI's
`mimz test`/`mimz sim`/`mimz compile` all run the checker first, which
would reject an unknown enum name with E0101 long before this code runs).
Still a real "no crash on any input" violation of this audit's own threat
model, and it blocks the CI fuzz job from passing.

**Fix.** `pattern_matches`'s `Pattern::Variant` arm
(`crates/mimz-sim/src/sim/value/mod.rs`) no longer panics — it returns
`false` (never matches) instead of `unreachable!()`. Minimal, targeted fix
over the alternative (teaching `comb.rs` a full enum-pattern lowering
pass): `comb.rs` is a narrow, special-purpose evaluator never meant to
support real enum matching, so the safety-net fix (never panic on
unchecked input) is the right-sized one, not a feature port. It also
matches this exact code's own pre-existing sibling error message
one arm down — `S0202`, "no `match` arm matched the value (**enum
patterns are not evaluated yet**)" — which already anticipated precisely
this case in its wording.

**Test.** `pattern_matches_never_panics_on_an_unlowered_variant`
(`crates/mimz-sim/src/sim/value/tests.rs`) — a direct unit test
constructing a `Pattern::Variant` for a nonexistent enum and asserting
`pattern_matches` returns `false` rather than panicking, watched fail
against the real `unreachable!()` first. `match_pattern_referencing_an_
unknown_enum_is_a_clean_err_not_a_panic` (`crates/mimz-sim/src/sim/
comb.rs`) — the full end-to-end path, right alongside the file's own
existing `zero_length_array_param_index_is_a_clean_err_not_a_panic` (same
"fuzz: lex_parse_eval" regression class): a single-arm match with no
wildcard fallback, asserting a clean `S0202` error rather than a crash.
Full workspace green (1188/1188, `REQUIRE_IVERILOG=1`), `fmt`/`clippy -D
warnings` clean.

---

## BUG-41 (CRITICAL, FIXED 2026-08-08) — The self-determined hoist gate is not exhaustive: BUG-28/29 reopen through `fn` calls, instance ports, `if`, `match` and `mem` reads

**What.** `extend()`/`abs()`/lossless arithmetic in a Verilog self-determined
position emit unsized operands again — the exact BUG-28/BUG-29 failure, with
byte-identical wrong Verilog — whenever the operand contains a `fn` call, an
instance port, an `if`/`match` expression, or a `mem` read. Simulator green,
hardware wrong, no diagnostic.

Five reproductions, each `mimz test` ok and each wrong under `iverilog 12.0`:

```text
① y = { b, ident4(a) + a }        emitted {b, (ident4(a) + a)}      iv 010101110    vs sim 101011110
② y = { b, s.q + a }              emitted {b, (s_q + a)}            iv 010101110    vs sim 101011110
③ y = { b, (if s { a } else { b }) + a }
                                  emitted {b, (((s)?(a):(b)) + a)}  iv 010101110    vs sim 101011110
④ y = { b, m[raddr] + m[raddr] }  emitted {b, (m[raddr]+m[raddr])}  iv 010101110    vs sim 101011110
⑤ y = { b, extend(ident4(a), 8) } emitted {b, (ident4(a))}          iv 000010101111 vs sim 101000001111
```

⑤ is byte-identical to BUG-28's original Repro A output. `extend` of an instance
port is the same; `abs` of an `if` expression reproduces BUG-29's `010101000`.
A shift in the same position (`{ b, ident4(a) << 1 }`) is equally broken.

**Cause.** `crates/mimz-core/src/emit_verilog/expr.rs:59` `kind_is_inferrable`
is the gate every hoist call site runs before `hoist_if_needed`, and therefore
before `verilog_self_determined_kind` is ever consulted. It ends in two
wildcards — `_ => false` at `:103` (over `Builtin`) and `:105` (over
`ExprKind`). The second swallows `FnCall`, `Field`, `IfExpr`, `Match`, `Index`,
`BundleLit`, `ArrayLit` and `EnumConstruct`, so any expression _containing_ one
is declared non-inferrable and the hoist silently never fires. BUG-28/29's fix
made the **classifier** (`self_determined.rs`) exhaustive; the **gate** was left
non-exhaustive, and BUG-35 already proved once that the gate alone is sufficient
to cause the miscompile.

Comparison operands and `$signed`/`$unsigned` arguments survive by accident —
Verilog's context-determination leaks a width in from the sibling or the
assignment target. The damage is concentrated in concat members and replication
bodies, the two positions with no context to leak.

**How found.** CTO review 2026-08-07, sweeping every `ExprKind` shape through
every self-determined position rather than every `Builtin` through one position.
Pre-existing, not a regression: `git show bb79838:.../expr.rs` has the same two
wildcards.

**Severity.** CRITICAL. Silent miscompile of ordinary RTL (a `fn` call or a
module instance inside an arithmetic expression inside a concat). Survives
`mimz check`, `mimz test`, the Icarus example suite, and the differential
fuzzer.

**Fix (2026-08-08).** Landed differently from the proposal above, and for a
stated reason: the two hand-synced matches (`kind_is_inferrable`'s gate and
`kinds::infer_kind`'s classifier) collapsed into ONE — `infer_kind` itself
now returns `Option<Kind>` directly, `kind_is_inferrable` deleted outright
rather than made exhaustive alongside it, so there is nothing left to
hand-sync. `Index`/`FnCall`/`Field`/`IfExpr`/`Match` now resolve through
`decls` (`build_decls` precomputes an instance-port/memory-element/`fn`-
return `Kind` under reserved keys). The "hoist conservatively" half of the
proposal was explicitly declined — a `wire` declaration needs a concrete
width, which `None` by definition doesn't have — so `None` still means
"skip the hoist," a deliberate choice (`kinds.rs`'s own doc comment) whose
consequence is [BUG-48](#bug-48-critical-fixed-2026-08-09--two-more-exprkind-shapes-fall-through-infer_kind-reopening-bug-2829-with-byte-identical-output):
`infer_kind` collapsing the SYNC-failure surface from two matches to one
did not, on its own, make the remaining one exhaustive. `SEC-10` closed in
the same change — `infer_kind`'s `panic!` is gone with `kind_is_inferrable`.

**Test.** All five filed reproductions as Icarus differentials in
`tests/self_determined_regression.rs`:
`bug_41_fn_call_operand_of_add_in_concat_matches_icarus`,
`bug_41_instance_port_operand_of_add_in_concat_matches_icarus`,
`bug_41_if_expr_operand_of_add_in_concat_matches_icarus`,
`bug_41_mem_read_operand_of_add_in_concat_matches_icarus`,
`bug_41_extend_of_a_fn_call_in_concat_matches_icarus`. Revert-checked
(`review-2026-08-09.md`): disabling the five classified arms with
`#[cfg(any())]`, falling back to `_ => None` (the retired gate's own
behavior), reproduces real Icarus differentials on exactly those five
tests and nothing else. `matrix_shape`'s own exhaustive-`Builtin`
enforcement (the proposal's second half, for the OTHER axis) is unrelated
scope — see [GAP-13](gaps.md) for the `ExprKind` axis this fix still
lacked, closed 2026-08-09.

---

## BUG-42 (CRITICAL, FIXED 2026-08-08) — `min`/`max` misclassified as "no mismatch possible" in `verilog_self_determined_kind`

**What.** `min`/`max` whose operand is itself a width-mismatched sub-expression
emit an unsized ternary, so Verilog self-determines it at the _rendered_ operand
width rather than at mimz's result width.

```mimz
module MM {
  in p: signed[6]
  out y: bits[11]
  y = unsigned(min(extend(p, 11), extend(p, 11)))
}
```

```text
emitted    assign y = $unsigned((((p) < (p)) ? ((p)) : ((p))));
mimz eval  y = 2039   (0b11111110111, sign-extended — correct)
iverilog   y = 55     (0b00000110111, zero-extended — wrong)
```

**Cause.** `crates/mimz-core/src/emit_verilog/self_determined.rs:72` classifies
`Min | Max => None` on the reasoning that "min/max render to a ternary whose
operands are same-width by the checker's own rule, so max(operand widths) == N."
That is true of **mimz's** widths and false of **Verilog's self-determined**
widths — the exact distinction the file exists to model. `extend(p, 11)` renders
as the bare `(p)`, i.e. 6 bits, not 11. The `SignedCast`/`UnsignedCast` arm
handles this correctly by recursing into the argument; `Min`/`Max` does not.

This is the third instance of the BUG-28/BUG-29 class, and it was introduced
**by their fix** — the recommended patch in `review-2026-08-02.md` listed
`Min | Max` under the "no mismatch possible" arm, and it was implemented
verbatim, comment included.

**How found.** The project's own differential fuzzer, at `MIMZ_DIFF_FUZZ_N=400`
(CI default is 20): `differential_fuzz_matches_icarus`, seed 12648675, vector 0
— kernel `y=82574208`, Icarus `y=2579328`. Minimized by hand to the module
above.

**Severity.** CRITICAL. Silent miscompile; `min`/`max` over an `extend`ed
operand is the standard clamp idiom.

**Fix.** `Min | Max` now recurses into both operands exactly like
`SignedCast`/`UnsignedCast` already did, taking `max` of their own
self-determined widths (mirroring how the generic binary-operator arm
above it already handles every other operator):

```rust
Builtin::Min | Builtin::Max => Some(Kind {
    width: self_determined_operand_width(&args[0], decls)?
        .max(self_determined_operand_width(&args[1], decls)?),
    signed: infer_kind(expr, decls)?.signed,
}),
```

Re-audited `Trunc`/`Nand`/`Nor`/`Xnor`/`Encoding`/`SignedCast`/
`UnsignedCast` under the corrected question — "is this argument's
_rendered_ width necessarily its mimz width?", not "is this operator's
_result_ width necessarily its mimz width?" All survive unchanged:
`Trunc` (explicit part-select, exactly N bits, base already hoisted by
BUG-36), the reductions (1 bit both sides), `Encoding`/`SignedCast`/
`UnsignedCast` (already recurse). The corrected question is now recorded
in the file's own module doc comment so the next builtin is classified
against it, not the one that shipped this bug.

**Test.** `bug_42_min_max_mismatched_operand_matches_icarus`
(`tests/self_determined_regression.rs`) — the exact repro above, watched
fail against the pre-fix code (kernel 2039, Icarus 55) before
implementing. Full workspace green (`REQUIRE_IVERILOG=1 cargo test
--workspace --release --no-fail-fast`, 1212 tests); `fmt`/
`clippy -D warnings` clean.

---

## BUG-43 (CRITICAL, FIXED 2026-08-07) — A negative literal is evaluated at its own natural width, so the simulator disagrees with the hardware

**What.** The simulator evaluates `-N` inside `natural_width(N)` bits, producing
a small **positive** value that then zero-extends into its signed destination.
The emitter is correct, so this is a straight simulator-vs-hardware divergence.

On a `signed[6]` port:

| source | simulator | correct |
| ------ | --------- | ------- |
| `-1`   | 1         | 63      |
| `-3`   | 1         | 61      |
| `-5`   | 3         | 59      |
| `-9`   | 7         | 55      |
| `-16`  | 16        | 48      |
| `-17`  | 15        | 47      |

The rule is `-n` becomes `2^natural_width(n) - n`; it is correct only when
`natural_width(n)` equals the target width, which is why BUG-29's Repro C
(`a = -8` on `signed[4]`) passed by coincidence.

End-to-end:

```mimz
wire w: signed[8] = -1
b = unsigned(w)
```

```text
emitted   assign w = (-1);  assign b = $unsigned(w);
mimz test FAIL: b = 1
iverilog  b = 255                    <- correct
```

Realistic clamp idiom (the shape `showcase/pid_controller.mimz` ships, with
different constants), `x = -1000`:

```text
y = max(-100, min(x, 100))
mimz eval  y = 28     WRONG
iverilog   y = -100   correct
```

A comparison against a negative literal is silently false: `q == -9` with
`q = -9` gives `0` in the simulator and `1` in Verilog.

**Cause.** `crates/mimz-sim/src/sim/value/binary.rs:55`:

```rust
UnOp::Neg => {
    let bits = v.as_i128().wrapping_neg() as u128;
    Val::new(bits, v.width, true)      // v.width = the LITERAL's natural width
}
```

`Val::from_int(9)` carries `width = 4`, so `-9` is negated inside 4 bits and
becomes `0b0111 = 7`. `Val` has no unsized (`Ty::CtInt`-equivalent)
representation, so the literal cannot adapt to its context the way the checker's
own `Unary::Neg` arm lets a `CtInt` adapt.

**How found.** CTO review 2026-08-07, while minimizing a fuzz divergence; the
`signed[6]` port sweep above pins the rule exactly.

**Severity.** CRITICAL, and broader than BUG-28/29: those broke a syntactic
position, this breaks a _value_ wherever it appears — wire initializers,
comparisons, `min`/`max` bounds, `fn` arguments, `test` stimulus. `reg` reset
values happen to be correct (width-aware path), which is why it has gone
unnoticed: the most visible use of a negative constant is the one that works.

**Fix.** Three sites, not one — the filing pinned the first, and the other two
only became visible once the value stopped being wrong at the source.

1. **`crates/mimz-sim/src/sim/value/mod.rs`** — `-<literal>` is folded as a
   constant in `eval`, matched on shape before the general `Unary` arm, via a
   new `Val::negated_literal`. It builds the value at `natural_width(n) + 1`
   bits, signed: one extra bit is exactly what a two's-complement negation
   needs, matching `Val::from_int`'s own `129 - leading_ones` rule but computed
   over `Bits`, so an arbitrarily-wide literal (BUG-13 layer 2, past `i128`) is
   served too. The runtime `-x` path is deliberately unchanged —
   `signed[N] -> signed[N]` matches Verilog, and `abs`'s explicit `N -> N+1`
   growth shows the language already made that call.
2. **`value::resize_to_width`, replacing three copies.** With the value now
   correctly a narrow SIGNED `Val`, the consumer dropped the sign:
   `wire w: signed[8] = -1` read back as 3 while the emitted Verilog said 255.
   `remask_to_width` existed three times — `comb.rs`, `kernel.rs`, and
   `value/mod.rs` — each documented as _"a pure reinterpret, NOT a
   sign-extending resize"_. Now one function: truncate when `w <= v.width`,
   otherwise fill from the SOURCE's signedness via `wide::extend`. Both
   duplicate copies deleted. Zero-padding was harmless only while every
   sub-width value reaching it was an unsigned literal, and the checker enforces
   exact width matching everywhere except literals — so the widening path _is_
   the literal path. Fixing one copy would have left the other two wrong.
3. **`Sim::set_val`** (`crates/mimz-sim/src/sim/kernel.rs`), used by
   `TestStmt::Drive` (`sim/harness/mod.rs`). `mimz test` still drove the wrong
   stimulus after both fixes above: the harness evaluated the expression to a
   `Val` and then handed `Sim::set` a raw `v.bits_masked()` pattern, destroying
   the width and signedness before `set` could act on them. `set` cannot be
   fixed in place — with only a raw pattern there is no source width to extend
   from — so the typed path is a separate entry point, and `set` keeps its
   raw-`Bits` signature for peripherals and clock/reset toggles.

**Test.** Eight, across the three layers the three sites live in:

- `crates/mimz-sim/src/sim/value/tests.rs` —
  `negated_literal_sign_extends_into_a_wider_signed_slot` (the exact
  `signed[6]` table from this filing),
  `negated_literal_minus_one_is_all_ones_not_one`, and
  `negated_literal_handles_a_wide_magnitude` (>128 bits, which `from_int`
  cannot serve).
- `tests/self_determined_regression.rs` — four Icarus differentials
  (`bug_43_negative_literal_in_a_wire_matches_icarus`,
  `..._comparison_...`, `..._clamp_idiom_...`, and
  `..._in_a_reg_reset_...`). The last pins the path that was **already
  correct** so the fix cannot regress it. A sim-only test cannot prove this
  bug fixed: the emitter was right all along and only the simulator moved.
- `tests/test_run.rs` —
  `a_negative_test_input_drives_its_twos_complement_pattern`, the whole table
  through the real binary, since site 3 was in the harness's drive path
  specifically.

Verified against the filing's own reproductions: `max(-100, min(x, 100))` with
`x = -1000` now gives `-100` (was 28), `unsigned(w)` for
`wire w: signed[8] = -1` now gives 255 (was 1), and `q == -9` with `q = -9` is
now true (was false) — each matching Icarus. Full workspace green (1196/1196,
35 suites, `REQUIRE_IVERILOG=1`), `fmt`/`clippy -D warnings` clean.

Still to do, tracked elsewhere: add this case to the width-conformance oracle
once [GAP-11](gaps.md) makes it non-vacuous — the current oracle checks only
top-level signal widths, and this bug's wrong value fits its declared width, so
the oracle would not have caught it.

---

## BUG-44 (HIGH, FIXED 2026-08-08) — `trunc` of a `signed` value renders as an always-unsigned Verilog part-select

**What.** `MIMZ_DIFF_FUZZ_CLOCKED_N=400`,
`differential_fuzz_clocked_matches_icarus`, seed 202427830, cycle 1: kernel
`y=13806`, Icarus `y=13726` (delta 80).

```mimz
module Fuzz {
  clock clk
  reset rst
  in p0: bits[9]
  in p1: bits[11]
  reg r0: signed[8] = 0
  out y: bits[14]
  on rise(clk) { r0 <- signed(extend(236, 8)) }
  y = ((unsigned((signed(extend(3, 3)) * trunc(r0, 3)))
       - (min(extend(3, 3), extend(3, 3)) * {extend(3, 3), extend(63, 7)}))
       *% extend(p1[4:3], 14))
}
```

**Cause.** `trunc(x, N)` **keeps** its operand's signedness. Three independent
authorities agree on that:

| Authority | Site                                      | Rule                             |
| --------- | ----------------------------------------- | -------------------------------- |
| checker   | `checker/widths/ops/builtins.rs`          | `Ty::Signed(_) => Ty::Signed(n)` |
| simulator | `sim/value/fn_eval.rs` (`Builtin::Trunc`) | `Val::new(.., v.signed)`         |
| emitter   | `emit_verilog/kinds.rs` (`infer_call`)    | `signed: base_signed`            |

The emitted **text** does not. `trunc` renders as an explicit part-select
`x[(N)-1:0]` (BUG-36's own fix), and a part-select is unconditionally
**unsigned** in Verilog-2005 (IEEE 1364-2005 section 5.1.7) even off a
`signed` wire. So the one thing that actually reaches Icarus disagrees with all
three.

Two distinct consequences, both live:

1. **Mis-extension.** Into a wider signed target the value zero-extends where
   mimz sign-extends.
2. **Demotion of the surrounding expression.** Verilog makes an arithmetic
   expression unsigned if _any_ operand is unsigned, so the unsigned
   part-select also discards a sibling operand's own `$signed`. This is the
   shape the fuzz seed took, and the one that produced the reported delta.

Walking the filed seed confirms it exactly: `r0` is `signed[8] = -20`
(`0b11101100`), so `trunc(r0, 3)` is `0b100` — `-4` to mimz, `+4` to Verilog.
`3 * -4 = -12` (`unsigned` -> 52) against Verilog's `3 * 4 = 12`; `52 - 1341 =
-1289` -> 15095 against `12 - 1341 = -1329` -> 15055; `* 2 mod 2^14` gives
**13806** against **13726**. BUG-42 was a candidate but is not the cause — the
divergence survives its fix, and `min`/`extend` are not on the path.

`ExprKind::Slice` deliberately needs no such treatment:
`width_rules::slice_result` types a slice `signed: false` (BUG-21), which is
precisely Verilog's own rule. `trunc` is the **only** construct in the emitter
where a mimz-signed value renders as an always-unsigned Verilog construct.

**Why the position matrix missed it.** `self_determined.rs` classifies
`Builtin::Trunc => None` ("no mismatch possible"), and Task 3's re-audit
(`docs/plan/v0.2-correctness-remediation.local.md`) explicitly re-confirmed
that arm: _"an explicit part-select is exactly N bits regardless of the base."_
True — of `width`. But `Kind` carries **`width` and `signed`**, and every arm in
that table was re-audited on the width half alone. Same
two-implementations-of-one-rule family as BUG-41/BUG-42, one field over.
Task 4's `debug_assert!` cannot see it either: it checks a bare identifier's
`decls` entry against the caller's `mimz_kind`, never a rendered construct's
kind against the mimz kind it claims to represent.

**How found.** CTO review 2026-08-07, extended fuzz run (400 seeds, 54.81 s).
Minimized 2026-08-08 by back-solving the reported delta to the single
sub-term that could produce it, then confirming against the emitted Verilog —
`assign y = (a[(3)-1:0]);` for a `signed[8] -> signed[6]` truncation.

**Severity.** HIGH, confirmed on minimization — a silent miscompile of ordinary
code (any `trunc` of a `signed` value), and it was **miscompiling a shipped
showcase**: `showcase/english/pid_controller.mimz` emitted
`(__mimz_sub_4[(16)-1:0] + (d_diff))`, where the unsigned part-select demoted
the whole add and zero-extended the signed derivative term. Not fuzz-only.

**Fix.** Wrap the rendered part-select in `$signed(...)` when the truncated
operand's own `Kind` is signed (`emit_verilog/expr.rs`, `Builtin::Trunc`) — the
emitted text then carries the signedness all three authorities already assign
it. An unresolvable base `Kind` keeps the previous unsigned rendering, the same
accepted residue every other `infer_kind` call site there already has.

**Test.** `bug_44_trunc_of_a_signed_value_stays_signed_in_verilog` (the
minimal mis-extension: `extend(trunc(a, 3), 6)`, `a = 236` — kernel 60 vs
Icarus 4 pre-fix) and
`bug_44_trunc_of_a_signed_value_as_a_multiply_operand` (the demotion the seed
took: kernel 52 vs Icarus 12 pre-fix), both in
`tests/self_determined_regression.rs`, both watched fail before the fix. Seed
202427830 passes; both fuzzers green at `N=400`. Seed 202427830 still belongs
in the replayed corpus of [GAP-11](gaps.md) (Task 5).

---

## BUG-45 (HIGH, FIXED 2026-08-08) — Hoisted wires spliced in before the `reg`/`wire`/`mem`/instance declarations they reference

**What.** Every hoisted `wire __mimz_sub_N` / `assign` pair (BUG-19/20/23/28/
29/36/41's shared mechanism) is spliced into the module body at `fn_pos` —
right after the port list, before enum localparams, `wire`/`reg`/`mem`
declarations, and instances. Any hoist whose rendered text references a
`reg`, a plain `wire`, a memory, or an instance's auto-wired output port
(`{inst}_{port}`) therefore forward-references a declaration that appears
_later_ in the file. Icarus Verilog 14.0 rejects this outright:

```text
error: Unable to bind wire/reg/memory `r0' in `diff_tb.uut'
      : A symbol with that name was declared here. Check for declaration after use.
error: Unable to elaborate r-value: (11'd1)-(r0)
```

Confirmed byte-for-byte pre-existing: reproduced on an unmodified checkout
(`git stash` before any BUG-41 work), and hit by content nobody was
touching — the shipped `examples/english/shift_register.mimz` example, a
random `differential_fuzz_clocked` seed (202427634), and this repo's own
pre-existing `bug_23_wrap_under_sibling_add_inside_a_concat_matches_icarus`
regression test all fail the identical way. Icarus 12.0 (this project's own
audit baseline) evidently tolerates the ordering; 14.0 does not — nothing
about the emitted Verilog's _meaning_ depends on declaration order (Verilog
module items are order-independent by the language spec), only this
specific tool's strictness.

**Cause.** `emit_verilog/module/mod.rs`'s `module()`: `fn_pos` is captured
immediately after the port-list header, then a single `inject` string
(the `clog2` helper, injected user `fn` bodies, and `self.hoisted_decls`)
is spliced there at the end — before wire/reg/mem declarations and before
`emit_instances` ever run, even though hoisting itself only happens later,
during `emit_drives`/`seq_stmts`' expression rendering.

**How found.** Discovered verifying BUG-41's fix in this environment
(Icarus 14.0) — 2 of 5 new BUG-41 regression tests (instance-port operand,
memory-read operand) and 4 pre-existing tests/examples failed this way,
none related to BUG-41's own classification logic.

**Severity.** HIGH — silently breaks compilation (not simulation) of any
design where a hoist happens to reference a `reg`/`wire`/`mem`/instance
port, which is the ordinary case, not a corner one; masked wherever the
toolchain's Icarus build tolerates forward references.

**Fix.** A second insertion point, `hoist_pos`, captured right after
`emit_instances(&m.items)` (i.e. after every `wire`/`reg`/`mem` declaration
and every instance's auto-wired output). `self.hoisted_decls` is spliced
there instead of at `fn_pos`; the `clog2`/user-`fn` injection stays at
`fn_pos` (function declarations and call sites are not order-sensitive in
Verilog, confirmed empirically — no test regressed). The two `insert_str`
calls run in `hoist_pos` order first, `fn_pos` second, since `fn_pos <
hoist_pos` and inserting at the larger offset first leaves the smaller one
valid.

**Test.** `tests/self_determined_regression.rs`'s
`bug_41_instance_port_operand_of_add_in_concat_matches_icarus` and
`..._mem_read_operand_...` now pass; the pre-existing `bug_23_wrap_under_
sibling_add_inside_a_concat_matches_icarus` and `differential_fuzz_clocked_
matches_icarus` (seed 202427634) regressions are unblocked; `examples/
english/shift_register.mimz` and `showcase/*/pid_controller.mimz`
(all 4 language flavors) compile and elaborate clean. Full workspace green
(`REQUIRE_IVERILOG=1 cargo test --workspace --release --no-fail-fast`),
`fmt`/`clippy -D warnings` clean. Goldens regenerated
(`MIMZ_UPDATE_GOLDENS=1`) and `crates/mimz-wasm/pkg` rebuilt
(`wasm-pack build crates/mimz-wasm --target web --release`) to match the
new (cosmetic-only) declaration order.

**Note.** Fixing this unmasked two unrelated, independently pre-existing
bugs in hand-written self-checking testbenches (never reached before
because elaboration always failed first in this environment) — see the
`sc_pid_controller_tb.v` and `sc_vga_pattern_tb.v` fixes in the same commit
(wrong hand-calculated expected values / an off-by-one tick count; the
design and emitter were both already correct) — and surfaced a third,
separate emitter defect, filed and later fixed as [BUG-46](#bug-46-medium-fixed-2026-08-08--truncs-base-hoist-does-not-cover-a-module-parameter-so-the-part-select-lands-on-a-composite-expression).

---

## BUG-46 (MEDIUM, FIXED 2026-08-08) — `Trunc`'s base-hoist does not cover a module parameter, so the part-select lands on a composite expression

**What.** `showcase/english/melody_player.mimz` emits:

```verilog
dur_cnt <= ((dur) * TICK)[(32)-1:0];
```

a Verilog part-select (`[...]`) applied directly to a parenthesized `*`
expression — a syntax error in any Icarus version (`Malformed statement`),
unlike [BUG-45](#bug-45-high-fixed-2026-08-08--hoisted-wires-spliced-in-before-the-regwirememinstance-declarations-they-reference)'s version-sensitive strictness. `TICK` is a module
`parameter (TICK = 50000)`.

**Cause.** `Builtin::Trunc`'s base-hoist (`emit_verilog/expr.rs`, BUG-36's
own fix) only unconditionally hoists a non-identifier base when
`kinds::infer_kind` can resolve its `Kind` (needed to size the hoisted
wire). Module parameters are deliberately excluded from `cur_decls`/
`build_decls` (kept symbolic, per that function's own doc — a real
per-instance override, not a fixed width), so `infer_kind(dur * TICK,
decls)` is `None` (`TICK` unresolvable), and the hoist that BUG-20/36
otherwise guarantee for a composite `Slice`/`Trunc` base never fires here.

**How found.** Discovered as a side effect of fixing BUG-45 (the ordering
bug) — this file's `self_checking_showcase_testbenches_pass` test never
reached this line before because Icarus errored on an earlier, unrelated
declaration-order issue first. Confirmed byte-for-byte pre-existing on an
unmodified checkout (`git stash`) — unrelated to BUG-41/BUG-45 or anything
else touched on this branch.

**Severity.** MEDIUM — breaks compilation (a clear, loud failure, not a
silent miscompile) of any design that truncates/slices a width-effect
expression involving a module parameter; `melody_player.mimz` is the first
shipped example to do so.

**Fix.** Not the symbolic-wire approach originally sketched above —
`kinds::infer_kind` now resolves a module-parameter `Ident` after all,
rather than teaching its callers to hoist without a `Kind`. A bare `Ident`
absent from `decls` is, by construction (this file's own module doc: only
module-body expressions ever reach `kinds.rs`), a module `int` parameter —
the checker's own `ident_ty` types it `Ty::CtInt`, exactly like a bare
integer literal, so it already had a rule to adapt to a sized sibling
operand's `Kind` rather than carry its own (`checker::widths::ops::
adapt_lossless`/`matched_ty`). New `adapts_to_sibling` recognizes both
shapes (bare literal OR unresolvable `Ident`); new `adapted_lossless_
operands` resolves such an operand to its sibling's own `Kind` for the
lossless family (`Add`/`Sub`/`Mul`), mirroring `adapt_lossless`'s growth
rule (`other.width.max(v)`, simplified to `other.width` — sound whenever
the parameter's value fits the sibling's width, true for every real
parameter used as a multiplier/addend against an already-sized signal).
The pre-existing `is_bare_int`-only check in the `AddWrap`/`BitAnd`-family
arm is widened to `adapts_to_sibling` for the same reason, matching
`matched_ty`'s handling exactly (no growth). `dur * TICK` (`dur: bits[8]`,
`TICK` defaulted to 50000) now resolves to a 64-bit `Kind`
(`lossless_result(Kind{32}, Kind{32}, is_mul=true)`, after `extend(dur,
32)`), which is enough to hoist `__mimz_sub_1` and slice it — verified
against Icarus directly (`wire [63:0] __mimz_sub_1; assign __mimz_sub_1 =
((dur) * TICK); ... dur_cnt <= __mimz_sub_1[(32)-1:0];`). Shift amounts
(`Shl`/`Shr`) are deliberately left unresolvable for a parameter — real
Verilog's own context growth is harmless there (BUG-30) and doesn't need
this.

**Test.** `lossless_mul_with_a_module_parameter_adapts_to_the_sized_operand`
(`emit_verilog/kinds.rs` unit test). `melody_player.mimz`'s
`self_checking_showcase_testbenches_pass` (`tests/icarus.rs`) now passes;
full workspace green (`REQUIRE_IVERILOG=1 cargo test --workspace --release
--no-fail-fast`) after regenerating goldens (`MIMZ_UPDATE_GOLDENS=1`) and
rebuilding `crates/mimz-wasm/pkg` — both `melody_player` and the
`tamil-pure` showcase's `isai` (same `TICK`-multiply shape) picked up the
new hoisted wire.

---

## BUG-47 (HIGH, FIXED 2026-08-09) — A stale `allow_shift: false` left a shift under `extend()` un-pinned, so a wider context re-widened it

**Status:** FIXED 2026-08-09. Filed the same day by the v0.2 release gate
(gates 2 and 5); both gates now pass at `N=1000`.

**Repro** — four lines, no fuzzer needed:

```mimz
module Fuzz {
  in p1: signed[4]
  out y: signed[20]
  y = extend((p1 >> extend(18, 5)), 20)
}
```

| authority           | `p1 = 4'b1111` (-1) |
| ------------------- | ------------------- |
| `mimz eval`         | **y = 0**           |
| Icarus (`iverilog`) | **y = 3**           |

**Cause.** The emitted Verilog is:

```verilog
output wire signed [(20)-1:0] y
assign y = ((p1 >> 5'd18));
```

mimz types `p1 >> 18` at the shift's own left-operand width — `signed[4]` — so
shifting right by 18 discards every bit and the result is 0. Verilog gives the
whole right-hand side the **assignment's** 20-bit context, so `p1` is
sign-extended to 20 bits _before_ the shift, and `>> 18` leaves `20'b11` = 3.

A shift's left operand is a width-effect position and must be hoisted to a wire
of its own mimz width so the surrounding context cannot re-widen it. Here it
never is — `p1` renders bare. The same shape drives the fuzz repro, where
`extend(p0, 4)` likewise renders as bare `(p0)` inside a 41-bit assignment:

```verilog
assign y = ((((p1 ^ (p0)) >> 5'd18)) << 5'd15);
```

**How found.** The v0.2 release gate, `MIMZ_DIFF_FUZZ_CLOCKED_N=1000` — clocked
seed **202428271** (i=642), cycle 0: kernel `y=0` against Icarus
`00011111111111111111111111000000000000000`. Past the per-PR depth of 400,
which is why only a gate run at 1000 reached it.

**Not an artifact of the GAP-11(a) sub-expression ports.** Re-run with
`MAX_SUB_OUTPUTS = 0` reproduces the divergence identically at the root `y`, so
this is a pre-existing miscompile that deeper fuzzing exposed, not something the
new materialized outputs introduced.

**Family.** [GAP-1](gaps.md) — "two implementations of one width rule
disagreed", now the seventeenth instance. Closest relatives are BUG-24 (a shift
nested under a sibling operator losing its width-effect hoist) and BUG-34
(chained shifts with a signed inner operand); both were fixed for their own
shapes without covering a shift whose left operand simply sits under a wider
assignment context.

**Severity.** HIGH — silent miscompile. Simulation passes, real hardware is
wrong, no diagnostic.

### Root cause — a guard that outlived its reason

The symptom above is "the shift is not pinned". The **cause** is one boolean.
`Builtin::Extend`'s codegen passed `allow_shift: false` to
`hoist_width_effect_operand`, deliberately suppressing the hoist for a shift
argument. Its comment justified that precisely:

> `call`'s `Builtin::Extend` arm explicitly threads THIS extend's own target
> width `n` into evaluating its argument (`eval_ctx(r, &args[0], Some(n))`) — a
> shift argument here is context-determined, not self-determined, so hoisting it
> would compute a value different from the simulator's reference semantics.

**`eval_ctx` no longer exists.** BUG-34's fused-shift rework replaced it: a
shift is now evaluated by `binary::eval_shift_chain`, which resolves the chain's
own bottom-up width and never consults an ambient expected width. The
justification was deleted; the guard was not. Nothing in the emitter or the
simulator threads a context width into an `extend` argument any more, so the
suppression stopped protecting anything and started hiding a divergence.

Fix: `allow_shift: true`. Two goldens changed (`shift.v` and its Tamil-pure
twin `tamil_pure_nakartthi.v`) — the only shipped examples with a shift under
`extend()`.

BUG-6's own guard still holds through the hoist:
`examples/english/shift.mimz`'s `extend(1 << 3, 8)` now emits
`wire [3:0] __mimz_sub_1; assign __mimz_sub_1 = (1 << 3);` — 8 in 4 bits, not
the 0 that bug was about. Its self-checking test (`expect literal_shift == 8`)
passes.

A shift as the LHS of **another** shift stays un-hoisted, gated separately by
`allow_shift_lhs` in the `Binary` arm. That one is a genuine threaded-width
case (BUG-24/BUG-34's fused chain), and it matters — see the false start below.

### False start worth recording

The first fix hoisted any signed `>>` from inside the `Binary` arm itself,
unconditionally. All 43 regression tests passed, including every BUG-24/BUG-34
shift test — and it was still wrong. The differential fuzzer caught it at comb
seed **12648749**: `(p0 >> 10) << 1`, a shift as the direct left operand of
another shift, which the fused-chain semantics require be left un-hoisted.
Confirmed by reverting just that change and re-running (comb clean at `N=1000`).

The lesson is the same one this file keeps recording: a fix rendered from inside
a node cannot see the position the node sits in. The position is the parent's
knowledge, which is exactly why `allow_shift` is a parameter and not a property
of the shift.

**Test.** `bug_47_signed_right_shift_into_a_wider_assignment`,
`bug_47_signed_right_shift_with_a_composite_left_operand`,
`bug_47_signed_right_shift_by_a_port_amount`, and the boundary guard
`bug_47_unsigned_right_shift_and_left_shift_stay_unhoisted`
(`tests/self_determined_regression.rs`), all watched fail before the fix except
the boundary case, which had to keep passing. Seed 202428271 added to
`tests/fixtures/fuzz-seeds/clocked.txt`. Both fuzzers green at `N=1000` each
plus corpus (155 s); full workspace green; `fmt`/`clippy -D warnings` clean.

**Diagnosis method.** A 21-case matrix over operator x signedness x context
width, each run through `mimz eval` and Icarus, established the boundary before
any code changed: only `Shr` with a signed left operand diverges;
`+%`/`-%`/`*%`/`^`/`&`/`+`/`*`, unsigned `>>`, and `<<` all match. One case in
that matrix (`(p1 ^ p2) >> 2`) initially read as passing and was a false
negative — `p1 ^ p2` happened to be positive for the chosen inputs, so
sign extension added zeros. It diverges once the operand is negative, and is now
pinned as its own regression test.

---

## BUG-48 (CRITICAL, FIXED 2026-08-09) — Two more `ExprKind` shapes fall through `infer_kind`, reopening BUG-28/29 with byte-identical output

**What.** `extend()`/lossless arithmetic in a Verilog self-determined position
emit unsized operands again — BUG-28/BUG-29/BUG-41's exact failure, with
byte-identical wrong Verilog — whenever the operand contains an **array-instance
output port** (`s[0].q`) or a **slice whose bounds are a `const`/parameter**
rather than a literal. Simulator green, hardware wrong, no diagnostic.

```text
A  y = { b, s[0].q + a }          emitted {b, (s__0_q + a)}    iv 010101110    vs sim 101011110
A' y = { b, extend(s[0].q, 8) }   emitted {b, (s__0_q)}        iv 000010101111 vs sim 101000001111
B  y = { b, a[HI:0] + a[HI:0] }   emitted {b, (a[3:0]+a[3:0])} iv 010101110    vs sim 101011110
```

`A'`'s output is byte-identical to BUG-28's original Repro A and to BUG-41's
repro ⑤. `A`'s is byte-identical to BUG-41's repro ②.

**Cause.** `crates/mimz-core/src/emit_verilog/kinds.rs::infer_kind` is now the
single gate (BUG-41's fix retired `kind_is_inferrable`), and it is exhaustive
over `Builtin` but **not** over `ExprKind`. Two of the arms BUG-41's own fix
added take an early `return None`:

- `:171` — `Field` requires `base.kind` to be `Ident`. An array instance is
  `Field { base: Index { Ident(arr), i } }`, which `expr.rs:280` renders as
  `{arr}__{n}_{field}` — a name `build_decls` does put in `decls`, just not
  under a key this arm ever asks for.
- `:132` — `Slice` folds `hi`/`lo` with the literal-only `const_fold`. A
  `const`/parameter bound folds fine in the emitted _text_ (`a[3:0]`) but the
  AST still holds an `Ident`, so `const_fold` returns `None`.

Plus the surviving `_ => None` at `:191`. Every one of those `None`s reaches
`expr.rs`'s hoist call sites, which respond by **skipping the hoist** — the same
unsafe default that caused BUG-41. Round 2's recommended "non-analyzable → hoist
conservatively" was declined (correctly, on the grounds that a `wire`
declaration needs a concrete width) but the consequence was not carried through.

**How found.** CTO review 2026-08-09, sweeping the shapes `infer_kind` still
returns `None` for, rather than the shapes previously filed. `fa[i - 1].cout` in
`examples/english/ripple_adder.mimz:22` is the same AST shape in shipped code.

**Severity.** CRITICAL. Silent miscompile of ordinary RTL. Survives `mimz
check`, `mimz test`, the Icarus example suite, and the differential fuzzer at
`N=2000` (whose generator emits neither shape).

**Fix (2026-08-09, `docs/plan/v0.2-class-closure-round3.local.md` Task 1).**
Both shapes classified, plus a real gap the `Field` fix exposed: `build_decls`
never populated a KEY for an array-instance's output port at all (its own
comment said so — "out of this task's scope", from BUG-41's fix) — the
`Field` arm couldn't resolve `s[0].q` no matter how it matched, because
`decls` had nothing under `s__0_q` to find. `build_decls`
(`crates/mimz-core/src/emit_verilog/module/ports.rs`) gained
`insert_repeat_instance_output_kinds`, unrolling each `ModuleItem::Repeat`
body the same way real emission does (`mod.rs`'s own `unroll`, `lo`/`hi`
folded via `consteval::eval`) and inserting every iteration's instance
output ports under `{inst}__{n}_{port}` — the exact key `expr.rs:280`
already renders. The existing plain-instance path was refactored into the
same shared `insert_instance_output_kinds_keyed` rather than duplicated.
`infer_kind`'s `Field` arm now looks up that key when `base` is
`Index { Ident(arr), idx }` and `idx` const-folds. `Slice`'s `hi`/`lo` now
fold through a new `slice_bound_fold` (`consteval::eval` against the module
env, superseding the literal-only `const_fold` for this one call site) — the
same authority `checker::widths::slice_ty` already used to accept the
program, so this can never admit a width the checker rejected.

Both new folds needed `env: &Env` reachable from `infer_kind`, which a
`Slice` can appear under anywhere in the expression tree — so `env` threads
through the whole call graph it was missing from: `infer_binary`,
`infer_call`, `adapted_lossless_operands`, and `self_determined.rs`'s own
`verilog_self_determined_kind`/`self_determined_operand_width` (both call
`infer_kind`). Every call site in `expr.rs`/`module/ports.rs` now passes
`&self.env`, already on hand at each one.

**Test.** `bug_48_array_instance_port_operand_of_add_in_concat_matches_icarus`,
`bug_48_extend_of_an_array_instance_port_in_concat_matches_icarus`,
`bug_48_const_bounded_slice_operand_of_add_in_concat_matches_icarus`
(`tests/self_determined_regression.rs`) — watched fail (kernel/Icarus
mismatch, matching the filing's own numbers) before the fix, pass after.
Revert-checked each arm independently (`if false &&` on the `Field`
array-instance branch; literal-only `const_fold` swapped back in for
`Slice`): each disables exactly its own 1–2 tests and nothing else — the two
fixes are load-bearing and independent. Full workspace green throughout
(`REQUIRE_IVERILOG=1 cargo test --workspace --no-fail-fast`), `fmt`/`clippy
-D warnings` clean. [GAP-13](gaps.md) (the `ExprKind` axis that would have
caught this class before a third round found it by hand) remains open —
this fix closes the two live instances, not the structural gap.

---

## BUG-49 (HIGH, FIXED 2026-08-09) — The same residue emits invalid Verilog: a part-select on a composite base

**What.** `mimz check` and `mimz test` pass; `mimz compile` emits Verilog that
does not parse.

```mimz
module B { in a: bits[4]  out y: bits[3]
  repeat i: 0..1 { let s[i] = Sub() { x: a } }
  y = trunc(s[0].q + a, 3) }

module D { const HI: int = 3  in a: bits[8]  out y: bits[3]
  y = trunc(a[HI:0] + a[HI:0], 3) }
```

```text
emitted   assign y = (s__0_q + a)[(3)-1:0];
          assign y = (a[3:0] + a[3:0])[(3)-1:0];
iverilog  syntax error / Syntax error in continuous assignment   (exit 2)
```

**Cause.** Same `infer_kind` → `None` residue as [BUG-48](#bug-48-critical-fixed-2026-08-09--two-more-exprkind-shapes-fall-through-infer_kind-reopening-bug-2829-with-byte-identical-output).
BUG-36 established that `trunc`'s base must be hoisted to a named wire because
Verilog's part-select grammar accepts only an identifier; BUG-46 extended that to
module parameters. Both hoists are gated on `infer_kind(base)`
(`expr.rs:893`, `:618`), so both fail open on the two shapes above.

**How found.** CTO review 2026-08-09, elaborating the sweep's output under
`iverilog -g2005 -t null`.

**Severity.** HIGH, not CRITICAL — it is loud rather than silent. But it means
`mimz compile`'s output is not guaranteed to be syntactically valid Verilog,
which is weaker than the guarantee the project states wherever it mentions the
Icarus differential.

**Fix (2026-08-09, `docs/plan/v0.2-class-closure-round3.local.md` Task 2).**
Fell out of [BUG-48](#bug-48-critical-fixed-2026-08-09--two-more-exprkind-shapes-fall-through-infer_kind-reopening-bug-2829-with-byte-identical-output)'s
fix exactly as expected — no separate emitter change. A dedicated elaboration
assertion turned out unnecessary: `differential`/`differential_clocked`'s own
`iverilog` BUILD step (`support::run_vvp`) already asserts
`build.status.success()` before any value is ever compared, so a syntax error
fails the test there, not silently.

**Test.** `bug_49_trunc_of_an_array_instance_port_sum_elaborates`,
`bug_49_trunc_of_a_const_bounded_slice_sum_elaborates`
(`tests/self_determined_regression.rs`) — revert-checked one (the
array-instance shape): disabling BUG-48's `Field` fix reproduces the filed
`iverilog` syntax error exactly, confirming this is the same residue, not a
coincidence.

---

## BUG-50 (CRITICAL, FIXED 2026-08-09) — `infer_kind`'s `Replicate` arm returned a too-narrow width, hoisting a replication into an under-sized wire

**What.** `trunc({2{p0}}, N)` (a replication used as a nested operand, not
the outer self-determined container) hoists correctly — BUG-36's
composite-base rule fires, and `mimz check`/`mimz test` both pass — but the
hoisted wire is declared at the wrong width, one `count`-th of the real
one:

```mimz
module Fuzz { in p0: bits[3]  out y: bits[5]  y = trunc({2{p0}}, 5) }
```

```verilog
wire [2:0] __mimz_sub_1;        // should be [5:0] — {2{p0}} is 6 bits
assign __mimz_sub_1 = {2{p0}};  // legal, but only the low 3 bits have anywhere to live
assign y = __mimz_sub_1[(5)-1:0];
```

`iverilog`/`vvp`: `y = xx101` — the top 2 bits of the 5-bit slice read as
`x` (undriven), not a wrong-but-defined value. `mimz eval`/`mimz test` both
report the correct value throughout, since the simulator evaluates the
replication directly from the AST and never goes through `infer_kind` at
all — a third variant of the "checker/simulator agree, `mimz compile`
alone is wrong" class BUG-6/11/16/17/28/29/41/48 all belong to, found via
a wrong-not-missing `Kind` rather than a `None`.

**Cause.** `kinds::infer_kind`'s `ExprKind::Replicate` arm computed only
the inner concat's PER-ITERATION width (`parts`' own summed width),
documented as intentional: _"the multiplier itself isn't needed for this
phase's self-determined check... matching what a caller needs when
checking a replication's REPEATED PART, not the whole replication's total
width."_ That reasoning describes a caller that does not exist — every
existing site that cares about a replication's repeated PART walks
`parts` directly (`expr.rs`'s own rendering code, `hoist_width_effect_
operand` per element), never through `infer_kind` on the `Replicate` node
itself. The one caller that DOES ask `infer_kind` about the `Replicate`
node as a whole — `Builtin::Trunc`'s BUG-36 base-hoist, or any other
self-determined position a `Replicate` could sit in as an operand — got
back `Kind{width: per_iter}` instead of `Kind{width: per_iter * count}`,
the actual width of the value the wire is declared to hold.

**How found.** Building [GAP-13](gaps.md)'s `ExprKind` axis
(`tests/self_determined_regression.rs`'s `expr_kind_self_determined_
coverage`, Task 3 of `docs/plan/v0.2-class-closure-round3.local.md`) —
every existing `Replicate` test used it as the OUTER self-determined
container (`bug_28_extend_in_replication_matches_icarus`), mirroring
BUG-36's own `Concat`-as-nested-operand shape for `Replicate` was the one
combination nothing had written. The axis is exactly what GAP-13 argued
for: it does not just fill in an `ExprKind` row, writing the genuinely
missing test found a genuinely live bug the row was there to catch.

**Severity.** CRITICAL — silent miscompile, narrower blast radius than
BUG-48 (needs a `Replicate` as a nested self-determined operand, not just
anywhere), but the same class: green `mimz check`/`mimz test`, wrong
hardware, no diagnostic.

**Fix.** `count` is always a compile-time constant per the checker's own
`replicate_ty` (same guarantee `Slice`'s bounds have) — folded through the
same `slice_bound_fold` BUG-48 added, multiplied into the per-iteration
width. No other `infer_kind` arm needed a matching audit: `Concat`'s own
arm already summed real widths (never had a multiplier to get wrong), and
this is the only arm whose stated design intentionally returned a smaller
number than the expression's own value.

**Test.** `shape_replicate_nested_in_trunc_hoists_the_base`
(`tests/self_determined_regression.rs`) — watched fail (the emitted
Verilog's `X` bits break `support::parse_icarus`'s binary parse, a blunter
failure than a value mismatch but unambiguous) before the fix, pass after.
Revert-checked (forcing the multiplier to `1`): reproduces the identical
parse failure. Full workspace green throughout, `fmt`/`clippy -D warnings`
clean.

---

## BUG-51 (HIGH, FIXED 2026-08-09) — `--emit-testbench`'s reset-deassert races the DUT's own synchronous reset check, silently skipping it

**What.** A clocked design's `test` block that follows the ordinary
`rst = 1; tick(clk); rst = 0; …` idiom can have its reset **silently
skipped** — a register that should power up at its declared reset value
instead stays `x` (undefined) for the entire run, and the test's own
`expect` can still report `PASS` if nothing downstream happens to notice.

```mimz
module Latch {
  clock clk
  reset rst
  out y: bits[4]
  reg r: bits[4] = 5
  on rise(clk) { r <- r }
  y = r
}

test "reset value survives exactly one tick" for Latch {
  rst = 1
  tick(clk)
  rst = 0
  tick(clk)
  expect y == 5
}
```

Under real `iverilog`/`vvp`, `r` never becomes `5` — it stays `x` for the
whole run. `y == 5` reads `x == 5`, which is `x` (falsy), so the `expect`
correctly fails here — but a design whose downstream logic happens not to
be sensitive to the specific reset value (or whose `expect` doesn't check
until several cycles later, by which point an UNRELATED write has already
overwritten the register) can pass while its reset was never actually
applied.

**Cause.** `TestStmt::Drive`'s codegen (`emit_verilog/testbench.rs`) wrote
every stimulus change — `rst = 0;`, `a = 3;`, etc. — with **blocking**
assignment (`=`). `rst = 0;` is the statement immediately following
`repeat(N) @(posedge clk);` in the generated testbench, so it executes in
the SAME simulation time step as the edge the testbench just waited for —
racing the DUT's own `always @(posedge clk) begin if (rst) … end`, which
is ALSO an active-region process triggered by the identical event. Per
IEEE 1364, the relative execution order of two active-region processes
triggered by the same event is **implementation-defined** — so whether the
DUT's own `if (rst)` check sees the OLD value (`1`, correct) or the NEW
one (`0`, the testbench's about-to-happen deassert, wrong) is a coin flip
the language does not resolve. Confirmed live: `iverilog` consistently
resolved it the wrong way for `rst`, silently steering every reset-edge
tick to the DUT's `else` branch instead of its reset branch.

**How found.** Verifying Task 8 #2 (`docs/plan/v0.2-class-closure-round3.
local.md`, the `cover` hit-count summary) against real `iverilog` — a
clocked cover expected to fire exactly once, on the first post-reset edge,
consistently read 0. Tracing why (not accepting the first plausible
explanation) found the register the cover's own condition depended on had
never actually been reset at all.

**Severity.** HIGH, not CRITICAL — `--emit-testbench` is a secondary
verification path (the primary `mimz test`/`mimz sim` interpreter has its
own, unaffected reset implementation, `crates/mimz-sim`), but it is the
ONE place this project's own "verify against real hardware" discipline
runs `mimz`-authored `test` blocks through actual Icarus, and this defect
made that verification silently unreliable for any design whose behavior
happens to be sensitive to its own first post-reset cycle — which is
precisely the cycle a reset exists to guarantee. Compounded by a real gap
in the test suite itself: `every_emitted_testbench_passes_iverilog`
(`tests/icarus.rs`, pre-existing) only proves an emitted testbench
**elaborates** (`iverilog -t null`) — nothing in the suite had ever
actually RUN a `--emit-testbench` output through `vvp` and checked its
verdict, so this race was invisible to the whole test suite until checked
by hand.

**Fix.** `TestStmt::Drive` now emits every stimulus write with
non-blocking assignment (`<=`) instead of `=` — the standard, textbook fix
for exactly this race class: a non-blocking assignment defers the actual
value change to the NBA region, which runs strictly AFTER every
active-region process (including the DUT's own synchronous logic) has
read the signal for that time step, so the DUT is now GUARANTEED to see
the pre-drive value for the edge that just occurred, regardless of
scheduling order. All testbench goldens regenerated (`MIMZ_UPDATE_
GOLDENS=1`); every changed line is exactly `=` -> `<=` on a stimulus
drive, confirmed by inspection.

**Test.** Two new Icarus-backed tests in `tests/icarus.rs`:
`emitted_testbench_reset_deassert_does_not_race_the_dut` (the exact repro
above, run for real, asserting `PASS`) and `emitted_testbench_prints_the_
cover_summary` (Task 8 #2's own feature, which incidentally exercises this
fix too — a clocked cover's first-edge hit count depends on the SAME
reset timing). Both watched fail before the fix (the first: `FAIL`
printed instead of `PASS`; the second: a clocked hit count of `0` instead
of `1`), pass after. `every_emitted_testbench_passes_iverilog`'s own gap
(elaboration-only, never actually running `vvp`) is unchanged — these two
new tests are the first in the suite to run a `--emit-testbench` output
to completion and check its verdict, not just its syntax.

---

## BUG-52 (CRITICAL, FIXED) — `verilog_self_determined_kind`'s `ExprKind` wildcard: an `if`/`match`/unary in a self-determined position never gets a hoist

**What.** An `if`/`match` expression, or a unary operator, sitting directly in a
self-determined position (concat member, replication body) and wrapping a
sub-expression whose Verilog rendering is narrower than its mimz width, is
emitted without the hoist that BUG-19/BUG-28's mechanism exists to apply.
`mimz check` passes, `mimz eval`/`mimz test` give the right answer, and real
Icarus gives a different one. Four reproductions (`a = 0b1111`, `b = 0b1010`,
`s = 1`):

```mimz
out y: bits[12]   y = { b, if s { extend(a, 8) } else { extend(a, 8) } }
  emitted   assign y = {b, ((s) ? ((a)) : ((a)))};
  mimz 2575     iverilog 175    x

out y: bits[12]   y = { b, match s { true => extend(a,8), false => extend(a,8) } }
  emitted   assign y = {b, (((s == 1'b1)) ? ((a)) : ((a)))};
  mimz 2575     iverilog 175    x

out y: bits[12]   y = { b, ~extend(a, 8) }
  emitted   assign y = {b, (~(a))};
  mimz 2800     iverilog 160    x

out y: bits[16]   y = {2{ if s { extend(a,8) } else { extend(a,8) } }}
  emitted   assign y = {2{((s) ? ((a)) : ((a)))}};
  mimz 3855     iverilog 255    x
```

Repro 1's wrong emission is the same eight bytes as BUG-28's Repro A, BUG-41's
repro 5 and BUG-48's repro 2 — the `extend`'s padding bits are never
materialized and every field to its left shifts down.

**Cause.** `emit_verilog/self_determined.rs`'s `verilog_self_determined_kind`
ends `_ => None`, so `IfExpr`, `Match`, `Unary` (and `Concat`/`Replicate`/
`Slice`/`Index`/`Field`/`FnCall`) are declared "Verilog agrees with mimz here"
with no reasoning written or checked. `hoist_if_needed`
(`emit_verilog/module/ports.rs:497`) reads that `None` as **skip the hoist** —
the identical unsafe default the round-3 plan named as this class's
one-sentence problem, in the one match of the gate/classifier pair that Task 3
did not make exhaustive. The gate (`kinds::infer_kind`) became exhaustive over
`ExprKind` in Task 3; the classifier never did.

**How found.** v0.2 release-readiness round 4
([`review-2026-08-10.md`](review-2026-08-10.md)), sweeping the _reasoning_ at
each arm of both authorities rather than its presence. Independently
corroborated by the project's own deep fuzz at `MIMZ_DIFF_FUZZ_CLOCKED_N=2000`:
clocked seed **202428078** (fresh index 449) fails with kernel 16358 vs Icarus
102 on an `if`-as-concat-member, minimized to 10 lines.

**Why the existing tests missed it.** `expr_kind_self_determined_coverage`'s
`IfExpr`/`Match` arms cite
`bug_41_if_expr_operand_of_add_in_concat_matches_icarus` and
`shape_match_operand_of_add_in_concat_matches_icarus`, both of which place the
`if`/`match` as an **operand of `+`** inside a concat — a position where the
enclosing `+` triggers the hoist and masks this. Neither places it as the concat
member itself. Its `Unary` arm marks the shape `NotApplicable` because
"`self_determined.rs`'s own catch-all confirms no arm ever differs"; a catch-all
confirms nothing. The axis's own doc comment is also stale — it says
`infer_kind` is not wildcard-free, which Task 3's stretch goal made false, while
never asking the question of `verilog_self_determined_kind`.

**Severity.** CRITICAL. Silent wrong hardware from ordinary syntax, invisible to
every `mimz` command a user runs, on the exact class the v0.2 release note
claims is closed.

**Fix (landed — round-4 plan Task 2).** Three arms in
`verilog_self_determined_kind`, mirroring `Min`/`Max` (already the same
ternary rendering), then `_ => None` deleted so the classifier axis is
exhaustive too:

- `IfExpr` → `max(self_determined_operand_width(then), ...(els))`, signedness
  from `infer_kind`
- `Match` → `max` over every arm's value, same shape
- `Unary` → `None` for `RedAnd`/`RedOr`/`RedXor` (1 bit in both models),
  otherwise the operand's own self-determined width
- every remaining `ExprKind` variant given its own explicit reasoned `None`
  arm (`Concat`/`Replicate`: each member is hoisted at its own position
  first; `Slice`/`Index`/`Field`/`FnCall`: rendered width already equals
  mimz's; `BundleLit`/`ArrayLit`/`EnumConstruct`: checker-rejected upstream)

**Test-first, watched fail before implementing.** 4 new differentials in
`tests/self_determined_regression.rs` (`bug_52_if_expr_as_a_concat_member_…`,
`_match_as_a_concat_member_…`, `_unary_not_of_an_extend_in_a_concat_…`,
`_if_expr_in_a_replication_body_…`), each placing the shape as the
concat/replication member itself rather than as an operand of `+`. Against
pre-fix HEAD all four failed at the exact filed values (2575/175, 2575/175,
2800/160, 3855/255); with the fix, all four pass. The fuzz-found clocked
instance (seed 202428078) is deliberately **not** hand-minimized into its own
test — it is a large multi-output generated program, and the four hand
repros already isolate each of the three new arms independently; it is
covered instead by the corpus-seed append (round-4 plan Task 7, after this
fix and BUG-55's both land).

**Build enforcement confirmed.** Deleting any one `ExprKind` arm (verified
with the `BundleLit`/`ArrayLit`/`EnumConstruct` line) fails the build with
`error[E0004]`, exit 101 — the classifier axis is now exhaustive, closing the
last cell of the gate/classifier × `Builtin`/`ExprKind` grid.

Full verification: `REQUIRE_IVERILOG=1 cargo test --workspace --release
--no-fail-fast` → **1233 passed / 0 failed** (1229 + the 4 new tests), 35
suites (including `bug_24_regression_shift_in_if_branch_stays_unhoisted` and
every emit golden, unmoved); `cargo fmt --check` clean; `cargo clippy
--workspace --all-targets -D warnings` clean (the one reported warning is the
build script's own git-hooks notice, same as round 4's baseline).

---

## BUG-53 (HIGH, FIXED 2026-08-11) — array-instance `decls` keys are built from the `repeat` loop counter, not the rendered index

**What.** An array instance whose index expression is not the bare loop
variable, or which sits inside a nested `repeat` or a `const if` inside a
`repeat`, is emitted with the correct Verilog instance name but is absent from
`build_decls`' key map — so `infer_kind`'s `Field` arm returns `None` and the
hoist BUG-48 added is silently skipped again. Three reproductions
(`a = 0b1111`, `b = 0b1010`; declared `out y: bits[9]`, mimz's own type gives
350):

```mimz
repeat i: 0..1 { let s[i + 1] = Sub() { x: a } }      y = { b, s[1].q + a }
repeat i: 0..1 { repeat j: 0..1 { let s[j] = ... } }  y = { b, s[0].q + a }
repeat i: 0..1 { const if (DEBUG) { let s[i] = ... } } y = { b, s[0].q + a }
```

All three emit `assign y = {b, (s__N_q + a)};` with **no hoisted wire**, and
Icarus returns **174** where the declared `bits[9]` requires 350.

Control case that pins the diagnosis: `repeat i: 1..2 { let s[i] = ... }`
reading `s[1].q` — index expression equal to the loop counter, at a non-zero
base — hoists correctly and Icarus agrees at 350. `s[1]` is not the problem; the
counter-vs-index divergence is.

**Cause.** `insert_repeat_instance_output_kinds`
(`emit_verilog/module/ports.rs:286`) keys each `repeat`-body instance port as
`{inst}__{i}_{port}` where `i` is the **loop counter**, and scans only `r.items`
for a direct `ModuleItem::Inst`. `inst_name` (`emit_verilog/mod.rs:678`) renders
the instance as `{inst}__{eval_const(inst.index)}` — the **folded index
expression** — and `emit_instances` (`emit_verilog/module/instances.rs:71`)
recurses into nested `Repeat`/`ConstIf`/`ForEach` bodies via `unroll`. The two
disagree wherever `inst.index` is not the loop variable, or the instance is more
than one level deep. Same "arm present, data source under-populated" shape as
BUG-48 itself: BUG-48's fix covered exactly the shape its three repros used.

`examples/english/ripple_adder.mimz` is not affected — it writes `let fa[i]`
(bare loop variable, one level) and only the _read_ side uses `fa[i - 1]`, which
folds correctly through `slice_bound_fold`.

**How found.** v0.2 round 4, checking whether the mid-Task-1 `decls` population
covers every place an array-instance port can be referenced or only the ones
Task 1's three repros exercised.

**Severity.** HIGH, not CRITICAL — all three shapes are unsimulatable, so the
`check -> test -> compile` workflow hits a hard error before the wrong Verilog:

| shape                  | `mimz check` | `mimz sim`                                                          | `mimz compile`        |
| ---------------------- | ------------ | ------------------------------------------------------------------- | --------------------- |
| offset index           | OK           | `error: unknown signal s__1_q`                                      | exit 0, wrong Verilog |
| nested `repeat`        | OK           | `error[S0125]: nested repeat is not supported by the simulator yet` | exit 0, wrong Verilog |
| `const if` in `repeat` | OK           | `error[S0126]: a repeat body may only contain instances and drives` | exit 0, wrong Verilog |

That table is a second finding in its own right: three components disagree about
whether these programs exist. Either the checker should reject them (and then the
offset-index case's `unknown signal s__1_q` is an internal error leaking to a
user), or the simulator and emitter should both support them.

**Fix.** Key from the same folded `inst.index` expression `inst_name` uses, not
the loop counter, and recurse into nested `Repeat`/`ConstIf`/`ForEach` the way
`emit_instances` already does — ideally by sharing one walk between the two, so
they cannot drift again.

**Fixed 2026-08-11.** `insert_repeat_instance_output_kinds`
(`emit_verilog/module/ports.rs`) is now three functions: the original entry
point, `insert_repeat_instance_output_kinds_in` (folds one `repeat` level
against an `Env`, recurses), and `insert_array_instance_output_kinds` (walks
`ModuleItem`s, keying each `Inst` by its own folded `index` expression exactly
as `inst_name` does, and recursing into nested `Repeat`, evaluated `ConstIf`
branches, and lowered `ForEach` bodies) — mirroring `emit_instances`'s own
traversal instead of diverging from it. Four new tests in
`tests/self_determined_regression.rs` (`bug_53_offset_array_instance_index_matches_icarus`,
`bug_53_nested_repeat_array_instance_matches_icarus`,
`bug_53_const_if_in_repeat_array_instance_matches_icarus`, plus the control-case
guard `bug_53_control_case_non_zero_base_identity_index_still_hoists`) cover the
three filed shapes and the non-regression case, via a new `emitter_only_clocked_check`
helper (bypasses the kernel — `mimz sim` still rejects all three shapes per the
table above — and instead checks the emitted Verilog directly against real
Icarus). Watched all three fail pre-fix at the exact filed values (Icarus 174,
declared 350) via `git stash` of just the emitter change, then pass restored.
Full workspace suite green (only the 2 known pre-existing `wasm_parity`
failures remain); `fmt`/`clippy -D warnings` clean. The check/sim/emit
disagreement table above is **not** resolved by this fix — that gap is tracked
separately (round-4 plan Task 8).

---

## BUG-54 (HIGH, FIXED 2026-08-11) — `--emit-testbench`'s `expect` reports PASS on an `x`-valued comparison

**What.** A generated testbench prints `PASS` when its `expect` condition
evaluates to `x`. A design that never resets, never drives an output, or holds
`x` for any other reason passes its own testbench.

`emit_test_stmts` (`emit_verilog/testbench.rs`) renders an `expect` as:

```verilog
if (!((y == 5))) begin
  $display("FAIL: expect %0s failed", "(y == 5)");
  $finish;
end
$display("PASS");
```

With any operand `x`/`z`, `y == 5` is `x`, `!(x)` is `x`, and Verilog's `if`
treats `x` as false — the FAIL branch is skipped and PASS is printed.

**Cause.** `!` plus a truthiness `if` is not a decision procedure over
four-valued logic. Verilog-2005 provides `===`/`!==` precisely for this.

**How found.** v0.2 round 4, while independently reproducing BUG-51: with the
blocking-assignment race reintroduced, `y` is `xxxx` at the expect point,
`(y == 5)` evaluates to `x`, and the testbench prints `PASS`. Instrumented with
`$monitor`/`$display` at `t=14`:

| drive style               | `y` at the expect point | `(y == 5)` | verdict printed |
| ------------------------- | ----------------------- | ---------- | --------------- |
| `<=` (HEAD)               | `0101`                  | `1`        | PASS            |
| `=` (BUG-51 reintroduced) | `xxxx`                  | `x`        | **PASS**        |

**Consequence.** `emitted_testbench_reset_deassert_does_not_race_the_dut`
(`tests/icarus.rs`) asserts `contains("PASS")` and `!contains("FAIL")`, so it
**cannot fail** on the BUG-51 regression it was written to catch — verified by
running the pre-fix emission and observing PASS. The sibling test
`emitted_testbench_prints_the_cover_summary` _is_ load-bearing (it asserts
`clocked first edge: 1`, which reads `0` with BUG-51 back). BUG-51's own entry
above also states that the `expect` "correctly fails here" for an `x` compare;
it does not.

**Severity.** HIGH. A false PASS is the worst failure mode a verification
feature has, and it has already silently disarmed one regression test.

**Fix.** `if (((cond)) !== 1'b1)` — case-inequality, so `x` and `z` both fail.
Legal Verilog-2005, one-token change. Optionally also `$display` a distinct
"unknown value" message so a user can tell an `x` from a wrong value.

**Fixed 2026-08-11.** `emit_test_stmts`'s `Expect` arm
(`emit_verilog/testbench.rs`) now renders `if ((cond) !== 1'b1) begin ... end`
instead of `if (!(cond))`. New unit test
`expect_guard_uses_case_inequality_not_plain_negation` (same file) pins the
emitted text directly and was watched fail against the pre-fix rendering
before being restored. `emitted_testbench_reset_deassert_does_not_race_the_dut`
(`tests/icarus.rs`) — previously vacuous per this bug's own "Consequence"
above — was re-verified: reintroducing BUG-51 (`<=` → `=`) now correctly
prints `FAIL` and fails the test, where before this fix it printed `PASS`
either way. 14 testbench goldens regenerated (`tests/golden/*_tb.v`) —
diffs are exactly and only the guard-shape change. Workspace suite green
(the 2 `wasm_parity` failures are the pre-existing, unrelated stale-WASM
issue). fmt/clippy clean.

---

## BUG-55 (CRITICAL, FIXED 2026-08-11) — a signed `>>` inside an `if`/`match` branch escapes BUG-47's context hoist

**What.** BUG-47's defect — a signed right shift whose left operand real Verilog
context-extends to the assignment's width _before_ shifting, so sign bits shift
down into the result — is still live when the shift sits inside an `if`/`match`
branch instead of at the assignment's top level. Minimized to nine lines:

```mimz
module Fuzz {
  in p0: signed[14]
  in p3: signed[4]
  out y: signed[16]
  y = extend((match unsigned(p3) {
    0 => signed(extend(22, 14))
    1 => signed(extend(22, 14))
    _ => (p0 >> extend(14, 4))
  }), 16)
}
```

```verilog
assign y = ((($unsigned(p3) == 0)) ? ($signed(__mimz_sub_1))
          : ((($unsigned(p3) == 1)) ? ($signed(__mimz_sub_2))
          : ((p0 >> 4'd14))));
```

`p0 = 14429` (`-1955` as `signed[14]`), `p3 = 2` selects the `_` arm.
`mimz eval` says **y = 0**; `iverilog` + `vvp` say **y = 3**.

**Cause.** Two mechanisms both correctly decline to act, and nothing else does.
BUG-47's fix hoists a signed `>>` at the assignment's top level;
`hoist_width_effect_operand` deliberately does **not** treat an `if`/`match`
branch as self-determined (a branch genuinely inherits the outer context — see
`bug_24_regression_shift_in_if_branch_stays_unhoisted`, which must stay green);
and `verilog_self_determined_kind` has no `Match` arm to report the mismatch
(BUG-52). `>>` is the one operator whose _value_ depends on the width it is
evaluated at, so a branch is exactly where its own width has to be pinned even
though every other operator's branch correctly must not be.

**How found.** The project's own deep differential fuzz at
`MIMZ_DIFF_FUZZ_N=2000`, comb seed **12649355** (fresh index 925 — past the
per-PR depth of 400, inside the nightly depth of 5000). Not found by inspection
in round 4's hand sweep.

**Severity.** CRITICAL. Silent wrong hardware; and it is a **previously fixed
bug still reachable one AST node deeper**, which is the sharpest available
evidence for GAP-1.

**Fix.** Narrower than "hoist branches": recurse BUG-47's signed-`>>` context
hoist into `IfExpr`/`Match` branch values specifically, leaving every other
operator's branch untouched so `bug_24_regression_shift_in_if_branch_stays_unhoisted`
and `examples/*/shift.mimz`'s goldens stay as they are. BUG-52's own three-arm
fix does **not** cover this — verified: with BUG-52 fixed, the clocked fuzz goes
green at 1000 seeds while comb seed 12649355 still fails.

**Fixed 2026-08-11.** `IfExpr`/`Match` rendering (`emit_verilog/expr.rs`)
always hoisted branch shifts with `allow_shift: false`, correct only when the
`if`/`match` itself sits at a context-determined top level (BUG-24) — wrong
whenever it sits in a self-determined position instead, since `eval_ctx`
propagates the same `expected_width` into every branch/arm regardless. Fixed
by factoring `if_expr_subst`/`match_subst` (new `allow_shift` param) plus a
`render_shift_ctx_operand` helper that every self-determined call site (~20)
now routes its child through, instead of `expr_subst` directly. New
differential `bug_55_signed_shift_right_inside_match_wildcard_arm_matches_icarus`
watched fail (kernel 0, Icarus 3 — the exact filed values), then pass.
`bug_24_regression_shift_in_if_branch_stays_unhoisted` — the guard this fix
must not break — stays green: its `if` sits at a bare top-level assignment
RHS, a position the new call sites never touch. `self_determined_regression`
55/55; full workspace 1234/1234 (2 pre-existing `wasm_parity` failures,
reproduced identically on unmodified HEAD, unrelated). fmt/clippy clean.

---

## BUG-56 (HIGH, OPEN) — a bare integer literal renders unsized, so Icarus refuses any concat/replicate member that nests one

**What.** Found auditing `verilog_self_determined_kind`'s `Int`/`Bool` arms
(round-4 plan Task 4, `docs/plan/v0.2-class-closure-round4.local.md`) — the
arm's own claim, "Verilog's self-determined width for an unsized literal
already equals mimz's, nothing to compare," is false whenever the literal is
nested (not the direct member) inside a concat or replication body:

```mimz
module M {
  in a: bits[4]
  in b: bits[4]
  out y: bits[8]
  y = { b, a & 15 }
}
```

```verilog
assign y = {b, (a & 15)};
```

`iverilog -g2005` refuses to elaborate this — not a silent miscompile, a hard
error:

```text
lit_test4.v:8: error: Concatenation operand "(a)&('sd15)" has indefinite width.
lit_test4.v:8: error: Unable to elaborate r-value: {b, (a)&('sd15)}
```

Same failure one level further in, inside a replication body:
`y = {2{ a & 15 }}` → `error: Concatenation operand "(a)&('sd15)" has
indefinite width.` `mimz check`/`mimz compile` both accept the source and
exit 0; the wrong Verilog is only caught by real Icarus, exactly the
BUG-49 shape (unparseable output, not a wrong value) rather than BUG-52's.

**Cause.** `verilog_literal` (`emit_verilog/mod.rs`) never emits a sized
Verilog literal: a `0b`/`0x`-prefixed source literal renders as bare
`'b<digits>`/`'h<digits>` (no size before the tick), and a plain decimal
literal renders as a bare number (`bits_to_decimal_string` returns just the
digits, no size prefix either) — both are Verilog's "unsized" literal forms,
whose width the LRM leaves implementation-defined, and which real Icarus
explicitly refuses inside a `{...}` once one is nested under an operator
(directly bare, `{b, 15}`, IS caught pre-emit: the checker's own concat-typing
rule, `checker/widths/ops/mod.rs:697`, rejects a concat member whose `Ty`
resolves to the untyped `Ty::CtInt` with E0405 — "a bare literal has no width
inside `{...}`"). `a & 15` does not hit that rule: `BitAnd`'s own type rule
lets the bare literal _adapt_ to `a`'s sibling type (`Ty::Bits(4)`), so the
concat member's own `Ty` is `Bits(4)`, not `CtInt` — checker-legal. But that
adaptation lives only in the type checker's/gate's `Kind` computation
(`infer_kind`'s `BitAnd` arm already resolves this correctly to `Kind{4}`);
nothing propagates it into how `verilog_literal` prints the literal TOKEN
itself, which stays the bare, unsized form regardless of what the enclosing
expression adapted it to.

**How found.** Round-4 plan Task 4 (`docs/plan/v0.2-class-closure-round4.local.md`)
— verifying the `Int`/`Bool` arms' "nothing to compare" claim as a checkable
fact rather than accepting the prose, per that task's own rule ("an arm may
not cite the absence of a rule as evidence for the rule"). Confirmed against
real `iverilog -g2005` + `vvp` (both the concat-member and replication-body
shapes above).

**Severity.** HIGH, not CRITICAL — same reasoning as BUG-49: the failure is a
loud Icarus elaboration error a user hits at `mimz compile`'s Icarus-backed
CI step (or any downstream synthesis/simulation tool), not a silent wrong
value. Not yet checked whether a synthesis tool other than Icarus accepts this
Verilog and, if so, what width it assumes — the LRM leaves the choice
implementation-defined, so a silent divergence on some other toolchain is not
ruled out.

**Not yet checked.** Whether the same nesting reaches a comparison operand or
a `$signed`/`$unsigned` argument without erroring (both tested bare and were
fine — Icarus's "indefinite width" restriction is specific to `{...}`
contexts per the LRM) — likely benign there since neither BUG-52's ternary
family nor a plain comparison requires a _definite_ self-determined width the
way a concat/replication member does, but not independently confirmed for
every such position. Also not yet checked: whether every arithmetic/logical
operator that lets a bare literal adapt to a sibling (not just `BitAnd`) hits
the same nested-in-concat shape — `AddWrap`/`SubWrap`/`MulWrap`/`BitOr`/
`BitXor` share `BitAnd`'s exact adapt-to-sibling rule in both the checker and
`infer_kind`, so the same repro shape almost certainly reproduces for each,
untested individually.

**Fix (not yet implemented — Task 4 is an audit, not a fix pass).** Make
`verilog_literal` always emit a _sized_ literal (`{width}'b...`/`{width}'h...`/
`{width}'d...`), with `width` computed the same way `infer_kind`'s own `Int`
arm already does (`min_width_for`/`natural_width`) — so the emitted token's
own Verilog self-determined width is equal to mimz's by construction, the
same invariant the `Int`/`Bool` arms' `None` claim already assumes but never
enforces. Needs care at literal sites that currently rely on the bare/wide
form on purpose (a `parameter` default, a `localparam`, a bit-index) —
audit those call sites before changing the shared helper. Precedent already
in the codebase: `ExprKind::EnumConstruct` rendering (`emit_verilog/expr.rs`,
~line 1098) already documents and avoids this exact class ("inside a `{}`
concatenation an unsized decimal literal defaults to 32 bits... which would
corrupt the tag/field/padding boundaries") by rendering every constant
payload argument as a sized literal (`{field_w}'d{value}`) instead of
`expr_subst`'s ordinary unsized form — the fix shape above generalizes that
existing, working pattern to every literal, not a new idea.

---

## BUG-57 (MEDIUM, OPEN) — indexing an array literal panics instead of erroring: `unreachable!("Task 8 or Task 9 wires this up")`

**What.** Found continuing Task 4's audit while checking `ExprKind::ArrayLit`'s
"the checker rejects it upstream" claim (`docs/plan/v0.2-class-closure-round4.local.md`,
same task as BUG-56). The claim is false — `mimz check` accepts an array
literal indexed by a constant, and `mimz compile` panics rendering it, in
ANY position (not specific to a self-determined one):

```mimz
module M {
  in a: bits[4]
  out z: bits[4]
  z = [a,a,a][0]
}
```

```text
thread 'main' panicked at crates\mimz-core\src\emit_verilog\expr.rs:1067:38:
internal error: entered unreachable code: Task 8 or Task 9 wires this up
```

`mimz check` on the same source: `OK: ... — 1 module(s), 0 test(s), 1 file(s)`.

**Cause.** `emit_verilog/expr.rs`'s `ExprKind::ArrayLit(_)` arm is
`unreachable!("Task 8 or Task 9 wires this up")` — a deliberate placeholder
for array-literal rendering, not yet implemented (the module doc's `ExprKind::
ArrayLit => unreachable!("Task 8 or Task 9 wires this up")` line names the
future work it is waiting on). The checker does not yet reject the shape
that reaches it — `Index` on a freshly-constructed `ArrayLit` (rather than a
named array `let`/param) round-trips through width-checking without
complaint, so a source program can reach the placeholder directly.

**How found.** Round-4 plan Task 4, verifying `expr_kind_self_determined_
coverage`'s `ArrayLit` citation ("not a `bits` value; the checker rejects it
upstream") as a checkable fact — it turned out false in a different way than
expected: not "the checker accepts it and the emitter miscompiles it
silently," but "the checker accepts it and the emitter crashes."

**Severity.** MEDIUM — same bar as BUG-40 (a `pattern_matches` `unreachable!()`
firing on a raw `Pattern::Variant`): a panic reachable from checker-accepted
source is a robustness defect (an ungraceful crash where a diagnostic
belongs), not a silent miscompile or a memory-safety issue. Not self-
determined-position-specific — it fires at the bare top-level assignment too,
so it sits outside this plan's own scope (the position-matrix class) even
though the audit that found it is in scope.

**Fix (not yet implemented — Task 4 is an audit, not a fix pass).** Either
(a) the checker gains a rule rejecting `Index` directly on an `ArrayLit`
(the array-literal-then-immediately-indexed shape has no named binding to
optimize away, so this may be intentionally out of the supported surface —
same "pick one, deliberately" framing as Task 8's check/sim/emit split), or
(b) the emitter actually implements it (the array literal's elements are
known statically, so `[a,a,a][0]` can constant-fold the INDEX the same way
`Index` on a named array does, or at minimum render the ternary-chain mux
`Index` already generates for a named array-typed `let`). (a) is the
smaller, more honest change if this shape isn't meant to be supported yet.
