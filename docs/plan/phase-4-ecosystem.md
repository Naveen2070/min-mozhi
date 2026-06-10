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
- [ ] VS Code extension: syntax highlighting (all flavors + thamizh-order), inline diagnostics via LSP
- [ ] `minmo fmt` stabilized; `minmo translate` promoted in docs as the learning tool
- [ ] Package manager (`minmo add <pkg>`) — design doc first, Decision-logged

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
