# Prior Art — What the Other Modern HDLs Did

> Research notes, not normative. Written 2026-06-11 from the projects'
> official docs (Veryl/Spade samples quoted verbatim from their sites).
> Maps each language's design choices to Min-Mozhi's open decisions so we
> steal lessons instead of re-learning them.

## The landscape in one table

| Language     | Kind                | Host/impl | Emits                  | One-line thesis                                    |
| ------------ | ------------------- | --------- | ---------------------- | -------------------------------------------------- |
| **Veryl**    | standalone language | Rust      | SystemVerilog          | Verilog's model, modern syntax, fewer footguns     |
| **Spade**    | standalone language | Rust      | Verilog                | Rust-grade type system for hardware                |
| **Amaranth** | embedded DSL        | Python    | Verilog / RTLIL        | circuits as Python objects, strong clock domains   |
| **Chisel**   | embedded DSL        | Scala     | FIRRTL → SystemVerilog | generator-first, powers real silicon (RISC-V)      |
| Min-Mozhi    | standalone language | Rust      | Verilog-2005           | beginner-first + trilingual + safe-by-construction |

Two camps: Veryl and Spade are **our camp** (own parser, own syntax,
transpile to Verilog). Amaranth and Chisel hide inside a host language —
powerful for generation, but the host's error messages and tooling leak
through, which is exactly wrong for our persona (spec/01). Nobody in
either camp does trilingual keywords — that part we pioneer alone.

## The same counter, five ways

Min-Mozhi (`examples/english/counter.mimz`):

```mimz
module Counter(WIDTH: int = 8) {
  clock clk
  reset rst
  out count: bits[WIDTH]

  reg value: bits[WIDTH] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

**Veryl** (shape per veryl-lang.org's showcase module):

```veryl
module Counter #(
    param Width: u32 = 8,
) (
    i_clk  : input  clock       ,
    i_rst  : input  reset       ,
    o_count: output logic<Width>,
) {
    var r_count: logic<Width>;

    always_ff {
        if_reset {
            r_count = 0;
        } else {
            r_count += 1;
        }
    }

    assign o_count = r_count;
}
```

**Spade** (register form per docs.spade-lang.org's blinky tutorial):

```spade
entity counter(clk: clock, rst: bool) -> uint<8> {
    reg(clk) count: uint<8> reset(rst: 0) = trunc(count + 1);
    count
}
```

**Amaranth** (Python):

```python
from amaranth import *

class Counter(Elaboratable):
    def __init__(self, width=8):
        self.count = Signal(width)

    def elaborate(self, platform):
        m = Module()
        m.d.sync += self.count.eq(self.count + 1)
        return m
