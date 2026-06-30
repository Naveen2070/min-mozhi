# Architectural Ideas & Future Challenges

While the foundation of Min-Mozhi is incredibly solid, here are a few architectural challenges and improvements to consider as the language and tooling mature.

## 1. AST Error Recovery for the LSP

Language Servers (LSPs) need to provide diagnostics even when the code is broken (e.g., while the user is actively typing). If the parser halts on the first syntax error, the LSP experience degrades.

**Idea:** Upgrade the parser to support "Error Recovery" (generating an incomplete AST with `Error` nodes). This ensures that hover states, semantic highlighting, and autocomplete still work on half-written lines or files with syntax errors.

## 2. Fuzzing and Differential Testing

Because Min-Mozhi ships its own simulator _and_ a Verilog emitter, the project inherently has two sources of truth.

**Idea:** Build a Differential Testing suite in CI. This would involve a fuzzer that:

1. Generates random, valid Min-Mozhi code.
2. Runs it through the built-in Min-Mozhi simulator.
3. Compiles the same code to Verilog.
4. Runs the Verilog through a trusted, industry-standard simulator (like Verilator or Icarus Verilog).
5. Asserts that the VCD waveforms from both simulators are byte-for-byte identical.

## 3. Black-box / External IP Integration

Eventually, hardware engineers will need to instantiate primitives that Min-Mozhi can't express natively (e.g., FPGA-specific DSP slices, PLLs, or hardened PCIe IP).

**Idea:** Design a clean `extern module` system. This would allow users to instantiate raw Verilog black-boxes securely, mapping Verilog ports to Min-Mozhi types without breaking the safety checks and width-inference of the core compiler.

## 4. Keeping the Core Wasm-Friendly

Compiling to WebAssembly for the playground (via `crates/mimz-wasm`) is the absolute best way to teach the language without installation friction.

**Idea:** To keep the Wasm build viable and lightweight, ensure the core compiler architecture strictly isolates OS-level operations (like File I/O, multithreading, or environment variables) from the parsing, checking, and emitting logic. The core compiler library should remain perfectly pure: it should only ever take strings as input and return strings/ASTs as output.

## 5. Toolchain as Middleware (Modular Crates)

Currently, Min-Mozhi is built as a monolithic binary (`mimz`) for simplicity. However, as the ecosystem grows, the community might want to plug in custom tools (e.g., a custom simulator, an alternative language frontend, or an LLVM/CIRCT synthesis backend).

**Idea:** Refactor the compiler using Rust's Cargo Workspace model to split it into isolated "middleware" library crates (e.g., `mimz-parser`, `mimz-checker`, `mimz-sim`, `mimz-emit`). These crates would compile down to a single static binary for standard users, but could be swapped or extended individually by community developers. Crucially, the `mimz-checker` crate remains the mandatory gatekeeper in the pipeline, ensuring that swapping other components will never compromise Min-Mozhi's "Safe-by-Default" guarantees.

---

## 6. Feasibility triage (2026-06-22)

Reviewed each idea against the compiler as it exists today: 522 tests; the
lib/bin split; `crates/mimz-wasm` building the lib with `default-features =
false`; the `fuzz/` cargo-fuzz crate (4 targets); and the
`our_simulator_matches_icarus_bit_for_bit` + `wasm_parity` differential tests.
For each: what already exists, the gap, the tier, and where it lands.

**Bottom line for the v0.1.0 tag:** only idea 3 touches the freeze, and only its
_keyword reservation_ (not the feature). Everything else is already done (4),
already a documented later-phase trigger/plan (1, 2, 5), or both. These ideas do
**not** block the public release.

Status key: ✅ done/enforced · 🟢 partly shipped, clear path · 🔵 planned phase ·
🟡 freeze-sensitive (act before v0.1.0).

### Idea 1 — AST error recovery for the LSP → 🟢 partly shipped (Phase 4)

- **Have:** statement-level panic-mode recovery already exists
  (`Parser::sync_to_newline`, `src/parser/mod.rs`) — the parser reports more than
  one error per run and skips to the next newline / `}` instead of halting on the
  first. "Diagnostics on broken code" therefore already works at line
  granularity, and the LSP v0 (diagnostics-only) consumes it.
- **Gap:** recovery _drops_ the broken statement rather than leaving an `Error`
  placeholder node in the AST. Hover, semantic highlighting, and completion on a
  half-typed line need those placeholder nodes — and those LSP features
  themselves are not built yet (LSP is v0, diagnostics-only).
