# 10 — The Ecosystem: Benchmarks, Fuzzing, Tests, CI, Examples, Spec & Site

Everything around the compiler that keeps it healthy.

## `src/bin/mimz-bench/` — The Benchmark Harness

This is a REPO tool, not a user-facing `mimz` subcommand. It measures the compiler against the entire example and test corpus and produces detailed reports with trend charts.

```
cargo run --release --bin mimz-bench
```

It measures four sections:

1. **Speed** — compiles every example file in all five flavors, timing each phase (lex, parse, check, emit) separately. Reports median milliseconds and lines-per-second.

2. **Accuracy** — three layers of checking:
   - **Golden files**: the emitted Verilog must match the stored golden outputs byte-for-byte
   - **Flavor identity**: translating to English, Tanglish, and Tamil and back must produce byte-identical output
   - **Icarus Verilog**: the emitted Verilog must compile and simulate correctly under `iverilog` (a real Verilog tool)

3. **Safety** — every error fixture in `tests/fixtures/errors/` must fire its expected E-code. Every diagnostic must carry a help line. Every example must check clean (no false positives).

4. **Coverage** — what percentage of checker E-codes have a test fixture, plus optional `cargo-llvm-cov` line/function coverage.

The output is `bench-report.html` (interactive Chart.js graphs) and `bench-report.json` (machine-readable). History is tracked in `bench-history.jsonl`.

**`html.rs`** — renders the HTML report with trend charts, metric cards, and failure lists.

**`metrics/`** — five submodules:

- **`speed.rs`** — per-phase timing using `std::time::Instant`
- **`accuracy.rs`** — golden file comparison, flavor identity tests, Icarus Verilog integration
- **`safety.rs`** — error fixture sweep, help line audit, false positive check
- **`memory.rs`** — peak RSS measurement over the full corpus
- **`coverage.rs`** — E-code coverage + `cargo-llvm-cov` integration

## `benches/compile.rs` — Criterion Micro-Benchmarks

This is a **separate** harness from `mimz-bench`. It uses `criterion` to benchmark each compiler phase _in isolation_ on a single representative example (the `traffic_light.mimz` FSM).

```
cargo bench
```

It measures:

- **Lexer** — raw source to tokens
- **Parser** — tokens to AST (clones a fresh token vec each iteration)
- **Checker** — AST through all six safety passes
- **Emitter** — AST to Verilog text (clones the AST each iteration for clean state)

Criterion's statistical warmup and outlier detection catch regressions that a single measurement wouldn't spot.

## `fuzz/` — Four Fuzzing Targets

The fuzz harness uses `libFuzzer` (via `cargo fuzz`) to throw random byte strings at the compiler and check that nothing crashes. Only runs on Linux/macOS (not Windows) with a nightly Rust toolchain.

```
cargo +nightly fuzz run lex_parse_eval -- -max_total_time=30
```

**`lex_parse_eval`** — for any valid UTF-8 input, runs lex → parse → `comb::eval_outputs`. First with empty inputs (exercising constant folding, width evaluation, slice bounds — the SEC-2 path), then with real input ports fed deterministic values derived from the fuzz bytes. Also tests edge-case values like `0`, `u128::MAX`, and values at bit-boundaries that previously triggered truncation bugs.

**`lex_parse_compile`** — runs the full Verilog backend path: lex → parse → check → transliterate → build project → emit. Only crashes are findings — any valid input that makes it through the checker must emit without panicking.

**`pretty_roundtrip`** — checks the pretty-printer for round-trip safety. Any parseable program, when pretty-printed back to source, must (1) re-lex and re-parse, and (2) for emittable programs, produce byte-identical Verilog.

**`translate_roundtrip`** — checks the keyword reskin for crash- AND round-trip safety. Translating to any flavor and romanizing identifiers must produce re-lexable output, and restoring via the name map must be token-equivalent to the plain reskin.

The fuzz crate has its own `[workspace]` to detach from the parent — normal `cargo build` at the root doesn't touch it.

## `tools/test-summary/` — A Fancy `cargo test` Wrapper

This is a standalone dev helper that runs the full test suite and prints a per-binary breakdown:

```
cargo test-summary [args]
```

It's registered as a cargo alias in `.cargo/config.toml`. The output looks like:

