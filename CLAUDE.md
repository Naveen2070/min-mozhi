## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:

- Refer to the codebase rules in [.claude/Rules.md](.claude/Rules.md) and [docs/RULES.md](docs/RULES.md) before making any changes.
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).

## Project structure

```
min-mozhi/
├── src/                         # Compiler source (Rust)
│   ├── main.rs                  # CLI entry (clap)
│   ├── lib.rs                   # Library root, re-exports everything
│   ├── span.rs                  # Source positions
│   ├── diag.rs                  # Error diagnostics with pretty underlines
│   ├── morph.rs                 # Error language detection + Tamil inflection
│   ├── config.rs                # mimz.toml project config
│   ├── project.rs               # File loading + import resolution
│   ├── runner.rs                # In-memory command engine (playground)
│   ├── translate.rs             # Keyword reskin between flavors
│   ├── pretty.rs                # AST → source pretty-printer
│   ├── explain.rs               # Long-form error code explanations
│   ├── version.rs               # Compiler version + language edition
│   ├── lsp.rs                   # Language server (optional, lsp feature)
│   ├── lexer/                   # Tokenizer (4 files)
│   ├── parser/                  # Recursive-descent parser (9 files)
│   ├── ast/                     # Shared AST (2 files)
│   ├── checker/                 # 6 safety passes (12 files)
│   ├── emit_verilog/            # Verilog-2005 code gen (5 files)
│   ├── sim/                     # Event-driven simulator (9 files)
│   ├── commands/                # CLI command handlers (16 files)
│   └── bin/mimz-bench/          # Benchmark harness
├── crates/mimz-wasm/            # WASM playground wrapper (40 lines)
├── tests/                       # 18 test files + fixtures/golden/icarus
├── benches/compile.rs           # Criterion micro-benchmarks
├── fuzz/                        # 4 libFuzzer targets
├── examples/                    # 23 designs × 4 complete flavors + 6 stdlib each + 13 tamil-pure (129 total)
│   └── {english,tanglish,tamil,tamil-pure,mixed}/
├── demo/                        # Real hardware demos (alu, cpu)
├── editors/vscode/              # VS Code extension (plain JS)
├── docs/                        # All documentation
│   ├── README.md                # Master index
│   ├── RULES.md                 # Repo working rules
│   ├── BUILD.md                 # Build reference
│   ├── architecture.md          # Pipeline & components
│   ├── prior-art.md             # Prior art comparison
│   ├── how-the-compiler-works.md# Beginner's tour
│   ├── guide/                   # Learn the language (12 chapters)
│   ├── code/                    # Maintainer docs (13 files)
│   ├── source-guide/            # Friendly Rust file tour (10 chapters)
│   ├── audit/                   # Security & robustness audit
│   ├── Ideas/                   # Forward-looking plans (5 files)
│   ├── plan/                    # Per-phase execution plans
│   ├── log/                     # Dev log (dated, append-only)
│   └── archive/                 # Closed working documents
├── lang/                        # Language data (TOML)
│   ├── keywords.toml            # Trilingual keyword table
│   ├── messages.toml            # Localized error templates
│   └── case_suffixes.toml       # Tamil case suffixes
├── spec/                        # Language specification (7 files)
├── site/                        # Astro documentation website
├── tools/test-summary/          # Dev helper (cargo test wrapper)
├── .github/workflows/           # CI/CD (ci, deploy-site, release)
├── Cargo.toml                   # Workspace root
├── CHANGELOG.md
├── CONTRIBUTING.md
└── README.md
```

## docs/ folder structure

| Folder / File                 | What lives here                              |
| ----------------------------- | -------------------------------------------- |
| `README.md`                   | Master docs index with table of all sections |
| `RULES.md`                    | Repository working rules (source of truth)   |
| `BUILD.md`                    | Build reference — tools, crates, commands    |
| `architecture.md`             | Compiler architecture — pipeline, components |
| `prior-art.md`                | Prior art: Veryl/Spade/Amaranth/Chisel       |
| `how-the-compiler-works.md`   | Beginner's tour — pipeline on one example    |
| `guide/` (README + 12 files)  | **Learn the language** — from-scratch book   |
| `code/` (README + 13 files)   | **Maintainer docs** — per-module internals   |
| `source-guide/` (README + 10) | **Friendly code tour** — every Rust file     |
| `audit/` (README + bugs)      | Security & robustness audit                  |
| `Ideas/` (5 files)            | Forward-looking plans, roadmaps              |
| `plan/`                       | Detailed per-phase execution plans           |
| `log/` (dated logs)           | Dev log — append-only decisions & progress   |
| `archive/`                    | Closed working documents                     |
