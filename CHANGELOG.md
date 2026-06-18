# Changelog

All notable changes to Min-Mozhi (மின்மொழி). The project has **two version
axes** (see [`spec/06-editions.md`](spec/06-editions.md)): the **compiler** (the
crate version in `Cargo.toml`) and the **language edition** (a variant codename +
year + code). This file is the human-readable mirror of `EDITION_HISTORY` in
[`src/version.rs`](src/version.rs) — the machine source of truth.

The format follows [Keep a Changelog](https://keepachangelog.com); versions
follow [SemVer](https://semver.org) for the compiler axis.

## [Unreleased] — compiler `0.1.0-dev`

Preparing the first public release. The crate keeps the `-dev` suffix until the
`v0.1.0` tag is cut (the release step is maintainer-gated).

### Language edition: Wingless Butterfly — `wingless-butterfly-2026-1`

The first language edition. Keyword set **v1** (trilingual table frozen
2026-06-15). Surfaced uname-style by `mimz --version`, in the emitted Verilog
header, and here.

Added in the pre-freeze RTL-parity batch:

- **Replication** `{N{x}}` — the inner concatenation group repeated `N` times.
- **Don't-care `match` patterns** `0b1??` — a binary pattern with don't-care bits
  (the Verilog `casez` idiom); binary-only for this cut.
- **Falling-edge blocks** `on fall(clk)` — the negedge sibling of `on rise(clk)`.
- **Memories** `mem m: bits[W][DEPTH] = init` — an addressable array with a
  combinational indexed read and a clocked indexed write; power-on init.
- **Asynchronous reset** `async reset rst` — widens the always-block to
  `@(posedge clk or posedge rst)`; active-high only (active-low polarity
  deferred). A plain `reset` stays synchronous.

Keywords promoted from reserved to active this edition (Tanglish/Tamil spellings
**provisional**, pending native review — R9/R11): `fall`, `mem`, `async`.

Tooling: a fast in-house cycle-based simulator (`mimz sim` / `mimz test`),
validated bit-for-bit against Icarus Verilog; `mimz translate` (keyword-flavor
and word-order reskins); `mimz fmt`; `mimz eval`; an LSP server and VS Code
extension.
