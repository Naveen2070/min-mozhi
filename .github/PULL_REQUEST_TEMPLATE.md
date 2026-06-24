## What does this change do?

<!-- One-paragraph summary. Link any related issue: Closes #N -->

## Type of change

- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change (grammar / safety rule / keyword / E-code change)
- [ ] Docs / spec only
- [ ] CI / tooling only

---

## Checklist

### Quality gate (R8 — CI enforces all of these)

- [ ] `cargo fmt --all` — clean
- [ ] `cargo clippy --all-targets -- -D warnings` — clean
- [ ] `cargo test` — all passing
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` — clean

### Tests

- [ ] New behaviour has a test in this PR (unit or integration)
- [ ] `docs/code/10-test-map.md` updated if test count changed

### Documentation (R1 / R2 / R3)

- [ ] Related `spec/` pages updated (grammar / keyword / safety-rule changes)
- [ ] Related `docs/code/` or `docs/guide/` pages updated
- [ ] Dev log entry written for today (`docs/log/YYYY-MM-DD.md`, R4)

### Examples (R9)

- [ ] Examples still compile in all four flavor folders (`english/tanglish/tamil/mixed/`)
- [ ] All four produce byte-identical Verilog (CI-asserted by `tests/examples.rs`)

### Stability (R10 / R13)

- [ ] No E-code was renumbered or reused
- [ ] No keyword was added without going through `lang/keywords.toml` + `spec/03` + TextMate grammar + lexer test

---

## Impact analysis _(required for spec / grammar / safety-rule / keyword changes)_

<!-- Does this touch spec/01, spec/02, or the keyword table?
     If yes: what breaks, and how was it handled?
     If no: write "N/A — additive / docs / tooling only." -->
