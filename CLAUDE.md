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
├── src/                         # Shell crate: fs I/O, CLI, LSP, hw emulation
│   ├── main.rs                  # CLI entry (clap)
│   ├── lib.rs                   # Facade: project/config/emulate + re-exports mimz-core/mimz-sim
│   ├── project.rs               # File loading + import resolution (fs-touching remainder)
│   ├── config.rs                # mimz.toml project config
│   ├── emulate/                 # Native hw-emulation peripherals (7 files, `hw-emulation` feature)
│   ├── lsp.rs                   # Language server (optional, lsp feature)
│   ├── commands/                # CLI command handlers (16 files)
│   └── bin/mimz-bench/          # Benchmark harness
├── crates/
│   ├── mimz-core/src/           # Pure pipeline + most tooling
│   │   ├── lib.rs                   # Library root, re-exports everything
│   │   ├── span.rs                  # Source positions
│   │   ├── diag.rs                  # Error diagnostics with pretty underlines
│   │   ├── morph.rs                 # Error language detection + Tamil inflection
│   │   ├── project.rs               # LoadedFile struct + render_diags(_lang) only (no fs I/O)
│   │   ├── translate.rs             # Keyword reskin between flavors
│   │   ├── pretty.rs                # AST → source pretty-printer
│   │   ├── explain.rs               # Long-form error code explanations
│   │   ├── version.rs               # Compiler version + language edition
│   │   ├── lexer/                   # Tokenizer (4 files)
│   │   ├── parser/                  # Recursive-descent parser (11 files)
│   │   ├── ast/                     # Shared AST (3 files)
│   │   ├── checker/                 # 7 safety passes (13 files)
│   │   └── emit_verilog/            # Verilog-2005 code gen (5 files)
│   ├── mimz-sim/src/            # Event-driven simulator + runner
│   │   ├── lib.rs                   # compile_string entry, re-exports sim/runner
│   │   ├── runner.rs                # In-memory command engine (playground)
│   │   └── sim/                     # Event-driven simulator (10 files, incl. EmulationHost trait)
│   └── mimz-wasm/               # WASM playground wrapper (depends on mimz-sim)
├── tests/                       # 20 test files + fixtures/golden/icarus
├── benches/compile.rs           # Criterion micro-benchmarks
├── fuzz/                        # 4 libFuzzer targets
├── examples/                    # english/tanglish/tamil: 42 each, mixed: 41, tamil-pure: 20 (187 total)
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
│   ├── guide/                   # Learn the language (13 chapters + stdlib/ subguide)
│   ├── code/                    # Maintainer docs (14 files)
│   ├── source-guide/            # Friendly Rust file tour (11 chapters)
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

| Folder / File                                    | What lives here                              |
| ------------------------------------------------ | -------------------------------------------- |
| `README.md`                                      | Master docs index with table of all sections |
| `RULES.md`                                       | Repository working rules (source of truth)   |
| `BUILD.md`                                       | Build reference — tools, crates, commands    |
| `architecture.md`                                | Compiler architecture — pipeline, components |
| `prior-art.md`                                   | Prior art: Veryl/Spade/Amaranth/Chisel       |
| `how-the-compiler-works.md`                      | Beginner's tour — pipeline on one example    |
| `guide/` (README + 13 files, + stdlib/ subguide) | **Learn the language** — from-scratch book   |
| `code/` (README + 14 files)                      | **Maintainer docs** — per-module internals   |
| `source-guide/` (README + 11)                    | **Friendly code tour** — every Rust file     |
| `audit/` (README + bugs)                         | Security & robustness audit                  |
| `Ideas/` (6 files)                               | Forward-looking plans, roadmaps              |
| `plan/`                                          | Detailed per-phase execution plans           |
| `log/` (dated logs)                              | Dev log — append-only decisions & progress   |
| `archive/`                                       | Closed working documents                     |
