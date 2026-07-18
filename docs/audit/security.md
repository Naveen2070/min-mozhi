# Security findings

Input-triggered crashes, overflow, and memory-safety, with fixes. See
[`README.md`](README.md) for the threat model and method.

---

## SEC-1 (CRITICAL) — Unbounded parser recursion → stack overflow

**What.** The recursive-descent parser had **no nesting-depth limit**. A crafted
file with deeply nested expressions — `y = (((( … ))))`, a long `!!!!…x` prefix
chain, or `if a { … } else if a { … } else if …` — drove the parser to recurse
until the thread stack was exhausted, **aborting the process** (Windows fast-fail
`0xC0000409`). A stack overflow is not a catchable error; it is a hard crash, and
it is reachable from every entry point (`compile`, `check`, `eval`) since all call
`parser::parse`.

**How found.** Audit confirmed the `Parser` struct held no depth counter, then
reproduced it: a generated file with 50,000 nested parentheses crashed with
`thread 'main' has overflowed its stack` and no diagnostic.

**Severity.** CRITICAL — denial of service from a small input; the headline
defect. (Production parsers, including rustc, all cap recursion depth.)

**Fix.** A depth guard in the recursive-descent core:

- `Parser` gained a `depth` counter and a `too_deep` latch
  (`src/parser/mod.rs`). `MAX_DEPTH = 64` — far above any human-written
  expression, far below the overflow threshold. Each source nesting level costs
  ~12 Rust frames (`expr → binary(0..9) → unary → postfix → primary`) and the
  CLI parses on the 1 MB Windows main-thread stack, so the cap is deliberately
  conservative (an initial 256 still overflowed before tripping — the cap **must**
  suit the smallest stack the parser runs on).
- `enter()` / `leave()` wrap the five recursion necks — `expr`, `unary`,
  `if_expr` (`src/parser/expr.rs`), `seq_if` (`src/parser/items/seq.rs`),
  `test_if` (`src/parser/items/test.rs`).
  On exceeding the cap, `enter()` emits the new diagnostic **E1113** once and
  returns `None`, so parsing fails cleanly instead of crashing.

**Verified.** The 50k-paren file now prints `error[E1113]: nested too deeply to
parse safely` and exits non-zero — no crash.

**Tests.** `deeply_nested_expression_errors_not_overflows`,
`deeply_nested_unary_errors_not_overflows` (`src/parser/tests.rs`).

---

## SEC-2 (HIGH) — Unhardened const-evaluator in the simulator (overflow / panic)

**What.** `sim::comb`'s private `const_eval` was a **divergent, naive copy** of
the checker's evaluator. It used raw `i128` arithmetic — `a + b`, `a - b`,
`a * b`, `a << b`, `a >> b`, unary `-v` — plus truncating `as u32` casts for bit
indices, slice bounds, and `extend`/`trunc` widths, and an `as_i128()`
subtraction in `binary()`. A crafted compile-time constant therefore caused:

- **debug:** a panic (`attempt to … with overflow`, or a shift `>= 128`);
- **release:** a silent wraparound to a **wrong value** — i.e. wrong hardware,
  produced quietly.

**How found / reachability.** Audit contrasted it with the checker's
`consteval::eval`, which is fully hardened (`checked_add/sub/mul/neg`,
`u32::try_from` + `checked_shl/shr`, `E0202` overflow diagnostic). Critically,
`main::eval_file` runs **lex → parse → eval and does NOT run the checker**, so the
checker's overflow guard and the `MAX_WIDTH` cap are both **bypassed** on the
`mimz eval` path — `const_eval` was the only guard there, and it had none. A const
like `1 << 200`, `9999999999 * 9999999999 * 9999999999 * 9999999999`, or `a[200]`
reached the naive arithmetic directly.

**Severity.** HIGH — input-triggered crash (debug) / silent miscompute (release)
on an unshielded path.

**Fix** (`src/sim/comb.rs`):

- `const_eval` now **delegates to the checker's hardened `consteval::eval`** —
  one evaluator, single source of truth — mapping its `Diag` to the evaluator's
  `String`. The naive copy is deleted.
