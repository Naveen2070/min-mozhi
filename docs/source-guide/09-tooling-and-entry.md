# 9 — Tooling, Entry Points & Editor Support

## `src/commands/` — CLI Command Handlers (11 Files)

These are thin functions that wire CLI arguments to the library modules. Nothing clever here — just plumbing.

- **`check.rs`** — load file, run lex→parse→check, print "OK" or errors
- **`compile.rs`** — full pipeline to `.v` file
- **`eval.rs`** — evaluate combinational modules
- **`sim.rs`** — simulate with sweep, steps, traces, VCD
- **`test.rs`** — run test blocks
- **`translate.rs`** — reskin keywords
- **`fmt.rs`** — in-place keyword normalization
- **`explain.rs`** — print long-form error explanations
- **`eject.rs`** — vendor the embedded standard library to disk (`mimz eject std`)
- **`helpers.rs`** — shared config resolution

## `src/main.rs` & `src/lib.rs` — The Front Door

### `main.rs` — CLI Entry

Uses `clap` to parse commands. Dispatches to the command handlers. Has a `--json` flag to switch from human-readable output to JSON.

### `lib.rs` — Library Root

The compiler is a **library** with a thin CLI wrapper. `lib.rs` re-exports everything. The library API (`compile_string`) can be consumed by:

- The WASM playground
- The LSP server
- Future tools
- Anyone embedding the compiler

## `src/analysis.rs` — Editor Symbol Index & Resolution

This is the **pure, async-free** analysis layer that powers the LSP's hover, go-to-definition, and completion. `src/lsp.rs` is a thin adapter on top; the WASM playground can reuse these APIs too. All offsets are **byte** offsets — UTF-16 conversion is the LSP adapter's job.

### `SymbolIndex` and `Symbol`

**`SymbolIndex`** is the project-wide definition table for one analysis run: a `Vec<(PathBuf, String)>` of loaded files, and a `Vec<Symbol>`.

**`Symbol`** is one named definition: name, kind (`SymKind` — Module/Param/Port/Clock/Reset/Wire/Reg/Mem/Const/Enum/EnumVariant/Inst), which file it's in (`file_idx`), its defining span (byte offsets), hover text, and the enclosing module's index.

### `build_index(files)`

Walks all loaded files' ASTs and emits one `Symbol` per declaration. Everything that has a name and a span ends up here: module names, parameters, ports, clocks, resets, wires, regs, mems, consts, enum types + variants, and instance names. The hover `render` is a one-liner like `out y: bits[8] — output port`.

### `resolve_at(index, file_idx, offset)`

Given a cursor position (byte offset into a specific file), finds the identifier under the cursor and returns the `Symbol` it resolves to — i.e. its **declaration** span, not the use site. This powers go-to-definition and hover.

It handles:

- Module-local names (port, reg, wire, const, param) — resolved within the enclosing module
- Module names at instantiation sites — cross-file, pointing into the imported file
- Names inside `test` blocks — ports of the module under test

**`parse_recover`** `Error` nodes don't crash resolution; good declarations around a broken line still resolve.

### `completions(index, file_idx, offset, majority_flavor)`

Returns a list of `Candidate`s for the current cursor position: all in-scope module members (as `CandKind::Ident`), plus the full keyword set for the file's majority flavor (as `CandKind::Keyword`). Keywords from other flavors are excluded — a Tamil-flavored file never offers English spellings.

---

## `src/lsp.rs` — The Language Server

The LSP server powers the **VS Code extension** (and potentially other editors). It's a diagnostics-only server in v0 — it reports errors and warnings as you type, but hover info and go-to-definition come later.

**`run()`** — starts the server over stdio. It creates a Tokio runtime and a `tower-lsp` service that listens for LSP messages.

**`Backend::recheck(uri, text)`** — the heart of it. Whenever you open, change, or save a `.mimz` file in VS Code, this runs:

1. Calls `analyze()` to lex, parse, and check the entire project (the file + its imports from disk)
2. Localizes each diagnostic to the file's predominant keyword flavor
3. Publishes diagnostics to the editor — each file gets its own `publishDiagnostics` call
4. Clears stale diagnostics for files that no longer have errors

**`analyze(entry, text)`** — the in-memory pipeline. It parses the entry document's current text (from the editor, not disk), then resolves imports by walking the filesystem. The checker runs across the whole project, and diagnostics are attributed to their source file.

**`to_lsp(d, src, flavor)`** — converts a `Diag` to an LSP `Diagnostic`. The WHAT line is localized if the catalog covers it, and the help line is appended after a `\nhelp:` prefix (with a trailing space). The span is converted to LSP `Range` with UTF-16 character offsets (because that's what the LSP protocol requires — important for Tamil text, where one character may be 1 or 2 UTF-16 units).

**`position(src, offset)`** — converts a byte offset to an LSP `Position`. LSP measures columns in UTF-16 code units, not bytes and not chars. A Tamil identifier like `மணி` is 9 UTF-8 bytes but only 3 UTF-16 units, so this function counts carefully.

The LSP feature depends on `tokio` and `tower-lsp`, which are **optional** behind the `lsp` feature flag. The WASM build excludes them because they won't compile on `wasm32`.

## `crates/mimz-wasm/` — The Browser Playground

This is a separate crate in the workspace (`crates/mimz-wasm/`) that wraps the compiler for the browser. It's only 40 lines of Rust — all the heavy lifting is in the core library.

**`compile_to_verilog(source)`** — compiled to WASM, exposed to JavaScript as `compileToVerilog(source)`. It calls `mimz::compile_string()` and either returns Verilog text or throws a JS `Error` with the rendered diagnostics.

**`run_command(source, command, args)`** — exposed as `runCommand(source, command, args)`. It calls `mimz::run_command()` — the same in-memory runner that powers the CLI's WASM-adjacent paths.

The WASM crate is **not** in the workspace's `default-members`, so everyday `cargo build`/`cargo test` at the root doesn't try to compile it (it targets `wasm32` and pulls in `wasm-bindgen`). You build it explicitly:

```
wasm-pack build crates/mimz-wasm --target web
```

The output lives in `crates/mimz-wasm/pkg/` — a `.wasm` file plus JS glue that the documentation website loads.

## `editors/vscode/` — The VS Code Extension

The extension is intentionally plain JavaScript — no build step, no TypeScript compilation. What's in the repo IS what ships in the `.vsix`.

**`extension.js`** — 44 lines. On activation, it starts the `mimz lsp` process as a language server client. The path to the `mimz` binary can be configured via `mimz.serverPath` in VS Code settings (default: just `mimz` on your PATH). If the server can't start, it shows a friendly warning — syntax highlighting still works, you just won't get live diagnostics.

**`syntaxes/mimz.tmLanguage.json`** — the TextMate grammar that gives you syntax highlighting in the editor. It's 126 lines and defines patterns for:

- Comments (`//` and `/* */`)
- Strings
- Module declarations (colors `module`/`thoguthi`/`தொகுதி` and the name after it)
- Enum declarations (colors `enum`/`vagai`/`வகை`)
- Declaration keywords (all 30+ in all three flavors)
- Control-flow keywords (`if`/`enil`/`எனில்`, `match`/`thernthedu`/`தேர்ந்தெடு`, etc.)
- Boolean constants (`true`/`false`/`unmai`/`உண்மை`)
- Types (`bit`, `bits`, `signed`, `int`, `bool`)
- Builtins (`extend`, `trunc`, `min`, `max`, `abs`, `nand`, `nor`, `xnor`)
- Numbers, operators
- Reserved words (highlighted as warnings)

It uses Unicode-aware word boundaries (`(?<![\\p{L}\\p{N}_])`) instead of `\b` so Tamil-script keywords match correctly. A test (`tests/grammar_sync.rs`) checks that the keyword list here stays in sync with `keywords.toml`.

**`language-configuration.json`** — defines comment toggles, bracket matching, and auto-closing pairs.
