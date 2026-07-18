//! T2 (docs/audit/review-2026-07-17.md sec 8, docs/plan/
//! phase-2-differential-fuzzing.md): random-program differential fuzzing.
//! v1 scope: unsigned bits[N] combinational expressions only — no
//! clock/reg/signed/enum/bundle/fn/foreach/imports. Full design:
//! docs/superpowers/specs/2026-07-18-differential-fuzzing-design.local.md
//!
//! Generates a real `.mimz` module as source text, checker-clean **by
//! construction** (every combine step unifies operand widths via
//! `extend()` before applying an operator, so no operator's own
//! fine-print width rule needs special-casing here), then runs it through
//! the full pipeline: lex -> parse -> checker::check -> our kernel ->
//! `mimz compile` -> Icarus. Never generates a write-slice (`sig[hi:lo] =
//! expr`, BUG-17) — only ever one whole-signal `out y = <expr>`.

use std::collections::BTreeMap;

use mimz::sim::comb;
use mimz::{checker, diag, lexer, parser};

mod support;

/// One input port: `(name, width)`.
type Port = (String, u32);

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

/// A generated expression fragment, alongside its own known width — the
/// generator's core invariant: every fragment's width is known by
/// construction, never inferred after the fact.
struct Frag {
    text: String,
    width: u32,
}

/// Hard cap on any single fragment's width — bounds the generator so it
/// always terminates well under the simulator's 128-bit ceiling (BUG-13),
/// regardless of how many width-growing combines (concat, lossless
/// `+`/`-`) get chosen along the way.
const MAX_WIDTH: u32 = 32;

/// Widen `f` to `target` bits via `extend()` if it's narrower; a no-op
/// otherwise (the caller always passes `target = max(a.width, b.width)`,
/// so "already wider than target" never actually happens for the
/// same-width family, but stays correct either way).
fn widen(f: Frag, target: u32) -> Frag {
    if f.width < target {
        Frag {
            text: format!("extend({}, {target})", f.text),
            width: target,
        }
    } else {
        f
    }
}

/// Clamp `f` to `cap` bits by slicing its low `cap` bits off if it's wider
/// — `(a + b)[cap-1:0]` is checker-legal (slicing an arbitrary computed
/// sub-expression, not just an identifier — verified live: `checker::check`
/// accepts `y = (a + b)[5:2]`). Called on every `gen_expr` result, which
/// makes "every recursive call returns width <= its caller's assumed cap"
/// a strict invariant — parent combinators never need their own width
/// bookkeeping beyond that.
fn clamp(f: Frag, cap: u32) -> Frag {
    if f.width <= cap {
        f
    } else {
        Frag {
            text: format!("{}[{}:0]", f.text, cap - 1),
            width: cap,
        }
    }
}

/// A leaf: either a reference to an existing input port, or a small
/// literal explicitly widthed via `extend()` (never a bare literal
/// mid-expression — matches the spec's own idiom for "give it a width
/// first").
fn gen_leaf(rng: &mut Rng, ports: &[Port]) -> Frag {
    if !ports.is_empty() && rng.next_range(3) != 0 {
        let (name, w) = &ports[rng.next_range(ports.len() as u64) as usize];
        Frag {
            text: name.clone(),
            width: *w,
        }
    } else {
        let w = (rng.next_range(8) + 1) as u32;
        let v = rng.next_u64() & support::mask(w) as u64;
        Frag {
            text: format!("extend({v}, {w})"),
            width: w,
        }
    }
}

/// Result-width effect of a same-width-family operator, tracked so the
/// generator knows the new fragment's width without re-deriving each
/// operator's own rule from scratch.
#[derive(Clone, Copy)]
enum WidthEffect {
    /// `+% -% & | ^` — preserve the (now-equal) operand width.
    Preserve,
    /// `+ -` — lossless, grows by one bit.
    GrowByOne,
    /// `== != < <= > >=` — always `bit` (width 1).
    ToBit,
}

