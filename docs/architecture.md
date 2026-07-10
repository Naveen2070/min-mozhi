# Min-Mozhi — Architecture

> Living document (RULES.md R3: update whenever components or data flow change).
> Status: **Phases 1, 1.8, and 1.5 complete** — lexer, parser (code-order +
> thamizh-order), full checker (seven passes), Verilog emitter (repeat unrolling,
> transliteration, signed), CLI
> (`check`/`compile`/`lsp`/`explain`/`translate`/`eval`/`sim`/`test`/`fmt`,
> `--json`), LSP v0, all Icarus-validated. The **own simulator** is built and shipped
> (`mimz sim` clocked + combinational, deterministic VCD; `mimz test`
> tick/expect; three-layer Icarus differential). The **formatter** is shipped
> (`mimz fmt` — keyword normalization, strict-mode mix detection). The IR is still design.
> Last updated: 2026-07-10 (workspace-split: the compiler is now a 3-crate
> Cargo workspace — `mimz-core` (pure pipeline), `mimz-sim` (simulator +
> runner), `mimz` (root shell crate: fs I/O, the new `emulate` hardware
> peripherals, CLI facade); §3 and Repository layout rewritten). Prior:
> 2026-07-06 (comprehensive doc audit — error codes E0001–E0909, example count 178, error fixtures 102, golden .v 68, lib.rs pub mod 18, accurate per-flavor breakdowns)
> spec version numbers, fixture counts, file counts across guide/code/source-guide)

---

## 1. The Pipeline

```
 source (.mimz)  ── any keyword flavor, code-order or thamizh-order
        │
        ▼
 ┌──────────────┐   one trilingual keyword table (lang/keywords.toml)
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
   └──────┬───────┘   clock-domain typing · function checking E0801–E0812   (spec/02 section 6)
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
                     word-order (--order thamizh) ✅ Phase 1.8;
                     --romanize-names + reversible name-map sidecar ✅ 2026-06-15
   mimz explain    — long-form text per E-code ✅ 2026-06-13 (lib `explain`)
   mimz eval       — combinational evaluator ✅ 2026-06-13 (lib `sim::comb`;
                     a slice of the Phase 1.5 simulator — no clocks/regs)
   mimz sim        — full simulator ✅ Phase 1.5 (lib `src/sim/`; clocked +
                     combinational, --in/--sweep, --cycles, --trace, -o .vcd)
   mimz test       — tick/expect test runner ✅ Phase 1.5 (lib `sim::harness`)
    mimz fmt        — keyword normalization + strict-mode ✅ 2026-06-15
    mimz lsp        — language server ✅ v0 SHIPPED 2026-06-12
                     (diagnostics-only; hover/go-to-def in Phase 4)
```

## 2. Components

Built ✅ as of 2026-06-12 (Phase 1 complete):

- keyword table, lexer, parser (code-order), AST;
- checker — ALL spec/02 section 6 safety rules (names/consts/E-codes,
  width/type E04xx, driver/cycle E05xx, instantiation completeness E0302,
  match exhaustiveness E06xx, clock domains E0701) + combinational functions
  E0801–E0808 (symbol registration, arity, return width, recursion,
  payload-bindings, OR-arm intersection);
- Verilog emitter (repeat unrolling, Tamil→ASCII transliteration,
  `wire signed`; validated by Icarus differential tests and golden files);
- CLI (`check`, `compile`, `lsp`, `--json`);
- the diagnostics-only LSP v0 with its VS Code client.

Also built (Phase 1.8 + 1.5): the thamizh-order parser profile, and the own
simulator (`src/sim/`, `mimz sim`/`mimz test`).

The IR and native backend remain planned.

