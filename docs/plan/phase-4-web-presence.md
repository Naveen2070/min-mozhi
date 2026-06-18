# Phase 4 (pulled forward) — Web presence: landing + docs + in-browser playground

> **One static site: a landing page, the docs, and a WASM playground where you
> write `.mimz` and see the Verilog _and the waveform_ in the browser.**
> Window: pre-v0.1.0 public launch · Status: 🟡 in progress · Maintainer-gated (R12)

## Why now

Min-Mozhi is about to go public for **v0.1.0**. Its audience
([`spec/01-goals-and-philosophy.md`](../spec/01-goals-and-philosophy.md)) is
**learners of digital design — especially those underserved by English-only
tools — with no toolchain or install rights, just a URL**. The highest-impact
thing for them is a **browser playground**: nothing to install, just type and run.

This is already the project's own #1 ecosystem priority
([`phase-4-ecosystem.md`](phase-4-ecosystem.md): "WASM build + browser playground —
FIRST bridge … no toolchain, no install rights needed in a college lab, just a
URL"), and its prerequisites — the lib/bin split and `--json` diagnostics — **landed
in Phase 1**. We pull it forward so the public launch **leads with the
differentiator** rather than a README alone.

**Feasibility, verified against the code:** the compiler+simulator library is
**WASM-ready today** — pure-sync, no C deps, no `build.rs`, nothing that breaks
`wasm32`; the whole pipeline is already string-in / string-out and in-memory.

## Decisions (locked with the maintainer, 2026-06-18)

- **Framework: a custom Astro app** (full design control; _not_ Starlight). Astro
  gives content-collections for docs (existing markdown → pages) + islands for the
  interactive playground, so we don't re-implement markdown rendering.
- **Sequencing: build the full site + playground first, _then_ flip public.** Launch
  with the wow factor. Still maintainer-gated (R12).
- **Waveform: a custom canvas/SVG renderer behind a swappable
  `WaveformViewer({vcd})` boundary.** The VCD string is the stable contract; **Surfer
  (surfer.dev, Rust+WASM)** is the documented drop-in upgrade if/when VCD gains x/z
  or large traces.
- **Repo layout: ONE repo (monorepo), not a separate web repo.** Docs stay a single
  source of truth (the site _sources_ `docs/**`, no copy/sync); the playground can't
  version-drift (the WASM crate builds against the exact `src/lib.rs` in the same
  checkout, and the "playground == CLI" byte-equal test lives in one suite);
  versioning is atomic (one tag moves compiler + docs + playground). The Node
  toolchain is isolated under `site/` (its own `package.json`), and
  `crates/mimz-wasm` is kept **out of `default-members`** so the existing R8 gate
  never builds it for the host — it's built for `wasm32` only, explicitly, in the
  deploy workflow.

## Architecture (one static site — no backend, free)

```
crates/mimz-wasm  →  mimz_wasm_bg.wasm + JS glue   ┐
src/lib.rs (+compile_string)                        ├─→  site/ (Astro)  →  Vercel
docs/guide/*, spec/*  (existing markdown)           ┘
```

### 1. WASM engine — `crates/mimz-wasm/` (new workspace member) + tiny lib add

- **`src/lib.rs`: add `compile_string(source, imports) -> Result<String, Vec<Diag>>`**
  (~30 lines). Wraps the existing pipeline _without_ `std::fs`: `lexer::lex` →
  `parser::parse` → `checker::check` → `emit_verilog::transliterate` →
  `Project::from_files` → `emit`. `imports` is a `name → source` map (the browser
  can't read files). This is the only net-new library code.
- **`crates/mimz-wasm`**: a `wasm-bindgen` shell exposing two functions, reusing the
  existing `--json` diagnostic shape for errors:
  - `compileToVerilog(source, imports) → { verilog } | { errors: Diag[] }`
  - `simulate(source, module, opts) → { vcd, signals } | { errors }` via
    `sim::elaborate::elaborate_project` → `sim::run::run`/`comb_run` (`Timeline`) →
    `sim::vcd::to_vcd`.
  - Built with `wasm-pack`/`wasm-bindgen` for `wasm32-unknown-unknown`, output into
    `site/`. (No `getrandom`/thread/fs issues — no feature gates needed;
    `tokio`/`tower-lsp`/`clap`/`memory-stats` are bin-only and not pulled in.)

### 2. Site — `site/` (Astro, custom-themed)

- **Landing** (`/`): reuse the README pitch — tagline ("a modern, safe-by-default
  HDL, built to teach — and the first to speak Tamil"; reads like Go/TS, safe like
  Rust), 3 highlights in pitch order (modern+safe → educational → trilingual/Tamil),
  a code sample, CTAs to **Playground**,
  **Docs**, **GitHub**. **Flashy hero allowed but earns its weight (deferred to Step
  6 polish):** a **domain-themed** animation (logic gates / flowing waveforms / clock
  pulse / silicon die — flashy _and_ meaningful), as a **lazy-loaded island** that
  loads only on `/`; **respect `prefers-reduced-motion`** with a static fallback;
  prefer a lightweight **WebGL shader / SVG-Canvas** over a full Three.js scene unless
  depth is truly needed (audience = college labs, modest GPUs/phones). Must not block
  first paint or compete with the playground.
- **Docs** (`/guide/*`, `/spec/*`): Astro **content collections** sourced from the
  existing `docs/guide/*.md` (12 chapters) and `spec/*.md` (6 files) — _sourced,
  not duplicated_, so docs never drift. Custom nav + free client-side search
  (**Pagefind**). Tamil/Tanglish identity in the theme.
- **Playground** (`/playground`): a **CodeMirror 6** editor with a lightweight `mimz`
  highlight mode (reuse the existing TextMate grammar via Shiki, or a small
  StreamLanguage mode), a **Compile → Verilog** read-only panel, and a **Simulate →
  waveform** panel. Loads the WASM module as an Astro island.

### 3. Waveform viewer — `site/` component, swappable

- A ~100-line VCD parser → normalized model → **custom canvas/SVG timeline**, wrapped
  as `<WaveformViewer vcd={…}/>`. VCD string is the contract; **Surfer** is the
  documented upgrade path (swap the component, no playground refactor).

### 4. Deploy — **Vercel, on a maintainer-owned subdomain** (e.g. `minmozhi.<domain>`)

Chosen over GitHub Pages: served at root (`/`) — no `base`-path config, which keeps
**WASM/asset loading in the playground** simple; first-class Astro support; a CDN; and
**per-branch preview deploys** for showing the maintainer the live site pre-launch.

- **Build wrinkle:** the build has two halves — compile `crates/mimz-wasm` (needs
  **Rust + `wasm-pack`**, _not_ in Vercel's default image) and `astro build`. Approach
  (open until Step 6):
  - **(B, leaning) prebuilt:** build wasm + site in our **SHA-pinned GitHub Actions**
    (reproducible, pinned toolchain — matches the CI hardening), then
    `vercel deploy --prebuilt`. Vercel = host + CDN + domain only.
  - **(A) Vercel-native:** bootstrap rustup + wasm-pack inside Vercel's build command.
    Simpler, less control over the toolchain. Fine for v0.1.0.
- **Subdomain:** add it in Vercel + a DNS `CNAME`.
- **R12 / outward-facing:** deploying — even a preview — makes the **site** reachable
  on the internet while the **repo** stays private. Good for maintainer preview; the
  "go public" gate still applies to the repo flip + `v0.1.0` tag. Vercel can
  password-protect the preview if a private pre-launch is wanted.

## Build sequence (each step independently reviewable / shippable)

0. **Persist this plan** (this file) + an **R4 dev-log entry**. _(done)_
1. **`compile_string` lib wrapper + unit tests** — Rust R8 gate stays green.
2. **`crates/mimz-wasm`** + `wasm-bindgen` API; prove load+compile in a throwaway HTML.
3. **Astro scaffold**: landing + docs (from existing markdown) + nav/search. Deployable.
4. **Playground page**: editor + Compile→Verilog panel wired to WASM.
5. **Waveform**: custom renderer behind the boundary + Simulate wiring.
6. **Vercel deploy** (subdomain) + landing polish (domain-themed flashy hero, see
   below) → (maintainer) flip public + tag `v0.1.0` (Workstream D, R12).

## Reused code (do not reinvent)

- Compile: `src/lexer/mod.rs::lex`, `src/parser/mod.rs::parse`,
  `src/checker/mod.rs::check`,
  `src/emit_verilog/mod.rs::{transliterate, Project::from_files, emit}`.
- Simulate: `src/sim/elaborate.rs::elaborate_project`,
  `src/sim/run.rs::{run, comb_run, Timeline}`, `src/sim/vcd.rs::to_vcd`.
- Diagnostics: `src/diag/mod.rs` (+ existing `--json` shape) for playground errors.
- Highlight: existing VS Code TextMate grammar (kept in sync by `tests/grammar_sync.rs`).
- Docs content: `docs/guide/*.md`, `spec/*.md` (sourced, not copied).

## Verification

- **Rust gate green**: `compile_string` unit tests;
  `cargo build --target wasm32-unknown-unknown -p mimz-wasm`.
- **Playground == CLI (differential, reuses the existing golden discipline):** for a
  BASE_EXAMPLE (counter), assert WASM `compileToVerilog` output **byte-equals** the
  golden Verilog, and `simulate` VCD **byte-equals** `mimz sim` VCD — guards the WASM
  path against drift from the native path.
- **Local end-to-end**: `npm run dev` in `site/`; write the counter, Compile shows
  Verilog, Simulate renders a waveform; docs pages render with working search.
- **Deploy**: a Vercel **preview deploy** (per-branch URL) renders the full site
  before any public flip; verify WASM loads at root and the playground runs (R12).
- **Process**: R4 dev log, R14 (`graphify update` + test-map) after each step, full R8 gate.

## Critical files

`src/lib.rs` (add `compile_string`), `crates/mimz-wasm/` (new), `site/` (new Astro
app: landing + docs + `/playground` + `WaveformViewer`), Vercel config
(`vercel.json` / project settings) + a deploy GitHub Action if approach (B).
References: [`phase-4-ecosystem.md`](phase-4-ecosystem.md),
[`../guide/`](../guide/), [`../spec/`](../spec/).

## Not in this milestone (deferred, per Phase 4)

Hardware REPL (`mimz repl`), `mimz tui`, npm/PyPI wrapper packages, and the Tamil
translation of docs prose. (The maintainer subdomain on Vercel is now _in_ scope — see
section 4.) The playground engine here is what those later ride
on.

## Build status (2026-06-18)

- **Phase 1 (landing + docs): built & verified.** Official `npm create astro@latest`
  scaffold under `site/` — **Astro 6.4.7**, **npm**, **React** (single island
  framework; Lit was considered then dropped), **Tailwind v4**, **Shiki** (reusing
  the TextMate grammar), **Pagefind**, **`@astrojs/vercel`** adapter. Palette: \*\*blue
  - lightning-yellow\*\*. Landing + `/guide/[slug]` + `/spec/[slug]` (docs sourced via
    the content-layer glob loader, never copied) + 404. `npm run build` clean (20
    pages), `astro check` 0/0/0. Details + decisions in `docs/log/2026-06-18.md`.
- **Open:** root domain for `mimz.<domain>` DNS; first Vercel preview deploy.
- **Next:** Phase 2 — `compile_string` lib wrapper + `crates/mimz-wasm` (the
  playground engine). No commit yet (R12).
