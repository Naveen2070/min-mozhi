# 06 — Diagnostics (`src/diag.rs`, `src/span.rs`)

Error quality is a core goal (spec/01 G1), not a feature. This is the
smallest subsystem and the most important one to keep healthy.

## The model

```rust
pub struct Diag {
    pub span: Span,          // WHERE — byte range into the source
    pub msg: String,         // WHAT is wrong — one sentence
    pub help: Option<String> // HOW to fix it — the teaching line
}
```

Three rules, enforced by convention everywhere in the codebase:

1. **Diagnostics are values.** Passes collect `Vec<Diag>` and keep
   working. Nothing prints mid-pass, nothing panics on user input.
   (Panics are reserved for OUR bugs — e.g. a malformed `keywords.toml`.)
2. **Multi-error always.** Lexer, parser, and emitter all continue after
   an error. A learner gets the whole list, not one error per compile.
3. **Render once, at the edge.** Only the CLI calls `diag::render`, which
   produces rustc-style output: message, `--> path:line:col`, the source
   line, a caret underline, and the help line.

`Span` is a half-open **byte** range into the NFC-normalized source.
`render`/`locate` convert to 1-based line/column (counting chars, not
bytes, so Tamil identifiers underline correctly).

## How to write a good Min-Mozhi error

The persona check: would a 20-year-old polytechnic student, not fully
comfortable in English, know what to DO after reading it?

- **`msg`** names the construct and the problem, quoting the user's own
  identifier: ``register `value` has no reset value``.
- **`help`** says how to fix it, shows the corrected shape, and where it
  earns its place, says WHY the rule exists and cites the spec:
  `every reg declares its reset value: 'reg name: type = 0' — no
uninitialized state (spec/02 section 1.2)`.
- The best errors teach hardware, not just syntax. House style examples:
  the missing-`else` error explains how latches are born; the `/` error
  explains division hardware cost.
- In the parser, prefer `expect(kind, "…")` with a learner-phrased
  `what` ("a module name", "`:` then the wire's type") — context beats
  "expected identifier".

Patterns in code:

- `Diag::new(span, msg).with_help(help)` — anywhere.
- Parser: `self.error(span, msg)` then optionally `self.help(text)`
  (attaches to the most recent error).
- Emitter: `self.err(span, msg, help)` (empty `help` = no help line).

## Known limitations / planned evolution

- **English only** today. Tanglish/Tamil error catalogs land with Phase
  1.8. Stable codes EXIST now (2026-06-11): `Diag.code` renders as
  `error[E0101]: ...`; every CHECKER error carries one (catalog in
  [`11-checker.md`](11-checker.md)). Lexer/parser errors still need the
  retrofit — do it before the Phase 1.8 catalogs, so translations key
  off codes, not English strings.
- Caret rendering clamps to a single line; multi-line spans underline
  only the first line. Fine for current errors.
- One span per diagnostic — no secondary labels ("first driver was
  here"). The single-driver checker error will want that; extend `Diag`
  with optional secondary spans when it does.

## Multi-file errors: the `file` field

A span is a byte range with no file identity, so `Diag` carries
`file: Option<usize>` — an index into the loaded file list:

- **Single-file passes** (lexer, parser) leave it `None`; the caller
  already knows which file it is processing and renders with
  `diag::render` directly.
- **Project-wide passes** (the checker, `Project::from_files`, the
  emitter) MUST set it — `Checker::err()` takes the file index as a
  required argument, `from_files` stamps the file it is iterating, and
  the `Emitter` stamps `cur_file` automatically inside `err()`. The CLI
  renders these via `project::render_diags`, which picks each
  diagnostic's own source file (entry file as the fallback).

Regression-guarded by `diags_carry_the_file_index`
(`src/emit_verilog/mod.rs`) and
`duplicate_module_across_files_is_e0001_in_the_right_file`
(`src/checker/tests.rs`). If you write a new project-wide pass: stamp
the file index on every diagnostic, and render through `render_diags`.
