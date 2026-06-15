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

- `Diag::new(span, msg).with_code(code).with_help(help)` — anywhere.
- Parser: `self.error(span, code, msg)` then optionally `self.help(text)`
  (attaches to the most recent error). The code argument is mandatory —
  same discipline as `Checker::err`.
- Emitter: `self.err(span, msg, help)` (empty `help` = no help line).

## Stable error codes — the full map

Every diagnostic in the compiler carries a code (retrofit completed
2026-06-12). They are a stable contract: tests assert on them, the
`--json` output exposes them, and the Phase 1.8 Tanglish/Tamil catalogs
will key off them — never renumber.

| Block       | Stage   | Catalog                          |
| ----------- | ------- | -------------------------------- |
| E0001–E0701 | checker | [`11-checker.md`](11-checker.md) |
| E10xx       | lexer   | below                            |
| E11xx       | parser  | below                            |
| E12xx       | loader  | below                            |

| Code  | Meaning                                                    |
| ----- | ---------------------------------------------------------- |
| E1001 | unterminated block comment                                 |
| E1002 | unterminated string                                        |
| E1003 | Tamil digits in a literal (ASCII digits are universal)     |
| E1004 | malformed number                                           |
| E1005 | reserved word used as a name                               |
| E1006 | division `/` does not exist (teaches the hardware cost)    |
| E1007 | modulo `%` does not exist (teaches `+%`/slicing)           |
| E1008 | unexpected character                                       |
| E1101 | expected-X-found-Y family (incl. terminators, missing `}`) |
| E1102 | bad top-level item                                         |
| E1103 | enum needs at least one variant                            |
| E1104 | register has no reset value                                |
| E1105 | `<-` outside an `on` block                                 |
| E1106 | `=` inside an `on` block                                   |
| E1107 | `test` block syntax (name, body statements)                |
| E1108 | value-driving `if` without `else` (the latch lesson)       |
| E1109 | chained comparison                                         |
| E1110 | call errors (not a builtin, wrong arity)                   |
| E1111 | parameter/const type is not `int`/`bool`                   |
| E1112 | unknown `syntax` profile (only `thamizh` is valid)         |
| E1201 | imported file does not exist                               |

Grouping rule: E1101 deliberately covers the whole expected/found
family — those messages share one translation shape; the codes that
stand alone are the TEACHING errors whose catalogs differ.

## The `--json` wire format

`mimz check --json` / `mimz compile --json` print **one JSON array on
stdout** — always, even on success (`[]`) — so editors and the
npm/PyPI wrappers never parse human text. The exit code still signals
pass/fail. Each entry is a `diag::JsonDiag`:

```json
{
  "severity": "error",
  "code": "E0601",
  "message": "`match` on enum `S` is missing `C`",
  "help": "every variant needs an arm, or end with `_ =>` ...",
  "path": "examples/english/traffic_light.mimz",
  "line": 14,
  "col": 13,
  "span": [195, 196]
}
```

`severity` is `"error"` or `"warning"`; `line`/`col` are 1-based (columns
count chars, matching the caret renderer); `span` is the byte range into
the NFC-normalized source. Locked end-to-end by
`json_flag_emits_machine_readable_diagnostics` (`tests/errors.rs`).

## Warnings (`Wxxxx`) — non-fatal lints

A `Diag` carries a `Severity` (`Error` or `Warning`). An **error** fails the
build; a **warning** is advisory — `check`/`compile`/`eval` print it (rendered
`warning[Wxxxx]: …`, or with `"severity": "warning"` under `--json`) and still
**succeed (exit 0) and still produce output**. The LSP shows warnings with
`DiagnosticSeverity::WARNING` (a yellow squiggle, not red). Warnings are opt-in
via `Diag::as_warning`; almost every diagnostic is an error.

W-codes are NOT in `ALL_CHECKER_CODES` (that list is checker errors, locked to
the table above and required to have an error-fixture). Current warnings:

| Code  | Fires when                                                                                                                                                                                                  |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| W0001 | a file mixes **Tamil** keywords with English/Tanglish ones — English+Tanglish share code order (SVO) and mix freely, but Tamil reads differently; run `mimz fmt` to normalize (`morph::flavor_mix_warning`) |

## Known limitations / planned evolution

- **English only** today. Tanglish/Tamil error catalogs land with Phase
  1.8, keyed off the codes above.
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
