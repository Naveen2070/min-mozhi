# Min-Mozhi — Repo Working Rules

> Process rules for this repository. Follow these every working session.
> The point: **the repo must always tell the true story of the project** —
> what the plan is now, what was decided, and why.

---

## R1 — Sources of truth

| Topic               | Source of truth                                                              | Everything else                                                                          |
| ------------------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Language design     | `spec/*.md`                                                                  | examples follow spec                                                                     |
| Execution plan      | `docs/plan/phase-*.md`                                                       | root `ROADMAP.md` is the summary                                                         |
| History & decisions | `docs/log/`                                                                  | never reconstructed from memory                                                          |
| Architecture        | `docs/architecture.md`                                                       | code follows it (or it gets updated)                                                     |
| How the code works  | the code itself (`src/` + rustdoc)                                           | `docs/code/` explains it — update the matching page in the same session behavior changes |
| Keyword words       | the keyword table in `spec/03` (later: `lang/keywords.toml` in the compiler) |                                                                                          |

If two documents disagree, fix the non-source one **the same day**.

## R2 — When the plan changes

Any change to phase scope, order, timeline, or deliverables:

1. Update the affected `docs/plan/phase-*.md` file(s).
2. Update the summary in root `ROADMAP.md` to match.
3. Update the status table in `docs/README.md` if a status changed.
4. Add a log entry the **same day** (see R4) saying what changed and why.

A plan change without a log entry explaining _why_ is not done.

## R3 — When a major design decision is made

"Major" = anything that affects: grammar/syntax, the safety rules, the keyword
table, type/width rules, phase scope or order, toolchain choices, or
architecture.

1. Update the relevant `spec/` doc (bump its version note: v0.1 → v0.2 …).
2. Update `docs/architecture.md` if components or data flow changed.
3. Add a **Decision block** to today's log (format in R4).
4. If examples are affected, update them in the same commit/session.

## R4 — Dev log discipline

- One file per working day: `docs/log/YYYY-MM-DD.md`.
- **Append-only history.** Never rewrite or delete past log files — the log
  records how the project took shape, including wrong turns.
- Every entry covers: what was done, what was decided, what's next.
- Decisions use this block:

```markdown
### Decision: <short title>

- **Context:** what raised the question
- **Decision:** what was chosen
- **Why:** the deciding reasons (and what was rejected)
- **Impact:** which docs/specs/code were updated because of it
```

## R5 — Spec versioning

- Each spec doc carries a version note in its header (currently v0.1).
- Grammar, keyword, or safety-rule changes bump the version and get a one-line
  changelog at the bottom of the spec file.
- DRAFT sections (e.g. the Tanglish/Tamil keyword columns) stay marked DRAFT
  until reviewed by native speakers; removing a DRAFT mark is itself a logged
  decision.

## R6 — Repo conventions

- File names: `kebab-case.md`; logs: `YYYY-MM-DD.md`.
- **No section-sign character** (the silcrow, Unicode U+00A7) anywhere — docs,
  code, comments, commit messages. Always spell it out: `section 8`,
  `spec/02 section 3`. It reads badly in plain-text terminals, breaks
  grep-by-word, and isn't on most keyboards. (`grep -rn` for it before closing
  a session; `docs/log/` history is exempt — append-only.)
- Markdown is formatted by **Prettier** (`npx prettier --write "**/*.md"`)
  and linted by **markdownlint** (`npx markdownlint-cli2`, config in
  `.markdownlint-cli2.jsonc`). Both run in CI. `docs/archive/` is exempt —
  history is never edited to satisfy a tool.
- Rust code carries rustdoc: every type and non-trivial function has a
  `///` comment, every module a `//!` header, and parser routines state
  their EBNF production (kept in sync with `spec/02` section 5). Browse with
  `cargo doc --document-private-items --open`.
- Examples in `examples/` must always match the current spec — a spec change
  that breaks an example fixes the example in the same session.
- Once the compiler exists: every example must compile in CI; the keyword
  table lives in a data file (`lang/keywords.toml`) so word changes are data
  changes, not code changes.
- English column of the keyword table is frozen for Phase 1; Tanglish/Tamil
  columns may change until native-speaker review closes.

## R7 — Session close checklist

Before ending a working session, check:

- [ ] Today's log entry written (`docs/log/`)
- [ ] Any plan change reflected in `docs/plan/` + root roadmap (R2)
- [ ] Any design decision logged with a Decision block (R3/R4)
- [ ] Examples still match the spec (R6) and the four-flavor rule (R9)
- [ ] Quality gate clean: `fmt`, `clippy -D warnings`, `test`, rustdoc, prettier, markdownlint (R8)
- [ ] New/removed tests reflected in `docs/code/10-test-map.md` (R8)
- [ ] No commit or tag made unless the user asked (R12)
- [ ] `graphify update .` run if code changed (R14)