```
================ test summary ================
  lib (unit)                     142 passed
  errors (integration)            18 passed
  sim (integration)               12 passed
  translate (integration)          5 passed
  doctests                         0 passed
  TOTAL                          177 passed
===============================================
```

**`main.rs`** — spawns `cargo test` with piped stdout/stderr. It pairs each `Running …` / `Doc-tests …` descriptor (from stderr) with the next `test result:` line (from stdout), labels them nicely, and prints the summary.

Cross-platform, pure std — no extra dependencies.

## `.github/workflows/` — CI That Actually Gates

Three workflows keep the repo healthy:

**`ci.yml`** — runs on every push and PR. Six jobs:

1. **fmt + clippy + test** — `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo doc` (warnings as errors), `cargo test --all-targets` (with `REQUIRE_IVERILOG=1` to enforce differential testing), `cargo bench --no-run` (type-checks the benchmark harness)
2. **mimz-bench correctness gate** — runs `mimz-bench --no-cov` to validate accuracy + safety (goldens match, error fixtures fire, no false positives)
3. **WASM build** — builds `mimz-wasm` for the `wasm32-unknown-unknown` target
4. **Fuzz** — runs `cargo +nightly fuzz` for a short smoke test
5. **Nightly bench** — a full `mimz-bench` run (with coverage), triggered by cron; commits the history to the repo
6. **Fuzz nightly** — a longer fuzz run, triggered by a separate weekly cron

**`deploy-site.yml`** — builds the Astro documentation site and deploys to Vercel on pushes to main.

**`release.yml`** — builds the VS Code `.vsix`, the WASM package, and creates a GitHub Release with all artifacts when a tag is pushed.

## `tests/` — Making Sure Everything Works (18 Test Files)

The test suite is thorough:

- **`errors.rs`** — for every E-code in `ALL_CHECKER_CODES`, loads the corresponding fixture from `tests/fixtures/errors/` and asserts it produces exactly that code
- **`examples.rs`** — loads every example in all five flavor directories and runs check + compile, verifying they produce no errors
- **`sim.rs`** — runs the simulator against various designs and checks output values
- **`eval.rs`** — tests combinational evaluation
- **`translate.rs`** — round-trip keyword reskin tests
- **`fmt.rs`** — formatter tests
- **`grammar.rs`** — EBNF grammar conformance tests
- **`grammar_sync.rs`** — checks the TextMate grammar stays in sync with `keywords.toml`
- **`docs_sync.rs`** — checks doc tables match code
- **`icarus.rs`** — differential testing against Icarus Verilog (the emitted Verilog must simulate correctly)
- **`morph.rs`** — error language / inflection tests
- **`config.rs`** — `mimz.toml` discovery and parsing
- **`compile_string.rs`** — library API tests
- **`test_run.rs`** — test block execution
- **`stdlib.rs`** — importable `std.*` library: embedded resolution, trilingual alias routing, eject
- **`cli.rs`** — CLI surface tests: `mimz init` scaffolding, `doctor` environment checks, `completions <shell>` generation
- **`lsp.rs`** — LSP server tests
- **`wasm_parity.rs`** — checks that the WASM commands produce the same output as the native CLI

**Fixtures:**

- `tests/fixtures/errors/` — 84 `.mimz` files, one per error code (44 checker + additional parser/lexer)
- `tests/fixtures/grammar/` — 8 grammar conformance examples
- `tests/golden/` — 50 golden Verilog outputs + 14 testbench goldens + 1 VCD trace
- `tests/icarus/` — 38 Icarus Verilog testbenches

## `examples/` — Designs in All Five Flavors

The `examples/` directory has the same 34 designs (plus 5 stdlib modules and 1 lib module) in **four** keyword flavors — English, Tanglish, Tamil, and mixed — plus a **fifth** `tamil-pure/` showcase with Tamil keywords AND identifiers. Think of it as the compiler's "hello world" collection showing that every keyword flavor works identically.

Designs include: adders, counters, FSMs (traffic light, blinker), comparators, multiplexers, shift registers, memories, stdlib modules (seg7, PWM, FIFO, UART, debouncer), and more.

Each flavor directory also has `lib/full_adder.mimz` and `std/` — demonstrating `import` with shared library and stdlib modules.

