# Changelog

All notable changes to **Min-Mozhi (மின்மொழி)**. The project has **two version
axes** (see [`spec/06-editions.md`](spec/06-editions.md)):

- **Compiler version** - the `mimz` binary / crate version (`Cargo.toml`).
- **Language edition** - a codename + year + serial (`wingless-butterfly-2026-1`).
  Surfaced by `mimz --version`, in every emitted Verilog header, and here.

Format follows [Keep a Changelog](https://keepachangelog.com).
Compiler versions follow [SemVer](https://semver.org).

---

## [0.2.0] - 2026-08-24 · Language edition: Wingless Butterfly `wingless-butterfly-2026-1`

> **Tag pending** - content frozen after the round-9 release gate went green
> (8/8, 2026-08-20), then amended 2026-08-24 to fold in post-gate items
> (diagnostic `E0420`, open bug `BUG-75`, test-count sync 1318 -> 1320).
> Cut the tag on master HEAD when publishing.

Keyword-set **version** stays `v1`, so this is not a new language edition -
but thirteen reserved words were **activated**, and nine of them break
v0.1.0 programs that used them as identifiers (see _Changed_).

### Added

Language surface:

- Combinational functions - `fn name(p: bits[8]) -> bits[16] { ... }`,
  lowered to Verilog `function automatic`; recursion banned (`E0805`),
  widths checked (`E0804`).
- Statement-based `fn` bodies - `let` / `if` / `return` inside a `fn` for
  guard-clause style (priority-selected result, not a silicon early-exit).
  New keyword `return`/`thirumbu`/`திரும்பு`; unreachable code after
  `return` is `E0812`.
- Tagged-union enums - variants carry payload fields, `match` patterns bind
  them, OR-arm binding intersection checked (`E0806`-`E0808`).
- `Enum.Variant(a, b)` construction - the write-side counterpart to
  tagged-union `match`; reuses `E0806`/`E0401`/`E0103`.
- Bundles - `bundle Name { field: type }`, parametric bundle types, bundle
  literals `{ ... }`, and `let` destructuring; flattened to prefixed
  scalars in emitter and simulator (`E0901`-`E0912`).
- Structural bundle matching (2.9) - bundles match by required-field shape,
  not declared name, across lets/drives/instantiation ports/fn signatures
  (`E0910`; also fixes non-flattened Verilog for bundle ports across
  instantiations).
- Valid-bundle sugar - `bit?` / `bits[N]?` / `signed[N]?` desugar to a
  synthesized `{ valid, data }` bundle; `??` unwraps (`T? ?? T`) or OR-muxes
  (`T? ?? T?`), always an ordinary mux, never tri-state (`E0911`/`E0912`).
- Array-typed `fn` parameters and array literals `[e1, ..., eN]` -
  elaborate to N scalars; constant indices fold, runtime indices become a
  priority mux (`E0411`-`E0417`).
- `foreach <var> in <source>` - range form and array/mem-element form;
  desugars to existing `repeat`/`loop` machinery before checking/emitting
  (`E0417`). Tanglish/Tamil spellings provisional.
- Control-flow additions:
  - `loop` in `on` blocks and `fn` bodies - unrolled, sharing `repeat`'s
    unroll budget.
  - `sync loop` - clock-domain counter loop lowered onto the reg/process
    machinery.
  - `const if (cond) { ... }` - compile-time module-item conditional
    (`E0811`).
  - `default name <- expr` assignment (`E0809`/`E0810`).
- Verilog FFI - `extern module Name(...) { ... }` declares the port shape
  of real hand-written/vendor Verilog; scalar ports only (`E1301`/`E1302`);
  simulator taints extern outputs unknown (`extern_sim = warn|strict` in
  `mimz.toml`); companion `.v` files join via `[compile] verilog_files`
  and `--extern-src`.
- CDC primitives - `sync.double_flop(...)` / `sync.pulse(...)` for
  clock-domain crossings, 1-bit only with a metastability guard
  (`E0702`-`E0705`, parser `E1116`); spec section 1.2b.
- New builtins - `clog2(n)` (`E0420`: cannot size a port - Verilog-2005
  port lists cannot call constant functions), `encoding(e)` enum-to-bits
  cast, `assert(cond [, "msg"])` hard runtime invariants,
  `cover(cond [, "label"])` coverage hit counters.
- Wide constants - compile-time values are arbitrary-width; decimal
  literals and const-folding work past 128 bits, wide `match`
  exhaustiveness no longer overflows.

Tooling:

- `mimz test --emulate` - `sim` blocks bind ports to virtual peripherals
  (`led`, `uart_tx`, `uart_rx`, `speaker`) with real-time pacing, a
  terminal dashboard, UART TCP sockets, and host audio (`cpal`). Opt-in;
  auto-degrades outside a TTY.
- New CLI commands - `init`, `doctor`, `completions`, `eject std`,
  `check --watch`; restructured colorized help.
- Embedded standard library - `std.*` imports with trilingual routing;
  `mimz eject std` vendors the sources; first modules shipped (debouncer).
- LSP upgrades - hover, go-to-definition, completion (was diagnostics-only
  in v0.1.0).
- Packages/namespacing - qualified references (`Module.item`), per-file
  uniqueness, import resolution with ambiguity diagnostics
  (`E0110`/`E0111`).
- Parser error recovery - placeholder nodes give multi-error reporting
  instead of fail-on-first.

### Changed

- **BREAKING:** `<<` is now lossless like `+`/`-`/`*`: `bits[W] << k` is
  `bits[W+k]`; a runtime shift amount grows by its own worst case
  (`2^N - 1` for a `bits[N]` amount). Previously `<<` silently kept its
  left operand's width and could drop bits real Verilog's own `<<`
  produces (BUG-30/BUG-11). `>>` is unchanged. Re-shifting idioms need an
  explicit `trunc`; `E0401` explains the growth rule at the point of
  failure.
- **BREAKING:** thirteen words became active keywords: `fn`, `sync`,
  `default`, `extern` (previously reserved - safe), plus `bundle`, `loop`,
  `foreach`, `return`, `assert`, `cover`, `sim`, `bind`, `speed` (not
  previously reserved). A v0.1.0 program using any of the latter nine as
  an identifier now fails with `E1101` and must be renamed; `mimz
translate` has no auto-migration for this yet.

### Performance

- `mimz compile` register bookkeeping behind an `Rc` - ~27x faster at
  4000 registers (GAP-12).
- Simulator skips timeline capture unless `--trace` is requested.

### Security

- `[lib] std` override in `mimz.toml` is sandboxed to the workspace root
  (SEC-7).

### Fixed

Eight rounds of adversarial review of the Verilog backend (`docs/audit/`,
one `review-*.md` per round). Ledger at this release: **74 bugs filed,
67 fixed, 5 open**, plus two special cases - BUG-10 is half-fixed (see
Known issues) and BUG-12 was re-filed under a later number.

- **Width-rule family (34 instances, BUG-28 .. BUG-68)** - checker, emitter,
  and simulator each implemented some width rules separately and
  disagreed; round 8 was the first round to add no new instance.
- **Self-determined-position hoist (BUG-63 .. BUG-72)** - concat members,
  reduction operands, `$signed`/`encoding` arguments and six other
  positions now get an explicitly-widened temporary, including symbolic
  (parameter) widths.
- **Declaration order (BUG-70)** - instance outputs could be referenced
  before their wire declaration (accepted by `mimz check`, rejected by
  every real elaborator); now declared in a separate pass.
- **Testbench const scope (BUG-71)** - `--emit-testbench` could take the
  wrong `const if` branch and report the opposite verdict to `mimz test`;
  the last silent divergence in the series.

Guarding the same ground going forward:

- Runtime declaration-order invariant over every emitted identifier.
- Full `iverilog -g2005` elaboration of all 226 corpus files on CI.
- 90 emitted testbench modules asserted PASS under real `vvp`.
- 5000-seed differential fuzz against Icarus (`tools/gate.sh`).

### Known issues

Ledger totals: eight open gaps (`docs/audit/gaps.md`), five open bugs
(`docs/audit/bugs.md`). The ones most likely to be met:

- **GAP-1** (HIGH, architectural) - no IR; width/kind semantics implemented
  three times over. Root cause of the width-rule family, and the v0.3
  direction. The claim "Verilog matches its own type system" is **not** made.
- **GAP-20** (HIGH, testing) - fuzz generator leaves `reg` resets / `mem`
  initialisers outside the generated grammar; needs an Icarus-only oracle.
- **GAP-8** (MEDIUM, language) - no division operator, attributes, or
  pipelines.
- **GAP-2** (MEDIUM, simulator) - simulator is 2-state; no per-bit X
  propagation. Use `vvp` on the emitted testbench when that matters.
- **BUG-10** (MEDIUM, half-fixed) - bundle-typed `fn` params flatten
  correctly, returns get a diagnostic, but the real returns-side flattening
  is still pending; avoid bundle-typed `fn` returns until closed.
- **BUG-32** (MEDIUM) - `mem` lowers to an `initial` block: FPGA-only, not
  ASIC-synthesizable, unresettable.
- **BUG-38** (MEDIUM) - simulator rejects enum-typed signals, ports, wires.
- **BUG-39** (MEDIUM) - a `reg`'s reset value cannot be a payload-carrying
  `EnumConstruct`.
- **BUG-74** (MEDIUM) - an `if`/`match`-wrapped `EnumConstruct` passed
  directly to `encoding()` is refused; bind it to a named `wire` first.
- **BUG-75** (LOW) - pretty-printer adds one parenthesization recursion
  level the parser's depth limit doesn't expect; an expression parsed at
  the exact ceiling may fail to re-parse after `mimz fmt`.

### Test suite

- **1320 passing tests** across unit (lexer, parser, checker, emitter,
  morph, sim, translate, grammar-sync, hardware-emulation) and integration
  (examples, golden files, Icarus differential, fuzz corpus,
  self-determined regression, external modules, packages, lab lessons,
  docs staleness guard).
- **Golden-file pinning** - example Verilog byte-pinned in `tests/golden/`
  (87 `.v` goldens: 70 module + 17 testbench).
- **`tests/fixtures/errors/`** - 121 `.mimz` files that must produce a
  specific E-code; a checker code without a fixture fails CI.
- **`grammar_sync`** - `lang/keywords.toml`, `spec/03`, and the TextMate
  grammar asserted mutually consistent.
- **`docs_sync`** - the test count in `docs/code/10-test-map.md`, the
  README badge, and `ROADMAP.md` must match the live suite.

---

## [0.1.0] - 2026-06-24 · Language edition: Wingless Butterfly `wingless-butterfly-2026-1`

The first public release. Phases 0, 1, 1.8, and 1.5 complete.
Keyword set v1 frozen 2026-06-15. 432 passing tests.

### Language - Core

#### Types and signals

- `wire` - combinational signal driven by `=` assignments; inferred-latch guard
  (unwired `wire` is a compile-time error `E0201`).
- `reg` - clocked state element driven by `<-` inside `on rise`/`on fall` blocks;
  mandatory reset value (no reset = `E0301`).
- `bits[N]` - unsigned integer of exactly `N` bits.
- `signed[N]` - two's-complement signed integer of exactly `N` bits; emitted as
  Verilog `wire signed` / `reg signed`.
- `int` - constant / parameter integer (not a signal type).
- `bool` - single-bit boolean (`true`/`false`).
- `clock` - dedicated clock-port type; drives `on rise`/`on fall` only.
- `reset` - dedicated synchronous-reset type; active-high, one reset per module.
- `async reset` - asynchronous reset variant; widens the always-block to
  `@(posedge clk or posedge rst)`; active-high only.
- `in`, `out`, `inout` - port directions.
- `mem m: bits[W][DEPTH] = init` - addressable register array; combinational
  indexed read, clocked indexed write, power-on initialiser.

#### Operators and arithmetic

- Lossless arithmetic: `+` / `-` / `*` grow width to hold the result; no silent
  truncation (`E0401`).
- Wrapping family: `+%` / `-%` / `*%` - explicit saturating/wrapping ops for
  when truncation is intended (emitted as Verilog `+`/`-`/`*` with the correct
  width).
- Bitwise: `&`, `|`, `^`, `~`.
- Shift: `<<`, `>>` (logical; `>>>` arithmetic).
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=` (always returns `bool`).
- Concatenation: `{a, b, c}`.
- Replication: `{N{x}}` - the inner group repeated `N` times (Verilog `{N{x}}`).
- Bit-select: `x[i]`; slice: `x[hi:lo]`.
- Signed/unsigned guard: mixing `bits` and `signed` without an explicit cast is
  `E0402`.

#### Control flow

- `if <cond> { … } else { … }` - expression-oriented; mandatory `else` when
  driving a wire (`E0501`).
- `match <expr> { pattern => expr, … }` - exhaustive by default (`E0502`);
  don't-care patterns `0b1??` (binary only, this edition) map to Verilog `casez`.
- `on rise(clk) { … }` - rising-edge clocked block.
- `on fall(clk) { … }` - falling-edge clocked block (negedge sibling).

#### Modules and instantiation

- `module Name(PARAM: type = default) { … }` - parameterised module.
- Port and wire declarations inside the module body.
- Module instantiation with named ports.
- Cross-file instantiation via `load` (no C-style preprocessor).

#### Constants and parameters

- `const NAME: type = value` - compile-time constant.
- Module parameters resolved at instantiation.

#### Testing

- `test "name" { tick { … } expect { … } }` - inline test blocks compiled by
  `mimz test`; `tick` sets inputs, `expect` asserts outputs.

---

### Language - Safety Rules (compile-time, stable E-codes)

Every rule produces a teaching diagnostic with a `help:` line.

| E-code range    | Rule                                                            |
| --------------- | --------------------------------------------------------------- |
| `E0101`–`E0199` | Loader errors (file not found, encoding)                        |
| `E0201`–`E0299` | Wire/signal errors: undriven wire, multiple drivers             |
| `E0301`–`E0399` | Register errors: missing reset value, wrong assignment operator |
| `E0401`–`E0499` | Type and width errors: lossless overflow, signed/unsigned mix   |
| `E0501`–`E0599` | Control-flow errors: missing `else`, non-exhaustive `match`     |
| `E0601`–`E0699` | Scope and reference errors: undefined identifier, port mismatch |
| `E0701`–`E0799` | Clock/reset errors: multiple clocks, wrong domain crossing      |
| `E1001`–`E1099` | Lexer errors: illegal character, malformed literal              |
| `E1101`–`E1199` | Parser errors: unexpected token, unclosed brace                 |

All codes are stable and will never be renumbered or reused.

---

### Trilingual keyword system

- **Three keyword skins over one grammar**: English, Tanglish, Tamil - freely
  mixable within a single file; identical semantics.
- **Keyword set v1 frozen 2026-06-15** - English column immutable from this
  point; Tanglish/Tamil columns ratified after native-speaker panel review (C3).
- `mimz translate --flavor <english|tanglish|tamil|mixed>` - lossless,
  round-trip keyword conversion; preserves identifiers and formatting.
- `mimz translate --order <code|thamizh>` - converts between SVO (code-order)
  and SOV (Tamil natural word order).
- Native Tamil/Tanglish error messages - `lang/messages.toml`; 33 of 36
  diagnostic codes have native-authored translations; structured-arg
  interpolation (signal names inflected with Tamil case suffixes via
  `lang/case_suffixes.toml`).

---

### Grammar Engine (Phase 1.8) - `thamizh-order`

Natural Tamil SOV word order - the postpositional clause forms that make Min-Mozhi
code read like Tamil, not transliterated English:

- `<cond> enil { }` - if-expression flip (condition-first → `enil`).
- `yetram(clk) pothu { }` - clocked-block flip.
- `<expr> thernthedu { }` - match-expression flip.
- File-level `syntax thamizh` directive - activates the SOV parser profile;
  produces the identical AST as code-order.
- Milestone: the traffic-light FSM in pure Tamil script, natural word order,
  compiling to byte-identical Verilog as its English twin.

---

### Compiler pipeline

| Stage               | What it does                                                                                                                                        |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Lexer**           | Tokenises all three keyword flavors; Unicode identifiers (Tamil script native); `E10xx` errors                                                      |
| **Parser**          | Recursive-descent; full grammar; SVO and SOV profiles; `E11xx` errors; statement-level error recovery (`sync_to_newline`) for multi-error reporting |
| **AST**             | Typed nodes for all language constructs; source-span attached to every node                                                                         |
| **Checker**         | Six passes; all spec safety rules; `E02xx`–`E07xx`; every diagnostic carries a `help:` teaching line                                                |
| **Verilog emitter** | Synthesizable Verilog-2005; `repeat` unrolling; Tamil→ASCII transliteration; `wire signed` / `reg signed`; golden-file output pinning               |

---

### CLI commands

| Command                          | What it does                                                                       |
| -------------------------------- | ---------------------------------------------------------------------------------- |
| `mimz check <file>`              | Lex + parse + all checker passes; prints diagnostics                               |
| `mimz check --json <file>`       | Machine-readable JSON diagnostics (LSP / CI use)                                   |
| `mimz compile <file> -o <out.v>` | Full pipeline → Verilog; `--emit-testbench` adds a self-checking `_tb.v`           |
| `mimz sim <file>`                | Event-driven simulation; `--cycles N`, `--in`, `--sweep`, `--trace`, `-o file.vcd` |
| `mimz test <file>`               | Runs all `test { tick/expect }` blocks; pass/fail per test                         |
| `mimz translate <file>`          | Keyword-flavor and word-order conversion; `--flavor`, `--order`                    |
| `mimz fmt <file>`                | Format a `.mimz` source file                                                       |
| `mimz eval <expr>`               | Evaluate a constant expression                                                     |
| `mimz lsp`                       | Start the LSP server (used by the VS Code extension)                               |
| `mimz --version`                 | Prints compiler version + language edition (three lines: codename banner first)    |

---

### Simulator (Phase 1.5)

- In-house event-driven cycle simulator written in Rust - no external tool at
  runtime.
- Supports clocked and combinational designs.
- `--in key=value` - set input signals; `--sweep` - enumerate all input
  combinations.
- `--trace` - print signal values every cycle.
- `-o file.vcd` - deterministic VCD waveform output (viewable in GTKWave).
- `mimz test` - runs `tick`/`expect` test blocks; exit 0 = all pass.
- **Icarus differential**: `our_simulator_matches_icarus_bit_for_bit` -
  every example's simulation output is byte-compared against Icarus Verilog in
  CI; the simulator is an Icarus-equivalent, not an approximation.

---

### Tooling and editor support

- **VS Code extension** (`editors/vscode/`) - syntax highlighting for `.mimz`;
  live diagnostics via `mimz lsp`.
- **LSP server** - `mimz lsp`; diagnostics-only for v0.1.0; hover/completion
  gated on Phase 4.
- **`mimz-bench`** - internal benchmark binary; measures speed, accuracy, safety
  coverage, and memory usage; outputs an HTML report (`bench-report.html`).
- **WASM wrapper** (`crates/mimz-wasm`) - `compile_string(source, imports)`
  binding for the browser playground (Phase 4 web presence); built separately
  (`cargo build -p mimz-wasm --target wasm32-unknown-unknown`).
- **Fuzz targets** (`fuzz/`) - four libFuzzer targets: lexer, parser, checker,
  translate round-trip; `translate_roundtrip` fuzz crash fixed (masked-int `?`
  byte glueing onto romanized identifiers).

---

### Examples and demos

- **23 example designs × 5 keyword folders**: `english/`, `tanglish/`, `tamil/`,
  `mixed/`, `tamil-pure/`.
- All four core-flavor folders produce **byte-identical Verilog** from every
  example (CI-asserted by `tests/examples.rs`).
- Every example validated by Icarus Verilog (lint + self-checking testbench).
- **`demo/`** - accumulator CPU showcase: `mimz check` → `mimz test` →
  `mimz sim` → VCD waveform; the canonical end-to-end demo.
- Designs shipped: adder, counter, ALU, traffic-light FSM, shift register,
  barrel shifter, comparator, mux, priority encoder, full adder, half adder,
  D flip-flop, JK flip-flop, SR latch, 7-segment decoder, PWM generator,
  memory controller, accumulator CPU, and more.

---

### Test suite

- **432 passing tests** across unit (lexer, parser, checker, emitter, morph,
  sim, translate, grammar-sync) and integration (examples, golden files, Icarus
  differential, fuzz corpus).
- **Golden-file pinning** - every example's Verilog output is byte-pinned in
  `tests/golden/`; any emitter regression is caught immediately.
- **`tests/fixtures/errors/`** - corpus of `.mimz` files that must produce a
  specific E-code; adding a checker code without a fixture fails CI.
- **`grammar_sync`** - asserts that `lang/keywords.toml`, `spec/03`, and the
  TextMate grammar are mutually consistent; no stale keyword spellings.
- **`docs_sync`** - asserts the test count in `docs/code/10-test-map.md` matches
  the actual suite.

---

### CI / Infrastructure

- **`ci.yml`** - `cargo fmt`, `cargo clippy -D warnings`, `cargo test`,
  `cargo audit` (supply-chain), `RUSTDOCFLAGS="-D warnings" cargo doc`,
  `prettier`, `markdownlint`; Icarus Verilog differential (`REQUIRE_IVERILOG=1`).
- **`release.yml`** - cross-platform native builds: Linux (musl static),
  Windows (MSVC), macOS Intel + Apple Silicon; SHA256SUMS; automated GitHub
  Release from `RELEASE_NOTES.md`.
- **`deploy-site.yml`** - Astro documentation site build + Vercel deploy.
- **`dependabot.yml`** - weekly Cargo + GitHub Actions dependency updates.
- All third-party Actions SHA-pinned; `contents: write` scoped to the release
  job only.
- Binaries are **unsigned** for v0.1.0 (code signing deferred); `UNSIGNED.txt`
  in each archive explains the one-time macOS/Windows allow step.

---

### Reserved keywords (growth doctrine - R11)

Keywords reserved pre-v0.1.0 so no valid v0.1.0 program can claim them:

`fn`, `function`, `interface`, `bundle`, `channel`, `prove`, `extern`, `fixed`,
`requires`, `ensures`, `secret`, `system_fault`, `unsafe`, `where`, `type`,
`impl`, `trait`, `use`, `pub`, `mod`, `struct`, `enum`.

---

### Notable fixes (pre-release)

- **Shift truncation** (`crates/mimz-sim/src/sim/value.rs`) - `Shl`/`Shr` now guard
  `if r >= 128 { 0 }` before the `as u32` cast; no silent wraparound.
- **Testbench panic** (`mimz compile --emit-testbench`) - stem-less `--output`
  path (e.g. `..`) now produces a clean error instead of a panic.
- **Partial output on testbench error** - testbench is generated before either
  file is written; a testbench error leaves no stray `.v`.
- **Fuzz crash** (`translate_roundtrip`) - `is_word_byte` now includes `?` so
  masked-int tokens (`0b1?`) don't glue onto romanized identifiers after
  round-trip.
- **`--emit-testbench` with no test blocks** - now prints a `note:` and writes
  only the `.v` instead of silently doing nothing.

---

_Built 2026-06-24 · © 2026 Naveen R · MIT + Apache-2.0_
