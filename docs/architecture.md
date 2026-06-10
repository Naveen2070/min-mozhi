# Min-Mozhi — Architecture

> Living document (RULES.md R3: update whenever components or data flow change).
> Status: **planned** — describes the Phase 1–2 design; nothing is built yet.
> Last updated: 2026-06-10

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
 └──────┬───────┘   clock-domain typing   (spec/02 §6)
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
   mimz translate  — flavor/order conversion (lossless, trivia-preserving)
   mimz fmt        — formatter
   mimz lsp        — language server (Phase 4)
```

## 2. Components

| Component           | Phase   | Key design points                                                                                                                                                  |
| ------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **CLI** (`mimz`)    | 1       | `clap`; subcommands: `compile`, `check`, `sim`, `test`, `translate`, `fmt`, `build`                                                                                |
| **Keyword table**   | 1       | `keywords.toml` = source of truth; three columns per token, disjoint; loaded into one static map. Word changes are data changes                                    |
| **Lexer**           | 1       | Exact-match keywords after NFC normalization; Unicode identifiers; newline-terminator with continuation rules; full span tracking                                  |
| **Parser**          | 1 / 1.8 | Handwritten recursive descent; syntax profiles share all expression/declaration code, differ only in clause-head order; `syntax thamizh` directive selects profile |
| **AST**             | 1       | Rust enums + exhaustive match; spans everywhere; the single contract between front and back ends                                                                   |
| **Checker**         | 1       | The seven safety rules (spec/02 §6); each rule = its own pass with its own tests; errors via `miette`/`ariadne`                                                    |
| **Diagnostics**     | 1 / 1.8 | Human-authored message catalogs per language; Phase 1.8 adds the morphology helper (Tamil case suffixes on interpolated names)                                     |
| **Verilog emitter** | 1       | Dumb, readable Verilog-2005; sync active-high reset from reg reset values; no optimization here                                                                    |
| **Simulator**       | 1.5     | Elaborate → flat graph; event-driven kernel with two-phase commit (compute `<-`, then commit); 2-state by design; VCD out                                          |
| **IR**              | 2       | Typed netlist (cells/nets/widths/clock domains); dumpable text format; own validation pass (defense in depth)                                                      |
| **Optimizer**       | 2–3     | Const fold/propagate, dead-cell elimination, mux simplification; later retiming/sharing                                                                            |
| **Native backend**  | 3       | iCE40 only: techmap → annealing placer → pathfinder router → IceStorm-DB bitstream; validated differentially vs Yosys/nextpnr                                      |

## 3. Code Layout (Rust)

Phase 1 starts as **one crate** with modules — split into a workspace only when
a real consumer appears (e.g. the LSP needing `syntax` without backends):

```
mimz/
  Cargo.toml
  keywords.toml          # trilingual table — data, not code
  src/
    main.rs              # CLI
    lexer/               # tokens, keyword table loader, scanner
    parser/              # profiles: code_order.rs, thamizh_order.rs (P1.8)
    ast/                 # node types, spans, pretty-printer
    elaborate/           # import resolution, const folding, repeat unrolling
    check/               # one module per safety rule
    diag/                # message catalogs, morphology (P1.8)
    emit_verilog/
    sim/                 # (P1.5) elaborate, kernel, vcd
    ir/                  # (P2)
  tests/
    golden/              # source → expected tokens/AST/Verilog
    examples/            # compile + Icarus differential suite
```

Planned crate split (when needed): `mimz-syntax` (lexer/parser/AST/printer) ·
`mimz-check` · `mimz-backends` · `mimz` (CLI).

## 4. Cross-Cutting Invariants

1. **One AST.** No flavor, keyword, or word-order information survives past the
   parser except as display metadata for diagnostics/fmt.
2. **Spans everywhere.** Every AST node and IR object carries a source span —
   error quality is a core goal, not a feature.
3. **Data over code for language identity.** Keywords (and later error
   catalogs) are data files, so community review never touches Rust.
4. **Safety rules are passes with tests.** Each spec/02 §6 rule maps to one
   checker module and at least one rejection test.
5. **Differential validation at every new layer.** Simulator vs Icarus (1.5),
   IR vs AST simulation (2), native flow vs Yosys/nextpnr (3).
6. **Dumb first, fast later.** Emitters/backends start naive and readable;
   optimization lives in dedicated IR passes, never hidden in emitters.

## 5. Open Questions (log a Decision when resolved)

- Reset style v2: async-reset option? active-low? (v1: sync, active-high)
- Memories/arrays (`mem[depth] of bits[w]`) — Phase 2 spec bump (confirmed)
- CDC `sync` construct design (Phase 2 plan item)
- External Verilog module wrapping construct (Constitution item — design in Phase 2+)

Resolved 2026-06-10 (see log + spec v0.2): `import` semantics, `repeat`
generation, signed rules, Rust precedence, logical-op aliases, `.mimz`/`mimz`
naming, test grammar.
