# mimz v0.2.0 — Wingless Butterfly

<!--
  RELEASE NOTES — the release pipeline reads this file:
    • line 1 (this `#` heading) → the GitHub Release TITLE
    • everything below          → the Release BODY (markdown)
  REWRITE this file for every release (new title + new notes), then tag.
  The longer human history lives in CHANGELOG.md; this is the per-release blurb.
-->

The second release of **Min-Mozhi (மின்மொழி)** — a modern, safe-by-default HDL,
built to teach digital design, and a Tamil-rooted one. This release adds
the language surface a real design needs (Verilog FFI, valid-bundles, structural
bundles, enum construction, arrays, `foreach`, CDC synchronizers) and — the
bigger half of the work — closes an eight-round correctness audit of the Verilog
backend.

**Language edition:** Wingless Butterfly (`wingless-butterfly-2026-1`) —
unchanged. No keyword-set change, so every v0.1.0 program still compiles, with
one deliberate exception noted under _Breaking_ below.

## Highlights

- **Verilog FFI** — `extern module` declares the port shape of real,
  hand-written Verilog (vendor IP, a hardened PLL, a protocol core) and
  instantiates it with no new syntax.
- **Optional values** — `bit?` / `bits[N]?` desugar to a `{ valid, data }`
  bundle, with `??` for unwrap and OR-mux. Always a two-way mux, never tri-state.
- **Structural bundle matching** — a bundle satisfies any bundle-typed slot whose
  required fields it covers, regardless of the two bundles' declared names.
- **Enum construction** — `Enum.Variant(a, b)`, the write-side counterpart to
  tagged-union `match`.
- **Arrays, `foreach`, and statement-based `fn` bodies** with `return`.
- **CDC synchronizers** — `sync.double_flop` / `sync.pulse`, with a
  bit-independence check (E0703) rather than a style note.
- **`mimz test --emulate`** — bind ports to virtual `led` / `uart_tx` /
  `uart_rx` / `speaker` peripherals with real-time throttling.

## Correctness

Between v0.1.0 and this release the Verilog backend went through eight rounds of
adversarial review (`docs/audit/`). Thirty-four of the defects found were one
family — two implementations of a single width rule disagreeing — and round 8
was the first round that added none to it.

What this release claims, and what the evidence supports: every shipped example
and demo compiles to Verilog that elaborates and whose self-checking testbench
reports PASS under real `vvp`; a 5000-seed differential fuzz over the generated
grammar finds no divergence; and the two known ways to get a width wrong are
both watched by a runtime invariant that fails loudly rather than emitting
silence.

What it does **not** claim: that the compiler produces Verilog matching its own
type system. That is not true until [GAP-1](docs/audit/gaps.md) lands — there is
no shared IR, so width and kind semantics are implemented three times (checker,
emitter, simulator) and kept in agreement by tests rather than by construction.
Closing it is the v0.3 direction.

## Known issues

**Every** open bug at tag time, and the gaps you are most likely to meet.
`docs/audit/gaps.md` is the full gap ledger — eight are open; the two HIGH
ones and the two most user-visible MEDIUM ones are listed here.

- **[GAP-1](docs/audit/gaps.md)** (HIGH, architectural) — no IR; width/kind
  semantics implemented three times over. The root cause of the width-rule
  family above, and the v0.3 direction.
- **[GAP-20](docs/audit/gaps.md)** (HIGH, testing) — the differential fuzz
  generator emits `reg` resets and `mem` initialisers as literals, so those two
  render sites are outside the generated grammar. Needs an Icarus-only oracle
  path; the third site (instance port connections) is covered as of this release.
- **[GAP-8](docs/audit/gaps.md)** (MEDIUM, language) — surface gaps you will
  meet early: no division operator, no attributes, no pipeline construct.
- **[GAP-2](docs/audit/gaps.md)** (MEDIUM, simulator) — `mimz-sim` is 2-state
  with a whole-value unknown flag; it does not model per-bit X propagation the
  way a 4-state simulator does. Run the emitted testbench under `vvp` when that
  distinction matters.
- **[BUG-32](docs/audit/bugs.md)** (MEDIUM) — `mem` lowers to an `initial`
  block: FPGA-only, not ASIC-synthesizable, and unresettable.
- **[BUG-38](docs/audit/bugs.md)** (MEDIUM) — `mimz-sim`'s combinational-only
  kernel rejects enum-typed signals, ports and wires.
- **[BUG-39](docs/audit/bugs.md)** (MEDIUM) — a `reg`'s reset value cannot be a
  payload-carrying `EnumConstruct` expression.
- **[BUG-74](docs/audit/bugs.md)** (MEDIUM) — an `if`/`match`-wrapped
  `EnumConstruct` passed directly to `encoding()` is refused at compile time
  rather than lowered. Binding it to a named `wire` first is unaffected.

All four bugs are compile-time refusals or simulator limitations, not silent
miscompiles.

## Breaking

- **`<<` is now lossless.** `bits[W] << k` is `bits[W+k]`; a runtime shift
  amount grows by its own worst case. Previously `<<` kept its left operand's
  width and could drop the bits real Verilog's `<<` produces (BUG-30/BUG-11). A
  shift-register or barrel-shifter idiom now needs an explicit `trunc` back down
  at each use — E0401 and `docs/guide/05-operators.md` Shifts both explain the
  growth rule at the point of failure.

## Install

Download the archive for your platform below, verify it against `SHA256SUMS`, and
put `mimz` on your `PATH`. Binaries are **unsigned** for this release — see
`UNSIGNED.txt` in the archive for the one-time macOS/Windows "allow" step. Full
instructions: `docs/guide/01-getting-started.md`. Or build from source with
`cargo build --release`.

See `CHANGELOG.md` for the complete change list.