| Component           | Phase   | Key design points                                                                                                                                                                                                 |
| ------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **CLI** (`mimz`)    | 1 / 1.5 | `clap`; subcommands: `check`, `compile`, `fmt`, `translate`, `eval`, `explain`, `lsp`, `sim`, `test`, `init`, `doctor`, `completions`, `lint`, `repl`, `eject` (handlers in `src/commands/`)                      |
| **Keyword table**   | 1       | `lang/keywords.toml` = source of truth; three columns per token, disjoint; loaded into one static map. Word changes are data changes                                                                              |
| **Lexer**           | 1       | Exact-match keywords after NFC normalization; Unicode identifiers; newline-terminator with continuation rules; full span tracking                                                                                 |
| **Parser**          | 1 / 1.8 | Handwritten recursive descent; syntax profiles share all expression/declaration code, differ only in clause-head order; `syntax thamizh` directive selects profile                                                |
| **AST**             | 1       | Rust enums + exhaustive match; spans everywhere; the single contract between front and back ends                                                                                                                  |
| **Checker**         | 1       | ✅ ALL spec/02 section 6 safety rules; seven passes (symbols/consteval/names/widths/drivers/clocks + funcs cycle detection + funcs unreachable), each with its own tests; stable E-codes E0001–E0909              |
| **Diagnostics**     | 1 / 1.8 | ✅ stable codes on EVERY stage (lexer E10xx, parser E11xx, loader E12xx) + `--json` wire format; Phase 1.8 adds the per-language catalogs + morphology helper                                                     |
| **Verilog emitter** | 1       | Dumb, readable Verilog-2005; sync active-high reset from reg reset values; no optimization here                                                                                                                   |
| **Simulator**       | 1.5     | ✅ Elaborate → flat graph; event-driven kernel with two-phase commit (compute `<-`, then commit); 2-state by design; deterministic VCD out; `src/sim/` (comb, kernel, elaborate, harness, run, value, vcd, trace) |
| **IR**              | 2       | Typed netlist (cells/nets/widths/clock domains); dumpable text format; own validation pass (defense in depth)                                                                                                     |
| **Optimizer**       | 2–3     | Const fold/propagate, dead-cell elimination, mux simplification; later retiming/sharing                                                                                                                           |
| **Native backend**  | 3       | iCE40 only: techmap → annealing placer → pathfinder router → IceStorm-DB bitstream; validated differentially vs Yosys/nextpnr                                                                                     |

## 3. Code Layout (Rust)

A Cargo **workspace** of three crates plus two thin binaries — the `mimz`
CLI and the `mimz-bench` harness. The lib/bin split happened 2026-06-12 when
the LSP arrived; the crate-per-boundary split into `mimz-core`/`mimz-sim`
happened later, once a second non-CLI, non-terminal consumer (the WASM
playground) needed the pure pipeline + simulator without dragging in
`ratatui`/filesystem/terminal code:

- **`mimz-core`** — the pure pipeline (lexer → parser → AST → checker →
  Verilog emitter) plus the tooling modules that never touch a filesystem
  or terminal (`explain`, `lint`, `translate`, `pretty`, `morph`,
  `analysis`, `stdlib`, `version`), plus the pure remainder of `project`
  (the `LoadedFile` struct and `render_diags`/`render_diags_lang`
  diagnostic rendering — no fs I/O, no NFC normalization, no import
  resolution; the shell's `project.rs` does those and re-exports these
  types via `pub use mimz_core::project::{LoadedFile, render_diags,
render_diags_lang};`).
- **`mimz-sim`** — the event-driven simulator (`sim/`) and the in-memory
  command runner (`runner.rs`), depending only on `mimz-core`. Defines the
  `EmulationHost` trait + `Direction` enum (`sim/host.rs`) so the simulator
  can drive hardware peripherals without depending on them.
- **`mimz`** (root `src/`, this crate) — the thin shell: filesystem I/O
  (`project.rs`, `config.rs`), the native hardware-emulation peripherals
  (`emulate/`, feature-gated behind `hw-emulation`, implementing
  `EmulationHost`), the CLI binary, the LSP server, `mimz-bench`, and a
  facade re-exporting `mimz-core`/`mimz-sim` under the same `mimz::…` paths
  that existed before the split (`src/lib.rs`'s crate-map table is the
  authoritative per-module list).

