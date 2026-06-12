# Min-Mozhi (மின்மொழி) — Goals & Philosophy

> **Spec v0.3** — v0.2 rewritten from the founder's answers to the 2026-06-10
> design review (`docs/archive/open-questions-2026-06-10.md`); v0.3 adds the
> modern-secure-HDL co-goal (Decision 2026-06-12, `docs/log/2026-06-12.md`).
> The first Tamil-rooted Hardware Description Language. Built in Tamil Nadu, India.

---

## 1. Why Min-Mozhi Exists (in the founder's order)

1. **To learn how compilers work** — by building a real one, end to end.
2. **To help Tamil Nadu students learn HDL** — lowering the entry barrier into
   digital design.
3. **To be a modern, secure HDL** — security and developer experience as
   compile-time design goals, **co-primary with education** since v0.3
   (Decision 2026-06-12).
4. **To prove India can build its own silicon tooling.**
5. **Personal mastery of compilers** — and a portfolio that shows it (the
   showcase is a side effect, never the driver).

This is an **educational project, honestly framed**: if even one person learns
from it or appreciates it, it has succeeded. The only true failure is if the
builder learns nothing. Since v0.3 it carries a second, equal ambition: a
modern HDL whose security and ergonomics are checked by the compiler, not
promised by convention. When the two ambitions conflict, the tie-breakers
(section 7) decide.

## 2. The One Person It Is For

> A 20-year-old polytechnic student in Tamil Nadu, curious about hardware
> design but **not comfortable in English**. Today, every HDL forces them to
> fight the language before they can fight the logic. Min-Mozhi lets them see
> their own language running in semiconductors — and that pride matters.

**Who it is NOT for:** professional Verilog/Chisel users who need production
completeness. Min-Mozhi is new and experimental — it is _not_ a replacement
for Verilog or Chisel, and does not try to be.

**Second audience (equal rank since v0.3):** a modern developer who wants a
secure, ergonomic HDL — drawn by the compile-time security checks (G5) and
the Go/TypeScript-feel syntax, not by Tamil. Production completeness is still
a non-goal; language quality is the draw, not feature parity.

Student-first in tone, but a working VLSI engineer should be able to read any
Min-Mozhi file and understand it immediately.

## 3. What Min-Mozhi Is

A hardware description language for digital circuits (FPGAs first). It is
**not** a general-purpose programming language: every line describes hardware
— wires, registers, modules — and what you write maps obviously to what gets
built on silicon.

> **Reads like Go/TypeScript. Safe like Rust. Speaks English, Tanglish, and Tamil.**

The pitch to a non-Tamil user: the **safety** and the **syntax** (and maybe
speed, later). The trilingual system is the soul, not the sales pitch.

## 4. The Constitution (non-negotiable, forever)

1. **Free and open source forever.** Licensed MIT + Apache-2.0 (dual, the
   Rust ecosystem norm).
2. **Verilog interop forever.** Min-Mozhi will always be able to emit Verilog
   — even after native backends exist — and will eventually be able to
   **wrap/instantiate existing Verilog modules** from Min-Mozhi code.
   (Interop with Verilog, never syntax-compatibility _with_ Verilog.)
3. **Tamil is first-class forever.** The trilingual system is never cut or
   deferred under pressure; it grows with the project, culminating in the
   Grammar Engine (`spec/04`).

## 5. Core Goals

### G1 — Beginner-first, measurably

The targets are numbers, not vibes:

- A beginner with basic programming understands the **basics in 1–2 hours**.
- From a fresh install, a **counter compiles within 5 minutes**.

These become CI'd tutorial benchmarks once the compiler exists. Supporting
rules: small keyword set, one obvious way to do each thing (one documented
exception: logical operators, section G1-x below), modern brace/`: type` syntax, and
error messages that teach — _what_ is wrong, _why_ it is unsafe in hardware
terms, _how_ to fix it, in the user's language.

**G1-x (the one-way exception):** logical operators accept both universal
symbols (`&&`, `||`, `!`) and translated keyword forms (`and/or/not`,
மற்றும்/அல்லது/இல்லா) — symbols for programmers, words for learners.
`mimz fmt --strict` normalizes to one style.

### G2 — Safe by construction

The hardware analogue of Rust's memory safety. The compiler statically rejects
the classic Verilog footguns:

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

English, Tanglish (romanized Tamil), and Tamil script are **three keyword
skins over one identical grammar** (Layer 1, `spec/03`), with natural Tamil
word order coming as a parser profile (Layer 2 Grammar Engine, `spec/04`).