```

**Chisel** (Scala):

```scala
class Counter(width: Int = 8) extends Module {
  val io = IO(new Bundle { val count = Output(UInt(width.W)) })
  val value = RegInit(0.U(width.W))
  value := value + 1.U
  io.count := value
}
```

## What each one teaches us

### Veryl — the closest cousin

- **Independently validates our biggest calls.** Dedicated `clock`/`reset`
  TYPES (not plain wires), generated reset logic (`if_reset` hides
  polarity/sync — our emitter generates the whole branch from the reg's
  reset value), and readable emitted SystemVerilog. Two projects arriving
  at the same design independently is good evidence for both.
- **Differences to note:** Veryl keeps Verilog vocabulary (`always_ff`,
  `assign`, `var`) for working engineers; we drop it for beginners
  (`on rise`, plain `=`). Veryl uses `=` inside `always_ff` and makes it
  non-blocking automatically — we instead use a different OPERATOR (`<-`)
  so the reader sees the semantic difference. Ours is the more teachable
  choice; theirs is less typing.
- **Steal later:** its interface/modport story and generate blocks when we
  design `repeat` hardening; trailing-comma-everywhere port lists.

### Spade — the type-system benchmark

- **Where our checker should aim.** Spade's compiler enforces widths at
  the type level: `count + 1` on a `uint<28>` is a `uint<29>` and must be
  explicitly `trunc()`-ed back. That is EXACTLY our lossless-`+` /
  wrapping-`+%` rule (spec/02 section 1) — Spade proves it works in
  practice and that users accept the explicit narrowing.
- **Registers as expressions:** `reg(clk) count: T reset(rst: 0) = next;`
  packs clock, reset, and next-state into one declaration. Compact, but
  reads poorly for beginners next to our split form (`reg` declares,
  `on rise` assigns). Keep ours; cite theirs in the checker design notes
  for the single-driver rule (their form makes a second driver
  unrepresentable — our checker has to enforce what their syntax gets free).
- **Steal later:** pipeline stages (`pipeline(3)` + `stage` markers) are
  the standout original feature — a candidate for a far-future phase;
  enum + match lowering details for our FSM checker.

### Amaranth — the clock-domain reference

- **The most thought-through clock-domain model** of the four: every
  assignment names its domain (`m.d.sync`, `m.d.comb`, `m.d.<custom>`),
  domains carry their own clock + reset, and crossing them requires
  explicit primitives (FIFO/synchronizer). When we design multi-clock +
  the `sync` construct (spec/02 section 6, Phase 2), this is the model to
  study first.
- **`ResetInserter`/`EnableInserter`** transforms (wrap a circuit, get a
  reset/enable for free) are what our generated-reset will want to become
  once designs grow beyond one always-block.
- **Also a warning:** as an embedded DSL, a typo gives you a Python
  traceback, not a hardware error. Confirms decision to be a standalone
  language with own diagnostics.

### Chisel — the IR lesson (and the cautionary tale)

- **FIRRTL is the homework for Phase 2.** Chisel doesn't emit Verilog
  directly — it emits FIRRTL, a small typed IR with a spec, and separate
  passes lower it (width inference, reset handling, optimization). When
  Phase 2's "own IR" starts, read the FIRRTL spec FIRST; it is the
  best-documented hardware IR design that exists.
- **Width inference** in FIRRTL (unknown widths solved as constraints) is
  the heavyweight version of what our checker does with explicit rules.
  We stay explicit (teaching > inference, tie-breaker: honesty), but
  their constraint rules are a correctness checklist for ours.
- **The cautionary tale:** Chisel's emitted Verilog is famously
  unreadable (`_T_42` everywhere). Our constitution says the Verilog
  stays readable (auto-wires are named `{instance}_{port}`, not gensyms).
  Chisel shows what happens when you don't make that a rule.

### Verilog internals — know the target

Not prior art but the substrate. Specific topics, each tied to a thing
we generate or guard:

| Topic to study                                          | Why it matters to us                                                        |
| ------------------------------------------------------- | --------------------------------------------------------------------------- |
| latch inference rules (`always @*` incomplete branches) | our mandatory-`else`/exhaustive-`match` rules exist to make this impossible |
| blocking `=` vs non-blocking `<=` scheduling            | we emit `<=` in always-blocks, `assign` outside — never mix                 |
| implicit width extension/truncation in expressions      | the checker's width rules must model what Verilog will do to our output     |
| signed arithmetic (`$signed`, sign extension rules)     | needed before emitting `signed[N]` (spec/02 section 1.7)                    |
| synthesizable subset vs simulation-only constructs      | everything we emit must be in the subset; tests can use the rest            |
| `localparam`/`parameter` elaboration order              | our symbolic widths `[(WIDTH)-1:0]` rely on it                              |

## Is Min-Mozhi syntax actually TypeScript/JavaScript-like?

Checked construct by construct against the spec. Verdict: **the
DECLARATIONS read like TypeScript, the STATEMENTS read like Go, the
SEMANTICS are Rust.** The README pitch ("reads like Go/TypeScript, safe
like Rust") holds up, with the borrowings split like this:

| Min-Mozhi construct                          | Closest ancestor | Notes                                                                        |
| -------------------------------------------- | ---------------- | ---------------------------------------------------------------------------- |
| `name: type` postfix annotations             | TypeScript       | `in a: bits[8]` ≈ `a: number`; the single most TS-looking feature            |
| `let` bindings                               | TS/JS            | same keyword, same "introduce a name" role                                   |
| `{ }` blocks, C-family operator set          | TS/JS (C family) | `&&`, `\|\|`, `==`, `<<` all read identically                                |
| named args in instantiation `{ a: x, b: y }` | TS/JS            | object-literal shorthand look                                                |
| default params `(WIDTH: int = 8)`            | TS/JS            | identical syntax                                                             |
| newline-terminated statements, no semicolons | Go               | incl. the break-AFTER-operator continuation rule — that's Go's exact rule    |
| `if`/`match` as expressions                  | Rust             | JS has no expression-if (only ternary) and no match                          |
| `match` + `=>` arms + exhaustiveness         | Rust             | straight from Rust, incl. mandatory coverage                                 |
| `enum` + `State.Red` access                  | Rust/TS hybrid   | Rust semantics, TS-style dot access                                          |
| `<-` reg assignment                          | none of them     | hardware-specific; closest relative is Verilog's `<=` made visually distinct |
| `+%` wrapping operators                      | Zig (`+%`)       | not TS/Go/Rust — Zig is the one mainstream language with this exact spelling |
| `on rise(clk) { }`                           | none (event-ish) | reads like a JS event handler, but it's structural, not callback             |

Two honest caveats the comparison surfaces:

- A JS developer's instincts will be wrong about **semicolons-optional**:
  in JS, ASI is a trap; here newline-termination is the designed rule
  (Go's), with the continuation set spelled out (docs/code/02). Same
  surface, opposite philosophy.
- `let` does NOT mean a mutable variable as in JS — it introduces an
  instance. If beginner confusion shows up in testing, that's a keyword
  to revisit (the Tanglish `vai` is already flagged weak in spec/03).

## Standing question for each future design decision

Before designing the checker passes, multi-clock/`sync`, `repeat`
hardening, or the Phase 2 IR: check this page's language first (Spade,
Amaranth, Spade/Veryl, FIRRTL respectively) and note in the decision log
what was adopted or rejected, and why.

---

Sources: [veryl-lang.org](https://veryl-lang.org/),
[docs.spade-lang.org](https://docs.spade-lang.org/blinky_sw.html),
[amaranth-lang.org](https://amaranth-lang.org/),
[chisel-lang.org](https://www.chisel-lang.org/) /
[FIRRTL spec](https://github.com/chipsalliance/firrtl-spec).