```
mimz/ (workspace root)
  Cargo.toml
  lang/keywords.toml          # trilingual table — data, not code
  crates/
    mimz-core/
      Cargo.toml
      src/
        lib.rs             # pub mod × 11 + REPEAT_BUDGET const         ✅
        span.rs              # byte-offset spans                        ✅
        diag.rs               # teaching diagnostics + JSON format       ✅
        morph.rs              # error-language + Tamil inflection        ✅
        translate.rs          # keyword reskin between flavors           ✅
        pretty.rs             # AST → source pretty-printer              ✅
        explain.rs            # long-form error code explanations        ✅
        lint.rs               # style/hygiene warnings                   ✅
        version.rs            # compiler version + language edition      ✅
        stdlib.rs             # embedded standard library modules        ✅
        analysis.rs           # editor symbol index + offset→def/completion ✅
        project.rs            # LoadedFile + render_diags(_lang) only (no fs I/O)   ✅
        ast/                  # the ONE shared AST                       ✅
          mod.rs                 #   files, modules, decls, statements
          expr.rs                #   expressions, patterns, operators
          sync_loop_lower.rs     #   sync-loop desugaring
        lexer/                # E10xx                                   ✅
          mod.rs                 #   scanner + newline policy
          token.rs               #   token kinds, keyword enum, flavors
          keywords.rs            #   lang/keywords.toml loader (REQUIRED_KEYS)
          tests.rs               #   unit tests
        parser/               # E11xx                                   ✅
          mod.rs                 #   entry, Parser state + Profile, plumbing
          items/                 #   file/module/inst/seq/test/func/bundle items
          expr.rs                #   precedence climbing, patterns
          tests.rs               #   unit tests
        emit_verilog/         #                                         ✅
          mod.rs                 #   Project symtab, entry, helpers
          module.rs              #   shells, instances, always-blocks
          expr.rs                #   expression rendering
          translit.rs            #   Tamil → ASCII identifier pre-pass
          testbench.rs           #   standalone Verilog testbench gen
        checker/              # seven passes, E0001–E0909              ✅
          mod.rs                 #   entry, Checker state, err plumbing
          symbols.rs             #   project tables + duplicates
          consteval.rs           #   compile-time evaluation
          names.rs               #   names, structure, E0302/E0303
          widths/                #   width/type + exhaustiveness (E04xx, E06xx)
          drivers.rs             #   single-driver + comb-DAG (E05xx)
          clocks.rs              #   clock-domain ownership (E0701)
          funcs.rs               #   fn safety (E0801–E0812)
          tests.rs               #   unit tests (one per E-code)
    mimz-sim/
      Cargo.toml
      src/
        lib.rs             # pub mod sim/runner; compile_string entry    ✅
        runner.rs            # in-memory command engine (playground)     ✅
        sim/                 # (P1.5)                                    ✅
          mod.rs                 #   module entry + re-exports
          comb.rs                #   combinational evaluator
          kernel.rs              #   event-driven kernel
          elaborate.rs           #   AST → flat Design
          harness.rs             #   test block runner (Box<dyn EmulationHost>)
          host.rs                #   EmulationHost trait + Direction enum
          run.rs                 #   default stimulus
          value.rs               #   bit-vector value model
          vcd.rs                 #   VCD waveform writer
          trace.rs               #   console trace renderer
    mimz-wasm/            # WASM playground wrapper, depends on mimz-sim  ✅
  src/                    # the `mimz` shell crate
    lib.rs               # facade: pub mod project/config/emulate + pub use mimz-core/mimz-sim ✅
    main.rs              # thin CLI (clap, dispatch, Output)     ✅
    commands/            #   per-subcommand handlers + helpers   ✅
    lsp.rs               # `mimz lsp` server (BIN-only module,  ✅
                         #   keeps tokio out of the lib)
    bin/mimz-bench/      # benchmark harness (docs/code/12)     ✅
                         #   main.rs / metrics/ / html.rs
    project.rs           # fs-touching remainder: file loading, LoadError ✅
    config.rs            # mimz.toml project defaults            ✅
    emulate/             # native hw-emulation peripherals (feature-gated: hw-emulation) ✅
      mod.rs               #   peripheral registry
      host.rs              #   shell's EmulationHost impl
      dashboard.rs         #   ratatui live dashboard
      led.rs, speaker.rs, uart_rx.rs, uart_tx.rs   # peripherals
    ir/                  # (P2)
  tests/                 # 18 test files
    examples.rs          # all 178 examples (34 × 4 complete flavors + 5 stdlib + 1 lib each + 19 tamil-pure) ✅
    cli.rs               # CLI surface: init / doctor / completions  ✅
    errors.rs            # broken fixtures, one code per E-code  ✅
    icarus.rs            # iverilog lint + self-checking TBs +   ✅
                         #   our_simulator_matches_icarus_bit_for_bit (~21 ex)
    sim.rs / test_run.rs # simulator + tick/expect runner tests  ✅
    eval.rs / fmt.rs / translate.rs / morph.rs / grammar.rs / config.rs  ✅
    lsp.rs               # wire-protocol smoke test             ✅
    docs_sync.rs         # docs ↔ code staleness guard          ✅
    grammar_sync.rs      # VS Code grammar ↔ lang/keywords.toml      ✅
    compile_string.rs    # library API tests                    ✅
    stdlib.rs            # importable std.* library tests       ✅
    wasm_parity.rs       # WASM ↔ CLI output parity             ✅
    golden/              # pinned .v output per base example (68 .v + 14 _tb.v + 1 .vcd)
    fixtures/errors/     # the broken corpus (102 .mimz files)
  benches/
    compile.rs           # criterion per-phase micro-benchmarks ✅
                         #   (cargo bench; lexer/parser/checker/emit)
  fuzz/                  # 4 libFuzzer targets (nightly only)   ✅
  editors/vscode/        # extension: grammar + LSP client      ✅
```

The `mimz-core`/`mimz-sim`/`mimz` workspace split above is the crate split;
no further split is planned unless a new trigger fires (section 5).

### Repository layout

