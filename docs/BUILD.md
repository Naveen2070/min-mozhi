# Building Min-Mozhi - Tools, Crates & Commands

A single reference for **what to install** and **how to build/run/test** every
part of this repository: the compiler, the WebAssembly crate, the website, and
the VS Code extension. All commands run from the **repo root** unless noted.

> Quick links: [Toolchain](#1-toolchain-prerequisites) ·
> [Workspace & crates](#2-workspace--crates) ·
> [Compiler (native)](#3-compiler-native---the-regular-build) ·
> [Tests & gate](#4-tests--quality-gate-r8) ·
> [WASM crate](#5-wasm-crate-cratesmimz-wasm) ·
> [Website](#6-website-site) · [VS Code extension](#7-vs-code-extension-editorsvscode) ·
> [Where artifacts land](#8-where-the-artifacts-land)

---

## 1. Toolchain (prerequisites)

| Tool                                  | Version                                                                                  | Needed for                                | Install                                                   |
| ------------------------------------- | ---------------------------------------------------------------------------------------- | ----------------------------------------- | --------------------------------------------------------- |
| **Rust** (`rustc` + `cargo`)          | **1.85+** (MSRV); edition 2024                                                           | the compiler, everything                  | <https://rustup.rs>                                       |
| **rustup**                            | any                                                                                      | managing the wasm target                  | comes with the rustup installer                           |
| **wasm32 target**                     | -                                                                                        | building the WASM crate                   | `rustup target add wasm32-unknown-unknown`                |
| **wasm-pack** _(recommended)_         | latest                                                                                   | web `.wasm` + JS glue (runs wasm-opt)     | `cargo install wasm-pack`                                 |
| **wasm-bindgen-cli** _(or)_           | **must match** the `wasm-bindgen` crate (see [section 5](#5-wasm-crate-cratesmimz-wasm)) | manual/headless wasm glue                 | `cargo install wasm-bindgen-cli --version <X.Y.Z>`        |
| **Node.js** + **npm**                 | Node ≥ 20 (dev on 24); npm 11                                                            | the website + VS Code extension           | <https://nodejs.org>                                      |
| **Icarus Verilog** (`iverilog`/`vvp`) | any                                                                                      | _optional_ - the differential tests       | <https://bleyer.org/icarus> (Win) / your package manager  |
| **OSS CAD Suite** (YosysHQ)           | latest nightly (checked: 20261001)                                                       | _optional_ - the synthesis path (Phase 2) | <https://github.com/YosysHQ/oss-cad-suite-build/releases> |

`prettier` and `markdownlint-cli2` are run via `npx` - no install needed.

### OSS CAD Suite (synthesis tools)

One download bundles Yosys, nextpnr-ice40, IceStorm (`icepack`, `icetime`,
`icebram`, `iceprog`), Icarus and GTKWave. Not needed to build or test the
compiler today; needed for the synthesis path (`.mimz -> IR -> Yosys ->
nextpnr -> bitstream`).

- **Linux / macOS:** unpack, then `source <suite>/environment`. Everything works
  as documented, including the default `synth_ice40` flow. CI uses Linux.
- **Windows - known Yosys bug: use `synth_ice40 -noabc`.** On Windows builds the
  default `synth_ice40` crashes in its ABC9 step (`ABC: execution of command ...
failed: return code 3` or `-2`; inside ABC, `Assertion failed: firstIn+i <
p->nCos`). Cause: the experimental `write_xaiger2` backend writes its binary
  file in text mode, so Windows inserts a CR before every LF byte and ABC reads
  a corrupt netlist. It hits most real designs and is in every Windows nightly
  checked (20260929, 20261001). `synth_ice40 -noabc` (Yosys's built-in LUT
  mapper) works and gives slightly larger netlists. `-nocarry` does not help.
  **Linux builds do not have this bug** - CI and any Linux machine (or WSL)
  run the default flow. Recorded in `docs/log/2026-10-02.md`.
- **Windows - do not put `<suite>\bin` on PATH.** Its tools need DLLs from
  `<suite>\lib`, and other MinGW programs on PATH (e.g. `C:\iverilog\bin`)
  ship same-named, older DLLs, so `yosys` fails to start (`0xC0000135` /
  `0xC0000139`). Putting `<suite>\lib` on PATH instead makes `python3`/`pip3`
  resolve to the suite's bundled Python. Either load the suite per session
  (`. <suite>\environment.ps1`) or use per-tool launcher `.exe`s that add
  `bin` and `lib` to PATH for the child process only (real `.exe`s, not `.cmd`:
  Rust's `Command::new` - used by the Icarus tests - only finds `.exe`).

---

## 2. Workspace & crates

This is a Cargo **workspace** (root = the compiler) plus two npm projects:

| Path                  | What it is                                                                                                                          | Built with                     |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| `.` (root, `src/`)    | **`mimz`** - shell crate (CLI, fs I/O, LSP, hw-emulation) + `mimz`/`mimz-bench` binaries; re-exports mimz-core/mimz-sim as a facade | cargo                          |
| `crates/mimz-core/`   | **`mimz-core`** - pure lexer/parser/ast/checker/emit_verilog/etc, zero optional deps                                                | cargo                          |
| `crates/mimz-sim/`    | **`mimz-sim`** - event-driven simulator + `runner.rs`, depends only on mimz-core                                                    | cargo                          |
| `crates/mimz-wasm/`   | **`mimz-wasm`** - wasm-bindgen wrapper (`compileToVerilog`), depends on `mimz-sim` directly                                         | cargo + wasm-pack/wasm-bindgen |
| `tools/test-summary/` | dev helper behind the `cargo test-summary` alias                                                                                    | cargo                          |
| `benches/compile.rs`  | per-phase `criterion` micro-benchmarks                                                                                              | `cargo bench`                  |
| `site/`               | the Astro website (landing + docs + playground)                                                                                     | npm                            |
| `editors/vscode/`     | the VS Code extension (`.vsix`)                                                                                                     | npm + `@vscode/vsce`           |

**Cargo features** (root `Cargo.toml`): `default = ["lsp", "bench", "watch",
"hw-emulation"]`. The CLI-only deps that don't build on wasm32 (`tokio`,
`tower-lsp`, `memory-stats`, `ratatui`/`crossterm`/`cpal`) are optional behind
those features, and live only in the root shell crate - `mimz-core` and
`mimz-sim` have zero optional deps. `mimz-wasm` depends on `mimz-sim`
directly (not on root `mimz`), so no `default-features = false` dance is
needed there. Root `Cargo.toml` sets `default-members = ["."]`, so the
everyday `cargo build`/`cargo test` only targets the shell crate - pass
`--workspace` to also build/test `mimz-core`/`mimz-sim` directly (CI does;
see `docs/code/10-test-map.md`).

---

## 3. Compiler (native) - the regular build

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
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
npx prettier --check "**/*.md"
npx markdownlint-cli2 "**/*.md"
```

`--workspace` is not optional here (see section 2) - CI (`.github/workflows/ci.yml`) runs every one of these with it.

Extras:

```sh
cargo test-summary --workspace       # per-binary test table + grand total (alias; --workspace required, same reason as above)
cargo doc --no-deps --workspace     # rustdoc (gate uses RUSTDOCFLAGS="-D warnings")
cargo bench                         # criterion per-phase benchmarks
cargo build --no-default-features   # proves the lib builds without lsp/bench (wasm-ready)
```

The Icarus differential tests (`tests/icarus.rs`) need `iverilog`/`vvp` on PATH;
set `REQUIRE_IVERILOG=1` to make them a hard failure instead of skipping.

---

## 5. WASM crate (`crates/mimz-wasm`)

Exposes two functions to JavaScript, both throwing a JS `Error` whose message
is the rendered diagnostics on failure:

- `compileToVerilog(source: string): string` - compile straight to Verilog.
- `runCommand(source: string, command: string, args: string[]): string` - run
  any `mimz` subcommand (`check`/`compile`/`eval`/`sim`/`test`) against
  in-memory source, the engine behind the playground's in-browser console.

Two build paths:

### A. Production build for the web - `wasm-pack` (recommended)

```sh
rustup target add wasm32-unknown-unknown        # one-time
cargo install wasm-pack                          # one-time
wasm-pack build crates/mimz-wasm --target web --release
```

Output: **`crates/mimz-wasm/pkg/`** - `mimz_wasm.js`, `mimz_wasm_bg.wasm`,
`*.d.ts`. `wasm-pack` runs `wasm-opt`, stripping dead code (e.g. the CLI's
`clap`). Consume it from the site:

```js
import init, { compileToVerilog } from "../pkg/mimz_wasm.js";
await init();
const verilog = compileToVerilog(source); // throws on a compile error
```

### B. Manual / headless - `cargo` + `wasm-bindgen-cli` (verified path)

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

The smoke test compiles the counter through wasm and checks an error path -
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
`site/src/lib/wasm/` (git-ignored - generated, not committed). The `build:wasm`
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

Most of these are git-ignored - regenerate with the command shown.

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
