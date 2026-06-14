# Min-Mozhi — Architecture

> Living document (RULES.md R3: update whenever components or data flow change).
> Status: **Phase 1 complete** (2026-06-12) — lexer, parser, full checker
> (six passes), Verilog emitter (repeat unrolling, transliteration,
> signed), CLI (`check`/`compile`/`lsp`/`explain`/`translate`/`eval`,
> `--json`), LSP v0, all Icarus-validated. The full simulator and IR are
> still design (`eval` is the combinational slice only).
> Last updated: 2026-06-13 (quick-wins tooling: explain/translate/eval)

---

## 1. The Pipeline

```
 source (.mimz)  ── any keyword flavor, code-order or thamizh-order
        │
        ▼
 ┌──────────────┐   one trilingual keyword table (keywords.toml)
 │    LEXER     │   Unicode NFC idents · spans on every token
 └──────┬───────┘   records flavor used → error language, fmt
        ▼
 ┌──────────────┐   recursive descent · profile: code-order (P1)
 │    PARSER    │   + thamizh-order (P1.8) — same productions, flipped
 └──────┬───────┘   clause heads, one-token lookahead
        ▼
 ┌──────────────┐   ONE SHARED AST — everything downstream is
 │     AST      │   flavor- and word-order-blind
 └──────┬───────┘
        ▼
 ┌──────────────┐   name resolution · const folding · width rules
 │   CHECKER    │   single-driver · DAG · exhaustiveness · =/<- ·
 └──────┬───────┘   clock-domain typing   (spec/02 section 6)
        ▼
 ┌─────────────────────────────────────────────────┐
 │                  BACKENDS                       │
 │  Phase 1    AST → Verilog emitter (.v)          │
 │  Phase 1.5  AST → elaborated graph → simulator  │
 │             (event kernel, two-phase commit,    │
 │              VCD writer, test runner)           │
 │  Phase 2    AST → IR → optimizer → Yosys/nextpnr│
 │  Phase 3    IR → techmap → place → route →      │
 │             iCE40 bitstream (native)            │
 └─────────────────────────────────────────────────┘

 side tools (share lexer/parser/AST + pretty-printer):
   mimz translate  — flavor reskin ✅ 2026-06-13 (--to; lossless, token-level);
                     word-order (--order thamizh) in Phase 1.8
   mimz explain    — long-form text per E-code ✅ 2026-06-13 (lib `explain`)
   mimz eval       — combinational evaluator ✅ 2026-06-13 (lib `sim::comb`;
                     a slice of the Phase 1.5 simulator — no clocks/regs yet)
   mimz fmt        — formatter
   mimz lsp        — language server ✅ v0 SHIPPED 2026-06-12
                     (diagnostics-only; hover/go-to-def in Phase 4)
```

## 2. Components

Built ✅ as of 2026-06-12 (Phase 1 complete): keyword table, lexer,
parser (code-order), AST, checker — ALL spec/02 section 6 safety rules
(names/consts/E-codes, width/type E04xx, driver/cycle E05xx,
instantiation completeness E0302, match exhaustiveness E06xx, clock
domains E0701), Verilog emitter (repeat unrolling, Tamil→ASCII
transliteration, `wire signed`; validated by Icarus differential tests
and golden files), CLI (`check`, `compile`, `lsp`, `--json`), and the
diagnostics-only LSP v0 with its VS Code client. Everything else:
planned.

| Component           | Phase   | Key design points                                                                                                                                                  |
| ------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **CLI** (`mimz`)    | 1       | `clap`; subcommands: `compile`, `check`, `sim`, `test`, `translate`, `fmt`, `build`                                                                                |
| **Keyword table**   | 1       | `keywords.toml` = source of truth; three columns per token, disjoint; loaded into one static map. Word changes are data changes                                    |
| **Lexer**           | 1       | Exact-match keywords after NFC normalization; Unicode identifiers; newline-terminator with continuation rules; full span tracking                                  |
| **Parser**          | 1 / 1.8 | Handwritten recursive descent; syntax profiles share all expression/declaration code, differ only in clause-head order; `syntax thamizh` directive selects profile |
| **AST**             | 1       | Rust enums + exhaustive match; spans everywhere; the single contract between front and back ends                                                                   |
| **Checker**         | 1       | ✅ ALL spec/02 section 6 safety rules; six passes (symbols/consteval/names/widths/drivers/clocks), each with its own tests; stable E-codes E0001–E0701             |
| **Diagnostics**     | 1 / 1.8 | ✅ stable codes on EVERY stage (lexer E10xx, parser E11xx, loader E12xx) + `--json` wire format; Phase 1.8 adds the per-language catalogs + morphology helper      |
| **Verilog emitter** | 1       | Dumb, readable Verilog-2005; sync active-high reset from reg reset values; no optimization here                                                                    |
| **Simulator**       | 1.5     | Elaborate → flat graph; event-driven kernel with two-phase commit (compute `<-`, then commit); 2-state by design; VCD out                                          |
| **IR**              | 2       | Typed netlist (cells/nets/widths/clock domains); dumpable text format; own validation pass (defense in depth)                                                      |
| **Optimizer**       | 2–3     | Const fold/propagate, dead-cell elimination, mux simplification; later retiming/sharing                                                                            |
| **Native backend**  | 3       | iCE40 only: techmap → annealing placer → pathfinder router → IceStorm-DB bitstream; validated differentially vs Yosys/nextpnr                                      |

