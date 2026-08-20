//! T2 (docs/audit/review-2026-07-17.md sec 8, docs/plan/
//! phase-2-differential-fuzzing.md): random-program differential fuzzing.
//! v3 scope adds clocked designs (`clock`/`reset`/`reg`/`on rise`) on top
//! of v1/v2's unsigned+signed combinational base — no enum/bundle/fn/
//! foreach/imports/dual-edge/multiple clocks yet. Full design (v1
//! baseline): docs/superpowers/specs/2026-07-18-differential-fuzzing-design.local.md
//!
//! Generates a real `.mimz` module as source text, checker-clean **by
//! construction** (every combine step unifies operand widths via
//! `extend()` and operand KIND via `signed()`/`unsigned()` before applying
//! an operator, so no operator's own fine-print width/kind rule needs
//! special-casing here), then runs it through the full pipeline: lex ->
//! parse -> checker::check -> our kernel -> `mimz compile` -> Icarus. Never
//! generates a write-slice (`sig[hi:lo] = expr`, BUG-17) — only ever one
//! whole-signal `out y = <expr>` (v1/v2) or one non-blocking `reg <- expr`
//! per register (v3).

use std::collections::BTreeMap;
use std::path::PathBuf;

use mimz::sim::comb;
use mimz::sim::elaborate::elaborate_project;
use mimz::sim::run::{SimOpts, run};
use mimz::{checker, diag, lexer, parser};

mod support;

/// Seed base for the combinational generator.
const COMB_SEED_BASE: u64 = 0xC0FFEE;
/// Seed base for the clocked generator — deliberately disjoint from
/// `COMB_SEED_BASE` so the two generators' seed spaces never alias.
const CLOCKED_SEED_BASE: u64 = 0xC10CCED;
/// Fresh-seed depth when neither `MIMZ_DIFF_FUZZ_N` nor
/// `MIMZ_DIFF_FUZZ_CLOCKED_N` is set — i.e. a plain local `cargo test`.
///
/// Deliberately small, and NOT the depth that finds bugs. Each seed shells out
/// to `mimz compile` plus `iverilog` plus `vvp`, so depth is paid in process
/// spawns (~3 per seed, per generator); on Windows that is far more expensive
/// than on CI's Linux runner, and it is charged to every unrelated `cargo test`
/// a developer runs. Depth is a *discovery* tool and belongs where a long run
/// is free — `ci.yml` sets 400 for the per-PR `check` job and 5000 for the
/// weekly `fuzz-nightly` job (GAP-11).
///
/// Regression protection does not depend on this number: the corpus below is
/// replayed unconditionally, at every depth including 0.
const DEFAULT_FUZZ_N: u64 = 20;

/// Every seed that has ever failed, from `tests/fixtures/fuzz-seeds/{kind}.txt`.
///
/// Plain newline-delimited decimal seeds, `#` starts a comment. A fuzzer
/// without a regression corpus re-finds the same bug and loses the old ones
/// (GAP-11) — and since the fresh seeds are `base + i`, changing the depth
/// silently changes which historical bugs are still covered. Replaying the
/// corpus explicitly decouples the two.
fn corpus_seeds(kind: &str) -> Vec<u64> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fuzz-seeds")
        .join(format!("{kind}.txt"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fuzz-seed corpus {} is unreadable: {e}", path.display()));
    let seeds: Vec<u64> = text
        .lines()
        .filter_map(|l| {
            let l = l.split('#').next().unwrap_or("").trim();
            (!l.is_empty()).then(|| {
                l.parse().unwrap_or_else(|e| {
                    panic!("{}: {l:?} is not a decimal seed: {e}", path.display())
                })
            })
        })
        .collect();
    // An empty corpus is silent data loss, not an empty test: it would sail
    // through at any depth while covering nothing. Fail loudly instead.
    assert!(
        !seeds.is_empty(),
        "{} parsed to zero seeds — the regression corpus is the only depth-independent \
         coverage these fuzzers have",
        path.display()
    );
    seeds
}

/// The regression corpus first, then `n` fresh seeds from `base` — the seed
/// list both Icarus differentials iterate. Corpus-first so a reintroduced bug
/// fails before a long fresh run has to finish.
fn fuzz_seeds(kind: &str, env_var: &str, base: u64) -> Vec<u64> {
    let n: u64 = std::env::var(env_var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FUZZ_N);
    corpus_seeds(kind)
        .into_iter()
        .chain((0..n).map(|i| base.wrapping_add(i)))
        .collect()
}

/// Parse `DIFF <step> name=<raw binary> …` into `step -> {name: raw string}`
/// — the wide-value counterpart of `support::parse_icarus` (which parses
/// straight into `u128` and can't represent a value over 128 bits, per
/// BUG-13 layer 1). Used by both fuzz tests below, since their generators
/// now emit widths past the old 128-bit ceiling (Task 12).
fn parse_icarus_raw(stdout: &str) -> BTreeMap<u64, BTreeMap<String, String>> {
    let mut icarus: BTreeMap<u64, BTreeMap<String, String>> = BTreeMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("DIFF ") else {
            continue;
        };
        let mut it = rest.split_whitespace();
        let step: u64 = it.next().unwrap().parse().unwrap();
        let row = icarus.entry(step).or_default();
        for pair in it {
            let (n, v) = pair.split_once('=').unwrap();
            row.insert(n.to_string(), v.to_string());
        }
    }
    icarus
}

/// Parse a `%b`-format binary string (Icarus's `$display` output, no
/// separators, any padding) into little-endian `u64` limbs sized for
/// `width` bits — the inverse of the kernel's own (crate-private)
/// `wide::to_binary_string`, needed since `u128::from_str_radix` can't
/// hold more than 128 bits.
fn limbs_from_binary(s: &str, width: u32) -> Vec<u64> {
    let n_limbs = (width as u64).div_ceil(64) as usize;
    let mut limbs = vec![0u64; n_limbs];
    for (i, c) in s.trim().chars().rev().enumerate() {
        if c == '1' {
            limbs[i / 64] |= 1u64 << (i % 64);
        }
    }
    limbs
}

/// `Bits` (either variant) as little-endian `u64` limbs sized for `width`
/// bits, so a kernel output compares against [`limbs_from_binary`]
/// regardless of which path (narrow/wide) produced it.
fn bits_to_limbs(b: &mimz::sim::value::Bits, width: u32) -> Vec<u64> {
    use mimz::sim::value::Bits;
    match b {
        Bits::Wide(limbs) => limbs.clone(),
        Bits::Small(v) => {
            let n_limbs = (width as u64).div_ceil(64) as usize;
            let mut limbs = vec![0u64; n_limbs];
            limbs[0] = *v as u64;
            if limbs.len() > 1 {
                limbs[1] = (*v >> 64) as u64;
            }
            limbs
        }
    }
}

/// GAP-5 direction 1 (docs/audit/gaps.md): after every simulator evaluation,
/// assert the produced `Bits` fits the width the SIMULATOR ITSELF resolved
/// for that signal — `Output::width` (from `comb::eval_outputs`) and
/// `Timeline::signals` (from `run`) both come from elaboration, which folds
/// the checker-validated AST type, not from anything this fuzzer's own
/// generator tracks. This is the oracle BUG-30 fell through: two
/// independent authorities agreeing on width is exactly what a checker
/// Ty-vs-simulator-Val divergence would break. `mimz-sim`'s kernel already
/// masks every stored value to its signal's width by construction, so this
/// should never fire today — it exists to catch a FUTURE regression in that
/// invariant (a `Val`/`Bits` construction site that skips the mask), which
/// is exactly the shape of bug BUG-11/BUG-13's fix-forward note already
/// found once in kernel.rs.
fn assert_bits_fit_width(ctx: &str, value: &mimz::sim::value::Bits, width: u32) {
    use mimz::sim::value::Bits;
    match value {
        Bits::Small(v) => {
            if width < 128 {
                let mask = (1u128 << width) - 1;
                assert_eq!(
                    v & !mask,
                    0,
                    "{ctx}: value {v:#x} has a bit set above its declared width {width}"
                );
            }
            // width >= 128: every bit of a Small value is legitimately in range.
        }
        Bits::Wide(limbs) => {
            for (i, limb) in limbs.iter().enumerate() {
                let limb_lo = i as u32 * 64;
                if limb_lo >= width {
                    assert_eq!(
                        *limb, 0,
                        "{ctx}: limb {i} ({limb:#x}) is entirely above declared width {width}"
                    );
                    continue;
                }
                let valid_bits = width - limb_lo;
                if valid_bits < 64 {
                    let mask = (1u64 << valid_bits) - 1;
                    assert_eq!(
                        limb & !mask,
                        0,
                        "{ctx}: limb {i} ({limb:#x}) has a bit set above declared width {width}"
                    );
                }
            }
        }
    }
}

/// One input port (or, in the v3 clocked generator, one register):
/// `(name, width, signed)`.
type Port = (String, u32, bool);

/// `bits[N]` or `signed[N]` — shared by `in`/`out`/`reg` declarations.
fn ty_str(w: u32, signed: bool) -> String {
    if signed {
        format!("signed[{w}]")
    } else {
        format!("bits[{w}]")
    }
}

