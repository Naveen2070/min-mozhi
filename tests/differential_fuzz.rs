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

use mimz::sim::comb;
use mimz::sim::elaborate::elaborate_project;
use mimz::sim::run::{SimOpts, run};
use mimz::{checker, diag, lexer, parser};

mod support;

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
struct Frag {
    text: String,
    width: u32,
    signed: bool,
    atomic: bool,
}

/// Hard cap on any single fragment's width — bounds the generator so it
/// always terminates well under the simulator's 128-bit ceiling (BUG-13),
/// regardless of how many width-growing combines (concat, lossless
/// `+`/`-`) get chosen along the way.
const MAX_WIDTH: u32 = 32;

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
fn gen_leaf(rng: &mut Rng, ports: &[Port]) -> Frag {
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

/// Shift: LHS keeps its own width AND kind (per spec/02 section 3, shift
/// preserves the left operand's type outright — `shift_ty`'s `Ty::Bit |
/// Ty::Bits(_) | Ty::Signed(_) => lt` arm); RHS is a separate, small (1-5
/// bit) fragment, always freshly generated as an unsigned literal — never
/// derived from an existing (possibly signed) `Frag` — since a shift
/// amount can never be `signed` (E0403, `shift_ty`'s `Ty::Signed(_)` arm
/// for `rt`). No width or kind relationship to LHS otherwise.
fn combine_shift(rng: &mut Rng, a: Frag) -> Frag {
    let shamt_w = (rng.next_range(5) + 1) as u32;
    let shamt_v = rng.next_u64() & support::mask(shamt_w) as u64;
    let op = if rng.next_range(2) == 0 { "<<" } else { ">>" };
    let width = a.width;
    let signed = a.signed;
    Frag {
        text: format!("({} {op} extend({shamt_v}, {shamt_w}))", a.text),
        width,
        signed,
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
fn gen_expr(rng: &mut Rng, ports: &[Port], depth: u32, cap: u32) -> Frag {
    let raw = if depth == 0 || rng.next_range(6) == 0 {
        gen_leaf(rng, ports)
    } else {
        match rng.next_range(6) {
            0 => {
                let a = gen_expr(rng, ports, depth - 1, cap);
                let b = gen_expr(rng, ports, depth - 1, cap);
                combine_same_width(rng, a, b)
            }
            1 => {
                let a = gen_expr(rng, ports, depth - 1, cap);
                combine_shift(rng, a)
            }
            2 => {
                let a = gen_expr(rng, ports, depth - 1, cap);
                let b = gen_expr(rng, ports, depth - 1, cap);
                combine_concat(a, b)
            }
            3 => {
                let a = gen_expr(rng, ports, depth - 1, cap);
                let b = gen_expr(rng, ports, depth - 1, cap);
                combine_lossless(rng, a, b)
            }
            4 => {
                let a = gen_expr(rng, ports, depth - 1, cap);
                let b = gen_expr(rng, ports, depth - 1, cap);
                combine_wrap(rng, a, b)
            }
            _ => gen_slice(rng, ports).unwrap_or_else(|| gen_leaf(rng, ports)),
        }
    };
    clamp(rng, ports, raw, cap)
}

/// Generate one random valid combinational `.mimz` module as source text.
/// Returns `(source, input_ports, output_width)` — the caller needs
/// `input_ports` to build stimulus vectors and a matching Verilog
/// testbench, and `output_width` to declare the testbench's `y` wire.
fn gen_module(seed: u64) -> (String, Vec<Port>, u32) {
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
    let body = gen_expr(&mut rng, &ports, depth, MAX_WIDTH);

    let mut src = String::from("module Fuzz {\n");
    for (name, w, signed) in &ports {
        src += &format!("  in {name}: {}\n", ty_str(*w, *signed));
    }
    src += &format!("  out y: {}\n", ty_str(body.width, body.signed));
    src += &format!("  y = {}\n", body.text);
    src += "}\n";
    (src, ports, body.width)
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
fn gen_clocked_module(seed: u64) -> (String, Vec<Port>, BTreeMap<String, u128>, u32, bool) {
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

    let mut src = String::from("module Fuzz {\n  clock clk\n  reset rst\n");
    for (name, w, signed) in &ports {
        src += &format!("  in {name}: {}\n", ty_str(*w, *signed));
    }
    for (name, w, signed) in &regs {
        src += &format!("  reg {name}: {} = 0\n", ty_str(*w, *signed));
    }

    let out_body = gen_expr(&mut rng, &leaves, depth, MAX_WIDTH);
    src += &format!("  out y: {}\n", ty_str(out_body.width, out_body.signed));

    src += "  on rise(clk) {\n";
    for (name, w, signed) in &regs {
        let next = gen_expr(&mut rng, &leaves, depth, *w);
        let next = force_width(&mut rng, next, *w, *signed);
        src += &format!("    {name} <- {}\n", next.text);
    }
    src += "  }\n";
    src += &format!("  y = {}\n", out_body.text);
    src += "}\n";

    (src, ports, held, out_body.width, out_body.signed)
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
        let (src, _, _) = gen_module(0xC0FFEE_u64.wrapping_add(seed));
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
/// `MIMZ_DIFF_FUZZ_N` (default 20) randomly generated combinational
/// programs. Gated by `require_iverilog()` exactly like every other
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
    let n: u64 = std::env::var("MIMZ_DIFF_FUZZ_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    for i in 0..n {
        let seed = 0xC0FFEE_u64.wrapping_add(i);
        let (src, ports, out_width) = gen_module(seed);

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

        // Our own kernel, one row per input vector.
        let mut kernel_rows: Vec<BTreeMap<String, u128>> = Vec::new();
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
                panic!("seed {seed}: our kernel rejected its own generated program:\n{src}\n{e}")
            });
            // Fuzzer widths are capped at `MAX_WIDTH` (32), so every output value
            // is always `Bits::Small` — narrow it back to `u128` to compare
            // against Icarus's own u128-typed parsed output.
            let row: BTreeMap<String, u128> = outputs
                .into_iter()
                .map(|o| {
                    let v = match o.value {
                        mimz::sim::value::Bits::Small(v) => v,
                        mimz::sim::value::Bits::Wide(_) => {
                            unreachable!("fuzzer widths are capped at MAX_WIDTH=32")
                        }
                    };
                    (o.name, v)
                })
                .collect();
            kernel_rows.push(row);
        }

        // Real Icarus.
        let design_v = support::compile_example(&path);
        let outputs_meta = vec![("y".to_string(), out_width)];
        let tb = support::comb_testbench("Fuzz", &[], &inputs_meta, &outputs_meta, &vectors);
        let stdout = support::run_vvp(&bin, &format!("fuzz seed {seed}"), &design_v, &tb);
        let icarus = support::parse_icarus(&stdout);

        for (idx, kernel_row) in kernel_rows.iter().enumerate() {
            let icarus_row = icarus
                .get(&(idx as u64))
                .unwrap_or_else(|| panic!("seed {seed}: Icarus produced no row for vector {idx}"));
            let kernel_y = kernel_row["y"];
            let icarus_y = icarus_row["y"];
            assert_eq!(
                kernel_y, icarus_y,
                "seed {seed}, vector {idx}: our kernel y={kernel_y} but Icarus y={icarus_y}\n\
                 source:\n{src}\nvector: {:?}",
                vectors[idx]
            );
        }
    }
}

/// v3's fast, Icarus-independent counterpart to
/// `differential_fuzz_generates_checker_valid_programs` — every generated
/// CLOCKED program must also pass `checker::check`.
#[test]
fn differential_fuzz_clocked_generates_checker_valid_programs() {
    for seed in 0..1000u64 {
        let (src, ..) = gen_clocked_module(0xC10CCED_u64.wrapping_add(seed));
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

/// v3's real differential, clocked counterpart to
/// `differential_fuzz_matches_icarus`: our own kernel (`elaborate_project`
/// and `run`, the exact engine behind `mimz sim`/`test`) vs. real Icarus
/// Verilog, over `MIMZ_DIFF_FUZZ_CLOCKED_N` (default 20) randomly
/// generated clocked programs, each run for a fixed number of cycles with
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
    let n: u64 = std::env::var("MIMZ_DIFF_FUZZ_CLOCKED_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    const CYCLES: u64 = 8;
    const RESET_CYCLES: u64 = 1;

    for i in 0..n {
        // A separate seed space from the combinational generator
        // (`0xC0FFEE`) so the two never accidentally alias.
        let seed = 0xC10CCED_u64.wrapping_add(i);
        let (src, ports, held, out_width, _out_signed) = gen_clocked_module(seed);

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

        let design = elaborate_project(std::slice::from_ref(&file), None, &BTreeMap::new())
            .unwrap_or_else(|e| {
                panic!(
                    "seed {seed}: our kernel failed to elaborate its own generated \
                     clocked program:\n{src}\n{e}"
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

        // A real temp file on disk, since `compile_example` shells out to
        // the real `mimz compile` binary.
        let path = std::env::temp_dir().join(format!("mimz_diff_fuzz_clocked_{seed}.mimz"));
        std::fs::write(&path, &src).unwrap();
        let design_v = support::compile_example(&path);

        let inputs_meta: Vec<(String, u32, u128)> = ports
            .iter()
            .map(|(n, w, _)| (n.clone(), *w, held[n]))
            .collect();
        let outputs_meta = vec![("y".to_string(), out_width)];
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
        let icarus = support::parse_icarus(&stdout);

        let mut compared = 0;
        for f in tl.frames.iter().filter(|f| f.cycle.is_some()) {
            let cyc = f.cycle.unwrap();
            let icarus_row = icarus
                .get(&cyc)
                .unwrap_or_else(|| panic!("seed {seed}: Icarus produced no row for cycle {cyc}"));
            let kernel_y = f.values["y"].clone();
            let icarus_y = icarus_row["y"];
            assert_eq!(
                kernel_y,
                mimz::sim::value::Bits::Small(icarus_y),
                "seed {seed}, cycle {cyc}: our kernel y={kernel_y:?} but Icarus y={icarus_y}\n\
                 source:\n{src}\nheld inputs: {held:?}"
            );
            compared += 1;
        }
        assert!(compared > 0, "seed {seed}: nothing was compared");
    }
}
