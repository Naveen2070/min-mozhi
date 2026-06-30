# 13 — Tooling modules (`explain`, `translate`, `pretty`, `morph`, `sim`, `config`, `version`, `analysis`) + operational commands

Lib modules that **consume** the pipeline rather than forming a stage in
it. Each is the lib-backed core of one CLI subcommand, kept in the library (not
`main.rs`) so editors over the LSP and the future WASM playground reuse them.
`explain`/`translate`/`sim` landed 2026-06-13 as the "quick wins" block
(`docs/Ideas/language_plan.md` section 9 / `docs/plan/`): incremental,
edition-safe, and each a down-payment on a later phase.

## `explain` (`src/explain.rs`) — `mimz explain <CODE>`

The classroom version of every diagnostic. The one-line `help:` on a `Diag`
says how to fix an error in the moment (spec/01 G1); `explain` gives the long
form: what the rule is, why silicon needs it, the corrected shape, and a small
honest ASCII diagram where it helps. This is the incremental build-out of idea
8.1 (Elm-style didactic errors).

- `explain(code) -> Option<&'static str>` — case-insensitive, trims; `None` for
  an unknown code (the CLI then lists the valid ones).
- `codes()` — every code with an entry, in catalog order.
- Keyed off the stable E-codes (catalogs: [`11-checker.md`](11-checker.md) for
  E0xxx, [`06-diagnostics.md`](06-diagnostics.md) for lexer E10xx / parser
  E11xx / loader E12xx). A unit test pins every `diag::ALL_CHECKER_CODES` entry
  to a row, so a new checker code cannot ship without its explanation — the same
  docs-sync spirit as the error-fixture guard in `tests/errors.rs`.

## `translate` (`src/translate.rs`) — `mimz translate --to <flavor>`

Reskins a file's KEYWORDS into another language flavor (english / tanglish /
tamil), losslessly. This is the flavor-only half of `spec/04`'s `translate`; the
natural Tamil WORD-ORDER half (`--order code|thamizh`) lives in `pretty` below.

- Mechanism: re-lex, copy the source verbatim, and substitute **only** keyword
  token spans with the target column's canonical spelling (via the reverse
  `Kw → spellings` map `keywords::KeywordTable::canonical`). Comments, layout,
  identifiers, and numbers are untouched — lossless by construction — and any
  accepted alias (e.g. `include`) normalizes to its canonical spelling.
- Oracle: the `examples/{english,tanglish,tamil}/` folders are byte-identical
  keyword-swaps (R9), so `tests/translate.rs` checks that translating one
  flavor reproduces another at the token level, and that round-trips are
  byte-identical (modulo alias canonicalization).
- NOTE: tanglish/tamil targets use the FINALIZED keyword set v1 (native-speaker
  review closed in Phase 0; lang/keywords.toml header).
- **`--romanize-names` (opt-in):** also rewrites non-ASCII (Tamil) IDENTIFIERS to
  readable Latin, reusing the emitter's `romanize` (`கணக்கி` → `kannakki`) with
  the same `_2`/`_3` uniquing so a romanization never shadows an ASCII name or
  re-lexes as a keyword (`build_rename_map`). This is **one-way** —
  transliteration can't be inverted by rule — so it is OFF by default and the
  lossless round-trip contract only holds with it off. Applies to the
  keyword-only reskin (ignored, with a warning, under `--order`). Validated in
  `tests/translate.rs` over the `examples/tamil-pure/` showcase, including the
  invariant that romanizing-then-compiling matches compiling the original.