- **Stage 1 targets: English and Tanglish.** Tamil script follows for free.
- All three flavors **mix freely in one file** — the migration path.
- `mimz translate` converts flavors (and later word orders) losslessly.
- Identifiers may use Tamil script anywhere; types, operators, and numbers are
  universal.

### G4 — Full-stack ownership over time

Phase 1 emits Verilog (pragmatic). The long-term goal is a fully native path
to an iCE40 bitstream — while Constitution section 4.2 guarantees the Verilog exit
ramp never closes.

### G5 — Secure by construction (v0.3)

Security promises are compile-time checks, exactly like the safety rules in
G2 — and stated honestly, with what they do NOT cover written down:

- **Information-flow tracking**: data labelled `secret` can never reach a
  public output or unlabelled storage except through a `declassify`-marked
  module (e.g. a cipher). Explicit flow only, the SecVerilog model — timing
  side channels are out of scope and the docs say so.
- **Fail-secure faults**: `system_fault(code)` synthesizes a sticky fault
  network — a `FAULT_OUT` pin, every output forced to its declared safe
  state, inputs ignored until cold reset. v1 is plain synthesizable logic
  (no clock gating).

Both land post-Phase-1 (feasibility triage, `docs/Ideas/language_plan.md`
section 7); G5 fixes them as goals, not stretch ideas.

## 6. Non-Goals (v0.2)

- **Not** a Verilog superset or preprocessor — clean-slate syntax (interop per
  the Constitution is about _output and wrapping_, not source syntax).
- **Not** analog/mixed-signal, **not** high-level synthesis.
- **Not chasing:** global adoption, industry/production use, Verilog feature
  parity, academic recognition. All are welcome; none are goals. This keeps
  scope honest for an educational project.
- **Not in v0.2 core:** idiomatic Tamil word order (→ Grammar Engine, Phase 1.8).
- **Not mixed:** simulation-only constructs live in `test` blocks, fenced.

## 7. Design Principles (tie-breakers, in order)

When two designs conflict, decide in this order (security inserted at rank 3
and DX merged into readability in v0.3, Decision 2026-06-12):

1. **Hardware honesty** — never hide what hardware gets generated.
2. **Compile-time safety** — prefer a compile error over a simulation surprise.
3. **Security** — prefer the design that keeps the G5 guarantees provable;
   never weaken a security check for convenience.
4. **Readability & developer experience** — prefer the form a first-year
   student can read aloud; modern ergonomics serve the same instinct.
5. **Speed** — of compilation and simulation.
6. **Brevity** — never save keystrokes at the cost of 1–5.
7. **Tamil idiom** — last as a _tie-breaker only_; Tamil's **presence** is
   constitutional (section 4.3) and never in question — this rank only means a design
   detail is never made dishonest, unsafe, or unreadable to be more idiomatic.

## 8. Success, Concretely

| Horizon             | Looks like                                                                                         |
| ------------------- | -------------------------------------------------------------------------------------------------- |
| Near                | One person besides the founder learns HDL with it, or appreciates it                               |
| Far (10-year scene) | A self-sustaining community repo; used by students to learn HDL/Verilog, perhaps by small industry |
| Always              | The founder keeps learning — the moment that stops, the project has failed its first purpose       |

---

## Changelog

- **v0.3 (2026-06-12):** Modern-secure-HDL becomes a co-primary goal, equal
  with education (Decision 2026-06-12, from the `docs/Ideas/language_plan.md`
  feasibility triage). Added: goal #3 in section 1, second audience in
  section 2, **G5 Secure by construction** (explicit-flow `secret` taint per
  the SecVerilog model; fail-secure `system_fault` sticky-fault network v1),
  tie-breakers reordered to honesty > safety > **security** >
  readability/DX > speed > brevity > Tamil idiom.
- **v0.2 (2026-06-10):** Rewritten from founder's design-review answers.
  Added: ranked "why", persona, not-for statement, Constitution (license,
  Verilog interop incl. future wrapping, Tamil permanence), measurable G1
  numbers, G1-x logical-operator exception, signed-mixing + precedence safety
  rows, 6-level tie-breakers (speed added, Tamil idiom ranked), honest
  educational success metrics.
- **v0.1 (2026-06-10):** Initial draft.

---

_Min-Mozhi — மின்மொழி — Speak in Circuits_
