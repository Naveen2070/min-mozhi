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

### 3.1 Pin third-party actions to commit SHAs (headline item) — ✅ DONE (2026-06-17)

**Done, and exceeded the plan.** Every action in **both** workflows
(`ci.yml` + `release.yml`) is pinned to a 40-char commit SHA with a trailing
`# vX` comment — including the GitHub first-party actions (`actions/checkout`,
`actions/upload-artifact`, `actions/download-artifact`) that this plan had
marked "optional". A hijacked tag can no longer inject code into the
write-enabled jobs. Original note retained below for context.

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

### 3.4 Tighten default permissions — ✅ DONE (2026-06-17)

Both workflows now carry a top-level `permissions: contents: read`; `write` is
scoped to only the jobs that need it (`nightly-bench` in `ci.yml`, the `release`
job in `release.yml`). Original note below.

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

## Status (2026-06-17)

- **Wired:** section 1 (all four jobs), history committed to the repo.
- **Done this session:** §3.1 SHA-pin (all actions in both workflows, first-party
  included) and §3.4 least-privilege default permissions. The release pipeline
  (`release.yml`) ships unsigned binaries + `SHA256SUMS` (signing deferred).
- **Security verdict:** meaningfully hardened and good for the v0.1.0 public
  release. The headline supply-chain risk (mutable action tags in a write job) is
  closed, privileges are least, fork PRs get a read-only token, the only write
  jobs never run on PRs, and `Cargo.lock` is committed. Two gaps remain to call it
  _complete_ (below).

## Next session — pick up here (remaining CI hardening, prioritized)

The two highest-value steps are done. What's left, in priority order:

1. **Dependabot (§3.3) — do this before going public.** SHA-pinning is the first
   half of the pattern; Dependabot is the second. Pins are immutable-safe but go
   **stale** — without it, no reviewed PR arrives when an action ships a security
   fix. Fix: add `.github/dependabot.yml` watching `github-actions` + `cargo`
   (~10 lines); the `check`/`bench` gates validate the bump PRs. Cheap, strongest
   recommendation.
2. **Crate supply-chain audit gate.** Actions are pinned and `Cargo.lock` is
   committed, but nothing flags a **known-vulnerable / yanked crate**. Add
   `cargo audit` (or `cargo-deny`, which also covers licenses) as a CI step. The
   pure-Rust tree shrinks but doesn't eliminate this. Strong nice-to-have.
3. **Branch protection (§3.2) — operational, do when going public.** Protect
   `master` + require CI before merge; reconcile with the `nightly-bench` bot push
   (allow bot bypass, or switch that step to a PR). Integrity, not a live hole.

Deferred / optional (not gaps): build provenance / signing (`SHA256SUMS` already
gives download integrity; SLSA `actions/attest-build-provenance` is the future
step if verifiable provenance is wanted); pinning runner images
(`ubuntu-24.04` vs `ubuntu-latest`) — reproducibility hygiene, not
security-critical; §3.5 public perf dashboard (not security); §3.6 PR timing gate.

> Both items 1 & 2 are new work beyond the approved Workstream C scope and are
> SHA-pin-consistent, push-gated, and uncommitted (R12) — awaiting founder go.