## 3. Code Layout (Rust)

One **library crate** with two thin binaries — the `mimz` CLI and the
`mimz-bench` harness (the lib/bin split happened 2026-06-12 when the LSP
arrived); a WORKSPACE split stays trigger-based:

```
mimz/
  Cargo.toml
  keywords.toml          # trilingual table — data, not code
  src/
    lib.rs               # pub mod × 8 + the crate map          ✅
    main.rs              # thin CLI (clap, human/JSON output)   ✅
    lsp.rs               # `mimz lsp` server (BIN-only module,  ✅
                         #   keeps tokio out of the lib)
    bin/mimz-bench/      # benchmark harness (docs/code/12)     ✅
                         #   main.rs / metrics.rs / html.rs
    project.rs           # loading, imports; LoadError values   ✅
    span.rs              # byte-offset spans                    ✅
    diag.rs              # teaching diagnostics + JSON format   ✅
    ast/                 # the ONE shared AST                   ✅
      mod.rs             #   files, modules, decls, statements
      expr.rs            #   expressions, patterns, operators
    lexer/               # E10xx                                ✅
      mod.rs             #   scanner + newline policy
      token.rs           #   token kinds, keyword enum, flavors
      keywords.rs        #   keywords.toml loader (REQUIRED_KEYS)
      tests.rs           #   unit tests
    parser/              # E11xx                                ✅
      mod.rs             #   entry, Parser state + Profile, plumbing
      items.rs           #   file/module/seq/test items; syntax directive +
                         #     clocked-block flip (P1.8 keystone)
      expr.rs            #   precedence climbing, patterns
      tests.rs           #   unit tests
      # thamizh_order.rs #   (P1.8, planned) the remaining flips move here
                         #     once the expr-first conditional/match path lands
    emit_verilog/        #                                      ✅
      mod.rs             #   Project symtab, entry, helpers
      module.rs          #   shells, instances, always-blocks
      expr.rs            #   expression rendering
      translit.rs        #   Tamil → ASCII identifier pre-pass
    checker/             # six passes, E0001–E0701              ✅
      mod.rs             #   entry, Checker state, err plumbing
      symbols.rs         #   project tables + duplicates
      consteval.rs       #   compile-time evaluation
      names.rs           #   names, structure, E0302/E0303
      widths/            #   width/type + exhaustiveness (E04xx, E06xx)
      drivers.rs         #   single-driver + comb-DAG (E05xx)
      clocks.rs          #   clock-domain ownership (E0701)
      tests.rs           #   unit tests (one per E-code)
    sim/                 # (P1.5) elaborate, kernel, vcd
    ir/                  # (P2)
  tests/
    examples.rs          # all 68 examples + 4-flavor identity + goldens ✅
    errors.rs            # ~71 broken fixtures, code per E-code ✅
    icarus.rs            # iverilog lint + 16 self-checking TBs ✅
    lsp.rs               # wire-protocol smoke test             ✅
    docs_sync.rs         # docs ↔ code staleness guard          ✅
    grammar_sync.rs      # VS Code grammar ↔ keywords.toml      ✅
    golden/              # pinned .v output per base example
    fixtures/errors/     # the broken corpus
  benches/
    compile.rs           # criterion per-phase micro-benchmarks ✅
                         #   (cargo bench; lexer/parser/checker/emit)
  editors/vscode/        # extension: grammar + LSP client      ✅
```