- **`--romanize-names` is reversible via a sidecar name-map.** With `-o <out>`,
  romanizing also writes `<out>.names.json` — a per-file [`NameMap`] (`{ version,
names }`, `romanized → original Tamil`, capturing the `_2`/`_3` uniquing).
  `restore_with_map` reads it on a reverse run and restores the exact Tamil
  identifiers, so `Tamil → Latin → Tamil` is byte-for-byte lossless for
  whitespace-separated code (the whole `examples/tamil-pure/` corpus)
  (`romanize_with_map` + `restore_with_map` share the `reskin` span-walk).
  **Boundary guard (2026-06-15):** Tamil script can be the ONLY separator between
  a numeric literal and a following keyword/identifier (`42தொகுதி`, `42கணக்கி`);
  reskinning to ASCII would glue them into an unlexable lexeme (`42module`,
  `42kannakki`), so `reskin` inserts a single separating space there. Such input
  round-trips token-equivalent but gains that space (not byte-identical) — caught
  by a deterministic fuzz audit and locked by a regression test in
  `tests/translate.rs`.
  Per-file, not central: the uniquing is per-file and the same Latin name can mean
  different Tamil words in different files. Without `-o` (stdout) no map is written
  and the CLI says so. `--names-map` and `--romanize-names` are opposite
  directions — using both is an error. The map carries a `version`; `load_name_map`
  rejects a map whose version this build does not understand with a clean error
  (fail closed, never mis-restore).
- **Auto-discovery (no flag needed).** A reverse reskin auto-loads the
  `<input>.names.json` sidecar when it sits next to the file — so
  `mimz translate k.mimz --to tamil` restores names with no `--names-map`. An
  explicit `--names-map <path>` overrides the discovered path; `--no-names-map`
  (or `[translate] names_map = "off"` in `mimz.toml`) disables it; `--order` and
  `--romanize-names` runs never auto-restore. The CLI prints a `note:` when it
  auto-loads a map.