- New `checked_index()` validates every bit index / slice bound against the
  value's width (rejecting negative / out-of-range instead of truncating with
  `as u32` and instead of a later oversized shift). `extend`/`trunc` widths go
  through the existing `checked_width()` (1..=128).
- `binary()` subtraction uses `wrapping_sub`, unary negation uses
  `wrapping_neg`, and the concat width sum accumulates in `u64` so many parts
  cannot wrap the guard.

**Verified.** `mimz eval` of `y = a[1 << 200]` prints `error: constant
evaluation overflowed` and exits 1 in **both** debug and release — no panic, no
silent wrap.

**Tests.** `oversized_shift_const_does_not_panic`,
`overflowing_multiply_const_does_not_panic`,
`out_of_range_index_is_rejected_cleanly` (`tests/eval.rs`).

---

## SEC-3 (informational) — Buffer overflow is impossible by construction

**What / how.** A grep of `src/` confirmed **zero `unsafe`** — no `transmute`,
raw pointers, `get_unchecked`, `set_len`, or `MaybeUninit`. In safe Rust every
out-of-bounds access is a bounds-checked panic, never memory corruption, so a
classic buffer overflow (out-of-bounds write) cannot occur. The risk reduces to
"don't panic on input", covered by SEC-1 and the clean indexing sweep.

**Hardening.** `#![forbid(unsafe_code)]` was added to `src/lib.rs` and
`src/main.rs` so this guarantee is now enforced by the compiler — any future
`unsafe` is a build error. See [`hardening.md`](hardening.md).

---

## SEC-4 (LOW) — Subtract overflow in zero-width output coverage check

**What.** `Checker::report_coverage` (`src/checker/drivers.rs`) computes per-bit
driver coverage for an output. For a **zero-width** output whose drivers are
per-bit `Range` sites, it built `covered = vec![false; 0]` and then clamped the
upper bound with `covered.len() as u128 - 1` — `0u128 - 1`, an **arithmetic
overflow** (debug panic / release wrap, since `overflow-checks = true`). A
zero-width type is already an `E0410`, but the checker records that error and
**continues**, so this later pass still ran on the bad signal.

**How found.** The `lex_parse_compile` fuzz target — its first finding. The
reproducer was a mutated `ripple_adder` with `out sum: bits[!WIDTH]` (the unary
`!` folds `9` to `0`) driven per-bit by `sum[i] = …` inside a `repeat`. Minimal
import-free trigger:

```mimz
module M {
  const W: int = 4
  in a: bits[W]
  out sum: bits[!W]
  repeat i: 0..W {
    sum[i] = a[i]
  }
}
```

**Severity.** LOW — a checker-internal panic on already-invalid input (the file
is rejected either way); reachable from `check`/`compile`. No miscompile: the
emitter never runs on a failed check.

**Fix.** `report_coverage` now skips a zero-width signal (`if width == 0 {
continue; }`) before allocating `covered` — coverage analysis is meaningless on
it, and the `E0410` already reports the real problem.

**Verified.** The reproducer now prints `error[E0410]: \`0\` is not a valid
width` and exits non-zero — no panic in debug or release.

**Tests.** `zero_width_output_with_indexed_drivers_does_not_panic`
(`src/checker/tests.rs`).

---

## SEC-5 (HIGH/MEDIUM) — Unbounded count inputs in the Phase 1.5 simulator → DoS

**What.** The audit of the new simulator (`src/sim/`, `src/commands/{sim,test}`)
found the count-like inputs were **not bounded**, unlike the rest of the
codebase (parser `MAX_DEPTH=64`, emitter `REPEAT_BUDGET=4096`, `MAX_SOURCE_BYTES`).
Three reachable DoS paths:

- **(HIGH, untrusted input)** `tick(clk, n)` in a `test` block (`src/sim/harness.rs`)
  looped `for _ in 0..n` — `n` evaluated from a test-block expression — pushing a
  timeline frame each iteration. `mimz test` on an **untrusted `.mimz`** with
  `tick(clk, 9999999999)` hung the process and exhausted memory.