## R8 — Code quality gate

Run the gate CI runs **before declaring any session done** — all clean, no
exceptions:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (all green — record the new total in `docs/code/10-test-map.md`;
  `--workspace` is required — root `Cargo.toml`'s `default-members = ["."]`
  means a bare `cargo test` silently skips `mimz-core`/`mimz-sim`)
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace` (docs build warning-free)
- `npx prettier --check "**/*.md"` + `npx markdownlint-cli2` (R6)

New behavior ships **with its test in the same session**. Prefer the existing
test layers (keyword/lexer/parser/checker units; integration over a real
example) over ad-hoc ones.

## R9 — Examples are four-flavor and byte-identical

- Every example exists in **all four** flavor folders —
  `examples/{english,tanglish,tamil,mixed}/` — with **identical identifiers**;
  only keywords differ.
- All four must compile to **byte-identical Verilog** (CI-asserted) and pass
  Icarus (`iverilog` lint + a self-checking testbench) where one exists.
- A new example: add it to all four folders, to `BASE_EXAMPLES` in
  `tests/examples.rs`, with a golden in `tests/golden/` and, ideally, an Icarus
  testbench in `tests/icarus/`.
- Keyword spellings come from `lang/keywords.toml` **only — never invent a Tanglish
  or Tamil spelling.** If a word is not in the table, it is not ready to use.
- **Exception — language-pure showcase examples.** A program written fully in one
  language (keywords AND identifiers, e.g. `examples/tamil-pure/`) cannot be
  byte-identical to the other flavors — localized identifiers transliterate to
  different Verilog names. Such examples live in their OWN folder, OUTSIDE the
  four-flavor set, and are validated instead by: (a) equivalence to an existing
  base example via canonical identifier renaming (alpha-equivalence —
  `pure_tamil_examples_are_equivalent_to_their_counterparts`), (b) their own
  golden (`tests/golden/tamil_pure_*.v`), and (c) their own self-checking Icarus
  testbench. They are NOT added to `BASE_EXAMPLES` and do NOT take part in the
  byte-identity test. Identifier names are the author's choice; keyword spellings
  still come from `lang/keywords.toml` only.

## R10 — Diagnostics are a stable contract

- Every error carries a stable `E`-code; codes are **never renumbered or
  reused** — editors, tests, and docs key off them.
- A new checker code needs all three, same session: an entry in
  `mimz::diag::ALL_CHECKER_CODES` (the machine list), a row in
  `docs/code/11-checker.md`, and an end-to-end fixture under
  `tests/fixtures/errors/` (the corpus test fails otherwise). Lexer/parser/
  loader codes are cataloged in `docs/code/06-diagnostics.md`.
- Every diagnostic carries a teaching `help:` line (G1, `spec/01`).

## R11 — Reserved words & future keywords

- Reserve a future keyword **as soon as a feature is planned**, so v0.1
  programs cannot claim it. Full pipeline, same session: the `reserved` list in
  `lang/keywords.toml` + the reserved table & changelog in `spec/03` + the TextMate
  grammar invalid pattern (`editors/vscode/syntaxes/mimz.tmLanguage.json`) + a
  reserved-word test in `src/lexer/keywords.rs`. `tests/grammar_sync.rs`
  enforces the grammar half.
- Reserved words stay **English-only** until their feature lands and native
  review supplies the Tanglish/Tamil spellings — same rule as R9.

## R12 — Version control discipline

- **Never commit or tag unless the user asks.** When work is done, report it
  and let the user decide the commit and any version tag.
- New work branches off `master`; never bypass hooks (`--no-verify`) or signing
  unless the user explicitly asks.

## R13 — Impact analysis & breaking-change alert (before writing code)

- Before executing a change, weigh it against the spec, the philosophy
  (`spec/01`), and existing features (mirrors `.claude/Rules.md` section 4).
- If a request **contradicts or breaks** existing spec, architecture, or
  behavior, **stop and alert the user** with the conflict and the options
  before writing any code.
- **Growth doctrine** (Decision 2026-06-13): break freely until v0.1.0, then
  freeze; break afterward only if it benefits the language's future, via
  **Editions + `mimz translate`**. Additive changes are edition-safe; breaking
  changes are not.

## R14 — Keep derived artifacts current

- After code changes, run `graphify update .` to refresh the knowledge graph
  (`graphify-out/`, AST-only, no API cost).
- When tests are added or removed, update the count and the ledger in
  `docs/code/10-test-map.md` (it is the human "what does a failing test mean"
  map; no test asserts the total).