Planned crate split (when needed): `mimz-syntax` (lexer/parser/AST/printer) ·
`mimz-check` · `mimz-backends` · `mimz` (CLI).

### Repository layout

```
min-mozhi/
  README.md, LICENSE-*, Cargo.toml
  keywords.toml        # language data (embedded at build time)
  min-mozhi-roadmap.md # roadmap summary
  spec/                # the LANGUAGE — normative, versioned (v0.2)
  docs/                # the PROJECT — plan/, log/, archive/, RULES, this file
  examples/            # .mimz example programs (no .rs files, so cargo's
                       #   examples/ auto-discovery is unaffected)
  src/                 # the compiler (tree above)
  tests/               # integration tests
  .github/workflows/   # CI
```

Separation rule: `spec/` defines the language and outlives any
implementation; `docs/` tracks this implementation and process. Never mix.

Future directories (created when their trigger fires, not before):

| Directory       | Arrives with                                   |
| --------------- | ---------------------------------------------- |
| `tools/vscode/` | TextMate grammar (Phase 1 work item 6)         |
| `tests/golden/` | golden `.v` files when the emitter hardens     |
| `stdlib/`       | `.mimz` standard library modules (Phase 4)     |
| `crates/`       | the workspace split (see Evolution Triggers)   |
| `targets/`      | board/constraint files (Phase 2 hardware flow) |
| `site/`         | docs website (Phase 4)                         |

## 4. Cross-Cutting Invariants

1. **One AST.** No flavor, keyword, or word-order information survives past the
   parser except as display metadata for diagnostics/fmt.
2. **Spans everywhere.** Every AST node and IR object carries a source span —
   error quality is a core goal, not a feature.
3. **Data over code for language identity.** Keywords (and later error
   catalogs) are data files, so community review never touches Rust.
4. **Safety rules are passes with tests.** Each spec/02 section 6 rule maps to one
   checker module and at least one rejection test.
5. **Differential validation at every new layer.** Simulator vs Icarus (1.5),
   IR vs AST simulation (2), native flow vs Yosys/nextpnr (3).
6. **Dumb first, fast later.** Emitters/backends start naive and readable;
   optimization lives in dedicated IR passes, never hidden in emitters.

## 5. Evolution Triggers (planned inflection points — not emergencies)

The architecture is staged on purpose; each piece below is _correct now_ and
has a known moment when it must change. When a trigger fires, do the listed
move and log it (R3). Letting a trigger pass is how architectures rot.

| Current shape                                        | Trigger                                                                              | Required move                                                                                                             |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| String-based Verilog emitter reading the AST         | Checker lands (work item 4)                                                          | Move all semantic errors (unknown module, port connectivity) out of the emitter into checker passes; emitter only renders |
| Emitter has no width knowledge (`extend` is a no-op) | IR exists (Phase 2)                                                                  | Emit from typed IR, demote AST→Verilog path to a debug backend                                                            |
| ~~Diagnostics are free-text, no IDs~~                | ✅ FIRED — every stage's errors carry stable codes (2026-06-12; map in docs/code/06) | Done: codes everywhere; the P1.8 message catalogs key off them                                                            |
| ~~Single binary crate~~                              | ✅ FIRED — LSP + `--json` consumers arrived (2026-06-12)                             | Done: `lib.rs` + thin `main.rs`; the WORKSPACE split (`mimz-syntax`/`mimz-check`/`mimz-backends`) stays trigger-based     |
| Lexer discards comments/whitespace                   | `mimz fmt` work starts                                                               | Add a trivia-preserving lexing mode; `translate` stays token-level and is unaffected                                      |
| Tokens own `String`s, cloned freely                  | Compile time on real projects becomes noticeable (not before)                        | String interning + token indices — contained inside `lexer/`                                                              |
| Emitter semantic checks duplicated per backend       | Simulator (P1.5) starts                                                              | Elaboration (`project.rs` + checker output) becomes the single pre-backend stage both consume                             |

## 6. Open Questions (log a Decision when resolved)

- Reset style v2: async-reset option? active-low? (v1: sync, active-high)
- Memories/arrays (`mem[depth] of bits[w]`) — Phase 2 spec bump (confirmed)
- CDC `sync` construct design (Phase 2 plan item)
- External Verilog module wrapping construct (Constitution item — design in Phase 2+)

Resolved 2026-06-10 (see log + spec v0.2): `import` semantics, `repeat`
generation, signed rules, Rust precedence, logical-op aliases, `.mimz`/`mimz`
naming, test grammar.
