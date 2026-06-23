# Min-Mozhi (மின்மொழி) — Goals & Philosophy

> **Spec v0.3.3.** A modern, safe-by-default hardware description language — with
> security as a first-class compile-time goal — built to teach digital design,
> and the first HDL with Tamil and Tanglish as first-class keyword flavors
> alongside English. Built in Tamil Nadu, India.

---

## 1. Goals

Min-Mozhi is a modern HDL with three goals, held in balance:

1. **Teach digital design.** Lower the barrier into hardware: a small, modern
   surface, one obvious way to do most things, and diagnostics that explain
   _what_ is wrong, _why_ it is unsafe in hardware terms, and _how_ to fix it.
2. **Be a modern, safe HDL — secure by design.** Safety is enforced by the
   compiler today (section 5, G2); security is a first-class compile-time goal on
   the roadmap (G5). Developer experience is treated as a feature.
3. **Be trilingual and Tamil-first.** English, Tanglish (romanized Tamil), and
   Tamil script are first-class keyword flavors over one grammar — the first
   Tamil-rooted HDL. Native Tamil serves a double purpose: it reaches learners
   underserved by English-only tools, and it advances Tamil as a language you can
   actually program in.

In one sentence: **Min-Mozhi is a modern, safe-by-default HDL — with security as a
first-class compile-time goal — built to teach digital design, and the first HDL
with Tamil and Tanglish as first-class keyword flavors alongside English.**

It is an **educational project, honestly framed**: new and experimental, built to
be learned from and read clearly — not to chase production use.

## 2. Who It Is For

**Learners of digital design** — anyone picking up hardware design who is better
served by a modern, safe, teaching-first language than by industrial Verilog. Most
HDLs make a beginner fight the tool before the logic; Min-Mozhi is built to teach.

It especially serves learners who are **not comfortable working in English**:
native Tamil lets them read and write hardware in their own language, and is a step
toward Tamil-rooted programming more broadly.

**Also for developers** who simply want a safe, ergonomic HDL — drawn by the
compile-time checks and the Go/TypeScript-feel syntax rather than the Tamil roots.

**Not for** production engineering that needs Verilog/Chisel completeness:
Min-Mozhi is new and experimental and does not try to replace them. It is
student-first in tone, but a working engineer should be able to read any Min-Mozhi
file and understand it at a glance.

## 3. What Min-Mozhi Is

- **A hardware description language** for digital circuits (FPGAs first)
- **Not** a general-purpose programming language — every line describes hardware
  (wires, registers, modules), and what you write maps obviously to silicon

> **Modern and safe by default. Built to teach. Tamil at heart.**
> Reads like Go/TypeScript. Safe like Rust. Speaks English, Tanglish, and Tamil.

Key facts:

- **Safety is enforced today** (section 5, G2) — compile-time guarantees, not conventions
- **Trilingual system** is constitutional (section 4.3) — the language's heart, not a marketing line
- **Security (G5)** is a compile-time design goal that lands after the first release — named as a goal, never claimed as already shipped

## 4. The Constitution (non-negotiable, forever)

1. **Free and open source forever.** Licensed MIT + Apache-2.0 (dual, the Rust
   ecosystem norm).
2. **Verilog interop forever.** Min-Mozhi will always be able to emit Verilog —
   even after native backends exist — and will eventually be able to
   **wrap/instantiate existing Verilog modules** from Min-Mozhi code. (Interop
   with Verilog, never syntax-compatibility _with_ Verilog.)
3. **Tamil is first-class forever.** The trilingual system is never cut or
   deferred under pressure; it grows with the project, culminating in the Grammar
   Engine (`spec/04`).

## 5. Core Goals

### G1 — Beginner-first, measurably

The targets are numbers, not vibes:

- A beginner with basic programming understands the **basics in 1–2 hours**.
- From a fresh install, a **counter compiles within 5 minutes**.

Supporting rules:

- a small keyword set
- one obvious way to do each thing (one documented exception: logical operators,
  G1-x below)
- modern brace / `: type` syntax
- error messages that teach — _what_ is wrong, _why_ it is unsafe in hardware
  terms, _how_ to fix it, in the reader's language

**G1-x (the one-way exception):** logical operators accept both universal symbols
(`&&`, `||`, `!`) and translated keyword forms (`and`/`or`/`not`,
மற்றும்/அல்லது/இல்லா) — symbols for programmers, words for learners. `mimz fmt
--strict` normalizes to one style.

### G2 — Safe by construction

The hardware analogue of Rust's memory safety. The compiler statically rejects the
classic Verilog footguns:

| Verilog footgun                     | Min-Mozhi rule                                                                                                             |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Implicit width truncation/extension | Widths must match exactly; widening and truncation are explicit (`extend`, slicing)                                        |
| Inferred latches                    | `if`-expressions driving wires **must** have `else`; `match` must be exhaustive                                            |
| Multiple drivers / no driver        | Every wire is driven **exactly once**, checked at compile time                                                             |
| Uninitialized registers             | Every `reg` declares a reset value; a module with regs must declare a `reset`                                              |
| Blocking vs non-blocking confusion  | `=` is _only_ for wires (combinational), `<-` is _only_ for registers (sequential). The compiler enforces which goes where |
| Accidental clock-domain crossing    | Every reg is owned by one clock; cross-domain reads are a compile error until the explicit `sync` construct (Phase 2)      |
| Silent arithmetic overflow          | `+` grows the result width (lossless); wrapping arithmetic is the explicit `+%` operator                                   |
| Signed/unsigned confusion           | `signed[N]` and `bits[N]` never mix implicitly; conversion is the explicit `signed()`/`unsigned()` casts                   |
| `x & 1 == 0` precedence trap        | Rust-style precedence: bitwise binds tighter than comparison                                                               |