## `demo/` — Real Hardware Demos

Two real designs that show Min-Mozhi in action:

**`alu.mimz`** — a tiny 8-bit ALU with four operations (add, subtract, AND, OR) selected by a 2-bit `op` signal. Clean `match` expression, no clock.

**`cpu.mimz`** — a more substantial CPU design. Demonstrates module composition — the ALU is instantiated as a child module. This is the "real hardware" proof that Min-Mozhi can express meaningful digital systems.

## `lang/` — The Language Data Files (Not Code, But Essential)

These TOML files are the project's **authoritative data**. The native-speaker panel edits these, never the Rust code.

**`keywords.toml`** — every keyword has three spellings (English, Tanglish, Tamil) plus optional alias lists per column. A `version` field at the root is cross-checked against `version.rs`. Reserved words for future features are listed at the bottom.

**`messages.toml`** — localized error templates for 33 of 44 checker E-codes (coverage growing), in both Tamil and Tanglish. Each template uses `{name}`, `{name.acc}`, `{name.dat}`, etc. for identifier interpolation.

**`case_suffixes.toml`** — the four Tamil case suffixes (accusative, dative, locative, instrumental) in both Tamil script and Tanglish romanization.

## `spec/` — The Language Specification

Seven markdown files that define **exactly** what Min-Mozhi is:

- **`01-goals-and-philosophy.md`** — why the language exists, its design values
- **`02-syntax-and-grammar.md`** — the full EBNF grammar
- **`03-keywords-trilingual.md`** — the keyword table design
- **`04-grammar-engine.md`** — how the code-order/thamizh-order system works
- **`05-simulator.md`** — the simulator's design
- **`06-editions.md`** — the language edition system
- **`README.md`** — spec overview

If there's ever a debate about how something should work, the spec wins. The spec is the truth.

## `site/` — The Documentation Website

An **Astro** static site that serves as the project's documentation hub. It lives at the project's Vercel deployment.

It includes:

- Interactive WASM playground (loads `mimz-wasm` to compile code in the browser)
- Full language guide
- API docs
- Live examples

Built with Astro + React for interactive components. The WASM playground is an Astro island that loads the compiled `.wasm` glue and provides an editor + console interface.

## Root Configuration Files

A few important ones at the project root:

**`Cargo.toml`** — the workspace root. Three interesting details:

- Feature flags: `default = ["lsp", "bench", "watch", "hw-emulation"]`. These
  CLI-only deps (tokio/tower-lsp/memory-stats/ratatui/crossterm/cpal) live
  only in the root shell crate — `mimz-core`/`mimz-sim` have no optional
  deps at all, so `mimz-wasm` depends on `mimz-sim` directly with no
  `default-features = false` needed
- `overflow-checks = true` in release — defense in depth, so any missed overflow aborts loudly instead of silently producing wrong hardware
- Workspace members: `["crates/mimz-core", "crates/mimz-sim", "crates/mimz-wasm"]`; `default-members = ["."]` (just the shell crate, for fast local iteration — CI passes `--workspace` explicitly), excludes `["tools/test-summary"]`

**`.github/workflows/`** — CI/CD as described above.

**`.editorconfig`** — consistent indentation across editors.
**`.markdownlint-cli2.jsonc`** — markdown linting rules.
**`.prettierrc`** / **`.prettierignore`** — Prettier config for non-Rust files.

## Design Principles (A Quick Recap)

1. **No unsafe code** — `#![forbid(unsafe_code)]` everywhere
2. **One AST** — flavor-blind and word-order-blind; the lexer and parser absorb all surface variation
3. **Stack overflow protection** — parser, elaborator, and emitter all have depth/budget limits
4. **Multi-error reporting** — no pass stops at the first error; all diagnostics are collected
5. **Stable E-codes** — error codes are never renumbered; `mimz explain` covers them permanently
6. **Teaching errors** — every error has a help line; long-form explanations live in `explain.rs`
7. **Shared expression semantics** — `sim/value.rs` has the single evaluator used by both the comb evaluator and the event-driven kernel
8. **Dumb emitter** — Verilog emission is deliberately naive, preserving symbolic widths and parenthesizing everything for correctness