- **`mimz fmt` rides this too.** The `fmt` subcommand (`fmt_file` in `commands/fmt.rs`)
  is `translate` pointed at a file in place: it normalizes every keyword to one
  flavor (default = the file's `morph::majority_flavor`, `--to` overrides),
  losslessly. `--strict` reports a mixed-flavor file (`morph::flavors_used`) and
  exits non-zero, still writing the fix. Word-ORDER reformatting stays with
  `--order` (the `pretty` path) — it is not lossless, so `fmt` does not use it.

## `pretty` (`src/pretty.rs`) — `mimz translate --order code|thamizh`

The word-ORDER half of `translate` (`spec/04` section 3, Phase 1.8). Where `translate`
re-spells keyword tokens, `pretty` re-emits the **AST** as Min-Mozhi source, so
it can move clause heads between the two word orders: `on rise(clk)` ⇄
`rise(clk) on`, `if c { }` ⇄ `c if { }`, `match e { }` ⇄ `e match { }`. Flavor
(from `translate`'s `TABLE.canonical`) and order compose freely, so
`--order thamizh --to tamil` yields natural-word-order Tamil. A thamizh-order
output gets a leading `syntax thamizh` directive so it re-parses.

- `pretty_print(&File, Flavor, Order) -> String`; `Order` is a public mirror of
  the parser's `pub(crate) Profile`. The order-sensitive forms are `OnBlock` /
  `SeqStmt::If` / `IfExpr` / `Match` and — since Phase 1.5 B7 — the **test header**
  (`TestDecl`: `M(args) kaaga "…" sodhanai { }` in thamizh order); the test-block
  `if` stays code-order. That completes all five word-order flips of the engine.
- **Canonical, not trivia-preserving (the key contrast with `translate`).** The
  AST carries no comments and no original layout, so the output is reformatted
  and **comments are dropped** — it is NOT byte-identical to the input. The
  contract is semantic: the output compiles to byte-identical Verilog and
  re-parses to the same AST. So `--to` alone stays lossless; `--order` re-emits.
- Precedence-sensitive operands (binary/unary operands, `if` conditions, `match`
  scrutinees) are parenthesized when non-atomic so the tree re-parses
  identically; `match` arms print one per line (the parser separates arms by
  newlines, not commas).
- Oracle (`tests/translate.rs`): for every example × flavor × order, the printed
  source compiles to the same Verilog as the original, and the printer is
  idempotent (a stable canonical form). The committed
  `tests/fixtures/grammar/traffic_light_tamil.thamizh.mimz` (generated by this
  tool) is a human-readable validation artifact.

## `morph` (`src/morph.rs`) — error-language selection + Tamil inflection

The "which language is this error in, and how do its identifiers inflect?" half
of the grammar engine (`spec/04` section 5, Phase 1.8). Two concerns, one module:

- **Selection.** `majority_flavor(tokens)` counts a file's keyword flavors (only
  keywords carry a `Flavor`); `effective_lang(cli, tokens)` lets a `--lang`
  override win, else uses the majority. Ties / keyword-free files default to
  English. This is spec/03's rule: errors render in the flavor the file
  predominantly uses, `--lang` overrides. `check`/`compile`/`eval` resolve it
  once and thread the `Flavor` into the human render path.
- **Inflection.** The four Tamil case suffixes (வேற்றுமை உருபுகள் -ஐ/-க்கு/-இல்/
  -ஆல்) are DATA in `lang/case_suffixes.toml` (the lang/keywords.toml doctrine — review
  edits the table, not the code); `inflect(name, case, flavor)` attaches one.
  This is "a suffix lookup table plus sandhi rules, not NLP" (spec/04 section 5).

- **Additive, English-fallback (the load-bearing contract).** The ~36 inline
  English `self.err()` messages are NOT touched. `localized_msg(diag, src,
flavor)` looks up a localized template for the diagnostic's E-code and, only if
  one exists for that flavor, returns it (interpolating the span-underlined
  identifier through `inflect`); otherwise the renderer keeps the English `msg`
  verbatim. So uncovered codes are byte-identical to before — proven by
  `tests/morph.rs::uncovered_code_is_identical_across_languages`. JSON diagnostics
  stay English (the machine contract in `06-diagnostics.md` is unchanged).
- **Native-speaker-authored (decision C3 ratified, 2026-06-15).** The localized
  catalog (`MESSAGES`, loaded once via `LazyLock` from `lang/messages.toml`) and the
  sandhi rules in `lang/case_suffixes.toml` came from native-speaker review — no longer
  a stub, no longer PROVISIONAL. `MESSAGES` localizes **33 of 44 checker codes**;
  E0403/E0404/E0405 are deferred (each emits many distinct message shapes — English
  kept, the Tamil drafts preserved as comments in `lang/messages.toml`). Templates also
  interpolate **structured args** the checker attaches via `Diag::with_arg`
  (`Checker::err_args`): `{expected}/{found}` (E0401), `{op}/{lhs}/{rhs}` (E0402),
  `{first}/{second}` (E0408), `{type}` (E0601). A leftover `{` in a rendered
  template forces the English fallback, so a typo'd placeholder fails safe (guarded
  by `tests/morph.rs::message_catalog_placeholders_are_known_tokens`).
- **Consumers.** `check`/`compile`/`eval` (`--lang`) and the **LSP** all localize
  through `morph::localized_msg` with `majority_flavor` — editors get the same
  flavored diagnostics as the CLI (`src/lsp.rs` `to_lsp`). JSON output stays
  English (machine contract).

## `sim` (`src/sim/`) — `mimz eval` / `mimz sim` / `mimz test` (Phase 1.5, complete)

The simulator. Phase 1.5 is **feature-complete** (B1–B8 + full parity C1–C4):
the combinational evaluator behind `mimz eval`, the event-driven kernel behind
`mimz sim` / `mimz test`, VCD + console-trace output, and the `test`-block
runner. The `src/sim/` directory:

