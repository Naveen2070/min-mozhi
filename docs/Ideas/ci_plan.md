# Min-Mozhi CI Plan

The continuous-integration strategy for the repo: what runs today, the security
model it relies on, and the hardening roadmap. The workflow lives in
`.github/workflows/ci.yml`; benchmark-specific detail is in
[`benchmark_plan.md`](benchmark_plan.md).

---

## 1. Wired today (`.github/workflows/ci.yml`)

| Job             | Trigger                    | Does                                                                                                                                                                                                                         |
| --------------- | -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `check`         | push / PR                  | The full R8 gate: `fmt --check`, `clippy --all-targets -D warnings`, **rustdoc `-D warnings`**, `cargo test` (`REQUIRE_IVERILOG=1`), `cargo build`, `cargo bench --no-run` (compile-check the `criterion` harness)           |
| `bench`         | push / PR                  | `mimz-bench --no-cov --no-icarus` — its non-zero exit is a hard correctness gate (goldens, flavor byte-identity, fixtures, no-false-positives). `--history` is routed to a temp path so the gate never records a point.      |
| `nightly-bench` | `workflow_dispatch` / cron | `mimz-bench --no-cov --iterations 500`, then **commits the appended `bench-history.jsonl` back to the repo** (`[skip ci]`) and uploads the report as an artifact. Has `permissions: contents: write`. Cron is commented out. |
| `docs`          | push / PR                  | markdownlint + prettier `--check`                                                                                                                                                                                            |

Job gating: `check` / `bench` / `docs` run on `push` / `pull_request`;
`nightly-bench` runs on `workflow_dispatch` / `schedule`.

---

## 2. Security model (why the current setup is safe)

- **Least privilege per job.** Only `nightly-bench` carries
  `permissions: contents: write`; every other job uses the default read-only
  `GITHUB_TOKEN`. A compromise in `check`/`bench`/`docs` cannot write to the repo.
- **Ephemeral, repo-scoped token.** `GITHUB_TOKEN` exists only for the duration
  of a job, is revoked after, and can touch **only this repo** — never other
  repos, account settings, or secrets.
- **Fork PRs get a read-only token** regardless of the `permissions:` block, and
  `nightly-bench` (the only write job) never runs on PRs — so untrusted
  contributors can't reach the write token.

The residual risk is **supply chain**: third-party actions or dependencies run
inside the write-enabled job. That's what the hardening below targets.

---

## 3. Hardening roadmap

### 3.1 Pin third-party actions to commit SHAs (headline item)

Today the workflow references actions by **moving tag refs**
(`dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`,
`DavidAnson/markdownlint-cli2-action@v23`). A tag can be re-pointed by the
action's maintainer (or an attacker who compromises their account) to arbitrary
code — and in `nightly-bench` that code would run **with `contents: write`**,
i.e. able to push to the repo.

**Plan:** pin every third-party action to a full 40-character commit SHA, with
the human-readable version in a trailing comment:

```yaml
# before
uses: Swatinem/rust-cache@v2
# after
uses: Swatinem/rust-cache@<full-40-char-sha> # v2.7.x
```

- Pin: `dtolnay/rust-toolchain`, `Swatinem/rust-cache`,
  `DavidAnson/markdownlint-cli2-action`.
- GitHub-first-party actions (`actions/checkout`, `actions/upload-artifact`) are
  lower risk; pinning them too is good hygiene but optional.
- Keep pins fresh with Dependabot (3.3) so SHA bumps are reviewed PRs, not
  silent tag drift.

A SHA is immutable, so a hijacked tag can no longer inject code into the
write-enabled job. This is the single highest-value CI hardening step.

### 3.2 Branch protection ↔ the bot push

If `master` is protected against direct pushes, the `nightly-bench` bot commit
is rejected. Pick one:

- allow the GitHub Actions bot to bypass protection for this path, or
- switch the commit step to a **PR-based** action (open a PR with the history
  update instead of pushing to `master`).

These two interact — decide protection vs. direct bot push together.

### 3.3 Dependabot for actions + Cargo

Add `.github/dependabot.yml` watching `github-actions` and `cargo` so action
SHAs and crate versions arrive as reviewable PRs (which the `check`/`bench`
gates then validate).

### 3.4 Tighten default permissions

Set a top-level `permissions: contents: read` so every job starts read-only and
only `nightly-bench` opts up to `contents: write` — makes the privilege boundary
explicit and future-proof against new jobs accidentally inheriting write.

### 3.5 Public performance dashboard

Publish the committed `bench-history.jsonl` / `bench-report.html` to GitHub
Pages so the trend is viewable without downloading artifacts. (Moved here from
`benchmark_plan.md`.)

### 3.6 PR timing gate (deferred)

`cargo bench` + `critcmp` / a threshold action to fail PRs that slow a phase —
deferred until run-to-run noise on shared runners is characterized; today the
benches are only compile-checked.

---

## Status

- Wired: section 1 (all four jobs), history committed to the repo.
- Open: section 3 hardening — 3.1 SHA pinning is the next recommended step.