- **(MEDIUM, operator flag)** `sweep_vectors` (`src/commands/helpers.rs`) built the
  `--sweep` cartesian product with an **unchecked `usize` multiply** and no cap →
  OOM/hang on a large sweep.
- **(MEDIUM, operator flag)** the clocked `run()` loop (`src/sim/run.rs`) ran
  `0..opts.cycles` with no cap; `--cycles` was an unbounded `u64`.

**How found.** Three-agent audit (severity critical→medium) weighted to the
newest code; each finding traced to a reachable path and verified by reading the
call chain (the multiply-overflow variants the agents also flagged are
**unreachable** — the loop cannot complete ~10^18 iterations — so bounding the
loops subsumes them; no separate fix).

**Severity.** HIGH for the test-harness path (untrusted-input hang/OOM), MEDIUM
for the operator-supplied `--sweep`/`--cycles` self-DoS. No memory unsafety / RCE
(safe Rust, `#![forbid(unsafe)]`).

**Fix** (extends the existing "bound every count" doctrine into `src/sim`):

- New `MAX_SIM_CYCLES = 1_000_000` and `MAX_SWEEP_VECTORS = 1_000_000`
  (`src/sim/run.rs`), documented like `REPEAT_BUDGET`.
- `harness.rs`: the `tick` handler rejects when the cumulative cycle count would
  exceed `MAX_SIM_CYCLES` (clean `Stop::Err`, fails fast — no loop).
- `sweep_vectors` returns `Result`, folding the product with `checked_mul` and
  erroring past `MAX_SWEEP_VECTORS` before allocating; caller updated.
- `run()` rejects `opts.cycles > MAX_SIM_CYCLES`, and `--cycles` is range-capped
  at parse time via clap (`value_parser!(u64).range(1..=MAX_SIM_CYCLES)`).

Two LOW defensive fixes landed alongside: `translate.rs` replaced a
`.expect("NameMap serializes")` with a clean error (no-input-panic rule), and
`config.rs` bounded the `mimz.toml` walk-up at `MAX_CONFIG_WALK_DEPTH = 256`.

**Verified.** `mimz test` with `tick(clk, 2000000)` now fails fast with
"test exceeds the 1000000-cycle simulation limit"; `mimz sim --cycles 2000000` is
rejected by clap; an oversized `--sweep` errors before allocating.

**Tests.** `a_tick_count_over_the_cycle_limit_errors_fast_not_hangs`
(`tests/test_run.rs`), `cycles_over_the_limit_is_rejected_by_the_cli`
(`tests/sim.rs`), `sweep_vectors_rejects_an_oversized_product` /
`_allows_a_normal_product` (`src/commands/helpers.rs`).

## SEC-6 (HIGH/LOW) — Phase 1.5 C2–C4 elaboration-time bounds → DoS

**What.** The C1–C4 audit (2026) found SEC-5 bounded the simulator's _runtime_
counts but left its _elaboration-time_ ones unbounded in the new structural
elaborator (`src/sim/elaborate.rs`). `mimz sim`/`mimz test` run on the parsed AST
**without the checker** (which has its own recursion guard), so:

- **(HIGH, untrusted input)** instance flattening recursed with **no depth limit
  or visited-set** — `elaborate_module` → `flatten_instance` → `elaborate_module`.
  A recursive/cyclic instantiation (`module A { let u = A() … }`, or A→B→A) in an
  untrusted `.mimz` overflowed the stack and aborted the process (verified: `mimz
sim`/`mimz test` "has overflowed its stack").
- **(LOW)** the `repeat` span `(hi - lo)` was a **raw `i128` subtraction** before
  the `REPEAT_BUDGET` check — extreme bounds (`hi - lo` past `i128::MAX`) panicked
  under release `overflow-checks`.
- **(LOW)** a bit-indexed drive `sig[idx] = …` cast `idx as u32` after only a
  `< 0` check — an index ≥ 2³² truncated silently.