const SAME_WIDTH_OPS: &[(&str, WidthEffect)] = &[
    ("+", WidthEffect::GrowByOne),
    ("-", WidthEffect::GrowByOne),
    ("+%", WidthEffect::Preserve),
    ("-%", WidthEffect::Preserve),
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
/// operator: unify both operands to `max(a.width, b.width)` via `widen`
/// first (legal for every operator in this family once widths match, no
/// per-operator special-casing needed), then apply it.
fn combine_same_width(rng: &mut Rng, a: Frag, b: Frag) -> Frag {
    let w = a.width.max(b.width);
    let a = widen(a, w);
    let b = widen(b, w);
    let (op, effect) = SAME_WIDTH_OPS[rng.next_range(SAME_WIDTH_OPS.len() as u64) as usize];
    let width = match effect {
        WidthEffect::Preserve => w,
        WidthEffect::GrowByOne => w + 1,
        WidthEffect::ToBit => 1,
    };
    Frag {
        text: format!("({} {op} {})", a.text, b.text),
        width,
    }
}

/// Shift: LHS keeps its own width (per spec/02 section 3, shift preserves
/// the left operand's width); RHS is a separate, small (1-5 bit) fragment
/// with no width relationship to LHS — spec-legal as-is, no unification.
fn combine_shift(rng: &mut Rng, a: Frag) -> Frag {
    let shamt_w = (rng.next_range(5) + 1) as u32;
    let shamt_v = rng.next_u64() & support::mask(shamt_w) as u64;
    let op = if rng.next_range(2) == 0 { "<<" } else { ">>" };
    let width = a.width;
    Frag {
        text: format!("({} {op} extend({shamt_v}, {shamt_w}))", a.text),
        width,
    }
}

/// Concat: `{a, b}`, no width unification needed — result width is the sum.
fn combine_concat(a: Frag, b: Frag) -> Frag {
    Frag {
        text: format!("{{{}, {}}}", a.text, b.text),
        width: a.width + b.width,
    }
}

/// A random sub-range read of an existing input port: `port[hi:lo]`,
/// `0 <= lo <= hi < port_width`. `None` if there are no ports (never
/// happens in practice — `gen_module` always creates 2-4 — but kept total
/// rather than panicking on an empty slice).
fn gen_slice(rng: &mut Rng, ports: &[Port]) -> Option<Frag> {
    if ports.is_empty() {
        return None;
    }
    let (name, w) = &ports[rng.next_range(ports.len() as u64) as usize];
    let lo = rng.next_range(*w as u64) as u32;
    let max_len = w - lo;
    let len = (rng.next_range(max_len as u64) + 1) as u32;
    let hi = lo + len - 1;
    Some(Frag {
        text: format!("{name}[{hi}:{lo}]"),
        width: len,
    })
}

/// Build one expression fragment, depth-bounded (stops at `depth == 0`,
/// or a 1-in-4 chance of bottoming out early so trees aren't all
/// maximum-depth). Every return is clamped to `MAX_WIDTH` — see `clamp`'s
/// doc comment for why that makes width bookkeeping trivial for callers.
fn gen_expr(rng: &mut Rng, ports: &[Port], depth: u32) -> Frag {
    let raw = if depth == 0 || rng.next_range(4) == 0 {
        gen_leaf(rng, ports)
    } else {
        match rng.next_range(4) {
            0 => {
                let a = gen_expr(rng, ports, depth - 1);
                let b = gen_expr(rng, ports, depth - 1);
                combine_same_width(rng, a, b)
            }
            1 => {
                let a = gen_expr(rng, ports, depth - 1);
                combine_shift(rng, a)
            }
            2 => {
                let a = gen_expr(rng, ports, depth - 1);
                let b = gen_expr(rng, ports, depth - 1);
                combine_concat(a, b)
            }
            _ => gen_slice(rng, ports).unwrap_or_else(|| gen_leaf(rng, ports)),
        }
    };
    clamp(raw, MAX_WIDTH)
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
            (format!("p{i}"), w)
        })
        .collect();
    let depth = (rng.next_range(3) + 2) as u32; // 2..=4
    let body = gen_expr(&mut rng, &ports, depth);

    let mut src = String::from("module Fuzz {\n");
    for (name, w) in &ports {
        src += &format!("  in {name}: bits[{w}]\n");
    }
    src += &format!("  out y: bits[{}]\n", body.width);
    src += &format!("  y = {}\n", body.text);
    src += "}\n";
    (src, ports, body.width)
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

        let vectors = support::gen_vectors(&ports, 8);

        // Our own kernel, one row per input vector.
        let mut kernel_rows: Vec<BTreeMap<String, u128>> = Vec::new();
        for v in &vectors {
            let outputs = comb::eval_outputs(
                std::slice::from_ref(&file),
                None,
                v,
                &BTreeMap::new(),
            )
            .unwrap_or_else(|e| {
                panic!("seed {seed}: our kernel rejected its own generated program:\n{src}\n{e}")
            });
            let row: BTreeMap<String, u128> =
                outputs.into_iter().map(|o| (o.name, o.value)).collect();
            kernel_rows.push(row);
        }

        // Real Icarus.
        let design_v = support::compile_example(&path);
        let outputs_meta = vec![("y".to_string(), out_width)];
        let tb = support::comb_testbench("Fuzz", &[], &ports, &outputs_meta, &vectors);
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
