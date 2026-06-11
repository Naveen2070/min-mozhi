# Min-Mozhi (மின்மொழி) — Roadmap & Phases

> **"Language of Electricity"** — The first Tamil-rooted Hardware Description Language
>
> ℹ️ This file is the **summary**. Detailed, task-level plans live in
> [`docs/plan/`](docs/plan/) (source of truth — see [`docs/RULES.md`](docs/RULES.md) R1/R2).
> Progress and decisions are logged in [`docs/log/`](docs/log/).

---

## Phase 0 — Foundation _(1–2 months)_

> Design before you code

- Define language goals and philosophy ✅ (`spec/01`)
- Design the syntax and grammar (on paper first) ✅ (`spec/02`)
- Write BNF/EBNF grammar spec ✅ (`spec/02` section 5)
- Design the trilingual keyword system (English/Tanglish/Tamil skins) ✅ (`spec/03`)
- Sketch the Grammar Engine for natural Tamil word order ✅ (`spec/04`)
- Study Verilog internals deeply — plus modern HDLs: **Veryl, Spade, Amaranth, Chisel**
- Set up GitHub repo, README, logo
- Compiler implementation language: **Rust** (decided — see below)

### Why Rust (decision record)

| Reason                         | Detail                                                                                                                                         |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| No Phase-2 rewrite             | The simulator (1.5) and synthesis (2) are performance-critical graph engines; Python would force a rewrite exactly when the project is busiest |
| Zero-friction install          | One static `mimz.exe` / binary — no Python/pip environment setup in college lab machines; this directly serves the "low friction" goal         |
| Compilers are Rust's home turf | Enums + exhaustive `match` model ASTs/tokens perfectly — the same safety style Min-Mozhi itself promises                                       |
| World-class error UX           | `miette`/`ariadne` crates give beautiful, span-highlighted diagnostics — our teaching-error goal — nearly for free                             |
| Brand honesty                  | "Safe like Rust" rings true when the toolchain itself is Rust                                                                                  |

Trade-off accepted: Rust is slower to _write_ than Python and has a learning
curve. (Go was the middle-ground candidate but lacks sum types/pattern
matching, which compilers lean on heavily.)

**Deliverable:** Language spec doc, grammar, GitHub repo

---

## Phase 1 — Verilog Backend _(3–5 months)_

> Get something working end-to-end

- Build **Lexer** — tokenize Min-Mozhi source ✅ (trilingual, Unicode idents)
- Build **Parser** — produce AST from tokens ✅ (full v0.2 grammar)
- Build **AST** — represent modules, signals, logic ✅
- Build **Verilog Emitter** — walk AST, output `.v` files ✅ v1 (repeat/checker pending)
- Build **Checker** — the safety rules + const-eval (next up)
- Test with **Icarus Verilog** (free simulator)
- Support: wires, registers, modules, basic logic, clocks ✅

**Milestone:** `mimz compile adder.mimz → adder.v → simulates correctly`

**Deliverable:** Working compiler — Min-Mozhi → Verilog

---

## Phase 1.5 — Simulator _(2–3 months, after Phase 1.8 — solo-dev order)_

> Your own behavioral engine

- Build a signal propagation engine in Rust
- Support waveform output (VCD format — viewable in GTKWave)
- Write testbench support in Min-Mozhi itself

**Milestone:** `mimz sim adder.mimz` runs without any external tool

**Deliverable:** Own simulator with waveform output

---

## Phase 1.8 — Grammar Engine _(1–2 months, directly after Phase 1 — solo dev runs 1.8 before 1.5)_

> Natural Tamil word order — இலக்கண இயந்திரம் (see `spec/04-grammar-engine.md`)

- Add the `thamizh-order` syntax profile to the parser (SOV/postpositional
  clause forms: `<cond> endral { }`, `yetram(clk) pothu { }`, `<expr> poruthu { }`)
- File-level `syntax thamizh` directive; same AST as code-order — parser-level only
- `mimz translate --order code|thamizh` — lossless conversion both directions
- Morphology helper for error messages (Tamil case suffixes on interpolated
  signal names — table-driven, not NLP)
