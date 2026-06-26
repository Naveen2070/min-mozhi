# Building Min-Mozhi — Tools, Crates & Commands

A single reference for **what to install** and **how to build/run/test** every
part of this repository: the compiler, the WebAssembly crate, the website, and
the VS Code extension. All commands run from the **repo root** unless noted.

> Quick links: [Toolchain](#1-toolchain-prerequisites) ·
> [Workspace & crates](#2-workspace--crates) ·
> [Compiler (native)](#3-compiler-native--the-regular-build) ·
> [Tests & gate](#4-tests--quality-gate-r8) ·
> [WASM crate](#5-wasm-crate-cratesmimz-wasm) ·
> [Website](#6-website-site) · [VS Code extension](#7-vs-code-extension-editorsvscode) ·
> [Where artifacts land](#8-where-the-artifacts-land)

---

## 1. Toolchain (prerequisites)

| Tool                                  | Version                                                                                  | Needed for                            | Install                                                  |
| ------------------------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------- | -------------------------------------------------------- |
| **Rust** (`rustc` + `cargo`)          | **1.85+** (MSRV); edition 2024                                                           | the compiler, everything              | <https://rustup.rs>                                      |
| **rustup**                            | any                                                                                      | managing the wasm target              | comes with the rustup installer                          |
| **wasm32 target**                     | —                                                                                        | building the WASM crate               | `rustup target add wasm32-unknown-unknown`               |
| **wasm-pack** _(recommended)_         | latest                                                                                   | web `.wasm` + JS glue (runs wasm-opt) | `cargo install wasm-pack`                                |
| **wasm-bindgen-cli** _(or)_           | **must match** the `wasm-bindgen` crate (see [section 5](#5-wasm-crate-cratesmimz-wasm)) | manual/headless wasm glue             | `cargo install wasm-bindgen-cli --version <X.Y.Z>`       |
| **Node.js** + **npm**                 | Node ≥ 20 (dev on 24); npm 11                                                            | the website + VS Code extension       | <https://nodejs.org>                                     |
| **Icarus Verilog** (`iverilog`/`vvp`) | any                                                                                      | _optional_ — the differential tests   | <https://bleyer.org/icarus> (Win) / your package manager |

`prettier` and `markdownlint-cli2` are run via `npx` — no install needed.

---

## 2. Workspace & crates

This is a Cargo **workspace** (root = the compiler) plus two npm projects:

| Path                  | What it is                                                           | Built with                     |
| --------------------- | -------------------------------------------------------------------- | ------------------------------ |
| `.` (root, `src/`)    | **`mimz`** — the compiler lib + the `mimz` and `mimz-bench` binaries | cargo                          |
| `crates/mimz-wasm/`   | **`mimz-wasm`** — wasm-bindgen wrapper (`compileToVerilog`)          | cargo + wasm-pack/wasm-bindgen |
| `tools/test-summary/` | dev helper behind the `cargo test-summary` alias                     | cargo                          |
| `benches/compile.rs`  | per-phase `criterion` micro-benchmarks                               | `cargo bench`                  |
| `site/`               | the Astro website (landing + docs + playground)                      | npm                            |
| `editors/vscode/`     | the VS Code extension (`.vsix`)                                      | npm + `@vscode/vsce`           |

**Cargo features** (root `Cargo.toml`): `default = ["lsp", "bench"]`. The
CLI-only deps that don't build on wasm32 (`tokio`, `tower-lsp`, `memory-stats`)
are optional behind those features. `mimz-wasm` depends on `mimz` with
`default-features = false`, and is kept out of `default-members`, so the everyday
host build/gate never compiles it.

---

## 3. Compiler (native) — the regular build

```sh
cargo build                 # debug build of the mimz CLI
cargo build --release       # optimized build (LTO + overflow-checks on)
```

Run it without installing:

```sh
cargo run -- compile examples/english/counter.mimz   # -> counter.v
cargo run -- check examples/english/counter.mimz
cargo run -- sim examples/english/counter.mimz --trace
cargo run -- eject std --to ./std                   # vendor stdlib to disk
cargo run -- --version
```

`cargo run` defaults to the `mimz` binary (`default-run`). The benchmark binary
is separate:

```sh
cargo run --release --bin mimz-bench        # end-to-end corpus benchmark
```

Install the CLI onto your PATH:

```sh
cargo install --path .          # installs `mimz` (and `mimz-bench`)
```

---

## 4. Tests & quality gate (R8)

The full gate CI enforces (also in [`../CONTRIBUTING.md`](../CONTRIBUTING.md)):

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
npx prettier --check "**/*.md"
npx markdownlint-cli2 "**/*.md"
```

Extras:

```sh
cargo test-summary                  # per-binary test table + grand total (alias)
cargo doc --no-deps --workspace     # rustdoc (gate uses RUSTDOCFLAGS="-D warnings")
cargo bench                         # criterion per-phase benchmarks
cargo build --no-default-features   # proves the lib builds without lsp/bench (wasm-ready)
```

The Icarus differential tests (`tests/icarus.rs`) need `iverilog`/`vvp` on PATH;
set `REQUIRE_IVERILOG=1` to make them a hard failure instead of skipping.

---

## 5. WASM crate (`crates/mimz-wasm`)

Exposes `compileToVerilog(source: string): string` to JavaScript (throws a JS
`Error` whose message is the rendered diagnostics). Two build paths:

### A. Production build for the web — `wasm-pack` (recommended)

```sh
rustup target add wasm32-unknown-unknown        # one-time
cargo install wasm-pack                          # one-time
wasm-pack build crates/mimz-wasm --target web --release
```

Output: **`crates/mimz-wasm/pkg/`** — `mimz_wasm.js`, `mimz_wasm_bg.wasm`,
`*.d.ts`. `wasm-pack` runs `wasm-opt`, stripping dead code (e.g. the CLI's
`clap`). Consume it from the site:

```js
import init, { compileToVerilog } from "../pkg/mimz_wasm.js";
await init();
const verilog = compileToVerilog(source); // throws on a compile error
```

### B. Manual / headless — `cargo` + `wasm-bindgen-cli` (verified path)

```sh
rustup target add wasm32-unknown-unknown

# Install the CLI MATCHING the crate version (else wasm-bindgen errors):
cargo tree -i wasm-bindgen -p mimz-wasm --depth 0   # shows e.g. v0.2.125
cargo install wasm-bindgen-cli --version 0.2.125

# Build the raw wasm, then generate Node glue and run the smoke test:
cargo build -p mimz-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target nodejs --out-dir crates/mimz-wasm/pkg \
  target/wasm32-unknown-unknown/release/mimz_wasm.wasm
node crates/mimz-wasm/smoke-test.cjs
```

The smoke test compiles the counter through wasm and checks an error path —
a fast, browserless proof the crate works.

### Just compile-check the wasm (no glue, fastest)

```sh
cargo build -p mimz-wasm --target wasm32-unknown-unknown
```

### Browser demo

After an `A`-style `--target web` build, serve the folder over HTTP and open
[`../crates/mimz-wasm/test.html`](../crates/mimz-wasm/test.html).

---

## 6. Website (`site/`)

```sh
cd site
npm install
npm run build:wasm      # build Rust → wasm → generate JS glue (one-time, or
                        # after compiler changes)
npm run dev             # local dev server
npm run build           # build:wasm + astro build + Pagefind → dist/
npm run build:site      # astro build + Pagefind only (skip wasm rebuild)
npx astro check         # type-check
```

Output: `site/dist/` (and `.vercel/output/` for the Vercel adapter). If you
change a markdown/rehype plugin and a rebuild looks stale, clear the content
cache: `rm -rf site/.astro site/node_modules/.astro` then rebuild.

**Playground prerequisite.** The `/playground` page imports the wasm glue from
`site/src/lib/wasm/` (git-ignored — generated, not committed). The `build:wasm`
script handles this: it compiles `crates/mimz-wasm` to wasm32 and runs
`wasm-bindgen --target web` into `site/src/lib/wasm/`.

(`wasm-pack build crates/mimz-wasm --target web` also works; point the import at
its `pkg/`.) Wiring this into the Vercel build is Step 6 of the web-presence plan.

---

## 7. VS Code extension (`editors/vscode/`)

```sh
cd editors/vscode
npm install
npx @vscode/vsce package      # -> mimz-<version>.vsix
```

Requires VS Code **^1.91** at runtime (`vscode-languageclient` 10). The extension
launches `mimz lsp`; set `mimz.serverPath` if `mimz` isn't on PATH.

---

## 8. Where the artifacts land

Most of these are git-ignored — regenerate with the command shown.

| Artifact           | Path                                                   | Produced by                                                          |
| ------------------ | ------------------------------------------------------ | -------------------------------------------------------------------- |
| `mimz` CLI         | `target/{debug,release}/mimz[.exe]`                    | `cargo build [--release]`                                            |
| `mimz-bench`       | `target/{debug,release}/mimz-bench[.exe]`              | `cargo build --bin mimz-bench`                                       |
| Raw wasm           | `target/wasm32-unknown-unknown/release/mimz_wasm.wasm` | `cargo build -p mimz-wasm --target wasm32-unknown-unknown --release` |
| Web wasm package   | `crates/mimz-wasm/pkg/`                                | `wasm-pack build crates/mimz-wasm --target web`                      |
| Website            | `site/dist/`                                           | `npm run build` (in `site/`)                                         |
| VS Code extension  | `editors/vscode/mimz-<version>.vsix`                   | `npx @vscode/vsce package` (in `editors/vscode/`)                    |
| API docs (rustdoc) | `target/doc/`                                          | `cargo doc --no-deps`                                                |

---

_See [`../CONTRIBUTING.md`](../CONTRIBUTING.md) for the contribution workflow and
[`RULES.md`](RULES.md) for the spec/doc/log discipline._
