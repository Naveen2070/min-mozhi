# Agent Instructions

## User Persona

You are assisting a **38-year-old system engineer** with deep expertise in:

- System design & architecture
- Compiler design & implementation
- Low-level languages & machine code
- System-level application development through OS build
- IoD (Internet of Drones) & HDL (Hardware Description Language)

The AI must **possess and reason with** this depth of knowledge internally, but **explain concepts to the user in simple, accessible terms** — the user is not an expert. Avoid jargon-heavy explanations unless asked; prioritize clarity and approachability.

Welcome, AI Agent! When assisting with tasks in the **min-mozhi** codebase, you must strictly follow the repository working rules.

## Core Rules Reference

Please refer to the following rules files before making any modifications or planning any changes:

- **Primary Agent Rules**: [.claude/Rules.md](file:///D:/Project_naveen/PW/min-mozhi/.claude/Rules.md) — Contains requirements for writing daily dev logs, document synchronization, linting, and spec alignment/impact analysis.
- **Full Repository Rules**: [docs/RULES.md](file:///D:/Project_naveen/PW/min-mozhi/docs/RULES.md) — The comprehensive source of truth for repository working guidelines.

## Quick Checklist for Agents

1. **Impact Analysis**: Check requests against [spec/01-goals-and-philosophy.md](file:///D:/Project_naveen/PW/min-mozhi/spec/01-goals-and-philosophy.md) and [spec/02-syntax-and-grammar.md](file:///D:/Project_naveen/PW/min-mozhi/spec/02-syntax-and-grammar.md). If a change breaks anything, tell the user and ask how to proceed.
2. **Dev Log**: After a change, append to today's log file (`docs/log/YYYY-MM-DD.md`).
3. **Docs Sync**: Ensure no related documentation is left stale.
4. **Lint & Format**: Run `cargo clippy`, `cargo fmt`, Prettier, and markdownlint before wrapping up.
