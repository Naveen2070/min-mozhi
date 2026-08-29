# mimz v0.2.0 - Wingless Butterfly

<!--
  RELEASE NOTES - the release pipeline reads this file:
    • line 1 (this `#` heading) → the GitHub Release TITLE
    • everything below          → the Release BODY (markdown)
  REWRITE this file for every release (new title + new notes), then tag.
  The longer human history lives in CHANGELOG.md; this is the per-release blurb.
-->

The second release of **Min-Mozhi (மின்மொழி)** - a modern, safe-by-default HDL,
built to teach digital design, and a Tamil-rooted one. This release adds the
language surface a real design needs (combinational `fn`, tagged-union enums,
bundles, arrays, Verilog FFI, CDC synchronizers) and - the bigger half of the
work - closes an eight-round correctness audit of the Verilog backend.

**Language edition:** Wingless Butterfly (`wingless-butterfly-2026-1`) - the
keyword-set version is still `v1`, but thirteen reserved words were activated
for new features, and nine of them break v0.1.0 identifiers (see _Breaking_).

## Highlights

- **Combinational functions** - `fn name(p: bits[8]) -> bits[16] { ... }`,
  statement bodies with `return` for guard-clause style, array-typed and
  bundle-typed parameters; lowered to Verilog `function automatic`.
- **Tagged unions** - enum variants carry payload fields, `match` patterns
  bind them, and `Enum.Variant(a, b)` construction completes the feature.
- **Bundles & valid-bundles** - `bundle` types with literals and
  destructuring; structural matching by required-field shape, not declared
  name; `bit?` / `bits[N]?` sugar with `??` unwrap/OR-mux - always an
  ordinary two-way mux, never tri-state.
- **Verilog FFI** - `extern module` declares the port shape of real,
  hand-written Verilog (vendor IP, a hardened PLL, a protocol core) and
  instantiates it with no new syntax.
- **CDC synchronizers** - `sync.double_flop` / `sync.pulse`, with a
  bit-independence check (`E0703`) rather than a style note.
- **Everyday control flow** - arrays and array literals, `foreach`, `loop`,
  `sync loop`, `const if`, `default`; builtins `clog2` / `encoding` /
  `assert` / `cover`.
- **`mimz test --emulate`** - bind ports to virtual `led` / `uart_tx` /
  `uart_rx` / `speaker` peripherals with real-time throttling.
- **Tooling** - embedded stdlib (`std.*`, `mimz eject std`), LSP hover /
  go-to-definition / completion, packages/namespacing with qualified
  references, new commands (`init`, `doctor`, `completions`,
  `check --watch`).

## Correctness

Between v0.1.0 and this release the Verilog backend went through eight rounds
of adversarial review (`docs/audit/`), plus one post-gate fuzzer find
(BUG-76). Seventy-five bugs were filed; sixty-eight are fixed and five
remain open, with BUG-10 half-fixed (its params side landed, the
returns-side flattening did not). Thirty-four of the
defects found were one family - two implementations of a single width rule
disagreeing - and round 8 was the first round that added none to it.

What this release claims, and what the evidence supports: every shipped example
and demo compiles to Verilog that elaborates and whose self-checking testbench
reports PASS under real `vvp`; a 5000-seed differential fuzz over the generated
grammar finds no divergence; and the two known ways to get a width wrong are
both watched by a runtime invariant that fails loudly rather than emitting
silence.

What it does **not** claim: that the compiler produces Verilog matching its own
type system. That is not true until [GAP-1](docs/audit/gaps.md) lands - there is
no shared IR, so width and kind semantics are implemented three times (checker,
emitter, simulator) and kept in agreement by tests rather than by construction.
Closing it is the v0.3 direction.

## Known issues

**Every** open bug at tag time, and the gaps you are most likely to meet.
`docs/audit/gaps.md` is the full gap ledger - eight are open; the two HIGH
ones and the most user-visible MEDIUM ones are listed here.

- **[GAP-1](docs/audit/gaps.md)** (HIGH, architectural) - no IR; width/kind
  semantics implemented three times over. The root cause of the width-rule
  family above, and the v0.3 direction.
