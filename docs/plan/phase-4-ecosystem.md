# Phase 4 — Ecosystem

> **Make it usable by others.**
> Window: ongoing (starts once Phase 1 ships) · Status: ⚪ not started

## Goal

Min-Mozhi becomes a community language with real users — students, hobbyists,
and the Tamil Nadu VLSI ecosystem.

## Work items

### Standard library

- [ ] Core modules in Min-Mozhi itself: UART, SPI, PWM, debouncer, FIFO, ALU, 7-segment driver
- [ ] Each stdlib module: trilingual doc page + testbench + waveform screenshot

### Tooling

- [ ] VS Code extension: syntax highlighting (all flavors + thamizh-order), inline diagnostics via LSP — **LSP v0 (diagnostics only) pulled into Phase 1** (Decision 2026-06-12); this phase adds hover, go-to-definition, completion, and `translate` integration on top
- [ ] `mimz fmt` stabilized; `mimz translate` promoted in docs as the learning tool
- [ ] Package manager (`mimz add <pkg>`) — design doc first, Decision-logged

### Ecosystem drivers (one core, thin wrappers — Decision 2026-06-11)

- [ ] **WASM build + browser playground** — FIRST bridge to other
      ecosystems: no toolchain, no install rights needed in a college
      lab, just a URL. Highest education-per-hour; serves the spec/01
      persona directly. Needs the simulator (Phase 1.5) to be a real
      playground, not just a Verilog printer.
- [ ] **Interactive hardware REPL `mimz repl`** (idea 8.5,
      `language_plan.md` section 9) — define an expression/gate, flip
      inputs, see combinational logic evaluate live. No new syntax: rides
      the WASM playground above + the Phase 1.5 sim evaluator. Scope to
      combinational logic.
- [ ] **Vim-like TUI workbench `mimz tui`** (idea 8.11,
      `language_plan.md` section 8) — a no-IDE, full-screen terminal driver
      for whole `.mimz` files: on start it asks the output mode (emit
      Verilog / run + log / waveform), then edits + re-runs on save with
      inline diagnostics, `test` results, a `$monitor` trace, and an
      optional VCD. The broader sibling of `mimz repl` (8.5): clocked sim + waveforms + emit, not just combinational expressions. Tool, not
      syntax — rides the Phase 1.5 sim (`src/sim`), the emitter, and the
      checker's diagnostics; pairs with the WASM playground (same engines,
      different shell). A TUI crate (e.g. `ratatui`) would be the first UI
      dependency — weigh against the minimal-dep ethos; MVP is the
      output-mode prompt + a re-run-on-save loop.
- [ ] **npm wrapper package** (esbuild model: tiny package that fetches
      the platform binary / loads the WASM and shells out) — TS/JS devs
      run `mimz` in their build like any other tool
- [ ] **PyPI wrapper package** (same model)
- [ ] Go / Java / Kotlin / etc. wrappers — only on demonstrated demand;
      each is ~100 lines around the same binary, never a reimplementation
- [ ] Prerequisite carried by Phase 1 work: keep the compiler core
      embeddable (lib/bin split so `project.rs` printing stays in the
      CLI shell) + a `--json` diagnostics flag for tool consumers —
      fold into the lexer/parser E-code retrofit

### Language-feature backlog (pointer)

The triaged feature backlog from `docs/Ideas/language_plan.md` sections 7
and 9 (tagged unions, interfaces/bundles, channels, `prove`/SymbiYosys, G5
security features, DX sugar, plus the section-8 additive ideas —
`fixed`-point, `requires`/`ensures` contracts, `..` spread/struct-update,
pipe `|>`, didactic errors) lives as work items in
**`docs/plan/phase-2-ir-synthesis.md` → "Language features"** — that list
is the single source of truth. The hardware REPL (8.5) and the `mimz tui`
workbench (8.11) are the section-8 items that land in this phase (above) —
both are tools, not syntax, so they carry no freeze cost. Rejected ideas
stay recorded with reasons in the ideas doc itself (Tier 4: physics, not
priorities).

### Documentation & learning

- [ ] Documentation site — English first; Tamil translation of docs begins **after Phase 1** (decision D9)
- [ ] "Day one" tutorial: counter on a real board in under an hour — in Tamil, Tanglish, and English
- [ ] Example gallery grown from community submissions

### Community

- [ ] Tamil Nadu outreach: engineering colleges, polytechnics, VLSI meetups
- [ ] Contribution guide + code of conduct; keyword-table change process opened to community (per `docs/RULES.md` R6)
- [ ] Talks/posts timed with India Semiconductor Mission news cycle

## Milestone

First external contributor PR merged; first classroom/workshop uses Min-Mozhi.

## Exit criteria (rolling)

- ≥10 stdlib modules, all tested on hardware
- VS Code extension published
- Docs site live in all three flavors

## Risks / notes

- Start the VS Code syntax highlighting early (it's cheap and high-visibility) —
  it can ship right after Phase 1 even though it lives in this phase.
