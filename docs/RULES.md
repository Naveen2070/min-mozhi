# Min-Mozhi — Repo Working Rules

> Process rules for this repository. Follow these every working session.
> The point: **the repo must always tell the true story of the project** —
> what the plan is now, what was decided, and why.

---

## R1 — Sources of truth

| Topic               | Source of truth                                                         | Everything else                            |
| ------------------- | ----------------------------------------------------------------------- | ------------------------------------------ |
| Language design     | `spec/*.md`                                                             | examples follow spec                       |
| Execution plan      | `docs/plan/phase-*.md`                                                  | root `min-mozhi-roadmap.md` is the summary |
| History & decisions | `docs/log/`                                                             | never reconstructed from memory            |
| Architecture        | `docs/architecture.md`                                                  | code follows it (or it gets updated)       |
| Keyword words       | the keyword table in `spec/03` (later: `keywords.toml` in the compiler) |                                            |

If two documents disagree, fix the non-source one **the same day**.

## R2 — When the plan changes

Any change to phase scope, order, timeline, or deliverables:

1. Update the affected `docs/plan/phase-*.md` file(s).
2. Update the summary in root `min-mozhi-roadmap.md` to match.
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
- Examples in `examples/` must always match the current spec — a spec change
  that breaks an example fixes the example in the same session.
- Once the compiler exists: every example must compile in CI; the keyword
  table lives in a data file (`keywords.toml`) so word changes are data
  changes, not code changes.
- English column of the keyword table is frozen for Phase 1; Tanglish/Tamil
  columns may change until native-speaker review closes.

## R7 — Session close checklist

Before ending a working session, check:

- [ ] Today's log entry written (`docs/log/`)
- [ ] Any plan change reflected in `docs/plan/` + root roadmap (R2)
- [ ] Any design decision logged with a Decision block (R3/R4)
- [ ] Examples still match the spec (R6)
