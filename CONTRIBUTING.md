# Contributing to Min-Mozhi

Welcome — and நன்றி for your interest. This is the quick version; the
detailed, recipe-level guide lives in
[`docs/code/08-contributing.md`](docs/code/08-contributing.md).

## Before you start

1. **Understand the project**: read the [README](README.md), then
   [`spec/01-goals-and-philosophy.md`](spec/01-goals-and-philosophy.md) —
   the Constitution there (open source forever, Verilog interop forever,
   Tamil first-class forever) is non-negotiable.
2. **Understand the code**: [`docs/code/`](docs/code/) explains how the
   compiler works, module by module, and why it is shaped that way.
   Reading order is in its [README](docs/code/README.md).
3. **Understand the process**: [`docs/RULES.md`](docs/RULES.md) — specs,
   plans, and the dev log are kept in sync with code, same day, every
   session. A change without its doc/log updates is not done.

## The 30-second rules

- **Spec first.** Language behavior changes start in `spec/`, not in code.
- **Building anything?** [`docs/BUILD.md`](docs/BUILD.md) is the full reference —
  required tools, every crate/package, and the native + WASM + site + extension
  build commands.
- **Quality gate** (CI enforces exactly this):

  ```text
  cargo fmt --all
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  npx prettier --check "**/*.md"
  npx markdownlint-cli2
  ```

  `--workspace` is required, not optional: root `Cargo.toml` sets
  `default-members = ["."]` (fast local iteration on the shell crate), so a
  bare `cargo test`/`cargo clippy` silently skips `mimz-core` and `mimz-sim`.
  For a full per-suite breakdown instead of a single pass/fail line, use
  `cargo test-summary --workspace` (a dev-helper crate at
  `tools/test-summary/`, aliased in `.cargo/config.toml`).

  The compiler is a 3-crate Cargo workspace: `mimz-core` (pure pipeline +
  most tooling), `mimz-sim` (event-driven simulator + runner), and the root
  `mimz` shell crate (fs I/O, CLI, LSP, hardware emulation) — this is the
  workspace `--workspace` runs the gates across.

- **Errors must teach.** Every diagnostic says what is wrong AND how to
  fix it, in words a learner understands
  ([`docs/code/06-diagnostics.md`](docs/code/06-diagnostics.md)).
- **Never break the thesis test**: English and Tanglish sources must
  compile to byte-identical Verilog (`tests/examples.rs`).
- **Document as you go**: rustdoc on new items, EBNF doc comments on
  parser routines, and update the matching `docs/code/` page when
  behavior changes.

## What help is most valuable right now

- **Native Tamil speakers**: reviewing the Tanglish/Tamil keyword table
  (`spec/03-keywords-trilingual.md` — provisional entries are marked DRAFT
  until reviewed). This needs no Rust at all.
- **Compiler work**: see [`docs/plan/`](docs/plan/) for the current
  per-phase execution plans — each phase file lists concrete, in-progress
  work items. Check which phase is active before picking one up; completed
  phases stay in `docs/plan/` as a historical record.
- **Testing**: trying the examples, filing confusing-error reports —
  a confusing error message is a bug here, by definition.

## Common changes, in one line each

| You want to…                 | See recipe in [`docs/code/08-contributing.md`](docs/code/08-contributing.md) |
| ---------------------------- | ---------------------------------------------------------------------------- |
| Change a Tanglish/Tamil word | data-only change in `lang/keywords.toml` + spec table                        |
| Add a keyword                | spec → TOML → `Kw` enum → `kw_for_key` → parser → tests                      |
| Add a syntax form            | spec → AST node → parse routine → emit (or clean error) → tests              |
| Extend the Verilog emitter   | symbol table, substitution, auto-wire contract, "errors never guesses"       |
| Write a good error message   | [`docs/code/06-diagnostics.md`](docs/code/06-diagnostics.md)                 |

## License

Dual MIT / Apache-2.0. By contributing, you agree your contributions are
licensed the same way (the standard Rust-ecosystem arrangement — see
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE)).
