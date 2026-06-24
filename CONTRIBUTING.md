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
  cargo clippy --all-targets -- -D warnings
  cargo test
  npx prettier --check "**/*.md"
  npx markdownlint-cli2
  ```

- **Errors must teach.** Every diagnostic says what is wrong AND how to
  fix it, in words a learner understands
  ([`docs/code/06-diagnostics.md`](docs/code/06-diagnostics.md)).
- **Never break the thesis test**: English and Tanglish sources must
  compile to byte-identical Verilog (`tests/examples.rs`).
- **Document as you go**: rustdoc on new items, EBNF doc comments on
  parser routines, and update the matching `docs/code/` page when
  behavior changes.

## What help is most valuable right now (Phase 1)

- **Native Tamil speakers**: reviewing the Tanglish/Tamil keyword table
  (`spec/03-keywords-trilingual.md` — it is DRAFT until reviewed).
  This needs no Rust at all.
- **Compiler work**: the checker passes
  ([`docs/plan/phase-1-verilog-backend.md`](docs/plan/phase-1-verilog-backend.md),
  work item 4).
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

> **Status note:** the repo is private until Phase 1 completes (decision
> D7), so external contributions can't land just yet — but everything
> above already applies, and this guide is how the project stays
> contributor-ready for the day it opens.