- **Verdict:** feasible, additive, edition-safe. Lands **with** the Phase 4 LSP
  hover/go-to-def/completion work (it is a prerequisite for them, not a
  standalone task). Not a v0.1.0 blocker.

### Idea 2 — Fuzzing + differential testing → 🟢 two of three legs shipped (Phase 4)

- **Have (leg 1 — robustness fuzzing):** `fuzz/` is a cargo-fuzz crate with four
  targets — `lex_parse_eval`, `lex_parse_compile`, `pretty_roundtrip`,
  `translate_roundtrip` — asserting the untrusted-input path never panics.
  (Detached workspace; libFuzzer = nightly/Linux, runs in the `fuzz` CI job.)
- **Have (leg 2 — differential oracle):**
  `our_simulator_matches_icarus_bit_for_bit` (~21 examples, `tests/icarus.rs`)
  already compares our simulator's VCD against Icarus on the **fixed** example
  corpus; `wasm_parity` checks WASM-vs-native parity.
- **Gap (leg 3 — the generative half):** a generator that emits **random valid**
  Min-Mozhi feeding the leg-2 oracle. The hard, valuable part is a
  _valid-by-construction_ program generator (random tokens only exercise the
  leg-1 fuzzers).
- **Verdict:** feasible, substantial. Phase 4 / post-launch. Sequence: build a
  typed-AST generator (respects width/driver/clock rules so output is valid) →
  wire it to the existing sim-vs-Icarus comparator → gate in the `fuzz` CI job.
  Not a v0.1.0 blocker.

### Idea 3 — `extern module` / external-IP black-box → 🟡 design later, RESERVE THE KEYWORD NOW

- **State:** not reserved (`extern` is absent from `lang/keywords.toml`
  `reserved`); this is the architecture.md open question "External Verilog module
  wrapping construct (Phase 2+)".
- **Feasibility:** a real language feature — new keyword + grammar (a port map
  with declared widths) + a checker rule that **trusts the declared port types as
  boundary axioms** (so width-inference and the safety passes stay sound across
  the black box) + emitter passthrough (instantiate, do not render the body).
  Honest fit: the black box is unchecked Verilog, so the checker treats its ports
  as a typed contract and stops there. Moderate; Phase 2+.
- **FREEZE IMPACT:** the _feature_ is additive (edition-safe, can land
  post-1.0), but the _keyword_ must be reserved **before v0.1.0** — otherwise a
  v0.1 program using `extern` as an identifier makes the later keyword a breaking
  change (R11 + the growth doctrine). This is the one item here that touches the
  freeze. Action is on the freeze checklist
  (`docs/Ideas/language_plan.md` section 9) and in "what to do next".

### Idea 4 — keep the core Wasm-friendly → ✅ already enforced; codified as an invariant

- **State:** already true. All OS-coupled code (`std::fs`/`env`/`process`,
  `tokio`) lives only in the CLI shell — `src/commands/`, `main.rs`,
  `config.rs`, `project.rs`, `lsp.rs`, `src/bin/`. The core stages (`lexer`,
  `parser`, `checker`, `emit_verilog`, `sim`, `ast`) are string → string/AST
  pure. `crates/mimz-wasm` builds the lib with `default-features = false`,
  dropping `lsp` + `bench` (tokio, memory-stats — they do not target wasm32);
  `wasm_parity` proves the browser pipeline matches native.
- **Verdict:** no work needed for v0.1.0. The discipline was implicit; it is now
  a cross-cutting invariant in `docs/architecture.md` (the purity boundary), with
  an optional CI guard noted in "what to do next".

### Idea 5 — toolchain as modular crates → 🔵 already a documented trigger; do NOT do pre-1.0

- **State:** the workspace skeleton already exists (root lib + `crates/mimz-wasm`
  = a 2-member workspace). `docs/architecture.md` already lists this as a
  trigger-based evolution: "Planned crate split:
  `mimz-syntax`/`mimz-check`/`mimz-backends`/`mimz`". The proposed
  `mimz-parser`/`-checker`/`-sim`/`-emit` split is a finer-grained version of the
  same move.
- **Verdict:** mechanically high-feasibility (Rust workspace; the wasm crate
  proves the pattern), but **premature** under the project's own "dumb first,
  split on trigger" doctrine — there is no community plugin consumer today and
  compile time is fine. Keep `mimz-checker` as the named mandatory gatekeeper
  when the split happens. Defer; explicitly **not** a pre-1.0 task.
