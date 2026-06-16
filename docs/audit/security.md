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
  `if_expr` (`src/parser/expr.rs`), `seq_if`, `test_if` (`src/parser/items.rs`).
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

**Audited clean (no change needed).** The core pipeline (lexer→parser→checker→
emitter) and the untrusted-input boundary (project/import loading, config
discovery, name-map deserialization) audited clean: SEC-1..4 + BUG-1/2 intact,
all five thamizh-order flips depth-guarded, checked arithmetic throughout, and
no path traversal (import segments are XID identifiers — `..`/`/` inexpressible).
