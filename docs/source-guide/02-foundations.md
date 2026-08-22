# 2 — Foundations: Spans, Diagnostics, Language, Config, Loading

These are the support modules that everything else depends on. They don't know about the AST or the grammar — they provide the plumbing that makes error messages pretty, configs loadable, and files importable.

---

## `crates/mimz-core/src/span.rs` — Remembering Where Things Are (71 lines)

This tiny file defines one thing: a **Span**. A span is a half-open range of byte offsets — `[start, end)` — into the source text. Every single token and every single AST node carries one.

**Why does this matter?** Without spans, error messages would be useless:

```
error: something's wrong
```

With spans, the compiler can say:

```
error[E0501]: signal `count` has more than one driver
  --> counter.mimz:7:3
   |
 7|   count = value
   |   ^^^^^
   = help: every wire/output is driven exactly once
```

That underlined `^^^^^` comes from the span. It tells the diagnostic renderer exactly which bytes to highlight.

**`Span::new(start, end)`** — just stores two numbers. The half-open `[start, end)` convention means `src[span.start..span.end]` gives you the original text.

**`Span::join(other)`** — when you build a compound node like `a + b`, you need a span that covers everything. `join` just takes the smaller start and bigger end: `min(a_start, b_start)..max(a_end, b_end)`.

**`Span::line_col(src)`** — a lightweight, self-contained byte-offset → 1-based `(line, column)` conversion for callers that just need a human-readable location without going through the full diagnostic renderer. Used to default-label an `assert`/`cover` statement's report entry (`"line:col"`) when it carries no explicit string label — see `crates/mimz-sim/src/runner.rs`.

---

## `crates/mimz-core/src/diag.rs` — Making Error Messages Pretty

This is the compiler's error and warning system. Every pass in the compiler reports problems by pushing a `Diag` value into a list — they never print directly, never panic, and never stop at the first error. They just collect all the problems and keep going.

### The Big Picture

There are two types of diagnostics:

- **Error** — fails the build, exit code isn't zero
- **Warning** — just a nudge, exit code is zero, output still happens

### `ALL_CHECKER_CODES` — The Master List of Error Codes

This is a compile-time array listing all 75 stable checker error codes (`E0001`–`E0912`, `E1301`–`E1302`), alongside the compiler flavor warning `W0001`. Once a code is assigned, it never changes — no renumbering, ever. This means documentation, `mimz explain`, and any Stack Overflow answers stay valid permanently.

A unit test checks that every code here has a test fixture, and another checks that `mimz explain` covers every one.

### `Diag` — One Error Message

Think of a `Diag` as a little package containing:

- **Where** it happened (a `Span`)
- **What** went wrong (a short message like "signal `x` has more than one driver")
- **How** to fix it (an optional help line)
- **Which file** it's in (an index into the loaded files list)
- **The error code** like `"E0501"`
- **Severity** — are we failing the build or just warning?
- **Arguments** for the localized error catalog (Tamil/Tanglish templates)

The builder pattern makes creating diagnostics painless:

```rust
Diag::new(span, "signal `x` has more than one driver")
    .with_code("E0501")
    .with_help("every wire/output is driven exactly once")
```

### `render(diags, src, path)` — The Pretty Printer

This takes a pile of diagnostics and turns them into the rustc-style output you saw above. For each diagnostic:

1. It figures out which line and column the span points to
2. It formats: `error[CODE]: message\n  --> path:line:col\n   |\n  N| line_text\n   | ^^^^^`
3. It appends `= help:` if there's a help line

### `render_lang(...)` — Errors in Your Language

Same as `render`, but it checks if the error code has a localized template in Tamil or Tanglish. If so, it uses that instead of the English message. This is the Phase 1.8 feature — errors in the language you wrote your code in.

### `locate(src, offset)` — Finding Position in Source

This converts a byte offset into a line and column. It walks through the source counting `\n` characters until it passes the offset. The column count handles Unicode characters properly (important for Tamil script).