### G3 — Trilingual by design, not by translation

English, Tanglish (romanized Tamil), and Tamil script are **three keyword skins
over one identical grammar** (Layer 1, `spec/03`), with natural Tamil word order
available as a parser profile (Layer 2 Grammar Engine, `spec/04`).

- All three flavors **mix freely in one file**, and `mimz translate` converts
  between flavors (and word orders) losslessly.
- Identifiers may use Tamil script anywhere; types, operators, and numbers are
  universal.

### G4 — Full-stack ownership over time

The first release emits Verilog (pragmatic). The long-term goal is a fully native
path to an FPGA bitstream — while the Constitution (section 4.2) guarantees the
Verilog exit ramp never closes.

### G5 — Secure by construction (planned)

Security promises are compile-time checks, exactly like the safety rules in G2 —
and stated honestly, with what they do **not** cover written down. These land
**after the first release** (they are committed goals, not stretch ideas):

- **Information-flow tracking**: data labelled `secret` can never reach a public
  output or unlabelled storage except through a `declassify`-marked module (e.g. a
  cipher). Explicit flow only (the SecVerilog model) — timing side channels are out
  of scope, and the docs say so.
- **Fail-secure faults**: `system_fault(code)` synthesizes a sticky fault network —
  a `FAULT_OUT` pin, every output forced to its declared safe state, inputs ignored
  until cold reset.

## 6. Non-Goals

- **Not** a Verilog superset or preprocessor — clean-slate syntax (interop per the
  Constitution is about _output and wrapping_, not source syntax).
- **Not** analog/mixed-signal, **not** high-level synthesis.
- **Not chasing** global adoption, production use, Verilog feature parity, or
  academic recognition. All are welcome; none are goals — this keeps scope honest
  for an educational project.
- **Not mixed:** simulation-only constructs live in fenced `test` blocks.

## 7. Design Principles (in priority order)

When two designs conflict, decide in this order:

1. **Hardware honesty** — never hide what hardware gets generated.
2. **Compile-time safety** — prefer a compile error over a simulation surprise.
3. **Security** — prefer the design that keeps the G5 guarantees provable; never
   weaken a security check for convenience.
4. **Readability & developer experience** — prefer the form a first-year student
   can read aloud; modern ergonomics serve the same instinct.
5. **Speed** — of compilation and simulation.
6. **Brevity** — never save keystrokes at the cost of 1–5.
7. **Tamil idiom** — a tie-breaker only. Tamil's **presence** is constitutional
   (section 4.3) and never in question; this rank only means a design detail is
   never made dishonest, unsafe, or unreadable in order to be more idiomatic.

## 8. Success, Concretely

| Horizon | Looks like                                                                                     |
| ------- | ---------------------------------------------------------------------------------------------- |
| Near    | A learner picks up digital design — or HDL/Verilog — using Min-Mozhi.                          |
| Far     | A self-sustaining community; students learn hardware with it, and perhaps some small industry. |
| Always  | The language stays honest — it never hides what hardware it generates, and it keeps teaching.  |

---

## Changelog

- **v0.3.3 (2026-06-18):** Rewritten as a **user-facing** spec — removed the
  internal/working-doc framing (the maintainer-ordered "why", the learn-compilers and
  prove-silicon-tooling and personal-mastery/portfolio motivations, inline decision
  tags) and the explicit Tamil-Nadu-student persona, broadening the audience to
  learners generally with native Tamil framed as reach + Tamil-rooted-programming
  growth. Normative content is unchanged: the Constitution, G1–G5, the safety
  table, non-goals, and the design-principle order all carry over
  (Decision 2026-06-18, `docs/log/2026-06-18.md`).
- **v0.3.2 (2026-06-18):** Outward pitch reordered to lead with modern + safe,
  then educational, then trilingual / first-native-Tamil; audience broadened;
  "secure" framed as a goal (G5, post-Phase-1), not a shipped feature.
- **v0.3.1 (2026-06-12):** Pitch reworded to lead with modern HDL / safe by
  default / Tamil-rooted / educational. Wording only.
- **v0.3 (2026-06-12):** Modern-secure-HDL became a co-primary goal; added **G5
  Secure by construction** (explicit-flow `secret` taint per the SecVerilog model;
  fail-secure `system_fault`), and inserted security into the design-principle
  order.
- **v0.2 (2026-06-10):** Rewritten from the design review. Added the ranked goals,
  the Constitution, measurable G1 numbers, the G1-x logical-operator exception, the
  safety table, the ordered principles, and honest success metrics.
- **v0.1 (2026-06-10):** Initial draft.

---

_Min-Mozhi — மின்மொழி — Speak in Circuits_
