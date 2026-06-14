# Min-Mozhi — Documentation Index

| Section                                                  | What lives here                                                                                                                |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| [`RULES.md`](RULES.md)                                   | **Repo working rules** — how plans, specs, and logs are kept in sync                                                           |
| [`how-the-compiler-works.md`](how-the-compiler-works.md) | **Start here** — beginner's tour: the five pipeline stations, traced on one real example                                       |
| [`architecture.md`](architecture.md)                     | Compiler architecture — pipeline, components, crate layout                                                                     |
| [`prior-art.md`](prior-art.md)                           | Prior art — Veryl/Spade/Amaranth/Chisel design choices mapped to our open decisions                                            |
| [`code/`](code/)                                         | **How the code works** — maintainer docs: pipeline, per-module internals, decisions, contributor recipes                       |
| [`audit/`](audit/)                                       | **Security & robustness audit** — input-hardening defects found and how each was fixed (security / bugs / hardening)           |
| [`Ideas/`](Ideas/)                                       | Forward-looking plans — language roadmap (`language_plan.md`), benchmark roadmap (`benchmark_plan.md`), CI plan (`ci_plan.md`) |
| [`plan/`](plan/)                                         | **Detailed per-phase plans** (source of truth for execution)                                                                   |
| [`log/`](log/)                                           | **Dev log** — dated, append-only record of decisions and progress                                                              |
| [`archive/`](archive/)                                   | Closed working documents (e.g. the answered 2026-06-10 design-review register)                                                 |
| [`../spec/`](../spec/)                                   | Language specification — philosophy **v0.3.1**, grammar **v0.2.6**, keywords **v0.2.5**, grammar engine **v0.2 DRAFT**         |
| [`../examples/`](../examples/)                           | Example `.mimz` programs                                                                                                       |
| [`../min-mozhi-roadmap.md`](../min-mozhi-roadmap.md)     | High-level roadmap summary (kept in sync with `plan/`)                                                                         |

## Plan files (solo-dev execution order)

| Order | Phase | Plan                                                                   | Status                                                                   |
| ----- | ----- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| 1     | 0     | [`plan/phase-0-foundation.md`](plan/phase-0-foundation.md)             | 🟢 Mostly complete                                                       |
| 2     | 1     | [`plan/phase-1-verilog-backend.md`](plan/phase-1-verilog-backend.md)   | 🟢 **Complete 2026-06-12** (target was 31 Dec 2026) — v0.1.0 tag pending |
| 3     | 1.8   | [`plan/phase-1.8-grammar-engine.md`](plan/phase-1.8-grammar-engine.md) | 🟡 In progress — keystone landed 2026-06-13 · target 28 Feb 2027         |
| 4     | 1.5   | [`plan/phase-1.5-simulator.md`](plan/phase-1.5-simulator.md)           | ⚪ Not started · target 31 May 2027                                      |
| 5     | 2     | [`plan/phase-2-ir-synthesis.md`](plan/phase-2-ir-synthesis.md)         | ⚪ Not started                                                           |
| 6     | 3     | [`plan/phase-3-native-backend.md`](plan/phase-3-native-backend.md)     | ⚪ Not started                                                           |
| 7     | 4     | [`plan/phase-4-ecosystem.md`](plan/phase-4-ecosystem.md)               | ⚪ Not started                                                           |

Status legend: ⚪ not started · 🟡 in progress · 🟢 done (per exit criteria in each plan)
