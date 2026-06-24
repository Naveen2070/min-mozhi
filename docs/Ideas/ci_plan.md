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
| `fuzz`          | push / PR                  | cargo-fuzz smoke run (60s each: `lex_parse_eval`, `lex_parse_compile`, `pretty_roundtrip`, `translate_roundtrip`), seeded from the example corpus; any panic/abort/timeout fails the job.                                    |
| `fuzz-nightly`  | `workflow_dispatch` / cron | Extended fuzz (10 min/target), weekly (Mon 04:00 UTC).                                                                                                                                                                       |
| `audit`         | push / PR                  | `cargo audit` over the committed `Cargo.lock` — non-zero on a RUSTSEC advisory or yanked crate (the supply-chain audit gate, section 3.3 below).                                                                             |
| `docs`          | push / PR                  | markdownlint + prettier `--check`                                                                                                                                                                                            |

Job gating: `check` / `bench` / `fuzz` / `audit` / `docs` run on `push` /
`pull_request`; `nightly-bench` runs on the perf cron / dispatch; `fuzz-nightly`
on the weekly fuzz cron / dispatch.

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

### 3.3 Dependabot for actions + Cargo — ✅ DONE (2026-06-22)

`.github/dependabot.yml` now watches `github-actions` (directory `/` — all three
workflows) and `cargo` (root workspace + `crates/mimz-wasm`), weekly, labelled
`dependencies`. SHA/crate bumps arrive as reviewable PRs that the
`check`/`bench`/`audit` gates validate. The companion `audit` job (section 1
above) closes the "known-vulnerable crate" gap. Original note below.

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
- **Done this session:** section 3.1 SHA-pin (all actions in both workflows, first-party
  included) and section 3.4 least-privilege default permissions. The release pipeline
  (`release.yml`) ships unsigned binaries + `SHA256SUMS` (signing deferred).
- **Security verdict:** meaningfully hardened and good for the v0.1.0 public
  release. The headline supply-chain risk (mutable action tags in a write job) is
  closed, privileges are least, fork PRs get a read-only token, the only write
  jobs never run on PRs, and `Cargo.lock` is committed. Two gaps remain to call it
  _complete_ (below).

## Next session — pick up here (remaining CI hardening, prioritized)

Items 1 & 2 below are now **✅ DONE (2026-06-22)**. Only the operational
branch-protection step (3) remains, and it must be done in the GitHub UI/API
when the repo goes public.

1. ~~**Dependabot (section 3.3).**~~ ✅ DONE — `.github/dependabot.yml` watches
   `github-actions` + `cargo`, weekly.
2. ~~**Crate supply-chain audit gate.**~~ ✅ DONE — the `audit` job runs
   `cargo audit` over the committed `Cargo.lock` on every push/PR (chose
   `cargo audit` over `cargo-deny`: no config file, advisory + yanked coverage;
   revisit `cargo-deny` if license/bans enforcement is later wanted).
3. **Branch protection (section 3.2) — operational, do when going public.** Protect
   `master` + require CI before merge; reconcile with the `nightly-bench` bot push
   (allow bot bypass, or switch that step to a PR). Integrity, not a live hole.
   **Maintainer action — not codeable in-repo** (GitHub Settings → Branches, or
   `gh api`).

Deferred / optional (not gaps): build provenance / signing (`SHA256SUMS` already
gives download integrity; SLSA `actions/attest-build-provenance` is the future
step if verifiable provenance is wanted); pinning runner images
(`ubuntu-24.04` vs `ubuntu-latest`) — reproducibility hygiene, not
security-critical; section 3.5 public perf dashboard (not security); section 3.6 PR timing gate.

> Both items 1 & 2 are new work beyond the approved Workstream C scope and are
> SHA-pin-consistent, push-gated, and uncommitted (R12) — awaiting maintainer go.
