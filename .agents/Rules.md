# Codebase Rules for Min-Mozhi

> [!IMPORTANT]
> All AI assistants, agents, and contributors must strictly adhere to these rules for every working session.
> For the complete set of repository working rules, always refer to [docs/RULES.md](file:///D:/Study_Works/Code_works/min-mozhi/docs/RULES.md).

---

## 1. Daily Dev Logs

* **Rule**: After every change in a working session, record it in the daily dev log.
* **Log Location**: A single file per calendar day named `docs/log/YYYY-MM-DD.md`.
* **Format**:
  * History is **append-only**. Never rewrite or delete past logs.
  * Every entry must document what was done, what was decided, and what's next.
  * Major decisions must use the standard decision block:

    ```markdown
    ### Decision: <short title>

    - **Context:** what raised the question
    - **Decision:** what was chosen
    - **Why:** the deciding reasons (and what was rejected)
    - **Impact:** which docs/specs/code were updated because of it
    ```

## 2. Documentation Sync

* **Rule**: After each change, update all related documentation. No documentation must be left stale.
* **Sources of Truth** (defined in [docs/RULES.md](file:///D:/Study_Works/Code_works/min-mozhi/docs/RULES.md)):
  * **Language Design**: `spec/*.md`
  * **Execution Plan**: `docs/plan/phase-*.md` (summarized in root `min-mozhi-roadmap.md`)
  * **History & Decisions**: `docs/log/`
  * **Architecture**: `docs/architecture.md`
  * **How Code Works**: Code itself (`src/` + rustdoc) and `docs/code/`
* Any disagreement between files must be resolved **the same day**.

## 3. Linting & Formatting

* **Rule**: Run Rust and Markdown formatting/linting tools after any writing session.
* **Rust Code**:
  * Run `cargo fmt` to format the code.
  * Run `cargo clippy --all-targets` to catch lints and static analysis errors.
* **Markdown Files**:
  * Run Prettier to format markdown: `npx prettier --write "**/*.md"` (excluding `docs/archive/`).
  * Run markdownlint to lint markdown: `npx markdownlint-cli2`.

## 4. Spec & Philosophy Alignment (Impact Analysis)

* **Rule**: Before executing a change, analyze the request against the language specification, philosophy, and existing features.
* **References**:
  * **Philosophy & Goals**: [spec/01-goals-and-philosophy.md](file:///D:/Study_Works/Code_works/min-mozhi/spec/01-goals-and-philosophy.md)
  * **Syntax & Grammar**: [spec/02-syntax-and-grammar.md](file:///D:/Study_Works/Code_works/min-mozhi/spec/02-syntax-and-grammar.md)
* **Breaking Changes**: If a change request contradicts or breaks existing specifications, architecture, or features, you **must alert the user immediately** with a clear explanation of the conflict, and ask them how they want to handle it before writing any code.