- **(LOW, found in the 2026-06-17 follow-up pass)** `int_expr`, which substitutes
  each flattened child const as a literal, built a negative value as
  `Neg(int_expr(-v))` — a **raw `i128` negation**. The one value whose magnitude
  does not fit `i128`, `i128::MIN` (magnitude 2¹²⁷), overflow-panicked. It is
  reachable on the checker-skipping sim path: a child const can evaluate to
  `i128::MIN` via checked arithmetic (`(-i128::MAX) - 1`), and **every** child
  const passes through `int_expr` during instance flattening.

**Fix** (extends the "bound every count" doctrine into the structural elaborator):

- `MAX_INSTANCE_DEPTH = 16` (`src/sim/elaborate.rs`), threaded through
  `elaborate_module`/`flatten_instance`; past it, a clean error naming the likely
  recursive module. Kept small so the bound fires well within the 1 MB default
  main-thread stack (each level is a large frame).
- The `repeat` span uses `checked_sub` (overflow ⇒ the existing over-budget error).
- A bit-index drive is bounded to `0..128` (the evaluator's max width) before the
  `as u32` cast.
- Flattened-instance merge now errors on a name collision instead of silently
  overwriting (a parent signal named like a flattened `inst_port` wire).
- `int_expr` is now non-recursive and takes the magnitude via `unsigned_abs`
  (always fits `u128`), so `i128::MIN` lowers to `-2¹²⁷` instead of panicking.

**Verified.** `mimz sim` / `mimz test` on a self-instantiating module now exit
non-zero with "instance nesting exceeds 16 levels …" instead of overflowing the
stack; the cpu/alu demos (depth-1 instances) still run.

**Tests.** `recursive_instantiation_errors_not_overflows`,
`extreme_repeat_bounds_error_not_overflow`, `an_out_of_range_bit_index_errors`,
`a_flatten_name_collision_errors`, `an_i128_min_const_elaborates_without_overflow`
(`src/sim/elaborate.rs`).

**Audited clean (no change needed).** The core pipeline (lexer→parser→checker→
emitter) and the untrusted-input boundary (project/import loading, config
discovery, name-map deserialization) audited clean: SEC-1..4 + BUG-1/2 intact,
all five thamizh-order flips depth-guarded, checked arithmetic throughout, and
no path traversal (import segments are XID identifiers — `..`/`/` inexpressible).

## SEC-7 (MEDIUM, FIXED 2026-07-18) — `[lib] std` override path traversal

**Note (2026-07-18).** This entry was originally filed as a second, duplicate
`SEC-5` (a copy-paste-without-renumbering slip when the CTO review's finding
was appended) — renumbered here, nothing else changed. Also corrected the
**Cause** location below: verified against current source, the join lives in
`src/commands/helpers.rs`, not `src/config.rs`.

**What.** The `[lib] std = "..."` override in `mimz.toml` resolves with a raw
path join and no containment check.

**Cause.** `lib_std_dir` (`src/commands/helpers.rs:63-72`) computes
`base.join(std)` (line 71) with no `canonicalize` and no check that the
result stays inside the workspace root. An attacker supplying a malicious
`mimz.toml` could specify `std = "../../../../etc/keys"` or similar to
traverse directories outside the workspace root — `import std.<m>` would
then load an arbitrary on-disk file as a standard-library module.

**How found.** CTO Architectural Review (July 2026); re-verified against
current source 2026-07-18 (still present, unchanged, same location modulo
the file-path correction above).

**Severity.** MEDIUM — if the compiler is executed in a shared cloud
environment (like the WASM playground or a CI server), this directory
traversal could leak host files by embedding them as standard library
modules.

**Fix (2026-07-18).** `lib_std_dir` (`src/commands/helpers.rs`) now
`canonicalize`s both the workspace root and the resolved `std` candidate and
rejects with a clean CLI error (`ExitCode::FAILURE`, no panic) when the
candidate doesn't stay inside the root — no escape-hatch flag; the roadmap's
two options were "subdirectory enforcement" OR "an explicit CLI flag to
escape it," and plain enforcement is the simpler, safer-by-default choice
with no accidental-opt-out foot-gun. If the candidate doesn't exist on disk,
`canonicalize` fails and the override is let through unchecked — nothing to
leak from a path that doesn't exist, and the later `import` resolution
fails on it normally either way. All 4 call sites (`check`, `compile`,
`sim`, `test` — `lib_std_dir`'s only callers) updated for the new
`Result<Option<PathBuf>, ExitCode>` signature (was `Option<PathBuf>`), same
error-handling convention as `resolve_config`/`resolve_lang`.

**Test.** `std_override_escaping_workspace_root_is_rejected`,
`std_override_inside_workspace_root_is_allowed` (`tests/config.rs`) — driven
through the real binary (`mimz check`), since the sandbox is a CLI-layer
concern (a real `mimz.toml` + on-disk directories), not a unit-testable one.

---

## SEC-8 (HIGH, OPEN) — `mimz eval`/`sim`/`test` run on untrusted input without `checker::check`

**What.** The three commands that directly execute a `.mimz` file's
semantics (`mimz eval`, `mimz sim`, `mimz test`) parse and elaborate/evaluate
the AST **without ever calling `checker::check`** first. The checker is the
compiler's one hardened, fully-checked-arithmetic semantic authority
(`MAX_WIDTH`, overflow guards, E02xx/E04xx width rules); every one of these
three paths runs the simulator's own, independently-implemented width/const
model directly against raw, unchecked AST.

**Cause.** `src/commands/eval.rs` (verified 2026-07-18, full file read): the
command's pipeline is lex → parse → `sim::comb::eval_outputs`, with no
`checker::check` call anywhere on the path. `sim`/`test` share the same
elaborator entry points. This was flagged once already as SEC-2 (the
simulator's `const_eval` was a divergent naive copy, fixed by delegating to
the checker's hardened evaluator) and again as S1-S3 in
`review-2026-07-03.md` — SEC-8 is the general form of the same root cause,
not a new mechanism: as long as any input-execution path can reach the
simulator's semantics without the checker ever running, every future
checker-vs-simulator divergence (see SEC-9/BUG-11 below) is silently
reachable from untrusted input, not just from already-checked source.

**How found.** CTO Architectural Review (July 2026, "A2"); re-verified
against current source 2026-07-18 (unchanged — confirmed no checker call on
any of the three command paths).

**Severity.** HIGH — this is what makes BUG-11 (SEC-9 below) reachable from
untrusted input with no warning at all: the checker would reject the same
divergent shift semantics were it ever run, but none of these three
commands run it. Matches the project's own stated threat model (`mimz eval |
check | sim <file>` run on an untrusted `.mimz` file) — `eval`/`sim`/`test`
are explicitly in scope for that model and currently bypass its strongest
guard.

**Fix (Pending).** Run `checker::check` before `eval`/`sim`/`test` evaluate
anything (reject on any checker error, same as `compile`/`check` already
do), then extract one shared width/const-eval module the checker and
simulator both call — the long-term fix that makes a second divergence
class like BUG-11 structurally impossible rather than merely caught.

---

## SEC-9 (CRITICAL, FIXED 2026-07-18) — Simulator vs. emitted Verilog disagree on left-shift result width

**Note.** Tracked in full in [`bugs.md`](bugs.md) as **BUG-11** (functional
divergence, not an input-triggered crash) — filed here too, under its own
SEC number, because the CTO review classifies it as the audit's headline
correctness/trust defect and because it is the concrete, reproduced instance
of the SEC-8 divergence-reachability class above. See `bugs.md` BUG-11 for
the full reproduction, cause, and fix; no separate write-up duplicated here
to avoid the two drifting out of sync.

**SEC-8 itself stays OPEN** — fixing BUG-11 closes this one _specific_
instance of the divergence-reachability class, not the class itself.
`eval`/`sim`/`test` still run without `checker::check`; a future construct
with its own checker-vs-simulator mismatch is exactly as reachable from
untrusted input as this one was, until SEC-8 lands.
