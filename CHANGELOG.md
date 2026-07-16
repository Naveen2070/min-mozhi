# Changelog

All notable changes to **Min-Mozhi (மின்மொழி)**. The project has **two version
axes** (see [`spec/06-editions.md`](spec/06-editions.md)):

- **Compiler version** — the `mimz` binary / crate version (`Cargo.toml`).
- **Language edition** — a codename + year + serial (`wingless-butterfly-2026-1`).
  Surfaced by `mimz --version`, in every emitted Verilog header, and here.

Format follows [Keep a Changelog](https://keepachangelog.com).
Compiler versions follow [SemVer](https://semver.org).

---

## [Unreleased]

### Added

- `mimz test --emulate`: `sim` blocks inside `test` blocks bind ports to
  virtual peripherals (`led`, `uart_tx`, `uart_rx`, `speaker`) with
  real-time throttling. `uart_tx`/`uart_rx` decode/encode 8-N-1 serial to
  a dashboard log and/or a local TCP socket; `speaker` plays the bound
  bit as a tone on the host's audio output (`cpal`). Opt-in, off by
  default, auto-degrades outside a real terminal.
- `return` statement and statement-based `fn` bodies (`if`/`return`/`let`)
  for guard-clause-style combinational functions (priority-selected result
  selection, not a silicon early-exit — every branch is still fully
  instantiated). New keyword `return`/`thirumbu`/`திரும்பு`. New diagnostic
  E0812 (unreachable code after `return`). Fully backward compatible with
  existing `fn` bodies.
- Array-typed `fn` parameters (`bits[8][4]`-style fixed-size, immutable
  arrays) and array literals (`[e1, ..., eN]`). An array is never real
  Verilog hardware — it elaborates to N independent scalar signals,
  matching how `repeat` already elaborates to N copies of hardware.
  Indexing with a compile-time-constant folds directly; a runtime index
  generates a priority-mux over the elements. New diagnostics E0411-E0415.
- `foreach <var> in <source> { ... }` sugar over `repeat`/`loop`: a range
  form (`foreach i in lo..hi`) and an array/mem-element form (`foreach v
in values`, its bound taken from the source's own declared length, never
  hand-written). Desugars to the existing `repeat`/`loop` machinery before
  the checker/emitter/simulator ever see it — no new codegen. New
  diagnostic E0417 (elements-form source must be array/mem-typed). New
  keyword `foreach`; the Tanglish/Tamil spellings (`ovvondraga`/
  ஒவ்வொன்றாக) are provisional, pending native-speaker review.
- Bundle-typed `fn` parameters and return values are now shape-checked
  (previously silently accepted/typed as `Unknown`): a bundle-shaped call
  argument or return value is validated against the bundle's declared
  fields, reusing the same `E0901`-family diagnostics as module-level
  bundle drives. Underlying checker fix: bundle-typed values now carry a
  real `Ty::Bundle` (nominal identity + on-demand field resolution)
  instead of falling through to `Ty::Unknown`, replacing the old
  `Wcx::bundle_sigs` side-table this had relied on.
- `Enum.Variant(arg1, arg2, ...)` construction syntax — the write-side
  counterpart to tagged-union `match`, completing that feature. Positional
  arguments only, in the variant's declared field order; a tag-only
  variant is constructed `Enum.Variant()`. Lowers to the same tag+payload
  bit layout `match` already extracts, on both the Verilog emitter and the
  simulator. No new diagnostics — reuses E0806 (arity), E0401 (width), and
  E0103 (unknown enum/variant), generalized to cover both call sites.
- `extern module Name(params) { doc: "...", ports }` — Verilog FFI: declares
  the port shape of a real, hand-written Verilog module (vendor IP, a
  hardened PLL, a protocol core) without defining its body, instantiable
  with zero new syntax through the existing `let u = Name(...) { conns }`
  form. Optional `= "RealName"` alias when the mimz-facing name differs
  from the real Verilog module's name; optional `doc: "..."` note. Ports
  are scalar-only (`bit`/`bits[N]`/`signed[N]`, plus `clock`/`reset`) — new
  diagnostics E1301 (duplicate extern module name) and E1302 (non-scalar
  extern port). The emitter instantiates the real module by name and never
  emits a definition for it. The simulator, which cannot execute real
  Verilog, taints an extern instance's outputs `unknown` in the new default
  `warn` sim mode (a `test`/`expect` against a tainted value still fails)
  or hard-errors immediately in `strict` mode; select the mode via
  `mimz.toml`'s top-level `extern_sim` field. Companion `.v` files reach
  the toolchain via `mimz.toml [compile] verilog_files` and the repeatable
  `--extern-src` CLI flag, which union additively.
- Structural bundle matching (feature 2.9): a bundle satisfies any
  bundle-typed slot whose required fields it covers with exactly-matching
  types, regardless of the two bundles' declared names — applies to `let`
  bindings, `Drive` assignments, module-instantiation port connections, and
  `fn` bundle-typed args/returns. Extra fields on the provided side are
  allowed; shared fields never coerce width. New diagnostic E0910 (a
  required field is missing entirely); E0907 now describes a structural
  field-type mismatch instead of a purely nominal one. Also fixes a
  pre-existing bug where a bundle-typed port connected across a module
  instantiation emitted broken (non-flattened) Verilog.

---

## [0.1.0] — 2026-06-24 · Language edition: Wingless Butterfly `wingless-butterfly-2026-1`

The first public release. Phases 0, 1, 1.8, and 1.5 complete.
Keyword set v1 frozen 2026-06-15. 432 passing tests.

### Language — Core

#### Types and signals

- `wire` — combinational signal driven by `=` assignments; inferred-latch guard
  (unwired `wire` is a compile-time error `E0201`).
- `reg` — clocked state element driven by `<-` inside `on rise`/`on fall` blocks;
  mandatory reset value (no reset = `E0301`).
- `bits[N]` — unsigned integer of exactly `N` bits.
- `signed[N]` — two's-complement signed integer of exactly `N` bits; emitted as
  Verilog `wire signed` / `reg signed`.
- `int` — constant / parameter integer (not a signal type).
- `bool` — single-bit boolean (`true`/`false`).
- `clock` — dedicated clock-port type; drives `on rise`/`on fall` only.
- `reset` — dedicated synchronous-reset type; active-high, one reset per module.
- `async reset` — asynchronous reset variant; widens the always-block to
  `@(posedge clk or posedge rst)`; active-high only.
- `in`, `out`, `inout` — port directions.
- `mem m: bits[W][DEPTH] = init` — addressable register array; combinational
  indexed read, clocked indexed write, power-on initialiser.

#### Operators and arithmetic

- Lossless arithmetic: `+` / `-` / `*` grow width to hold the result; no silent
  truncation (`E0401`).
- Wrapping family: `+%` / `-%` / `*%` — explicit saturating/wrapping ops for
  when truncation is intended (emitted as Verilog `+`/`-`/`*` with the correct
  width).
- Bitwise: `&`, `|`, `^`, `~`.
- Shift: `<<`, `>>` (logical; `>>>` arithmetic).
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=` (always returns `bool`).
- Concatenation: `{a, b, c}`.
- Replication: `{N{x}}` — the inner group repeated `N` times (Verilog `{N{x}}`).
- Bit-select: `x[i]`; slice: `x[hi:lo]`.
- Signed/unsigned guard: mixing `bits` and `signed` without an explicit cast is
  `E0402`.

#### Control flow

- `if <cond> { … } else { … }` — expression-oriented; mandatory `else` when
  driving a wire (`E0501`).
- `match <expr> { pattern => expr, … }` — exhaustive by default (`E0502`);
  don't-care patterns `0b1??` (binary only, this edition) map to Verilog `casez`.
- `on rise(clk) { … }` — rising-edge clocked block.
- `on fall(clk) { … }` — falling-edge clocked block (negedge sibling).

#### Modules and instantiation

- `module Name(PARAM: type = default) { … }` — parameterised module.
- Port and wire declarations inside the module body.
- Module instantiation with named ports.
- Cross-file instantiation via `load` (no C-style preprocessor).

#### Constants and parameters

- `const NAME: type = value` — compile-time constant.
- Module parameters resolved at instantiation.

#### Testing

- `test "name" { tick { … } expect { … } }` — inline test blocks compiled by
  `mimz test`; `tick` sets inputs, `expect` asserts outputs.

---

### Language — Safety Rules (compile-time, stable E-codes)

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

- **Three keyword skins over one grammar**: English, Tanglish, Tamil — freely
  mixable within a single file; identical semantics.
- **Keyword set v1 frozen 2026-06-15** — English column immutable from this
  point; Tanglish/Tamil columns ratified after native-speaker panel review (C3).
- `mimz translate --flavor <english|tanglish|tamil|mixed>` — lossless,
  round-trip keyword conversion; preserves identifiers and formatting.
- `mimz translate --order <code|thamizh>` — converts between SVO (code-order)
  and SOV (Tamil natural word order).
- Native Tamil/Tanglish error messages — `lang/messages.toml`; 33 of 36
  diagnostic codes have native-authored translations; structured-arg
  interpolation (signal names inflected with Tamil case suffixes via
  `lang/case_suffixes.toml`).

---

### Grammar Engine (Phase 1.8) — `thamizh-order`

Natural Tamil SOV word order — the postpositional clause forms that make Min-Mozhi
code read like Tamil, not transliterated English:

- `<cond> enil { }` — if-expression flip (condition-first → `enil`).
- `yetram(clk) pothu { }` — clocked-block flip.
- `<expr> thernthedu { }` — match-expression flip.
- File-level `syntax thamizh` directive — activates the SOV parser profile;
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
| `mimz --version`                 | Prints compiler version + language edition on two lines                            |

---

### Simulator (Phase 1.5)

- In-house event-driven cycle simulator written in Rust — no external tool at
  runtime.
- Supports clocked and combinational designs.
- `--in key=value` — set input signals; `--sweep` — enumerate all input
  combinations.
- `--trace` — print signal values every cycle.
- `-o file.vcd` — deterministic VCD waveform output (viewable in GTKWave).
- `mimz test` — runs `tick`/`expect` test blocks; exit 0 = all pass.
- **Icarus differential**: `our_simulator_matches_icarus_bit_for_bit` —
  every example's simulation output is byte-compared against Icarus Verilog in
  CI; the simulator is an Icarus-equivalent, not an approximation.

---

### Tooling and editor support

- **VS Code extension** (`editors/vscode/`) — syntax highlighting for `.mimz`;
  live diagnostics via `mimz lsp`.
- **LSP server** — `mimz lsp`; diagnostics-only for v0.1.0; hover/completion
  gated on Phase 4.
- **`mimz-bench`** — internal benchmark binary; measures speed, accuracy, safety
  coverage, and memory usage; outputs an HTML report (`bench-report.html`).
- **WASM wrapper** (`crates/mimz-wasm`) — `compile_string(source, imports)`
  binding for the browser playground (Phase 4 web presence); built separately
  (`cargo build -p mimz-wasm --target wasm32-unknown-unknown`).
- **Fuzz targets** (`fuzz/`) — four libFuzzer targets: lexer, parser, checker,
  translate round-trip; `translate_roundtrip` fuzz crash fixed (masked-int `?`
  byte glueing onto romanized identifiers).

---

### Examples and demos

- **23 example designs × 5 keyword folders**: `english/`, `tanglish/`, `tamil/`,
  `mixed/`, `tamil-pure/`.
- All four core-flavor folders produce **byte-identical Verilog** from every
  example (CI-asserted by `tests/examples.rs`).
- Every example validated by Icarus Verilog (lint + self-checking testbench).
- **`demo/`** — accumulator CPU showcase: `mimz check` → `mimz test` →
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
- **Golden-file pinning** — every example's Verilog output is byte-pinned in
  `tests/golden/`; any emitter regression is caught immediately.
- **`tests/fixtures/errors/`** — corpus of `.mimz` files that must produce a
  specific E-code; adding a checker code without a fixture fails CI.
- **`grammar_sync`** — asserts that `lang/keywords.toml`, `spec/03`, and the
  TextMate grammar are mutually consistent; no stale keyword spellings.
- **`docs_sync`** — asserts the test count in `docs/code/10-test-map.md` matches
  the actual suite.

---

### CI / Infrastructure

- **`ci.yml`** — `cargo fmt`, `cargo clippy -D warnings`, `cargo test`,
  `cargo audit` (supply-chain), `RUSTDOCFLAGS="-D warnings" cargo doc`,
  `prettier`, `markdownlint`; Icarus Verilog differential (`REQUIRE_IVERILOG=1`).
- **`release.yml`** — cross-platform native builds: Linux (musl static),
  Windows (MSVC), macOS Intel + Apple Silicon; SHA256SUMS; automated GitHub
  Release from `RELEASE_NOTES.md`.
- **`deploy-site.yml`** — Astro documentation site build + Vercel deploy.
- **`dependabot.yml`** — weekly Cargo + GitHub Actions dependency updates.
- All third-party Actions SHA-pinned; `contents: write` scoped to the release
  job only.
- Binaries are **unsigned** for v0.1.0 (code signing deferred); `UNSIGNED.txt`
  in each archive explains the one-time macOS/Windows allow step.

---

### Reserved keywords (growth doctrine — R11)

Keywords reserved pre-v0.1.0 so no valid v0.1.0 program can claim them:

`fn`, `function`, `interface`, `bundle`, `channel`, `prove`, `extern`, `fixed`,
`requires`, `ensures`, `secret`, `system_fault`, `unsafe`, `where`, `type`,
`impl`, `trait`, `use`, `pub`, `mod`, `struct`, `enum`.

---

### Notable fixes (pre-release)

- **Shift truncation** (`src/sim/value.rs`) — `Shl`/`Shr` now guard
  `if r >= 128 { 0 }` before the `as u32` cast; no silent wraparound.
- **Testbench panic** (`mimz compile --emit-testbench`) — stem-less `--output`
  path (e.g. `..`) now produces a clean error instead of a panic.
- **Partial output on testbench error** — testbench is generated before either
  file is written; a testbench error leaves no stray `.v`.
- **Fuzz crash** (`translate_roundtrip`) — `is_word_byte` now includes `?` so
  masked-int tokens (`0b1?`) don't glue onto romanized identifiers after
  round-trip.
- **`--emit-testbench` with no test blocks** — now prints a `note:` and writes
  only the `.v` instead of silently doing nothing.

---

_Built 2026-06-24 · © 2026 Naveen R · MIT + Apache-2.0_
