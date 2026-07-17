# Agent Instructions

## User Persona

You are assisting a **38-year-old system engineer** with deep expertise in:

- System design & architecture
- Compiler design & implementation
- Low-level languages & machine code
- System-level application development through OS build
- IOT (Internet of Things) & HDL (Hardware Description Language)

The AI must **possess and reason with** this depth of knowledge internally, but **explain concepts to the user in simple, accessible terms** — the user is not an expert. Avoid jargon-heavy explanations unless asked; prioritize clarity and approachability.

Welcome, AI Agent! When assisting with tasks in the **min-mozhi** codebase, you must strictly follow the repository working rules.

## Core Rules Reference

Please refer to the following rules files before making any modifications or planning any changes:

- **Primary Agent Rules**: [.claude/Rules.md](.claude/Rules.md) — Contains requirements for writing daily dev logs, document synchronization, linting, and spec alignment/impact analysis.
- **Full Repository Rules**: [docs/RULES.md](docs/RULES.md) — The comprehensive source of truth for repository working guidelines.

## Quick Checklist for Agents

1. **Impact Analysis**: Check requests against [spec/01-goals-and-philosophy.md](spec/01-goals-and-philosophy.md) and [spec/02-syntax-and-grammar.md](spec/02-syntax-and-grammar.md). If a change breaks anything, tell the user and ask how to proceed.
2. **Dev Log**: After a change, append to today's log file (`docs/log/YYYY-MM-DD.md`).
3. **Docs Sync**: Ensure no related documentation is left stale.
4. **Lint & Format**: Run `cargo clippy`, `cargo fmt`, Prettier, and markdownlint before wrapping up.

## Project structure — quick reference

```
src/               Compiler source (21 entries: lexer, parser, ast, checker, emit_verilog, sim, commands…)
crates/mimz-wasm/  WASM playground wrapper
tests/             Test suite (17 files + fixtures/golden/icarus)
benches/           Criterion micro-benchmarks
fuzz/              libFuzzer targets (4)
examples/          23 designs × 5 keyword flavors
demo/              Real hardware demos (alu, cpu)
editors/vscode/    VS Code extension (plain JS, no build)
lang/              Language data: keywords.toml, messages.toml, case_suffixes.toml
spec/              Language specification (7 .md files)
docs/              All documentation (see table below)
site/              Astro documentation website
tools/test-summary/Cargo test wrapper (dev helper)
.github/workflows/ CI/CD pipelines (ci.yml, deploy-site.yml, release.yml)
```

## docs/ folder structure — quick reference

Each docs folder has a README.md that explains its audience and links to its chapters:

| docs/ subfolder | README / index     | Purpose                               |
| --------------- | ------------------ | ------------------------------------- |
| `guide/`        | `README.md`        | Learn the language (user-facing book) |
| `code/`         | `README.md`        | Maintainer docs (compiler internals)  |
| `source-guide/` | `README.md`        | Friendly Rust file tour (newcomers)   |
| `audit/`        | `README.md`        | Security & robustness audit           |
| `plan/`         | (phase-plan files) | Per-phase execution plans             |
| `log/`          | (dated files)      | Dev log, append-only decisions        |
| `Ideas/`        | (5 files)          | Forward-looking ideas, roadmaps       |
| `archive/`      | (single file)      | Closed working documents              |

Top-level docs files:

- `docs/README.md` — master index of everything
- `docs/RULES.md` — repo working rules
- `docs/BUILD.md` — build reference
- `docs/architecture.md` — pipeline & components
- `docs/prior-art.md` — prior art comparison
- `docs/how-the-compiler-works.md` — beginner's compiler tour
