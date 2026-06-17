# mimz v0.1.0 — Wingless Butterfly

<!--
  RELEASE NOTES — the release pipeline reads this file:
    • line 1 (this `#` heading) → the GitHub Release TITLE
    • everything below          → the Release BODY (markdown)
  REWRITE this file for every release (new title + new notes), then tag.
  The longer human history lives in CHANGELOG.md; this is the per-release blurb.
-->

The first public release of **Min-Mozhi (மின்மொழி)** — a modern, safe-by-default
HDL, and the first Tamil-rooted one. It reads like Go/TypeScript, is safe like
Rust, and speaks English, Tanglish, and Tamil from one grammar.

**Language edition:** Wingless Butterfly (`wingless-butterfly-2026-1`).

## Highlights

- **RTL parity batch** — replication `{N{x}}`, don't-care `match` patterns
  (`0b1??`), falling-edge blocks (`on fall(clk)`), memories (`mem`), and
  asynchronous reset (`async reset`).
- **Trilingual keywords** — write the same circuit in English, Tanglish, or Tamil
  (or mix them); every flavor compiles to byte-identical Verilog-2005.
- **Safety by construction** — mandatory reset values, lossless arithmetic by
  default, exhaustive `match`, single-driver and clock-domain checks, with
  teaching-quality diagnostics.
- **Built-in simulator** — `mimz sim` / `mimz test` run an in-house cycle-based
  engine, validated bit-for-bit against Icarus Verilog.
- **Versioning model** — two clear axes: the compiler version and the language
  edition (`mimz --version` shows both).

## Install

Download the archive for your platform below, verify it against `SHA256SUMS`, and
put `mimz` on your `PATH`. Binaries are **unsigned** for this release — see
`UNSIGNED.txt` in the archive for the one-time macOS/Windows "allow" step. Full
instructions: `docs/guide/01-getting-started.md`. Or build from source with
`cargo build --release`.

See `CHANGELOG.md` for the complete change list.