| File           | Owns                                                                                                 |
| -------------- | ---------------------------------------------------------------------------------------------------- |
| `mod.rs`       | the module tree + the shared overview                                                                |
| `value.rs`     | the 2-state value model + expression evaluator (a `Resolver` trait both engines implement)           |
| `comb.rs`      | the combinational evaluator (`eval_outputs`) behind `mimz eval`                                      |
| `elaborate.rs` | `elaborate_project` flattening (instances, `repeat`, enums) + the `Rw` rewriter → a `Design`         |
| `kernel.rs`    | the event-driven, two-phase commit kernel that interprets a `Design`                                 |
| `run.rs`       | the default stimulus + `comb_run` per-vector settle; the `MAX_SIM_CYCLES`/`MAX_SWEEP_VECTORS` bounds |
| `vcd.rs`       | the hand-written 2-state VCD writer                                                                  |
| `trace.rs`     | the console trace table (`--trace` / `--trace=changes`)                                              |
| `harness.rs`   | the `test`-block runner (`drive`/`tick`/`expect`/`if`) behind `mimz test`                            |

- **`mimz eval` (`comb`).** `sim::comb::eval_outputs` interprets a single
  **combinational** module: given a value per input, it computes the outputs by
  walking the AST. Values are unsigned bit-vectors up to 128 bits, carrying a
  width and a signed flag; it honors the spec's width semantics (lossless
  `+ - *` grow, the `+% -% *%` family wraps, slices/concat/`extend`/`trunc`
  resize), so a result matches what the Verilog emitter would produce. A private
  `const_eval` delegates to the checker's hardened `consteval::eval` (single
  source of truth, `checked_*` arithmetic) — the 2026-06-14 audit removed the
  earlier divergent copy (`docs/audit/security.md`, SEC-2). Surface:
  `--in a=3,b=5`, `--module`, `--param`.
- **`mimz sim` (`elaborate` → `kernel` → `run`/`vcd`/`trace`).** `load_project`,
  flatten to a `Design` (folding widths/resets, inlining instances, unrolling
  `repeat`, encoding enum signals by variant index), then run the event-driven
  two-phase kernel under a default stimulus (clocked) or settle one frame per
  input vector (combinational `comb_run`). Emits a GTKWave VCD (`-o`) and/or a
  console trace. `--cycles`/`--sweep` are bounded by `MAX_SIM_CYCLES` /
  `MAX_SWEEP_VECTORS` (`run.rs`, audit SEC-5).