### `JsonDiag` — For Tools and Editors

Same information, but in JSON format. When you pass `--json`, diagnostics come out as structured data instead of human text. The VS Code extension and LSP server use this format.

---

## `crates/mimz-core/src/morph.rs` — Speaking Your Language

This file does two things that go together: it figures out **which language to show errors in**, and it attaches **Tamil case suffixes** to names so error messages read naturally.

### Picking the Error Language

**`majority_flavor(tokens)`** counts every keyword token in your file — English keywords vs Tanglish vs Tamil — and returns whichever one you used most. If you wrote 10 Tamil keywords and 2 English ones, you get Tamil errors. Ties are broken in column order: English > Tanglish > Tamil.

You can override this with `--lang` on the command line.

**`effective_lang(cli_override, tokens)`** is the single source of truth: if you passed `--lang`, use that; otherwise use `majority_flavor`.

**`flavors_used(tokens)`** just lists which flavors appear in a file. Used by `mimz fmt --strict` to detect mixed-flavor files.

**`flavor_mix_warning(tokens)`** produces a non-fatal warning (W0001) when you mix Tamil with English/Tanglish. The reason: Tamil uses a different sentence structure (SOV: subject-object-verb), while English and Tanglish share SVO (subject-verb-object). Mixing them makes reading harder. It's just a friendly nudge though — not a build failure.

### Tamil Case Suffixes

Tamil has four grammatical cases (வேற்றுமை) that get attached to nouns depending on their role in a sentence:

- **Accusative** (-ஐ) — the object of an action
- **Dative** (-க்கு) — "to / for" something
- **Locative** (-இல்) — "in / at" something
- **Instrumental** (-ஆல்) — "by / with" something

**`inflect(name, case, flavor)`** — this is the function that attaches suffixes. The rules (ratified by a native-speaker panel in June 2026) are:

- If the name is in English/Latin letters: add a hyphen before the suffix → `sum` + `-ஐ` = `sum-ஐ`
- If the name is in Tamil script: join directly, no hyphen → `நிலை` + `இல்` = `நிலைஇல்`
- Tanglish: always hyphenate → `sum` + `-aal` = `sum-aal`
- English: no suffix at all, just return the bare name

### The Localized Error Catalog

**`localized_msg(d, src, flavor)`** is the main entry point the renderer calls. It:

1. Looks up the diagnostic's E-code in `messages.toml` for the target flavor
2. If a template exists, fills in `{name}`, `{name.acc}`, `{name.dat}`, etc.
3. If the result still has unfilled `{...}` tokens (meaning the template doesn't fit this diagnostic), falls back to English
4. If no catalog entry exists, returns `None` (English fallback)

**`fill(template, name, args, flavor)`** does the actual text substitution — replacing `{name}` with the identifier, `{name.acc}` with the inflected form, and any structured `{key}` args from the diagnostic.

---

## `crates/mimz-core/src/bits.rs`, `wide.rs`, `width_rules.rs` — Arbitrary-Width Values

Three files behind one guarantee: a signal can be wider than 128 bits and
every pass (checker, simulator, emitter) still agrees on its value and
width.

**`bits.rs`** defines `Bits`, the raw bit-pattern representation shared by
`Val` (simulator values) and `ConstVal` (compile-time constants — see
[`06-checker.md`](06-checker.md)): `Small(u128)` for the fast path (width ≤
128), `Wide(Vec<u64>)` for anything larger. Whoever constructs a value must
guarantee a `Wide` variant is never used when the value actually fits in
128 bits — every constructor in this module upholds that invariant.

**`wide.rs`** is the "slow path": arbitrary-width arithmetic over
little-endian `u64` limb vectors (a `Vec<u64>` for width `w` always holds
exactly `limb_count(w)` limbs). Not a general-purpose bignum library —
only the handful of operations the evaluator actually needs (no division;
the language has none).

**`width_rules.rs`** holds `MAX_WIDTH` (1,000,000 — the ceiling every pass
enforces) and `Kind` (width + signedness), plus the width/signedness
inference rules themselves (`shift_result` and friends). The checker's
`widths` pass and the simulator's `sim/value/` each used to compute these
independently — two copies of the same rule can drift, which is exactly
how BUG-21 happened (the simulator's slice evaluator disagreed with the
checker's own `slice_ty` on signedness). This module holds one
implementation per rule that both consume.

---

## `src/config.rs` — Reading `mimz.toml` Settings

This lets you set default CLI flags in a per-project config file, just like `Cargo.toml` or `rustfmt.toml`. The precedence is simple: **CLI flag beats `mimz.toml` beats built-in default**.

### How Config Discovery Works

**`Config::discover(start)`** walks up from your input file's directory, checking each parent for `mimz.toml`. It stops after 256 levels (just a safety bound — no real project nests that deep).

**`Config::load(path)`** reads and parses the file. Unlike the embedded keyword tables (which panic on error because they're developer-controlled), `mimz.toml` is user-authored and must fail gracefully with a clear message.

**`Config::resolve(input, explicit)`** ties it together: if you passed `--config`, load that; otherwise try `discover`; no file found means all defaults.

### What You Can Set

```toml
lang = "tamil"

[compile]
emit_testbench = true

[translate]
to = "english"
romanize_names = true

[fmt]
strict = true
```

---

## `src/project.rs` — Loading Files and Following Imports

`project.rs` is split across the workspace split: the fs-touching functions
below (`read_source`, `parse_file`, `load_project`) stay in the root shell
crate at `src/project.rs`, since they do real disk I/O. The pure
`LoadedFile` struct and `render_diags`/`render_diags_lang` (no fs I/O, just
rendering) moved to `crates/mimz-core/src/project.rs` — mimz-core has no I/O
of its own.

This is how the compiler reads your `.mimz` files, normalizes them, runs the lexer/parser, and follows `import` declarations.

### Safety First: 32 MB Limit

`MAX_SOURCE_BYTES = 32 * 1024 * 1024` — this prevents a huge file from blowing up memory. The lexer pre-collects all character positions (several times the file size), so this ceiling keeps things sane.

### `read_source(path)` — Reading and Normalizing

Reads a file from disk, checks it's under the size limit, and NFC-normalizes the text. NFC normalization is important for Tamil: it ensures that combining characters (`க்`, `ச்`, etc.) compare consistently regardless of how they were encoded.

### `parse_file(path)` — One File Through the Pipeline

This is the three-step pipeline for a single file:

1. `read_source` → get the text
2. `lexer::lex` → get tokens
3. `parser::parse` → get AST

If any step fails, it returns `LoadError::Source` with all the diagnostics plus the source text (so the caller can render pretty error messages).

### `load_project(entry)` — Following Imports

This is the multi-file version. It uses a queue and a visited set (based on canonicalized paths):

1. Start with the entry file
2. Pop a path from the queue, parse it
3. For each `import lib.adder` in the parsed AST:
   - Convert dots to path separators: `lib/adder.mimz`
   - Resolve relative to the importing file's directory
   - Check the file exists (E1201 if not)
   - Push it onto the queue
4. The entry file is always `files[0]`

The visited set handles cycles — if file A imports B and B imports A, the second one is silently skipped.

### `render_diags(diags, files)` — Project-Wide Error Rendering (`crates/mimz-core/src/project.rs`)

For single-file passes, `diag::render` works fine. But the checker and emitter see the whole project, and their spans may point into any loaded file. This function routes each diagnostic to the right file's source text for rendering.

---

## `crates/mimz-sim/src/runner.rs` — The In-Memory Command Engine

This is what powers the **browser playground** — it runs any `mimz` command against a source _string_ instead of a file. No filesystem, no I/O, no process exits.

### Argument Parsing Helpers

These are shared by both the in-memory runner and the CLI command handlers. Having one definition prevents drift.

**`parse_u128(s)`** — parses decimal `42`, hex `0xFF`, or binary `0b1010` to a `u128`.

**`parse_bindings(s, val_parser)`** — parses `"name=val,name=val"` into a `BTreeMap`. Empty string → empty map.

**`parse_sweep(s)`** — parses `"name=v1|v2|v3,other=w1|w2"` into `[(name, [values])]` pairs. This defines dimensions for a cartesian product of test vectors.

**`parse_steps(s)`** — parses `"a=3,b=5;a=7,b=1"` into explicit per-step vectors. Groups are split on `;`. This is what the playground's step table produces.

**`sweep_vectors(base, sweep)`** — computes the cartesian product of sweep dimensions. So `--sweep a=0|1,b=5|6` with `--in c=1` produces:

```
[{a:0, b:5, c:1}, {a:0, b:6, c:1}, {a:1, b:5, c:1}, {a:1, b:6, c:1}]
```

It checks the product doesn't exceed `MAX_SWEEP_VECTORS` before allocating, so a large sweep can't crash the tool.

**`trace_scope(all, default, verbose, signals, module)`** — resolves which signals to trace. `--signals a,b` (explicit set) beats `--verbose` (all signals) beats the default (interface + state).

### The Commands Themselves

**`run_command(source, command, argv)`** — this is the main entry. It dispatches to:

- `check` — lex, parse, check. Returns "OK" or diagnostics.
- `compile` — full pipeline to Verilog. Rejects `import` (single-file only).
- `eval` — evaluate a combinational module with given inputs, print outputs.
- `ports` — describe a module's interface as JSON (for the playground UI).
- `sim` — simulate a clocked or combinational design with stimulus.
- `test` — run test blocks and report pass/fail.

---

## `crates/mimz-core/src/stdlib.rs` — The Embedded Standard Library

This file bakes the standard library **directly into the compiler binary** using `include_str!`. No install path, no filesystem dependency — it works in WASM and in any bare-binary distribution.

### Why embed it?

The five stdlib modules (`debouncer`, `seg7`, `pwm`, `fifo`, `uart_tx`) are already tested as real example files under `examples/english/std/` and `examples/tamil-pure/`. `stdlib.rs` just `include_str!`s those same files — a single source of truth, no duplication.

### The catalog

`static MODULES: &[StdModule]` — one entry per module. Each entry has:

- `stem` — the import path segment (`"fifo"`, `"uart_tx"`, etc.)
- `canonical_src` / `canonical_name` — English-identifier source and module name (`Fifo`)
- `twin_src` / `twin_name` / `twin_roman` — pure-Tamil twin source (`தொகுதி வரிசை`), name, and its romanization (`varisai`)

### Resolution

**`resolve(ns, module) -> Option<(&StdModule, StdVariant)>`** — the routing entry. First checks `ns` via `is_std_namespace` (`NS_ALIASES`: `["std", "nuulagam", "நூலகம்"]`) so any of the three namespace spellings work; `None` if not. Then matches `module` against canonical stem, twin name, and twin romanization, returning the catalog row plus which `StdVariant` (Canonical/Twin) the spelling selected.

**`is_std_namespace(alias)`** — thin predicate used by the project loader to decide whether to try the embedded library.

**`eject_to(dir, tamil, force)`** — vendors the stdlib to disk: writes all modules' source files to `dir/`, returning the paths written. `tamil = true` writes the pure-Tamil twin spellings (files named after the twin; for a Tamil-first project). `force = false` aborts on any pre-existing file before touching anything else (all-or-nothing invariant). Used by `mimz eject std` (`src/commands/eject.rs`).

### Override path

When `mimz.toml [lib] std = "<dir>"` is set, `project::load_project_with_lib` bypasses this module entirely and loads `<dir>/<stem>.mimz` from disk instead.