- **[GAP-20](docs/audit/gaps.md)** (HIGH, testing) - the differential fuzz
  generator emits `reg` resets and `mem` initialisers as literals, so those
  two render sites are outside the generated grammar. Needs an Icarus-only
  oracle path.
- **[GAP-8](docs/audit/gaps.md)** (MEDIUM, language) - surface gaps you will
  meet early: no division operator, no attributes, no pipeline construct.
- **[GAP-2](docs/audit/gaps.md)** (MEDIUM, simulator) - `mimz-sim` is 2-state
  with a whole-value unknown flag; it does not model per-bit X propagation the
  way a 4-state simulator does. Run the emitted testbench under `vvp` when that
  distinction matters.
- **[BUG-10](docs/audit/bugs.md)** (MEDIUM, half-fixed) - bundle-typed `fn`
  params flatten correctly and returns get a diagnostic, but the real
  returns-side flattening is still pending; avoid bundle-typed `fn` returns
  until it lands.
- **[BUG-32](docs/audit/bugs.md)** (MEDIUM) - `mem` lowers to an `initial`
  block: FPGA-only, not ASIC-synthesizable, and unresettable.
- **[BUG-38](docs/audit/bugs.md)** (MEDIUM) - `mimz-sim`'s combinational-only
  kernel rejects every enum-typed signal, port or wire.
- **[BUG-39](docs/audit/bugs.md)** (MEDIUM) - a `reg`'s reset value cannot be a
  payload-carrying `EnumConstruct` expression.
- **[BUG-74](docs/audit/bugs.md)** (MEDIUM) - an `if`/`match`-wrapped
  `EnumConstruct` passed directly to `encoding()` is refused at compile time
  rather than lowered. Binding it to a named `wire` first is unaffected.
- **[BUG-75](docs/audit/bugs.md)** (LOW) - the pretty-printer adds one
  parenthesization recursion level the parser's depth limit doesn't expect; an
  expression parsed exactly at the ceiling may fail to re-parse after
  formatting.
- **Linux (musl) release binary ships without `--emulate`.** `hw-emulation`
  (dashboard + `speaker` audio) needs `cpal`, which needs ALSA dev headers
  that don't cross-compile for musl in CI - so that one binary is built
  `--no-default-features --features lsp,bench,watch`. `--emulate` still
  parses but errors at runtime. Windows and macOS release binaries are
  unaffected. Build from source with default features for the full set on
  Linux.

Of the five open bugs, four are compile-time refusals or simulator limitations;
the remaining two items (BUG-10's pending half, BUG-75) are an emitter
flattening gap and a printer edge case - neither a silent width miscompile.

## Breaking

- **`<<` is now lossless.** `bits[W] << k` is `bits[W+k]`; a runtime shift
  amount grows by its own worst case. Previously `<<` kept its left operand's
  width and could drop the bits real Verilog's `<<` produces (BUG-30/BUG-11).
  A shift-register or barrel-shifter idiom now needs an explicit `trunc` back
  down at each use - `E0401` and `docs/guide/05-operators.md` Shifts both
  explain the growth rule at the point of failure.
- **Thirteen reserved words became active keywords.** `fn`, `sync`, `default`,
  and `extern` came from the pre-v0.1.0 reserve, so activating them breaks
  nothing. But `bundle`, `loop`, `foreach`, `return`, `assert`, `cover`,
  `sim`, `bind`, and `speed` were not previously reserved: a v0.1.0 program
  using any of the nine as an identifier now fails with `E1101` and must be
  renamed. `mimz translate` has no auto-migration for this yet.

## Install

Download the archive for your platform below, verify it against `SHA256SUMS`, and
put `mimz` on your `PATH`. Binaries are **unsigned** for this release - see
`UNSIGNED.txt` in the archive for the one-time macOS/Windows "allow" step. Full
instructions: `docs/guide/01-getting-started.md`. Or build from source with
`cargo build --release`.

See `CHANGELOG.md` for the complete change list.
