# 13 — Tooling modules (`explain`, `translate`, `pretty`, `morph`, `sim`)

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
  review closed in Phase 0; keywords.toml header).
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

The word-ORDER half of `translate` (`spec/04` §3, Phase 1.8). Where `translate`
re-spells keyword tokens, `pretty` re-emits the **AST** as Min-Mozhi source, so
it can move clause heads between the two word orders: `on rise(clk)` ⇄
`rise(clk) on`, `if c { }` ⇄ `c if { }`, `match e { }` ⇄ `e match { }`. Flavor
(from `translate`'s `TABLE.canonical`) and order compose freely, so
`--order thamizh --to tamil` yields natural-word-order Tamil. A thamizh-order
output gets a leading `syntax thamizh` directive so it re-parses.

- `pretty_print(&File, Flavor, Order) -> String`; `Order` is a public mirror of
  the parser's `pub(crate) Profile`. Only `OnBlock` / `SeqStmt::If` / `IfExpr` /
  `Match` are order-sensitive; the test header and test-block `if` stay
  code-order (the test-form flip is deferred to Phase 1.5).
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
of the grammar engine (`spec/04` §5, Phase 1.8). Two concerns, one module:

- **Selection.** `majority_flavor(tokens)` counts a file's keyword flavors (only
  keywords carry a `Flavor`); `effective_lang(cli, tokens)` lets a `--lang`
  override win, else uses the majority. Ties / keyword-free files default to
  English. This is spec/03's rule: errors render in the flavor the file
  predominantly uses, `--lang` overrides. `check`/`compile`/`eval` resolve it
  once and thread the `Flavor` into the human render path.
- **Inflection.** The four Tamil case suffixes (வேற்றுமை உருபுகள் -ஐ/-க்கு/-இல்/
  -ஆல்) are DATA in `case_suffixes.toml` (the keywords.toml doctrine — review
  edits the table, not the code); `inflect(name, case, flavor)` attaches one.
  This is "a suffix lookup table plus sandhi rules, not NLP" (spec/04 §5).

- **Additive, English-fallback (the load-bearing contract).** The ~36 inline
  English `self.err()` messages are NOT touched. `localized_msg(diag, src,
flavor)` looks up a localized template for the diagnostic's E-code and, only if
  one exists for that flavor, returns it (interpolating the span-underlined
  identifier through `inflect`); otherwise the renderer keeps the English `msg`
  verbatim. So uncovered codes are byte-identical to before — proven by
  `tests/morph.rs::uncovered_code_is_identical_across_languages`. JSON diagnostics
  stay English (the machine contract in `06-diagnostics.md` is unchanged).
- **Stub / panel-gated.** The localized catalog (`MESSAGES`) holds ONE worked
  shape (E0501) so the select → catalog → inflect → render path is real and
  tested; the full Tamil + Tanglish catalog and the final sandhi rules await the
  native-speaker panel (decision C3). The committed join rule is minimal and
  marked PROVISIONAL.
- **Consumers.** `check`/`compile`/`eval` (`--lang`) and the **LSP** all localize
  through `morph::localized_msg` with `majority_flavor` — editors get the same
  flavored diagnostics as the CLI (`src/lsp.rs` `to_lsp`). JSON output stays
  English (machine contract).

## `sim` (`src/sim/`) — `mimz eval` (combinational slice of Phase 1.5)

`sim::comb::eval_outputs` interprets a single **combinational** module: given a
value per input, it computes the outputs by walking the AST. No clock, no `reg`,
no instances, no `repeat` — those are rejected with a clear message, not
half-evaluated (the full event-driven engine, VCD, and `test` execution are
Phase 1.5 proper, `docs/plan/phase-1.5-simulator.md`).

- Values are unsigned bit-vectors up to 128 bits, carrying a width and a signed
  flag; it honors the spec's width semantics (lossless `+ - *` grow, the
  `+% -% *%` family wraps, slices/concat/`extend`/`trunc` resize), so a result
  matches what the Verilog emitter would produce for the same logic.
- A private `const_eval` delegates to the checker's hardened
  `consteval::eval` (single source of truth, `checked_*` arithmetic) for widths,
  parameters, consts, and slice/index bounds — the 2026-06-14 security audit
  removed the earlier divergent copy (`docs/audit/security.md`, SEC-2).
- This is the engine the 8.5 hardware REPL and the WASM playground will ride on,
  which is why it lives in the lib and stays callable on a single module. The
  `mimz eval` CLI is its experimental surface (`--in a=3,b=5`, `--module`,
  `--param`).

## `config` (`src/config.rs`) — `mimz.toml` project defaults

Per-project defaults for CLI flags, so a flag set once for a project need not be
repeated. **Precedence: CLI flag › `mimz.toml` value › built-in default** — the
config only fills in what the command line omitted.

- **Discovery.** `Config::discover` walks up from the input file (canonicalized
  first) to the nearest `mimz.toml`, like `Cargo.toml`/`rustfmt.toml`; the global
  `--config <path>` overrides the search. `Config::resolve(input, explicit)` is
  the entry point used by every subcommand handler in `commands/`; no file found ⇒
  `Config::default()` (all `None`).
- **Format & shape.** TOML (matching `keywords.toml`/`case_suffixes.toml`; the
  machine-written name-map sidecar stays JSON). All fields are `Option`, so
  "absent" is distinct from "set", and the CLI does the
  `cli.or(config).unwrap_or(default)` merge. `deny_unknown_fields` turns a typo'd
  key into an error, not a silent no-op; a malformed file is a clean error
  (user-authored + per-project — unlike the embedded keyword tables, which panic).
- **Keys.** Top-level `lang` (diagnostics language for `check`/`compile`/`eval`);
  `[translate]` `to` / `order` / `romanize_names` / `names_map` (`"auto"` | `"off"`
  — controls the sidecar auto-discovery above); `[fmt]` `to` / `strict`.

```toml
# mimz.toml — CLI flags always override these.
lang = "tamil"

[translate]
to             = "tanglish"
romanize_names = false
names_map      = "auto"

[fmt]
strict = true
```

## Scope discipline

These are intentionally small slices, not finished features: `explain` grows one
code at a time, `translate`/`pretty` cover keyword flavor and the four landed
word-order flips (the test-form flip is still ahead), `morph` ships the
selection + inflection mechanism with a stub catalog (the human-authored
catalog + final sandhi are panel-gated, C3), and `sim::comb` is combinational
only (the kernel is Phase 1.5). Each
documents its own limits in its module header so the honesty rule (spec/01)
holds for the tooling too.