- **`mimz test` (`harness`).** Runs each `test` block (`drive`/`tick`/`expect`/
  `if`) on the kernel, halting a failing `expect` with a teaching message
  (expression source + cycle + each operand's value) and exiting non-zero on any
  failure; `--filter`/`--trace`/`--verbose`/`--signals` supported. The
  `tick`-count is bounded by `MAX_SIM_CYCLES`.
- This is the engine the 8.5 hardware REPL and the WASM playground will ride on,
  which is why it lives in the lib and stays callable on a single module.
- **Independently judged.** The Layer-3 Icarus differential
  (`tests/icarus.rs::our_simulator_matches_icarus_bit_for_bit`) pits this
  simulator (engine AND VCD waveform) against Icarus bit-for-bit across the whole
  single-file corpus (21 examples).

## `config` (`src/config.rs`) — `mimz.toml` project defaults

Per-project defaults for CLI flags, so a flag set once for a project need not be
repeated. **Precedence: CLI flag › `mimz.toml` value › built-in default** — the
config only fills in what the command line omitted.

- **Discovery.** `Config::discover` walks up from the input file (canonicalized
  first) to the nearest `mimz.toml`, like `Cargo.toml`/`rustfmt.toml`; the global
  `--config <path>` overrides the search. `Config::resolve(input, explicit)` is
  the entry point used by every subcommand handler in `commands/`; no file found ⇒
  `Config::default()` (all `None`).
- **Format & shape.** TOML (matching `lang/keywords.toml`/`lang/case_suffixes.toml`; the
  machine-written name-map sidecar stays JSON). All fields are `Option`, so
  "absent" is distinct from "set", and the CLI does the
  `cli.or(config).unwrap_or(default)` merge. `deny_unknown_fields` turns a typo'd
  key into an error, not a silent no-op; a malformed file is a clean error
  (user-authored + per-project — unlike the embedded keyword tables, which panic).
- **Keys.** Top-level `lang` (diagnostics language for `check`/`compile`/`eval`);
  `[compile]` `emit_testbench`; `[translate]` `to` / `order` / `romanize_names` / `names_map` (`"auto"` | `"off"`
  — controls the sidecar auto-discovery above); `[fmt]` `to` / `strict`.

```toml
# mimz.toml — CLI flags always override these.
lang = "tamil"

[compile]
emit_testbench = true

[translate]
to             = "tanglish"
romanize_names = false
names_map      = "auto"

[fmt]
strict = true
```

## `version` (`src/version.rs`) — the two version axes

Min-Mozhi has **two independent versions**, the way `rustc 1.x` is distinct from
the Rust `2021` edition; conflating them is the confusion this module removes.

- **Compiler version** — `COMPILER_VERSION` (from `env!("CARGO_PKG_VERSION")`, the
  single crate source). Also stamped into the Verilog header banner.
- **Keyword-set version** — `KEYWORD_SET_VERSION`, cross-checked against the
  `version` field in `lang/keywords.toml` (`KeywordTable::version()`); a unit test
  asserts the two agree, so a keyword-table bump that forgets the constant fails
  CI.
- **Language edition** — an `Edition { variant, year, code }` shown
  `variant-year-code` (e.g. `wingless-butterfly-2026-1`). `EDITION_HISTORY` is a
  `const` table, one row per edition, kept **in source** so the language's history
  is transparent; `current()` returns the last row, and a test asserts it is the
  tail.

`version_block()` renders the uname-style block `mimz --version` prints — the
edition codename on top, then the compiler and edition lines:

```text
Wingless Butterfly
mimz    0.1.0                       (compiler)
edition wingless-butterfly-2026-1   (language)
```

`main.rs` intercepts `--version` to print this block (clap's own `--version`
would prepend the binary name and lose the codename-on-top layout); `-V` keeps
clap's short form. The edition design rationale lives in `spec/06-editions.md`.

## `analysis` (`src/analysis.rs`) + the LSP editor features

The editor-DX layer added 2026-06-25 (`phase-4-lsp-dx`, on top of the
diagnostics-only LSP v0). It rides on the parser's `parse_recover` partial
trees, so it works on half-typed files.

The logic splits cleanly across the lib/bin boundary, the same way `morph`'s
localization does:

- **`src/analysis.rs` (library, pure, async-free)** — reusable by the future
  WASM playground, unit-tested without tower-lsp. All offsets are **bytes**.
  - `build_index(&[LoadedFile]) -> SymbolIndex` — one pass over every file's AST
    collecting a `Symbol` (name, `SymKind`, `file_idx`, defining span, hover
    `render`, enclosing `module_idx`) for each definition. `Error` placeholder
    nodes are skipped at every level (no cascade on broken input).
  - `resolve_at(&index, &files, file_idx, offset) -> Option<usize>` — smallest
    name span covering the cursor (definition **or** use), resolved by scope:
    enclosing module → same module any file → file-level → anywhere. Cross-file,
    in-tree. `test "…" for M { … }` blocks resolve too: the module-under-test
    name and the body's driven inputs / `expect` signals scope to M's ports
    (the cross-file `same_module_any_file` tier carries this).
  - `completions(&index, &files, file_idx, offset) -> Vec<Candidate>` — in-scope
    identifiers (the enclosing module's members + file-level consts/enums + every
    module name) plus keywords in the file's **majority flavor**
    (`morph::majority_flavor` + `KeywordTable::canonical_spellings`); no
    cross-flavor keyword leak. Prefix filtering is left to the editor.
- **`src/lsp.rs` (binary, tower-lsp adapter)** — caches each open document's
  text (`docs: Mutex<HashMap<Url, String>>`, updated on didOpen/didChange/
  didSave), registers `hover` / `definition` / `completion` capabilities, and in
  each handler converts the LSP UTF-16 `Position` to a byte offset (`offset`, the
  inverse of `position`), runs `load_for_features` (parse_recover + the import
  walk, skipping `std.*` virtual imports), then calls the lib and maps the result
  to `Hover` / `Location` / `CompletionItem[]`. A `std:` virtual path yields no
  go-to-def location (no real file URI). **Diagnostics are untouched** — `analyze`
  keeps using the strict `parser::parse` (no checker cascade on half-typed input).

Deferred: dot-member completion (`inst.port`, enum variants after `.`),
flavor-localized hover render (English in v1), `ExprKind::Error` for
expression-level recovery, and `did_close` cache eviction (the doc cache grows
for the session).

## Operational commands (bin-only: `init` / `doctor` / `completions` / `check --watch`)

These are **not** lib modules — they live in `src/commands/` (bin-only) and touch
the OS, not the pipeline, so they stay out of the library (the WASM build and LSP
never need them). They are thin by design; documented here so the maintainer map
is complete (the friendly walkthrough is `docs/source-guide/09-tooling-and-entry.md`).

- **`mimz init <name>` (`commands/init.rs`).** Scaffolds `./<name>/`: a documented
  `mimz.toml` and a starter `<name>.mimz` (a free-running counter + a passing inline
  `test` block), so `mimz test`/`mimz compile` work on the new project immediately.
  `module_name` derives a valid identifier from the project name (PascalCase the
  alphanumeric runs, Tamil letters kept as-is, `Top` fallback). Refuses a non-empty
  target directory; a name containing a path separator is an error.
- **`mimz doctor` (alias `mimz env`, `commands/doctor.rs`).** Prints a toolchain &
  environment report and runs an in-memory pipeline smoke test (`compile_string` on a
  4-bit adder). The runtime is fully in-process (sim/test/eval never shell out), so
  `iverilog`/`verilator`/`gtkwave` are **optional** cross-check/waveform tools —
  missing ones are `Warn`, only a broken pipeline / unwritable temp dir / malformed
  `mimz.toml` is a `Fail` (non-zero exit). `--dev` adds the contributor toolchain
  (rustc, cargo, the `wasm32-unknown-unknown` target, wasm-pack, nextest, node).
- **`mimz completions <shell>` (`commands/completions.rs`).** Prints a shell
  tab-completion script (bash/zsh/fish/powershell/elvish) to stdout, generated from
  the live clap command tree (`crate::Cli::command()`), so it can never drift from
  the real subcommands/flags.
- **`mimz check --watch` (`commands/check.rs`, `watch` feature).** Re-runs `run_check`
  on every save. It watches the **directories** holding the entry file and every
  transitive import (not the files — so editor atomic-saves still fire), reacting only
  to `.mimz` changes, debouncing the per-save event burst (100 ms drain). The watch set
  is reconciled to the project after each run: new import dirs are added; dirs that
  dropped out of the project are unwatched — but only after a _successful_ load, so a
  parse error never loses the last good watch set and the fix-save is never missed
  (rationale: `docs/log/2026-06-25.md`). Gated behind the `watch` feature (on by
  default; the WASM build drops it, since `notify` pulls OS file-watch APIs that don't
  build on `wasm32`).

## Scope discipline

Most of these grow incrementally: `explain` grows one code at a time,
`translate`/`pretty` cover keyword flavor and all five landed word-order flips
(clocked block, conditional, if-expression, match, test header), and `morph`
ships the selection + inflection mechanism with the native-authored catalog
(33 of 44 codes; C3 ratified 2026-06-15). `sim` is the exception — Phase 1.5 is
feature-complete (the combinational `comb`, the event-driven kernel, VCD/trace,
and `mimz test`). Each documents its own limits in its module header so the
honesty rule (spec/01) holds for the tooling too.