```
min-mozhi/
  README.md, LICENSE-*, Cargo.toml
  lang/keywords.toml        # language data (embedded at build time)
  ROADMAP.md                 # roadmap summary
  spec/                     # the LANGUAGE — normative, versioned (v0.2)
  docs/                     # the PROJECT — plan/, log/, archive/, RULES,
                            #   guide/, code/, source-guide/, audit/, Ideas/
  src/                      # the shell crate: fs I/O, emulate/, CLI, LSP (tree above)
  crates/mimz-core/         # pure pipeline + most tooling (tree above)
  crates/mimz-sim/          # event-driven simulator + runner (tree above)
  crates/mimz-wasm/         # WASM playground wrapper (depends on mimz-sim)
  tests/                    # integration tests (18 files)
  benches/                  # Criterion micro-benchmarks
  fuzz/                     # libFuzzer targets (4)
  examples/                 # .mimz programs (34 designs × 4 complete flavors + 5 stdlib + 1 lib each + 19 tamil-pure = 178)
  demo/                     # alu + cpu hardware demos
  editors/vscode/           # VS Code extension (grammar + LSP client)
  site/                     # Astro documentation website (deployed)
  tools/test-summary/       # cargo test wrapper (dev helper)
  .github/workflows/        # CI (ci, deploy-site, release)
```

Separation rule: `spec/` defines the language and outlives any
implementation; `docs/` tracks this implementation and process. Never mix.

Future directories (created when their trigger fires, not before):

| Directory  | Arrives with                                   |
| ---------- | ---------------------------------------------- |
| `stdlib/`  | `.mimz` standard library modules (Phase 4)     |
| `targets/` | board/constraint files (Phase 2 hardware flow) |

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
7. **Pure core, impure shell.** The compiler stages — `lexer`, `parser`,
   `checker`, `emit_verilog`, `sim`, `ast` — are string → string/AST pure: no
   `std::fs`/`env`/`process`, no `tokio`, no globals. All OS coupling lives in
   the CLI shell (`src/commands/`, `main.rs`, `config.rs`, `project.rs`,
   `lsp.rs`, `src/bin/`) and the optional `lsp`/`bench` features. This is what
   lets `crates/mimz-wasm` build the lib with `default-features = false` and run
   the whole pipeline in the browser (`wasm_parity` guards it). Keep it: a new
   OS dependency belongs in the shell, never in a core stage.

## 5. Evolution Triggers (planned inflection points — not emergencies)

The architecture is staged on purpose; each piece below is _correct now_ and
has a known moment when it must change. When a trigger fires, do the listed
move and log it (R3). Letting a trigger pass is how architectures rot.

| Current shape                                        | Trigger                                                                              | Required move                                                                                                                          |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| String-based Verilog emitter reading the AST         | Checker lands (work item 4)                                                          | Move all semantic errors (unknown module, port connectivity) out of the emitter into checker passes; emitter only renders              |
| Emitter has no width knowledge (`extend` is a no-op) | IR exists (Phase 2)                                                                  | Emit from typed IR, demote AST→Verilog path to a debug backend                                                                         |
| ~~Diagnostics are free-text, no IDs~~                | ✅ FIRED — every stage's errors carry stable codes (2026-06-12; map in docs/code/06) | Done: codes everywhere; the P1.8 message catalogs key off them                                                                         |
| ~~Single binary crate~~                              | ✅ FIRED — LSP + `--json` consumers arrived (2026-06-12)                             | Done: `lib.rs` + thin `main.rs`; the WORKSPACE split (`mimz-syntax`/`mimz-check`/`mimz-backends`) stays trigger-based                  |
| Lexer discards comments/whitespace                   | `mimz fmt` work starts                                                               | Add a trivia-preserving lexing mode; `translate` stays token-level and is unaffected                                                   |
| Tokens own `String`s, cloned freely                  | Compile time on real projects becomes noticeable (not before)                        | String interning + token indices — contained inside `lexer/`                                                                           |
| ~~Emitter semantic checks duplicated per backend~~   | ✅ FIRED — Simulator (P1.5) shipped                                                  | Done: the simulator elaborates from `project.rs` + checker output (`src/sim/elaborate.rs`); both backends consume the same checked AST |

## 6. Open Questions (log a Decision when resolved)

- Reset style v2: async-reset option? active-low? (v1: sync, active-high)
- Memories/arrays (`mem[depth] of bits[w]`) — Phase 2 spec bump (confirmed)
- CDC `sync` construct design (Phase 2 plan item)
- External Verilog module wrapping construct (Constitution item — design in Phase 2+;
  `extern` keyword to be RESERVED before v0.1.0 so the additive feature stays
  edition-safe — see `docs/Ideas/architectural_ideas.md` idea 3 + the freeze
  checklist in `docs/Ideas/language_plan.md` section 9)

Resolved 2026-06-10 (see log + spec v0.2): `import` semantics, `repeat`
generation, signed rules, Rust precedence, logical-op aliases, `.mimz`/`mimz`
naming, test grammar.
