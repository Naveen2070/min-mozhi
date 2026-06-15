# Phase 0 — Foundation

> **Design before you code.**
> Window: months 1–2 · Status: ✅ COMPLETE (2026-06-15)

## Goal

A complete, reviewable language design on paper: philosophy, grammar, the
trilingual keyword system, and the project's working process — so Phase 1
coding starts with zero open design questions.

## Work items

- [x] Define language goals and philosophy → `spec/01-goals-and-philosophy.md`
- [x] Design syntax and grammar, write EBNF → `spec/02-syntax-and-grammar.md`
- [x] Design trilingual keyword system (Layer 1: keyword skins) → `spec/03-keywords-trilingual.md`
- [x] Design Grammar Engine (Layer 2: Tamil word order) → `spec/04-grammar-engine.md`
- [x] Decide compiler implementation language → **Rust** (decision record in root roadmap)
- [x] Write reference examples → `examples/` (adder, counter EN+Tanglish, ALU, traffic light)
- [x] Set up docs structure, dev log, repo rules → `docs/`
- [x] Design review: 44-question register answered → spec v0.2 (`docs/archive/open-questions-2026-06-10.md`)
- [x] License decided: **MIT + Apache-2.0 dual**
- [x] Naming decided: extension **`.mimz`**, CLI **`mimz`**, project name Min-Mozhi
- [ ] Native-speaker review of Tanglish/Tamil keyword table (panel: tech/coder friends)
- [x] Study list: Verilog internals + **Veryl, Spade, Amaranth, Chisel** → `docs/prior-art.md` (2026-06-11)
- [x] `git init` + first commit + LICENSE files (done 2026-06-10; repo stays **private until after Phase 1.8** — decision D7)

## Milestone

Spec v0.1 complete and internally consistent; a stranger can read `spec/` and
write valid Min-Mozhi on paper.

## Exit criteria

1. All four spec docs exist and agree with each other and the examples. ✅
2. Keyword table reviewed by native speakers (DRAFT marks removed). ✅ (keyword set v1, 2026-06-15)
3. Repo under git with LICENSE files (private; public comes after Phase 1.8). ✅

## Risks / notes

- Keyword word choices are the only externally-blocked item — don't let review
  block Phase 1 coding; English flavor is frozen and sufficient to build against.