- Validate word-order table with native-speaker panel

**Milestone:** the traffic-light FSM written in pure Tamil script, natural word
order, compiles to the same Verilog as its English twin

**Deliverable:** Tamil/Tanglish code that reads like Tamil, not transliterated English

---

## Phase 2 — IR + Synthesis _(6–12 months)_

> Own your middle layer

- Design **Min-Mozhi IR** — your own netlist-like format
- Build **IR emitter** from AST
- Build **Logic Synthesizer** — map IR to gates (AND/OR/NOT/FF)
- Integrate or study **Yosys** internals for technology mapping
- Target: FPGA primitive mapping (LUTs, flip-flops)

**Milestone:** `.mimz → IR → FPGA bitstream` via open source toolchain (hardware demo once a board is acquired; until then simulation/emulation only)

**Deliverable:** IR + synthesis, FPGA bitstream via open toolchain

---

## Phase 3 — Native Backend _(1–2 years)_

> Full end-to-end ownership

- Build **target-specific backends** per FPGA architecture
- Direct bitstream generation (iCE40 family is open and well documented — best starting target)
- Optimizer passes on IR (dead signal elimination, constant folding)

**Milestone:** `mimz build blink.mimz --target ice40` → programs FPGA directly

**Deliverable:** 100% native end-to-end compiler

---

## Phase 4 — Ecosystem _(ongoing)_

> Make it usable by others

- Standard library (common modules: UART, SPI, PWM, ALU)
- Package manager for Min-Mozhi modules
- VS Code extension (syntax highlighting, errors)
- Documentation site
- Ecosystem drivers: WASM browser playground first, then npm/PyPI
  wrappers around the one Rust core (thin wrappers, never
  reimplementations — Decision 2026-06-11)
- Community + Tamil Nadu semiconductor outreach

**Deliverable:** Community language with real users

---

## Timeline Summary

```
Month 1–2    Phase 0     Language design & spec
Month 3–7    Phase 1     Verilog backend (working compiler)
Month 8–9    Phase 1.8   Grammar Engine (first — identity feature)
Month 10–12  Phase 1.5   Own simulator
Month 13–24  Phase 2     IR + synthesis engine
Year 2–4     Phase 3     Native bitstream generation
Ongoing      Phase 4     Ecosystem & community
```

### Proposed solo-dev deadlines (assumes ~8–10 h/week — correct me and these shift)

```
Phase 0 wrap-up       → 30 Jun 2026   (keyword review can trail into Phase 1)
Phase 1  compiler     → 31 Dec 2026   (v0.1.0 tag when executable + testable)
Phase 1.8 grammar eng → 28 Feb 2027   (go public after this works)
Phase 1.5 simulator   → 31 May 2027
Phase 2  IR+synthesis → mid 2028      (bitstream in CI; hardware when board arrives)
```

---

## Deliverables at Each Phase

| Phase | Description     | What you can show the world             |
| ----- | --------------- | --------------------------------------- |
| 0     | Foundation      | Language spec doc, grammar, GitHub repo |
| 1     | Verilog Backend | Working compiler — Min-Mozhi → Verilog  |
| 1.5   | Simulator       | Own simulator with waveform output      |
| 1.8   | Grammar Engine  | Tamil code in natural Tamil word order  |
| 2     | IR + Synthesis  | FPGA bitstream via open toolchain       |
| 3     | Native Backend  | 100% native end-to-end compiler         |
| 4     | Ecosystem       | Community language with real users      |

---

## Why Min-Mozhi Matters

| Dimension     | Significance                                                                 |
| ------------- | ---------------------------------------------------------------------------- |
| **Cultural**  | First Tamil-rooted HDL — anywhere in the world                               |
| **Technical** | Full compiler stack from language to silicon                                 |
| **Timing**    | India's semiconductor boom — TATA, Vedanta fabs, India Semiconductor Mission |
| **Community** | Tamil Nadu has a growing VLSI and chip design ecosystem                      |

---

_Min-Mozhi — மின்மொழி — Speak in Circuits_
