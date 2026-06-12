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

The triaged feature backlog from `docs/Ideas/language_plan.md` section 7
(tagged unions, interfaces/bundles, channels, `prove`/SymbiYosys, G5
security features, DX sugar) lives as work items in
**`docs/plan/phase-2-ir-synthesis.md` → "Language features"** — that list
is the single source of truth. Rejected ideas stay recorded with reasons
in the ideas doc itself (Tier 4: physics, not priorities).

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
