---
title: CLI reference
description: Every mimz subcommand and its flags — check, compile, test, sim, eval, repl, fmt, translate, lint, explain, doctor, init, eject and completions.
order: 7
---

# CLI reference

```console
$ mimz <command> [file] [flags]
```

Per-project defaults live in a `mimz.toml`; CLI flags override it.

**Exit codes:** `0` success · `1` any failure — compilation errors, failed
checks, I/O errors.

## Commands

| Command       | Does                                                                 |
| ------------- | -------------------------------------------------------------------- |
| `init`        | scaffold a new project in `./<name>/`                                |
| `check`       | check a `.mimz` file for errors                                      |
| `compile`     | compile a `.mimz` file to Verilog                                    |
| `test`        | run the file's `test` blocks and report pass/fail                    |
| `sim`         | simulate a clocked module _(experimental)_                           |
| `eval`        | evaluate a combinational module _(experimental)_                     |
| `repl`        | interactive REPL for a combinational module _(experimental)_         |
| `lint`        | style and hygiene warnings                                           |
| `fmt`         | normalize a file's keyword flavor in place                           |
| `translate`   | reskin a file's keywords into another flavor                         |
| `explain`     | explain a diagnostic code, or `--list` them all                      |
| `doctor`      | report toolchain and environment health (aliased `env`)              |
| `eject`       | write the embedded standard library out for vendoring                |
| `completions` | generate a shell tab-completion script                               |
| `lsp`         | run the language server over stdio                                   |

There is **no `build` and no `run`**. `check` is the fast path; `compile`
produces Verilog; `test` runs test blocks.

## Global flags

| Flag                  | Meaning                                          |
| --------------------- | ------------------------------------------------ |
| `-c, --config <FILE>` | use a specific `mimz.toml`                       |
| `-q, --quiet`         | suppress status banners                          |
| `-d, --debug`         | verbose progress                                 |
| `--color <WHEN>`      | `always` \| `never` \| `auto`                    |
| `-l, --lang <FLAVOR>` | `english` \| `tanglish` \| `tamil` (`en`/`tl`/`ta`) |

## Per-command flags

### `check`

`--tokens` dump tokens · `--json` machine-readable · `--watch` re-check on save
· `-l/--lang`

### `compile`

`-o/--output <path>` · `--emit-testbench` · `--json` · `--extern-src <file>` ·
`--extern-sim warn|strict` · `-l/--lang`

### `test`

`-f/--filter <substr>` · `--trace[=<style>]` · `--verbose` · `--signals <a,b>` ·
`--extern-sim warn|strict` · `--emulate` · `--step` (implies `--emulate`) ·
`-l/--lang`

`--emulate` and `--step` gate the **live** peripheral view, and only when stdout
is a real terminal — see [quirks](/handbook/06-quirks).

### `sim`

`-o <path.vcd>` · `--cycles N` · `--clock <c>` · `--in a=1,b=2` · `--param W=8` ·
`--sweep a=0|1,b=2` · `--module <M>` · `--trace[=changes]` · `--verbose` ·
`--signals <a,b>` · `-l/--lang`

### `eval`

`--in a=1,b=2` · `--module <M>` · `--param W=8` · `-l/--lang`

### `repl`

`--param W=8` · `--module <M>` · `-l/--lang`

### `lint`

`--json` · `-l/--lang`

### `fmt`

`--to <flavor>` · `--strict` (warn on mixed flavors) · `-o <path>`

### `translate`

`--to <flavor>` · `--order code|thamizh` · `--romanize-names` ·
`--names-map <file>` · `--no-names-map` · `-o <path>`

`fmt` normalizes in place; `translate` reskins into another flavor and can also
switch word order.

### `explain`

Takes an `E`-code, case-insensitive, or `--list` for every code with a one-line
summary.

### `eject`

`--to <dir>` · `--flavor english|tamil` · `--force`

Then point `mimz.toml [lib] std` at that directory.

### `doctor`

`--dev` runs the contributor toolchain check as well.

### `completions`

Takes a shell name: `bash` \| `zsh` \| `fish` \| `powershell` \| `elvish`.

## Getting more detail

Every subcommand has its own full help:

```console
$ mimz <command> --help
```