/// Deterministic splitmix-style PRNG — same shape
/// `tests/icarus.rs::gen_vectors` already uses. Seeded per-iteration so a
/// run (and any failure) is always reproducible by seed number alone.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(2_654_435_761).wrapping_add(0x9E3779B9);
        self.0
    }

    /// Uniform in `0..n` (`n` must be > 0).
    fn next_range(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// A generated expression fragment, alongside its own known width and kind
/// (signed vs. unsigned) — the generator's core invariant: every
/// fragment's width AND kind are known by construction, never inferred
/// after the fact. Every leaf is either a port reference (kind = the
/// port's declared kind) or a literal explicitly wrapped via `extend()`
/// (unsigned) or `signed(extend(...))` (signed) — so a bare `Ty::CtInt`
/// never reaches a combine step, and `signed`/`unsigned` casts applied to
/// an already-typed `Frag` are always checker-legal in either direction.
///
/// `atomic` tracks whether `widen()` (which renders `extend(text,
/// target)`) can trust real Verilog to actually apply that growth: TRUE
/// only for a plain identifier (`gen_leaf`'s port/reg branch, `gen_slice`)
/// or a literal already resolved to an explicit width (`gen_leaf`'s
/// literal branch, post-BUG-18) — anything whose OWN width is a fixed,
/// self-determined fact regardless of where it lands. FALSE for every
/// combinator's result (`combine_same_width`/`combine_shift`/
/// `combine_concat`): `extend()` is a pure passthrough for a non-literal
/// argument (`Builtin::Extend`'s codegen only special-cases a resolved
/// constant), so widening a COMPUTED expression this way only works when
/// the surrounding context happens to re-derive the same width on its
/// own — which real Verilog does NOT reliably do (confirmed live, twice,
/// during v2/v3 development: BUG-19's `+%`/`-%` case, and a
/// `signed(extend(<2-bit concat>, 4))` register-assign case where the
/// kernel and Icarus disagreed outright, not just on a growth bit).
/// `cast_to` (`signed(x)`/`unsigned(x)`) never touches width, so it
/// preserves `atomic` either way — a reinterpretation cast doesn't depend
/// on context regardless of what's underneath it.
#[derive(Clone)]
struct Frag {
    text: String,
    width: u32,
    signed: bool,
    atomic: bool,
}

/// Hard cap on any single fragment's width. Raised past the simulator's
/// former 128-bit ceiling (BUG-13, fixed 2026-07-22 — this generator's own
/// `MAX_WIDTH` is unrelated to `mimz_core::width_rules::MAX_WIDTH`, same
/// name, different scope: this one just bounds how wide a FUZZED program's
/// signals get, purely for keeping generated `.mimz` sources a readable
/// size) so the differential suite actually exercises the wide
/// (`Bits::Wide`) code path against Icarus, not just the narrow one.
const MAX_WIDTH: u32 = 512;

/// Widen `f` to `target` bits if it's narrower; a no-op otherwise. When
/// `f` is `atomic` (see `Frag`'s doc comment), this is a plain
/// `extend()` wrap — safe, since real Verilog's context-determined
/// propagation reliably applies to a plain identifier or an
/// already-explicitly-sized literal. When `f` is a COMPUTED expression
/// (not atomic), `extend()`-wrapping it is unsound (the same growth-gets-
/// silently-dropped-or-mishandled risk `SAME_WIDTH_OPS`'s doc comment
/// documents for BUG-19) — so instead of trusting it, `f` is discarded
/// and replaced with a fresh literal sized EXACTLY to `target`, at `f`'s
/// own kind (the caller — `combine_same_width` — already unified kind via
/// `cast_to` before calling this, so `f.signed` is already the kind the
/// result needs). The result is always `atomic: true`, so it stays safe
/// under a FURTHER `widen()` call too.
fn widen(rng: &mut Rng, f: Frag, target: u32) -> Frag {
    if f.width >= target {
        return f;
    }
    if f.atomic {
        return Frag {
            text: format!("extend({}, {target})", f.text),
            width: target,
            signed: f.signed,
            atomic: true,
        };
    }
    let v = rng.next_u64() & support::mask(target) as u64;
    if f.signed {
        Frag {
            text: format!("signed(extend({v}, {target}))"),
            width: target,
            signed: true,
            atomic: true,
        }
    } else {
        Frag {
            text: format!("extend({v}, {target})"),
            width: target,
            signed: false,
            atomic: true,
        }
    }
}

/// Cast `f` to `want_signed` if it isn't already that kind — `signed(x)`
/// (legal on any unsigned `Frag`: it's always `Ty::Bit`/`Ty::Bits`, never
/// `Ty::CtInt` or already-`Signed`, by the leaf-construction invariant) or
/// `unsigned(x)` (legal on any signed `Frag`, always `Ty::Signed` — the
/// only type `Builtin::UnsignedCast` accepts). A no-op when already the
/// right kind, so callers can call it unconditionally.
fn cast_to(f: Frag, want_signed: bool) -> Frag {
    if f.signed == want_signed {
        return f;
    }
    let atomic = f.atomic;
    if want_signed {
        Frag {
            text: format!("signed({})", f.text),
            width: f.width,
            signed: true,
            atomic,
        }
    } else {
        Frag {
            text: format!("unsigned({})", f.text),
            width: f.width,
            signed: false,
            atomic,
        }
    }
}

/// Force `f` to land on EXACTLY `target_w` bits and `target_signed` kind —
/// used only by v3's per-register next-state expressions, where the
/// target type is fixed in advance (the register was already declared
/// with it) rather than derived from whatever the body produces (v1/v2's
/// `out y` approach). `gen_expr(..., cap: target_w)` only guarantees
/// width `<= target_w`, not `==`.
///
/// CAST FIRST, then widen — not the other way around. `signed(x)`/
/// `unsigned(x)` have a **self-determined argument** in real Verilog (the
/// LRM: a `$signed`/`$unsigned` argument is evaluated at its own natural
/// width, never extended from the surrounding context) — so
/// `signed(extend(x, W))` (cast-of-widen) NEVER actually widens `x`
/// before the reinterpretation: `extend()`'s codegen contributes nothing
/// syntactically for a non-literal argument, so the rendered Verilog is
/// just `$signed(x)`, reinterpreting `x`'s bits at its OWN width, THEN
/// whatever sign-extension the outer register assignment applies —
/// found live during v3 development, seed `202427986`: kernel computed
/// `signed(extend(p1[5:3], 8))` per mimz's own type-level model
/// (zero-extend the 3-bit slice to 8, THEN reinterpret as signed — value
/// `6`), but real Icarus reinterpreted the 3-bit slice as signed FIRST
/// (`p1[5:3] = 0b110 = -2` as 3-bit signed), THEN sign-extended -2 to 8
/// bits assigning into the reg (`0b11111110`) — a genuine value mismatch,
/// not just a lost growth bit. `extend(signed(x), W)` (widen-of-cast)
/// doesn't have this problem: `signed(x)` self-determines at `x`'s own
/// width (matching what real Verilog does anyway), and `widen` extending
/// an ALREADY-signed value is just ordinary sign-extension into a
/// directly-assigned (context-determined) register target — the checker's
/// own type-level model AGREES with this order (`Extend` on a `Signed(n)`
/// argument returns `Signed(target)`, i.e. "sign-extend what's already
/// signed"), so kernel and Icarus can no longer disagree on which
/// operation happens at which width. `combine_same_width` already casts
/// before widening for exactly this reason (found correct by construction,
/// not by observation) — this function was the one call site that had it
/// backwards.
fn force_width(rng: &mut Rng, f: Frag, target_w: u32, target_signed: bool) -> Frag {
    widen(rng, cast_to(f, target_signed), target_w)
}

/// Clamp `f` to `cap` bits if it's wider. `checker::check` accepts slicing
/// ANY computed sub-expression (`y = (a + b)[5:2]` is checker-legal, not
/// just `y = a[5:2]`) — but the emitter renders `ExprKind::Slice`/`Index`
/// as bare `{base}[hi:lo]` with no grouping, and Verilog's part-select
/// grammar only accepts an identifier before `[...]`, not an arbitrary
/// expression. Confirmed live: `iverilog` rejects both `(a & b)[2:0]` and
/// `{a, b}[3:0]` as a syntax error — a genuine, previously-unknown emitter
/// bug (filed as **BUG-20**, `docs/audit/bugs.md`), distinct from BUG-17
/// (write-slice) and BUG-19 (self-determined-position value mismatch):
/// this one is a hard compile failure for ANY read-slice of a non-identifier
/// base, not a wrong value. So this generator only ever slices a fragment
/// that IS a bare port identifier (safe — matches `gen_slice`'s own
/// existing restriction, which already never slices anything else); an
/// over-cap composite fragment (`combine_concat`'s sum, `combine_lossless`'s
/// `max+1`/product growth, or `combine_wrap`'s own operand-width result
/// all can exceed `cap`) is discarded and replaced with a fresh literal
/// sized EXACTLY to `cap`, built directly
/// here rather than via `gen_leaf` — `gen_leaf`'s port branch could itself
/// return something wider than `cap` (v3's per-register `cap` can be
/// narrower than any port), so the fallback must not risk exceeding the
/// very bound it exists to enforce. Called on every `gen_expr` result,
/// making "every recursive call returns width <= its caller's assumed
/// cap" a strict invariant — parent combinators never need their own
/// width bookkeeping beyond that.
fn clamp(rng: &mut Rng, ports: &[Port], f: Frag, cap: u32) -> Frag {
    if f.width <= cap {
        return f;
    }
    if ports.iter().any(|(name, _, _)| name == &f.text) {
        // A slice always yields unsigned `bits` per `slice_ty`
        // (`checker/widths/expr.rs`), regardless of the sliced
        // fragment's own kind.
        return Frag {
            text: format!("{}[{}:0]", f.text, cap - 1),
            width: cap,
            signed: false,
            atomic: true,
        };
    }
    let v = rng.next_u64() & support::mask(cap) as u64;
    if rng.next_range(2) == 0 {
        Frag {
            text: format!("extend({v}, {cap})"),
            width: cap,
            signed: false,
            atomic: true,
        }
    } else {
        Frag {
            text: format!("signed(extend({v}, {cap}))"),
            width: cap,
            signed: true,
            atomic: true,
        }
    }
}

/// A leaf: either a reference to an existing input port (kind = the
/// port's own declared kind), or a small literal explicitly widthed via
/// `extend()` (unsigned) or `signed(extend(...))` (signed) — never a bare
/// literal mid-expression. `extend(<CtInt>, N)` alone always yields
/// unsigned `bits` (`call_ty`'s `Ty::CtInt` arm fits + returns `bits(n)`
/// unconditionally, even when eventually cast) — so a signed literal leaf
/// needs the outer `signed(...)` reinterpretation cast, not a different
/// `extend` argument.
/// GAP-13 direction 2 (`docs/audit/gaps.md`): `special` is the pool
/// `gen_special_leaves` built — a `fn` call, an instance-port read, etc.
/// — each a fully pre-rendered `Frag`. Drawn from on the same footing as
/// an ordinary port: about 1 time in 4 when the pool is non-empty, matching
/// the existing 1-in-3 port bias below closely enough that these shapes
/// show up often without dominating every generated program.
fn gen_leaf(rng: &mut Rng, ports: &[Port], special: &[Frag]) -> Frag {
    if !special.is_empty() && rng.next_range(4) == 0 {
        return special[rng.next_range(special.len() as u64) as usize].clone();
    }
    if !ports.is_empty() && rng.next_range(3) != 0 {
        let (name, w, signed) = &ports[rng.next_range(ports.len() as u64) as usize];
        Frag {
            text: name.clone(),
            width: *w,
            signed: *signed,
            atomic: true,
        }
    } else {
        let w = (rng.next_range(8) + 1) as u32;
        let v = rng.next_u64() & support::mask(w) as u64;
        if rng.next_range(2) == 0 {
            Frag {
                text: format!("extend({v}, {w})"),
                width: w,
                signed: false,
                atomic: true,
            }
        } else {
            Frag {
                text: format!("signed(extend({v}, {w}))"),
                width: w,
                signed: true,
                atomic: true,
            }
        }
    }
}

/// Result-width effect of a same-width-family operator, tracked so the
/// generator knows the new fragment's width without re-deriving each
/// operator's own rule from scratch.
///
/// Excludes `+`/`-` (lossless) AND `+%`/`-%` (wrapping) — not because
/// they're unsafe (BUG-19, `docs/audit/bugs.md`, is now FIXED — the
/// emitter hoists a self-determined-position mismatch instead of
/// trusting a passthrough `extend()`), but because they don't fit this
/// table's width-effect model: lossless growth needs no prior width
/// unification (`combine_lossless` unifies KIND only) and yields
/// `max(w1,w2)+1`/`w1+w2` rather than `Preserve`/`ToBit`, while wrapping
/// keeps the operand width but isn't a `Preserve`-style bitwise/compare
/// op either. Both families are generated separately by
/// `combine_lossless`/`combine_wrap` and wired into `gen_expr`'s own
/// dispatch, re-enabled once BUG-19's fix was confirmed live against
/// Icarus. `&`/`|`/`^` and every comparison stay in this table: bitwise
/// ops commute correctly with zero/sign-extension regardless of WHEN
/// Verilog performs it, and a comparison's result is always exactly 1
/// bit either way, so neither operator family was ever sensitive to the
/// self-determined-position timing difference BUG-19 was about.
#[derive(Clone, Copy)]
enum WidthEffect {
    /// `& | ^` — preserve the (now-equal) operand width.
    Preserve,
    /// `== != < <= > >=` — always `bit` (width 1).
    ToBit,
}

const SAME_WIDTH_OPS: &[(&str, WidthEffect)] = &[
    ("&", WidthEffect::Preserve),
    ("|", WidthEffect::Preserve),
    ("^", WidthEffect::Preserve),
    ("==", WidthEffect::ToBit),
    ("!=", WidthEffect::ToBit),
    ("<", WidthEffect::ToBit),
    ("<=", WidthEffect::ToBit),
    (">", WidthEffect::ToBit),
    (">=", WidthEffect::ToBit),
];

/// Combine two fragments under a randomly chosen same-width-family
/// operator: unify both operands' KIND first (`signed`/`unsigned`
/// mixing is E0403 — cast one side to whichever kind was picked, a no-op
/// if it already matches), then unify WIDTH to `max(a.width, b.width)` via
/// `widen` (legal for every operator in this family once both match, no
/// per-operator special-casing needed), then apply it.
fn combine_same_width(rng: &mut Rng, a: Frag, b: Frag) -> Frag {
    let target_signed = if rng.next_range(2) == 0 {
        a.signed
    } else {
        b.signed
    };
    let a = cast_to(a, target_signed);
    let b = cast_to(b, target_signed);
    let w = a.width.max(b.width);
    let a = widen(rng, a, w);
    let b = widen(rng, b, w);
    let (op, effect) = SAME_WIDTH_OPS[rng.next_range(SAME_WIDTH_OPS.len() as u64) as usize];
    let (width, signed) = match effect {
        WidthEffect::Preserve => (w, target_signed),
        // Comparisons always yield an unsigned `bit`, regardless of
        // operand kind (`binary_ty`'s `Eq|Ne|Lt|Le|Gt|Ge` arms return
        // `Ty::Bit` unconditionally after the matched-kind check).
        WidthEffect::ToBit => (1, false),
    };
    Frag {
        text: format!("({} {op} {})", a.text, b.text),
        width,
        signed,
        atomic: false,
    }
}

/// Combine two fragments under a randomly chosen lossless operator
/// (`+`/`-`/`*`) — Stage 4, Phase A1b re-enables this family now that
/// the emitter hoists a self-determined-position mismatch instead of
/// trusting a passthrough (BUG-19, `docs/audit/bugs.md`, now fixed).
/// Unlike `combine_same_width`, no width-unification is needed first —
/// lossless growth accepts unequal operand widths by design; only KIND
/// must match (mixing `signed`/`bits` is E0403).
fn combine_lossless(rng: &mut Rng, a: Frag, b: Frag) -> Frag {
    let target_signed = if rng.next_range(2) == 0 {
        a.signed
    } else {
        b.signed
    };
    let a = cast_to(a, target_signed);
    let b = cast_to(b, target_signed);
    let (op, width) = match rng.next_range(3) {
        0 => ("+", a.width.max(b.width) + 1),
        1 => ("-", a.width.max(b.width) + 1),
        _ => ("*", a.width + b.width),
    };
    Frag {
        text: format!("({} {op} {})", a.text, b.text),
        width,
        signed: target_signed,
        atomic: false,
    }
}

/// Combine two fragments under a randomly chosen wrapping operator
/// (`+%`/`-%`/`*%`) — re-enabled alongside `combine_lossless` for the
/// same reason (BUG-19 fixed). Re-enabling this combinator also surfaced
/// BUG-23 (wrapping operators lose their width truncation when nested
/// under sibling context-determined operators) — now fixed; default N
/// (`cargo test --test differential_fuzz`) is green. This deep-N pass
/// also once surfaced BUG-24 (seed `12648537` — a shift's left operand
/// losing its width growth under a sibling context-determined operator)
/// — also now fixed, see `docs/audit/bugs.md`. Needs width-unification
/// first (the wrap family keeps the operand width, mirroring
/// `combine_same_width`'s own approach).
fn combine_wrap(rng: &mut Rng, a: Frag, b: Frag) -> Frag {
    let target_signed = if rng.next_range(2) == 0 {
        a.signed
    } else {
        b.signed
    };
    let a = cast_to(a, target_signed);
    let b = cast_to(b, target_signed);
    let w = a.width.max(b.width);
    let a = widen(rng, a, w);
    let b = widen(rng, b, w);
    let op = match rng.next_range(3) {
        0 => "+%",
        1 => "-%",
        _ => "*%",
    };
    Frag {
        text: format!("({} {op} {})", a.text, b.text),
        width: w,
        signed: target_signed,
        atomic: false,
    }
}

/// Shift: `>>` keeps LHS's own width AND kind unchanged (right-shifting
/// only ever reduces magnitude, so the left operand's own width already
/// bounds it). `<<` GROWS instead (BUG-30, `docs/audit/bugs.md`) — the
/// amount here is always wrapped in `extend(shamt_v, shamt_w)`, which the
/// checker treats as a genuine SIZED `bits[shamt_w]` value (`extend(x, N)`
/// is the established idiom for giving a literal a fixed, non-adapting
/// width — `checker::widths::ops::builtins::call_ty`'s `Ty::CtInt(v)`
/// arm), not a compile-time constant the shift can grow by exactly — so
/// growth is the amount's own worst case, `2^shamt_w - 1`, same as any
/// other runtime `bits[shamt_w]` amount. RHS is a separate, small (1-5
/// bit) fragment, always freshly generated as an unsigned literal — never
/// derived from an existing (possibly signed) `Frag` — since a shift
/// amount can never be `signed` (E0403, `shift_ty`'s `Ty::Signed(_)` arm
/// for `rt`). No width or kind relationship to LHS otherwise.
fn combine_shift(rng: &mut Rng, a: Frag) -> Frag {
    let shamt_w = (rng.next_range(5) + 1) as u32;
    let shamt_v = rng.next_u64() & support::mask(shamt_w) as u64;
    let is_left = rng.next_range(2) == 0;
    let op = if is_left { "<<" } else { ">>" };
    let width = if is_left {
        a.width + ((1u32 << shamt_w) - 1)
    } else {
        a.width
    };
    let signed = a.signed;
    Frag {
        text: format!("({} {op} extend({shamt_v}, {shamt_w}))", a.text),
        width,
        signed,
        atomic: false,
    }
}

/// GAP-13 direction 2 (`docs/audit/gaps.md`): a fresh 1-bit `if`
/// condition — two operands unified and compared with `!=`, reusing
/// `combine_same_width`'s own cast+widen machinery but pinned to a
/// `ToBit` result instead of picking an operator at random (an `if`
/// condition must be exactly 1 bit — `checker::widths`' own rule).
fn gen_cond(rng: &mut Rng, a: Frag, b: Frag) -> Frag {
    let target_signed = if rng.next_range(2) == 0 {
        a.signed
    } else {
        b.signed
    };
    let a = cast_to(a, target_signed);
    let b = cast_to(b, target_signed);
    let w = a.width.max(b.width);
    let a = widen(rng, a, w);
    let b = widen(rng, b, w);
    Frag {
        text: format!("({} != {})", a.text, b.text),
        width: 1,
        signed: false,
        atomic: false,
    }
}

/// `if cond { a } else { b }` — BUG-41's repro ③ shape, never generated
/// randomly before this (only hand-picked, `tests/self_determined_
/// regression.rs`). Both branches must share exactly one `Ty` (the
/// checker's own rule), so reuse the same cast+widen unification every
/// other same-typed combinator here does.
fn combine_if(rng: &mut Rng, cond: Frag, a: Frag, b: Frag) -> Frag {
    let target_signed = a.signed;
    let a = cast_to(a, target_signed);
    let b = cast_to(b, target_signed);
    let w = a.width.max(b.width);
    let a = widen(rng, a, w);
    let b = widen(rng, b, w);
    Frag {
        text: format!("(if {} {{ {} }} else {{ {} }})", cond.text, a.text, b.text),
        width: w,
        signed: target_signed,
        atomic: false,
    }
}

/// `match sel { 0 => a  1 => b  _ => c }` — three arms, exhaustive via
/// the wildcard regardless of `sel`'s own width or kind, unified the
/// same way `combine_if`'s two branches are. Never generated randomly
/// before this either (the only `Match` differential,
/// `shape_match_operand_of_add_in_concat_matches_icarus`, was hand-
/// written building GAP-13's own axis).
fn combine_match(rng: &mut Rng, sel: Frag, a: Frag, b: Frag, c: Frag) -> Frag {
    let target_signed = a.signed;
    let a = cast_to(a, target_signed);
    let b = cast_to(b, target_signed);
    let c = cast_to(c, target_signed);
    let w = a.width.max(b.width).max(c.width);
    let a = widen(rng, a, w);
    let b = widen(rng, b, w);
    let c = widen(rng, c, w);
    Frag {
        text: format!(
            "(match {} {{\n    0 => {}\n    1 => {}\n    _ => {}\n  }})",
            sel.text, a.text, b.text, c.text
        ),
        width: w,
        signed: target_signed,
        atomic: false,
    }
}

/// Concat: `{a, b}`. A `signed` fragment cannot concatenate directly
/// (E0403, `concat_ty`'s `Ty::Signed(_)` arm) — cast each operand to
/// unsigned first via `unsigned(x)` (a no-op for an already-unsigned
/// fragment). No width unification needed — result width is the sum,
/// always unsigned.
fn combine_concat(a: Frag, b: Frag) -> Frag {
    let a = cast_to(a, false);
    let b = cast_to(b, false);
    Frag {
        text: format!("{{{}, {}}}", a.text, b.text),
        width: a.width + b.width,
        signed: false,
        atomic: false,
    }
}

/// Wrap `f` in a randomly chosen `Builtin` call, producing a new
/// composite (non-atomic) fragment whose width/kind follow the exact
/// checker rule for that builtin (`crates/mimz-core/src/checker/widths/
/// ops/builtins.rs::call_ty`). GAP-5 direction 2 (docs/audit/gaps.md,
/// the fuzzer's own position-aware generation): this is what makes a
/// BUILTIN call (not just a plain operator) reachable inside a
/// self-determined position — a concat member via `combine_concat`, a
/// comparison operand via `combine_same_width`'s `ToBit` ops, or a
/// `signed`/`unsigned` cast argument via `cast_to` — through ordinary
/// composition. No separate per-position wiring is needed: those three
/// call sites already accept any non-atomic `Frag` as an operand, so once
/// a builtin-wrapped fragment exists in the pool `gen_expr` draws from,
/// it lands in every position those call sites already reach — exactly
/// how BUG-28/BUG-29 (an `extend`/`abs` call in a concat member) would
/// have been found by RANDOM generation instead of only the hand-picked
/// static matrix (`tests/self_determined_regression.rs`).
///
/// `Min`/`Max`/`Nand`/`Nor`/`Xnor` pass `f` as their own second operand
/// where one is needed (`min(f, f)`) rather than generating an
/// independent partner — a deliberate simplification: it still exercises
/// the exact call-site code path GAP-5 cares about (the SHAPE of the
/// call, and the mismatch between mimz's `Kind` and Verilog's own
/// self-determined width for it); an independent second operand would
/// only add corpus diversity, not new position coverage. `Clog2`/
/// `SyncDoubleFlop`/`SyncPulse` are never picked — `NotApplicable` in
/// `tests/self_determined_regression.rs`'s own matrix (compile-time-only
/// / lowered to items before emit), for the identical reasons documented
/// there.
fn wrap_builtin(rng: &mut Rng, f: Frag) -> Frag {
    // Candidates valid for `f`'s CURRENT kind (`SignedCast`/`Abs` need
    // unsigned/signed respectively; `UnsignedCast` the reverse; `Nand`/
    // `Nor`/`Xnor` reject `signed` per `call_ty`). `Trunc` only when
    // there's something to drop.
    let mut candidates: Vec<u32> = vec![0]; // Extend: valid on any kind
    if f.width > 1 {
        candidates.push(1); // Trunc
    }
    candidates.push(4); // Min/Max: valid on any kind (matched against itself)
    if f.signed {
        candidates.push(2); // UnsignedCast
        candidates.push(3); // Abs
    } else {
        candidates.push(5); // SignedCast
        candidates.push(6); // Nand/Nor/Xnor
    }
    match candidates[rng.next_range(candidates.len() as u64) as usize] {
        0 => {
            let target = (f.width + (rng.next_range(8) + 1) as u32).min(MAX_WIDTH);
            Frag {
                text: format!("extend({}, {target})", f.text),
                width: target,
                signed: f.signed,
                atomic: false,
            }
        }
        1 => {
            let target = 1 + rng.next_range((f.width - 1) as u64) as u32;
            Frag {
                text: format!("trunc({}, {target})", f.text),
                width: target,
                signed: f.signed,
                atomic: false,
            }
        }
        2 => Frag {
            text: format!("unsigned({})", f.text),
            width: f.width,
            signed: false,
            atomic: false,
        },
        3 => Frag {
            text: format!("abs({})", f.text), // `Signed(n) -> Signed(n+1)`, lossless like unary `-`.
            width: (f.width + 1).min(MAX_WIDTH),
            signed: true,
            atomic: false,
        },
        4 => {
            let name = if rng.next_range(2) == 0 { "min" } else { "max" };
            Frag {
                text: format!("{name}({}, {})", f.text, f.text),
                width: f.width,
                signed: f.signed,
                atomic: false,
            }
        }
        5 => Frag {
            text: format!("signed({})", f.text),
            width: f.width,
            signed: true,
            atomic: false,
        },
        _ => {
            let name = ["nand", "nor", "xnor"][rng.next_range(3) as usize];
            // Round-5 plan Task 4 (BUG-60, docs/audit/bugs.md): a
            // reduction only exposes a self-determined mismatch when its
            // operand RENDERS NARROWER than its mimz width — an already-
            // `atomic` fragment (a bare port/literal, `gen_leaf`'s own
            // definition) never does, so wrapping one directly here can
            // never reach BUG-60's shape; it needed a SEPARATE, earlier
            // `wrap_builtin` pick to have already produced an `extend(...)`
            // for `f` to be. That double-pick is rare enough that 0 of
            // 10,000 programs generated while testing this fix contained
            // a reduction over an extend at all — depth cannot fix a shape
            // the generator essentially never emits. Force that position
            // here, on a coin flip, when `f` doesn't already guarantee it,
            // the same `extend`-wrap the `Extend` candidate above uses.
            //
            // Round-6 plan Task 9 (round-6 review Part 4.2): the first cut
            // of this forced it UNCONDITIONALLY, which made `nand`/`nor`/
            // `xnor` of a BARE port unreachable at any depth — the shape
            // `matrix_nand_in_concat_matches_icarus` pins and all four
            // flavours of the shipped `bitops.mimz` example actually use.
            // One token (`&& rng.next_range(2) == 0`) keeps both shapes
            // reachable instead of swapping one kind of coverage for the
            // other.
            let arg = if f.atomic && rng.next_range(2) == 0 {
                let target = (f.width + (rng.next_range(8) + 1) as u32).min(MAX_WIDTH);
                format!("extend({}, {target})", f.text)
            } else {
                f.text
            };
            Frag {
                text: format!("{name}({arg})"), // negated reduction: always 1-bit unsigned.
                width: 1,
                signed: false,
                atomic: false,
            }
        }
    }
}

/// A random sub-range read of an existing input port: `port[hi:lo]`,
/// `0 <= lo <= hi < port_width`. `None` if there are no ports (never
/// happens in practice — `gen_module` always creates 2-4 — but kept total
/// rather than panicking on an empty slice). Always yields unsigned
/// `bits` (`slice_ty`), regardless of the port's own declared kind.
fn gen_slice(rng: &mut Rng, ports: &[Port]) -> Option<Frag> {
    if ports.is_empty() {
        return None;
    }
    let (name, w, _signed) = &ports[rng.next_range(ports.len() as u64) as usize];
    let lo = rng.next_range(*w as u64) as u32;
    let max_len = w - lo;
    let len = (rng.next_range(max_len as u64) + 1) as u32;
    let hi = lo + len - 1;
    Some(Frag {
        text: format!("{name}[{hi}:{lo}]"),
        width: len,
        signed: false,
        atomic: true,
    })
}

/// Build one expression fragment, depth-bounded (stops at `depth == 0`,
/// or a 1-in-4 chance of bottoming out early so trees aren't all
/// maximum-depth). Every return is clamped to `cap` — see `clamp`'s doc
/// comment for why that makes width bookkeeping trivial for callers. v1/v2
/// always pass `MAX_WIDTH` (the derived-output-port case: `out y`'s
/// declared type follows whatever the body produces, so nothing needs an
/// exact target). v3 passes a REGISTER's own declared width instead, since
/// a register's next-state expression must land on that EXACT width, not
/// just "under some generous ceiling" — reusing the same recursion with a
/// tighter `cap` guarantees that by construction rather than needing a
/// separate narrowing pass after the fact.
/// Every INTERNAL node of one generated expression, in construction order,
/// each with the width and kind the generator built it at.
///
/// GAP-11(a). The old width-conformance oracle compared a top-level signal's
/// value against the width `mimz-sim` itself resolved for that signal — the
/// kernel masks every stored value to exactly that width by construction, so
/// the check could not fail, and its own doc comment said so. It therefore
/// could not catch the class it was written for: BUG-30's wrong value was an
/// *intermediate* (`din << 2` typed `bits[4]`, valued 60) that still fit the
/// `bits[8]` output it landed in. BUG-42 and BUG-44 were the same shape.
///
/// The fix is to give every intermediate its own declared, checked home.
/// Each collected fragment becomes a real `out` port typed from the
/// generator's own model, so:
///
/// * the **checker** endorses that width by accepting the program at all
///   (the generator is checker-clean by construction, so a declaration it
///   would reject is a generator bug that surfaces immediately), and
/// * the **simulator** and **Icarus** each produce a value for it, compared
///   against each other and against the declared width.
///
/// That is the two-independent-authorities property the oracle was supposed
/// to have. Leaves are deliberately skipped — a bare port reference or a
/// sized literal has no width rule worth testing.
struct Subs {
    frags: Vec<Frag>,
}

impl Subs {
    fn new() -> Self {
        Subs { frags: Vec::new() }
    }

    /// Record one internal node. Cloning the text duplicates that subtree
    /// into its own port, which is the point: the sub-expression is emitted
    /// exactly as it renders inside the root, on the same code path.
    fn push(&mut self, f: &Frag) {
        if self.frags.len() < MAX_SUB_OUTPUTS {
            self.frags.push(Frag {
                text: f.text.clone(),
                width: f.width,
                signed: f.signed,
                atomic: f.atomic,
            });
        }
    }
}

/// Ceiling on materialized sub-expression ports per module. Each one
/// duplicates its whole subtree into the source and adds a column to every
/// testbench row, so an uncapped depth-4 tree would bloat both the generated
/// program and the Icarus run for diminishing returns.
const MAX_SUB_OUTPUTS: usize = 8;

/// [`gen_expr`], additionally recording every internal node into `subs`.
/// `special` is the pool `gen_special_leaves` built for this module — see
/// `gen_leaf`'s own doc comment.
fn gen_expr_collecting(
    rng: &mut Rng,
    ports: &[Port],
    special: &[Frag],
    depth: u32,
    cap: u32,
    subs: &mut Subs,
) -> Frag {
    let raw = if depth == 0 || rng.next_range(6) == 0 {
        gen_leaf(rng, ports, special)
    } else {
        match rng.next_range(9) {
            0 => {
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let b = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                combine_same_width(rng, a, b)
            }
            1 => {
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                combine_shift(rng, a)
            }
            2 => {
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let b = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                combine_concat(a, b)
            }
            3 => {
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let b = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                combine_lossless(rng, a, b)
            }
            4 => {
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let b = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                combine_wrap(rng, a, b)
            }
            // GAP-5 direction 2 (docs/audit/gaps.md): a builtin call, not
            // just a plain operator, as the fragment about to be combined
            // further up the tree — see `wrap_builtin`'s own doc comment
            // for why no separate per-position wiring is needed.
            5 => {
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                wrap_builtin(rng, a)
            }
            // GAP-13 direction 2 (docs/audit/gaps.md): `if`/`match` as a
            // combinator, not just a hand-picked shape — BUG-41's repro
            // ③ and the axis's own `Match` gap were both unreachable by
            // random generation before this.
            6 => {
                let x = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let y = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let cond = gen_cond(rng, x, y);
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let b = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                combine_if(rng, cond, a, b)
            }
            7 => {
                let sel = cast_to(gen_leaf(rng, ports, special), false);
                let a = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let b = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                let c = gen_expr_collecting(rng, ports, special, depth - 1, cap, subs);
                combine_match(rng, sel, a, b, c)
            }
            _ => gen_slice(rng, ports).unwrap_or_else(|| gen_leaf(rng, ports, special)),
        }
    };
    let out = clamp(rng, ports, raw, cap);
    // Record the CLAMPED fragment — that is the text and width that actually
    // reaches the parent, so it is the one whose declaration must hold.
    if depth > 0 {
        subs.push(&out);
    }
    out
}

/// GAP-13 direction 2 (`docs/audit/gaps.md`): the shapes the generator
/// could never emit before this — a `fn` call, a plain instance-port
/// read, an array-instance-port read, a `mem` read, and a `const`-
/// bounded slice. Round 3's own finding was that 2,000 fresh seeds ran
/// clean while BUG-41 and BUG-48 both sat at HEAD — not because depth
/// was too shallow, but because the generator's VOCABULARY never reached
/// those shapes at all. Computed ONCE per module, before
/// `gen_expr_collecting` runs, exactly like `ports`/`regs` already are.
///
/// Returns `(file_scope_prelude, body_prelude, mem_write_stmts, leaves)`:
/// the first two are spliced into the generated source at fixed points,
/// the third only used by the clocked generator (inside `on rise`), and
/// the fourth is a pool `gen_leaf` draws from on the same footing as an
/// ordinary port — each a fully pre-rendered `Frag` with known
/// width/kind/atomicity.
///
/// Instance-port and `mem` leaves are gated on `clocked`: `comb::
/// eval_outputs` (the kernel behind the v1/v2 comb differential) does
/// not elaborate instances at all — confirmed live, the same reason
/// `tests/self_determined_regression.rs`'s own `bug_41_instance_port_
/// operand_of_add_in_concat_matches_icarus` needs `differential_clocked`
/// instead of `differential` even though its design has no registers.
/// `fn`-call and `const`-bounded-slice leaves need no such gate — both
/// are exercised through `differential` (not `_clocked`) in that same
/// file, confirming the comb kernel supports them directly.
fn gen_special_leaves(
    rng: &mut Rng,
    ports: &[Port],
    clocked: bool,
) -> (String, String, Vec<String>, Vec<Frag>) {
    let mut prelude = String::new();
    let mut body = String::new();
    let mut mem_writes = Vec::new();
    let mut leaves = Vec::new();

    let pick =
        |rng: &mut Rng| -> Port { ports[rng.next_range(ports.len() as u64) as usize].clone() };
    let arg_of = |name: &str, signed: bool| -> String {
        if signed {
            format!("unsigned({name})")
        } else {
            name.to_string()
        }
    };

    // fn call — BUG-41 repro ①/⑤'s shape. Round-6 Task 6 (GAP-16/BUG-62):
    // the body used to be a bare parameter passthrough (`{ x }`), which
    // can never make `infer_kind` return `None` inside a `fn` — exactly
    // the shape round 6 found the whole hoist machinery was untested
    // against (`fn allset(x) { &extend(x, 8) }` etc.). Give it a REAL
    // body instead: the same `gen_expr_collecting` machinery the module
    // body uses, over the fn's own single param only (a `fn` can't see
    // outer ports/instances/mem, so the module's own `special` pool is out
    // of scope here — the one below is the fn's own), at a shallow
    // depth (this is a leaf drawn sparingly, not the module's own tree),
    // then forced onto the declared return type exactly like a v3
    // register's next-state expression already is.
    {
        let (name, w, signed) = pick(rng);
        let fn_ports: Vec<Port> = vec![("x".to_string(), w, false)];
        // Round-7 Task 11 (BUG-67): a SECOND `fn`, callable from the first
        // one's body. The generator used to emit exactly one `fn` per
        // program, so a `fn` calling another `fn` — the sixth context
        // BUG-28 reappeared in, where `render_fn_decl`'s fresh `decls`
        // omitted the callee's own `__mimz_fnret__` key — was structurally
        // unreachable at any depth. Offered to the outer body as an
        // ordinary `special` leaf, on the same footing as a port, so the
        // call lands in whatever concat/builtin/`if` position the tree
        // happens to build around it rather than only at the root.
        let mut inner_subs = Subs::new();
        let inner_body = gen_expr_collecting(rng, &fn_ports, &[], 1, w, &mut inner_subs);
        let inner_body = force_width(rng, inner_body, w, false);
        let inner_leaf = Frag {
            text: format!("inner{w}(x)"),
            width: w,
            signed: false,
            atomic: false,
        };

        let mut fn_subs = Subs::new();
        let fn_depth = 1 + rng.next_range(2) as u32; // 1..=2
        let fn_body = gen_expr_collecting(
            rng,
            &fn_ports,
            std::slice::from_ref(&inner_leaf),
            fn_depth,
            w,
            &mut fn_subs,
        );
        let fn_body = force_width(rng, fn_body, w, false);
        // Declare the callee only when the body actually kept the call —
        // `clamp`/`force_width` can drop the leaf again, and an unused
        // `fn` is a different (and uninteresting) shape to be fuzzing.
        if fn_body.text.contains(&format!("inner{w}(")) {
            prelude += &format!(
                "fn inner{w}(x: bits[{w}]) -> bits[{w}] {{\n  {}\n}}\n\n",
                inner_body.text
            );
        }
        prelude += &format!(
            "fn ident{w}(x: bits[{w}]) -> bits[{w}] {{\n  {}\n}}\n\n",
            fn_body.text
        );
        leaves.push(Frag {
            text: format!("ident{w}({})", arg_of(&name, signed)),
            width: w,
            signed: false,
            atomic: false,
        });
    }

    // const-bounded slice — BUG-48's own `Slice` shape.
    {
        let (name, w, _signed) = pick(rng);
        let hi = if w > 1 {
            1 + rng.next_range((w - 1) as u64) as u32
        } else {
            1
        };
        body += &format!("  const HI: int = {}\n", hi - 1);
        leaves.push(Frag {
            text: format!("{name}[HI:0]"),
            width: hi,
            signed: false,
            atomic: true,
        });
    }

    if clocked {
        // Plain instance port — BUG-41 repro ②'s shape. `s.q` flattens to
        // the plain Verilog identifier `s_q` (`expr.rs`'s `Field`
        // rendering), so this leaf genuinely is atomic, the same reason
        // `gen_slice`'s port-slice leaf already is.
        let sub_decl = |w: u32| {
            format!("module Sub{w} {{\n  in x: bits[{w}]\n  out q: bits[{w}]\n  q = x\n}}\n\n")
        };
        let (name, w, signed) = pick(rng);
        prelude += &sub_decl(w);
        body += &format!("  let s = Sub{w}() {{ x: {} }}\n", arg_of(&name, signed));
        leaves.push(Frag {
            text: "s.q".to_string(),
            width: w,
            signed: false,
            atomic: true,
        });

        // Array instance port — BUG-48's own `Field { base: Index }`
        // shape, the one that reopened BUG-28/41 with byte-identical
        // wrong output. `sa[0].q` flattens to `sa__0_q`, same reasoning.
        let (name2, w2, signed2) = pick(rng);
        if w2 != w {
            prelude += &sub_decl(w2);
        }
        // Round-8 plan Task 9: BUG-70's OWN reproduction is a SECOND
        // instance's connection reading an EARLIER instance's output
        // wrapped in `extend(...)`, as a CONCAT member — `.d({b,
        // extend(u1.q, 8)})`, verbatim. Confirmed live that BOTH layers
        // are load-bearing: a bare `extend(s.q, w2)` alone as the WHOLE
        // connection compiles fine even with Task 1 reverted (its target
        // width is already explicit, so no hoist is needed at all when
        // it's the sole top-level expression); only nesting it as one
        // MEMBER of an outer concat reproduces GAP-18's own "declared name
        // `s_q` referenced before its own declaration" with Task 1
        // reverted, and compiles clean with Task 1 restored. `arg_of`'s
        // bare port/reg leaf can never reach either shape: only an
        // instance's own OUTPUT wire is declared where `declare_instance_
        // outputs` (Task 1) or the old inline `emit_instances` path put it,
        // so `s.q` is the one leaf whose declaration position this
        // depends on. Wired in directly (not through the general `Frag`
        // pool, which has no way to prefer `s.q` over any other leaf)
        // whenever there's room for both a real `extend` growth on `s.q`
        // and a nonzero pad member (`w2 >= w + 2`).
        let x2_arg = if w2 >= w + 2 && rng.next_range(2) == 0 {
            let target_w = w + 1 + rng.next_range((w2 - w - 1) as u64) as u32; // w+1 ..= w2-1
            let pad = w2 - target_w;
            let padv = rng.next_u64() & support::mask(pad) as u64;
            format!("{{extend({padv}, {pad}), extend(s.q, {target_w})}}")
        } else {
            arg_of(&name2, signed2)
        };
        body +=
            &format!("  repeat i: 0..1 {{\n    let sa[i] = Sub{w2}() {{ x: {x2_arg} }}\n  }}\n");
        leaves.push(Frag {
            text: "sa[0].q".to_string(),
            width: w2,
            signed: false,
            atomic: true,
        });

        // `mem` read — BUG-41 repro ④'s shape. One write so the read
        // isn't trivially always the reset value.
        let mw = (rng.next_range(8) + 1) as u32;
        let init = rng.next_u64() & support::mask(mw) as u64;
        body += &format!("  mem m: bits[{mw}][4] = 0\n");
        mem_writes.push(format!("    m[0] <- {init}\n"));
        leaves.push(Frag {
            text: format!("m[{}]", rng.next_range(4)),
            width: mw,
            signed: false,
            atomic: true,
        });
    }

    (prelude, body, mem_writes, leaves)
}

/// Generate one random valid combinational `.mimz` module as source text.
/// Returns `(source, input_ports, output_width)` — the caller needs
/// `input_ports` to build stimulus vectors and a matching Verilog
/// testbench, and `output_width` to declare the testbench's `y` wire.
fn gen_module(seed: u64) -> (String, Vec<Port>, u32, Vec<Port>) {
    let mut rng = Rng::new(seed);
    let n_ports = (rng.next_range(3) + 2) as usize; // 2..=4
    let ports: Vec<Port> = (0..n_ports)
        .map(|i| {
            let w = (rng.next_range(16) + 1) as u32; // 1..=16
            let signed = rng.next_range(2) == 0;
            (format!("p{i}"), w, signed)
        })
        .collect();
    let depth = (rng.next_range(3) + 2) as u32; // 2..=4
    let (prelude, body_prelude, _mem_writes, special) = gen_special_leaves(&mut rng, &ports, false);
    let mut subs = Subs::new();
    let body = gen_expr_collecting(&mut rng, &ports, &special, depth, MAX_WIDTH, &mut subs);

    // Round-6 Task 6 (GAP-16/BUG-62(b)): a module `parameter` used as an
    // `extend` width, in about a third of programs — round 6's other
    // structurally-unreachable shape (`module Fuzz(W: int = 8) { y =
    // &extend(a, W) }`), since no generator here ever emitted a
    // `parameter` before. Built as its own materialized output (the same
    // GAP-11(a) "extra port" mechanism below), NOT fed into `special`/
    // `gen_leaf`'s general pool: the emitter's own symbolic-width fix
    // (Task 3, `expr.rs::try_widen_symbolic_extend`) only rewrites
    // `extend(x, W)` when it is the DIRECT operand at one of the
    // self-determined positions it names (reduction, concat member,
    // cast, nand/nor/xnor) — nesting it deeper (e.g. under a further
    // `trunc`) is a documented, still-open case that legitimately
    // diagnoses rather than compiles (Task 3's own status note), which
    // this differential test isn't set up to expect. Keeping the shape
    // fixed at a covered position, rather than recursing it through
    // `gen_expr_collecting`, reaches the bug without also reaching that
    // open ceiling.
    let param_port = if rng.next_range(3) == 0 {
        let (name, w, signed) = &ports[rng.next_range(ports.len() as u64) as usize];
        let pwidth = (w + (rng.next_range(8) + 1) as u32).min(MAX_WIDTH);
        let arg = if *signed {
            format!("unsigned({name})")
        } else {
            name.clone()
        };
        let text = if rng.next_range(2) == 0 {
            format!("(&extend({arg}, W))")
        } else {
            format!("nand(extend({arg}, W))")
        };
        Some((
            "W".to_string(),
            pwidth,
            Frag {
                text,
                width: 1,
                signed: false,
                atomic: false,
            },
        ))
    } else {
        None
    };

    let mut src = prelude;
    src += "module Fuzz";
    if let Some((pname, pwidth, _)) = &param_port {
        src += &format!("({pname}: int = {pwidth})");
    }
    src += " {\n";
    for (name, w, signed) in &ports {
        src += &format!("  in {name}: {}\n", ty_str(*w, *signed));
    }
    src += &body_prelude;
    src += &format!("  out y: {}\n", ty_str(body.width, body.signed));
    // GAP-11(a): one extra output per internal node, declared at the width
    // and kind the generator built it at. ADDITIVE — `y` below still renders
    // the whole expression inline, on the original code path. That matters:
    // BUG-30 was "naming an intermediate changes the result", so replacing
    // the root with references to these ports would test a different program
    // than the one the differential is supposed to cover.
    // The outermost internal node IS the body, so materializing it would
    // duplicate the whole expression into a second identical port.
    subs.frags.retain(|f| f.text != body.text);
    let mut sub_ports: Vec<Port> = subs
        .frags
        .iter()
        .enumerate()
        .map(|(i, f)| (format!("y{i}"), f.width, f.signed))
        .collect();
    for ((name, w, signed), f) in sub_ports.iter().zip(&subs.frags) {
        src += &format!("  out {name}: {}\n", ty_str(*w, *signed));
        src += &format!("  {name} = {}\n", f.text);
    }
    if let Some((_, _, frag)) = &param_port {
        let name = format!("y{}", sub_ports.len());
        src += &format!("  out {name}: {}\n", ty_str(frag.width, frag.signed));
        src += &format!("  {name} = {}\n", frag.text);
        sub_ports.push((name, frag.width, frag.signed));
    }
    src += &format!("  y = {}\n", body.text);
    src += "}\n";
    (src, ports, body.width, sub_ports)
}

/// Generate one random valid CLOCKED `.mimz` module as source text (v3): a
/// `clock`, a `reset`, 1-3 registers each driven by one `on rise` block,
/// and one combinational `out y` derived from register/port values — the
/// state-holding shape v1/v2 never touch. `reset` needs no body-level
/// logic: the emitter auto-generates each `on` block's reset branch from
/// every assigned register's own declared init value
/// (`crates/mimz-core/src/emit_verilog/module.rs`, confirmed live against
/// `examples/english/blinker.mimz`, which never references its own `rst`
/// in its body either) — so the generator only needs to declare `reset
/// rst` and give every `reg` an `= 0` init, matching that same convention.
///
/// Reuses the SAME expression generator as v1/v2 (`gen_expr`/leaf/combine
/// functions) for both the per-register next-state expression and the
/// output expression, just over a wider leaf pool: `ports ++ regs` (a
/// register's CURRENT value is readable exactly like an input port — the
/// standard `cnt <- cnt +% 1` feedback idiom, modulo the `+%` exclusion
/// above). A register's next-state expression is generated with
/// `gen_expr`'s `cap` set to that register's OWN declared width (not the
/// generator-wide `MAX_WIDTH`) so it can never come back over-wide, then
/// `widen`+`cast_to` finish the match to the register's exact declared
/// type — safe here specifically because a non-blocking register assign
/// (`reg <- expr`) IS a context-determined position (the same reasoning
/// `y = expr` already relies on in v1/v2): nothing wraps this further, so
/// none of the BUG-19-class risk (a `widen()`ed result later nested inside
/// ANOTHER self-determined construct) applies at this exact position.
///
/// Returns `(source, input_ports, held_input_values, output_width,
/// output_signed)` — the caller needs the held values to build BOTH our
/// kernel's `SimOpts.inputs` (which holds every input constant for the
/// whole run, `crates/mimz-sim/src/sim/run.rs`) and the Verilog
/// testbench's held `reg` initializers from the exact same vector.
#[allow(clippy::type_complexity)]
fn gen_clocked_module(
    seed: u64,
) -> (
    String,
    Vec<Port>,
    BTreeMap<String, u128>,
    u32,
    bool,
    Vec<Port>,
) {
    let mut rng = Rng::new(seed);

    let n_ports = (rng.next_range(3) + 1) as usize; // 1..=3
    let ports: Vec<Port> = (0..n_ports)
        .map(|i| {
            let w = (rng.next_range(16) + 1) as u32; // 1..=16
            let signed = rng.next_range(2) == 0;
            (format!("p{i}"), w, signed)
        })
        .collect();
    let held: BTreeMap<String, u128> = ports
        .iter()
        .map(|(name, w, _)| {
            let v = rng.next_u64() & support::mask(*w) as u64;
            (name.clone(), v as u128)
        })
        .collect();

    let n_regs = (rng.next_range(3) + 1) as usize; // 1..=3
    let regs: Vec<Port> = (0..n_regs)
        .map(|i| {
            let w = (rng.next_range(16) + 1) as u32; // 1..=16
            let signed = rng.next_range(2) == 0;
            (format!("r{i}"), w, signed)
        })
        .collect();

    // A register's current value is just as readable as an input port —
    // one combined leaf pool serves both the next-state and output exprs.
    let leaves: Vec<Port> = ports.iter().chain(regs.iter()).cloned().collect();
    let depth = (rng.next_range(3) + 2) as u32; // 2..=4
    // GAP-13 direction 2 (docs/audit/gaps.md): the clocked generator gets
    // every special leaf, including the instance/mem shapes the comb
    // generator can't run (`gen_special_leaves`'s own doc comment).
    let (prelude, body_prelude, mem_writes, special) = gen_special_leaves(&mut rng, &leaves, true);

    let mut src = prelude;
    src += "module Fuzz {\n  clock clk\n  reset rst\n";
    for (name, w, signed) in &ports {
        src += &format!("  in {name}: {}\n", ty_str(*w, *signed));
    }
    for (name, w, signed) in &regs {
        src += &format!("  reg {name}: {} = 0\n", ty_str(*w, *signed));
    }
    src += &body_prelude;

    // GAP-11(a), clocked half: collect the internal nodes of the output
    // expression AND of every register's next-state expression. A
    // next-state sub-expression is exactly where BUG-44 lived, so leaving
    // this generator root-only would keep the weaker oracle on the half
    // that has historically found the most.
    let mut subs = Subs::new();
    let out_body = gen_expr_collecting(&mut rng, &leaves, &special, depth, MAX_WIDTH, &mut subs);
    src += &format!("  out y: {}\n", ty_str(out_body.width, out_body.signed));

    let mut next_states: Vec<String> = Vec::new();
    for (_, w, signed) in &regs {
        let next = gen_expr_collecting(&mut rng, &leaves, &special, depth, *w, &mut subs);
        let next = force_width(&mut rng, next, *w, *signed);
        next_states.push(next.text);
    }

    // Each materialized intermediate becomes a combinational output, read
    // from the SAME leaf pool (ports + current register values) the
    // expression was built from. Additive: `y` and every `<-` below still
    // render their whole expression inline, on the original code path.
    subs.frags
        .retain(|f| f.text != out_body.text && !next_states.contains(&f.text));
    let sub_ports: Vec<Port> = subs
        .frags
        .iter()
        .enumerate()
        .map(|(i, f)| (format!("y{i}"), f.width, f.signed))
        .collect();
    for (name, w, signed) in &sub_ports {
        src += &format!("  out {name}: {}\n", ty_str(*w, *signed));
    }

    src += "  on rise(clk) {\n";
    for ((name, _, _), next) in regs.iter().zip(&next_states) {
        src += &format!("    {name} <- {next}\n");
    }
    for w in &mem_writes {
        src += w;
    }
    src += "  }\n";
    for ((name, _, _), f) in sub_ports.iter().zip(&subs.frags) {
        src += &format!("  {name} = {}\n", f.text);
    }
    src += &format!("  y = {}\n", out_body.text);
    src += "}\n";

    (src, ports, held, out_body.width, out_body.signed, sub_ports)
}

/// Fast, Icarus-independent: every generated program must pass
/// `checker::check`. This should never fail — a failure means the
/// generator itself has a bug (emitted something not actually
/// spec-legal), not a product bug. Runs on every `cargo test`, even on a
/// machine with no Icarus installed, so a generator regression is caught
/// immediately.
#[test]
fn differential_fuzz_generates_checker_valid_programs() {
    for seed in 0..1000u64 {
        let (src, _, _, _) = gen_module(0xC0FFEE_u64.wrapping_add(seed));
        let tokens = lexer::lex(&src).unwrap_or_else(|e| {
            panic!(
                "seed {seed} produced an unlexable program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        let file = parser::parse(tokens).unwrap_or_else(|e| {
            panic!(
                "seed {seed} produced an unparsable program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        if let Err(e) = checker::check(std::slice::from_ref(&file)) {
            panic!(
                "seed {seed} produced a checker-rejected program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            );
        }
    }
}

/// The real differential: our own kernel vs. real Icarus Verilog, on
/// `MIMZ_DIFF_FUZZ_N` (default `DEFAULT_FUZZ_N`) randomly generated
/// combinational programs, plus every seed in
/// `tests/fixtures/fuzz-seeds/comb.txt` that has ever failed (see
/// `fuzz_seeds`). Gated by `require_iverilog()` exactly like every other
/// Icarus differential test (`tests/icarus.rs`) — skips locally without
/// Icarus, hard-fails in CI (`REQUIRE_IVERILOG=1`, already set in
/// `.github/workflows/ci.yml`).
/// NOTE: BUG-23 (docs/audit/bugs.md) is fixed — this test passes at default N.
/// This deep-N pass also once surfaced BUG-24 (seed `12648537` — a
/// context-determined operator losing its width growth as the left
/// operand of a shift) — also now fixed, see `docs/audit/bugs.md`.
#[test]
fn differential_fuzz_matches_icarus() {
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    for seed in fuzz_seeds("comb", "MIMZ_DIFF_FUZZ_N", COMB_SEED_BASE) {
        let (src, ports, out_width, sub_ports) = gen_module(seed);

        // Parse + check in-memory — the exact object our kernel will run.
        let tokens = lexer::lex(&src).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: unlexable:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        let file = parser::parse(tokens).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: unparsable:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        if let Err(e) = checker::check(std::slice::from_ref(&file)) {
            panic!(
                "seed {seed}: checker rejected its own generated program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            );
        }

        // A real temp file on disk, since `compile_example` shells out to
        // the real `mimz compile` binary.
        let path = std::env::temp_dir().join(format!("mimz_diff_fuzz_{seed}.mimz"));
        std::fs::write(&path, &src).unwrap();

        // `support::gen_vectors`/`comb_testbench` only need name+width (the
        // testbench connects by raw bits regardless of a port's declared
        // signed-ness — see `tests/support/mod.rs`'s `comb_testbench` doc).
        let inputs_meta: Vec<(String, u32)> =
            ports.iter().map(|(n, w, _)| (n.clone(), *w)).collect();
        let vectors = support::gen_vectors(&inputs_meta, 8);

        // Every output this module declares — the root plus one per
        // materialized internal node (GAP-11(a)) — at the width the
        // generator built it at.
        let declared_widths: BTreeMap<&str, u32> = std::iter::once(("y", out_width))
            .chain(sub_ports.iter().map(|(n, w, _)| (n.as_str(), *w)))
            .collect();

        // Our own kernel, one row per input vector. Values stay `Bits`
        // (Small or Wide) — comparison against Icarus normalizes both to
        // limbs (`bits_to_limbs`/`limbs_from_binary`) so a >128-bit output
        // (BUG-13 layer 1, Task 12) compares correctly, not just a narrow one.
        let mut kernel_rows: Vec<BTreeMap<String, mimz::sim::value::Bits>> = Vec::new();
        for v in &vectors {
            let v_bits: BTreeMap<String, mimz::sim::value::Bits> = v
                .iter()
                .map(|(k, val)| (k.clone(), mimz::sim::value::Bits::Small(*val)))
                .collect();
            let outputs = comb::eval_outputs(
                std::slice::from_ref(&file),
                None,
                &v_bits,
                &BTreeMap::new(),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "seed {seed}: our kernel rejected its own generated program:\n{src}\n{}",
                    e.msg
                )
            });
            for o in &outputs {
                // GAP-11(a) authority 1: the width the GENERATOR built this
                // expression at, which the checker endorsed by accepting the
                // declaration. Comparing the simulator's independently
                // resolved width against it is the non-tautological half —
                // `assert_bits_fit_width` below only ever sees the kernel's
                // own number, which the kernel masks to by construction.
                let declared = declared_widths.get(o.name.as_str());
                if let Some(&w) = declared {
                    assert_eq!(
                        o.width, w,
                        "seed {seed}: output `{}` — the simulator resolved width {} but the \
                         generator declared (and the checker accepted) {w}\nsource:\n{src}",
                        o.name, o.width
                    );
                }
                assert_bits_fit_width(
                    &format!("seed {seed}: output `{}`", o.name),
                    &o.value,
                    o.width,
                );
            }
            let row: BTreeMap<String, mimz::sim::value::Bits> =
                outputs.into_iter().map(|o| (o.name, o.value)).collect();
            kernel_rows.push(row);
        }

        // Real Icarus.
        let design_v = support::compile_example(&path);
        // The root AND every materialized internal node. Comparing each
        // intermediate is the point of GAP-11(a): BUG-30, BUG-42 and BUG-44
        // were all wrong at a sub-expression, and a root-only differential
        // sees that only when the error happens to survive to the top.
        let outputs_meta: Vec<(String, u32)> = std::iter::once(("y".to_string(), out_width))
            .chain(sub_ports.iter().map(|(n, w, _)| (n.clone(), *w)))
            .collect();
        let tb = support::comb_testbench("Fuzz", &[], &inputs_meta, &outputs_meta, &vectors);
        let stdout = support::run_vvp(&bin, &format!("fuzz seed {seed}"), &design_v, &tb);
        let icarus = parse_icarus_raw(&stdout);

        for (idx, kernel_row) in kernel_rows.iter().enumerate() {
            let icarus_row = icarus
                .get(&(idx as u64))
                .unwrap_or_else(|| panic!("seed {seed}: Icarus produced no row for vector {idx}"));
            for (name, width) in &outputs_meta {
                let kernel_v = bits_to_limbs(&kernel_row[name], *width);
                let icarus_v = limbs_from_binary(&icarus_row[name], *width);
                assert_eq!(
                    kernel_v, icarus_v,
                    "seed {seed}, vector {idx}: our kernel {name}={:?} but Icarus {name}={:?}\n\
                     source:\n{src}\nvector: {:?}",
                    kernel_row[name], icarus_row[name], vectors[idx]
                );
            }
        }
    }
}

/// Round-6 plan Task 9 (round-6 review Part 4.2): `wrap_builtin`'s
/// `nand`/`nor`/`xnor` arm used to force EVERY atomic (bare-port/literal)
/// operand through an `extend(...)` wrap unconditionally, which made a
/// reduction over a bare port unreachable at any depth — a real coverage
/// loss (`matrix_nand_in_concat_matches_icarus`'s own control shape, and
/// what all four flavours of the shipped `bitops.mimz` example actually
/// emit), not just a hypothetical one. Demonstrates the fix rather than
/// asserting it: scans a sample of generated programs and requires BOTH
/// the narrow-rendering shape (BUG-60's own repro — what makes the fuzzer
/// useful for this bug class at all) and the bare-operand shape (what the
/// unconditional version silently stopped emitting) to actually appear.
#[test]
fn task9_reduction_fuzz_bias_reaches_both_bare_and_extended_operands() {
    let mut bare = 0u32;
    let mut extended = 0u32;
    for seed in 0..2000u64 {
        let (src, ..) = gen_module(0xC0FFEE_u64.wrapping_add(seed));
        for name in ["nand(", "nor(", "xnor("] {
            let mut rest = src.as_str();
            while let Some(i) = rest.find(name) {
                let after = &rest[i + name.len()..];
                if after.starts_with("extend(") {
                    extended += 1;
                } else {
                    bare += 1;
                }
                rest = after;
            }
        }
    }
    assert!(
        bare > 0,
        "2000 seeds produced zero bare-operand nand/nor/xnor calls — the \
         coin flip (`f.atomic && rng.next_range(2) == 0`) regressed back \
         to unconditional extend-wrapping"
    );
    assert!(
        extended > 0,
        "2000 seeds produced zero extend-wrapped nand/nor/xnor calls — \
         BUG-60's own narrow-rendering shape stopped being reachable"
    );
}

/// v3's fast, Icarus-independent counterpart to
/// `differential_fuzz_generates_checker_valid_programs` — every generated
/// CLOCKED program must also pass `checker::check`.
#[test]
fn differential_fuzz_clocked_generates_checker_valid_programs() {
    for seed in 0..1000u64 {
        let (src, ..) = gen_clocked_module(CLOCKED_SEED_BASE.wrapping_add(seed));
        let tokens = lexer::lex(&src).unwrap_or_else(|e| {
            panic!(
                "seed {seed} produced an unlexable clocked program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        let file = parser::parse(tokens).unwrap_or_else(|e| {
            panic!(
                "seed {seed} produced an unparsable clocked program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        if let Err(e) = checker::check(std::slice::from_ref(&file)) {
            panic!(
                "seed {seed} produced a checker-rejected clocked program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            );
        }
    }
}

/// Round-8 plan Task 9's own acceptance criterion: "name the seed inside
/// 400 that first produces a hoisting instance-port connection." `x:
/// {extend(` is a safe, unique textual marker for BUG-70's OWN shape — a
/// SECOND instance's OWN CONNECTION reading the FIRST instance's output
/// through `extend(s.q, ...)` as a CONCAT member — BUG-70's own
/// reproduction verbatim, `{ b, extend(u1.q, 8) }` (`gen_special_leaves`'s
/// array-instance branch). `x: {extend(` is a safe, unique marker: `arg_of`
/// (the pre-existing bare-identifier fallback) never renders it, and it
/// only ever appears at the connection SITE, not at some later unrelated
/// use of `s.q` in the leaf pool (an earlier draft of this marker,
/// `, s.q}`, matched both and found a false positive at seed index 3 where
/// the connection itself was still a bare `arg_of`). Confirms the
/// generator actually reaches the shape at a depth well within what CI's
/// `check` job runs (`ci.yml` sets `MIMZ_DIFF_FUZZ_CLOCKED_N=400`) — the
/// separate, manual half of this criterion (that reverting Task 1's own
/// fix makes THIS shape fail) was verified by hand, the same way Task 1's
/// own "watch fail first" step was: temporarily restored Task 1's own
/// "before" shape (disabled `declare_instance_outputs`'s pre-pass call in
/// `module()`, restored inline wire declaration in `instance()`'s own
/// `Dir::Out` arm — a bare disable of the pre-pass ALONE does not
/// reproduce BUG-70, it reproduces a DIFFERENT failure, an undeclared
/// implicit net, since Task 1 replaced the declare mechanism rather than
/// just moving it) and ran `mimz compile` directly on the seed this test
/// finds (`CLOCKED_SEED_BASE+15` at the time of writing): confirmed it now
/// fails GAP-18's own widened invariant (Task 2) — "declared name `s_q`
/// referenced before its own declaration" — with Task 1 reverted, and
/// compiles clean with Task 1 restored. Not repeated here as an automated
/// test, since Task 1 is landed and there is nothing left to revert in CI.
#[test]
fn task9_instance_port_connection_reaches_a_hoisting_expression_within_400_seeds() {
    let found = (0..400u64).find(|&i| {
        let seed = CLOCKED_SEED_BASE.wrapping_add(i);
        let (src, ..) = gen_clocked_module(seed);
        src.contains("x: {extend(")
    });
    assert!(
        found.is_some(),
        "no seed inside the first 400 fresh clocked seeds (base {CLOCKED_SEED_BASE:#x}) \
         produced a hoisting cross-instance port connection (`x: {{extend(...), \
         extend(s.q, ...)}}`) — Task 9's own generator extension in `gen_special_leaves` \
         regressed"
    );
}

/// v3's real differential, clocked counterpart to
/// `differential_fuzz_matches_icarus`: our own kernel (`elaborate_project`
/// and `run`, the exact engine behind `mimz sim`/`test`) vs. real Icarus
/// Verilog, over `MIMZ_DIFF_FUZZ_CLOCKED_N` (default `DEFAULT_FUZZ_N`)
/// randomly generated clocked programs plus every ever-failing seed in
/// `tests/fixtures/fuzz-seeds/clocked.txt` (see `fuzz_seeds`), each run for
/// a fixed number of cycles with
/// held (constant) input values and one reset cycle — the same default
/// clocked stimulus `tests/icarus.rs`'s own differential already uses.
/// Gated by `require_iverilog()` exactly like every other Icarus
/// differential test.
/// NOTE: BUG-23 (docs/audit/bugs.md) is fixed — this test passes at default N.
/// This deep-N pass also once surfaced BUG-24 (seed `12648537` — a
/// context-determined operator losing its width growth as the left
/// operand of a shift) — also now fixed, see `docs/audit/bugs.md`.
#[test]
fn differential_fuzz_clocked_matches_icarus() {
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    const CYCLES: u64 = 8;
    const RESET_CYCLES: u64 = 1;

    for seed in fuzz_seeds("clocked", "MIMZ_DIFF_FUZZ_CLOCKED_N", CLOCKED_SEED_BASE) {
        let (src, ports, held, out_width, _out_signed, sub_ports) = gen_clocked_module(seed);

        let tokens = lexer::lex(&src).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: unlexable clocked program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        let file = parser::parse(tokens).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: unparsable clocked program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            )
        });
        if let Err(e) = checker::check(std::slice::from_ref(&file)) {
            panic!(
                "seed {seed}: checker rejected its own generated clocked program:\n{src}\n{}",
                diag::render(&e, &src, "generated")
            );
        }

        // GAP-13 direction 2 (docs/audit/gaps.md): the generator can now
        // emit companion `Sub{w}` modules (instance-port special leaves)
        // alongside `Fuzz`, so the top module must be named explicitly —
        // `elaborate_project`'s own `None` case only works for a
        // single-module file.
        let design = elaborate_project(std::slice::from_ref(&file), Some("Fuzz"), &BTreeMap::new())
            .unwrap_or_else(|e| {
                panic!(
                    "seed {seed}: our kernel failed to elaborate its own generated \
                     clocked program:\n{src}\n{}",
                    e.msg
                )
            });
        let opts = SimOpts {
            clock: None,
            inputs: held
                .iter()
                .map(|(k, v)| (k.clone(), mimz::sim::value::Bits::Small(*v)))
                .collect(),
            cycles: CYCLES,
            reset_cycles: RESET_CYCLES,
        };
        let tl = run(design, &opts).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: our kernel failed to run its own generated clocked \
                 program:\n{src}\n{e}"
            )
        });

        // GAP-5 width-conformance oracle: every signal (registers included,
        // not just `y`) at every captured instant must fit the width the
        // kernel itself resolved for it during elaboration.
        for sig in &tl.signals {
            for f in &tl.frames {
                let Some(v) = f.values.get(&sig.name) else {
                    continue;
                };
                assert_bits_fit_width(
                    &format!("seed {seed}: signal `{}` at time {}", sig.name, f.time),
                    v,
                    sig.width.bits,
                );
            }
        }

        // A real temp file on disk, since `compile_example` shells out to
        // the real `mimz compile` binary.
        let path = std::env::temp_dir().join(format!("mimz_diff_fuzz_clocked_{seed}.mimz"));
        std::fs::write(&path, &src).unwrap();
        let design_v = support::compile_example(&path);

        let inputs_meta: Vec<(String, u32, u128)> = ports
            .iter()
            .map(|(n, w, _)| (n.clone(), *w, held[n]))
            .collect();
        // GAP-11(a): the root plus every materialized intermediate.
        let outputs_meta: Vec<(String, u32)> = std::iter::once(("y".to_string(), out_width))
            .chain(sub_ports.iter().map(|(n, w, _)| (n.clone(), *w)))
            .collect();
        let tb = support::clocked_testbench(
            "Fuzz",
            &[],
            "clk",
            Some("rst"),
            &inputs_meta,
            &outputs_meta,
            CYCLES,
            RESET_CYCLES,
        );
        let stdout = support::run_vvp(&bin, &format!("clocked fuzz seed {seed}"), &design_v, &tb);
        let icarus = parse_icarus_raw(&stdout);

        let mut compared = 0;
        for f in tl.frames.iter().filter(|f| f.cycle.is_some()) {
            let cyc = f.cycle.unwrap();
            let icarus_row = icarus
                .get(&cyc)
                .unwrap_or_else(|| panic!("seed {seed}: Icarus produced no row for cycle {cyc}"));
            for (name, width) in &outputs_meta {
                let kernel_v = f.values[name].clone();
                let kernel_limbs = bits_to_limbs(&kernel_v, *width);
                let icarus_limbs = limbs_from_binary(&icarus_row[name], *width);
                assert_eq!(
                    kernel_limbs, icarus_limbs,
                    "seed {seed}, cycle {cyc}: our kernel {name}={kernel_v:?} but Icarus \
                     {name}={}\nsource:\n{src}\nheld inputs: {held:?}",
                    icarus_row[name]
                );
                compared += 1;
            }
        }
        assert!(compared > 0, "seed {seed}: nothing was compared");
    }
}
