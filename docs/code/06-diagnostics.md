# 06 — Diagnostics (`crates/mimz-core/src/diag.rs`, `crates/mimz-core/src/span.rs`)

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
   (Panics are reserved for OUR bugs — e.g. a malformed `lang/keywords.toml`.)
2. **Multi-error always.** Lexer, parser, and emitter all continue after
   an error. A learner gets the whole list, not one error per compile.
3. **Render once, at the edge.** Only the CLI calls `diag::render`, which
   produces rustc-style output: message, `--> path:line:col`, the source
   line, a caret underline, and the help line.

`Span` is a half-open **byte** range into the NFC-normalized source.
`render`/`locate` convert to 1-based line/column (counting chars, not
bytes, so Tamil identifiers underline correctly).

## How to write a good Min-Mozhi error

The persona check: would a student new to hardware design know what to DO after
reading it — including the native-Tamil audience this is built for, a Tamil-speaking
polytechnic student not fully comfortable in English?

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

| Block       | Stage                   | Catalog                          |
| ----------- | ----------------------- | -------------------------------- |
| E0001–E0912 | checker                 | [`11-checker.md`](11-checker.md) |
| E10xx       | lexer                   | below                            |
| E11xx       | parser                  | below                            |
| E12xx       | loader                  | below                            |
| E1301–E1302 | checker (extern module) | [`11-checker.md`](11-checker.md) |
| W000x       | lint / flavor mixing    | below (§Warnings)                |
| S01xx–S04xx | simulator runtime       | [`13-tooling.md`](13-tooling.md) |

**How to read a code at a glance** — the first digit pair says WHICH
stage rejected your program, so you know what kind of mistake it is
before reading the message:

| Prefix                   | Stage that raised it | What it means for you                                                            |
| ------------------------ | -------------------- | -------------------------------------------------------------------------------- |
| `E00xx`–`E09xx`, `E13xx` | checker              | The text parsed fine; the _hardware rules_ were broken (widths, drivers, clocks) |
| `E10xx`                  | lexer                | The characters could not be turned into words — a typo at the character level    |
| `E11xx`                  | parser               | The words could not be arranged into a program — a typo at the grammar level     |
| `E12xx`                  | loader               | An `import` could not be resolved to a file                                      |
| `W000x`                  | lint / flavor        | Advisory only — the build still succeeds                                         |
| `S0xxx`                  | simulator            | The program compiled; something went wrong while RUNNING it                      |

`mimz-sim`'s own runtime diagnostics (`S01xx`–`S04xx`) are a SEPARATE
catalog — fires at elaboration/execution time, after the checker has
already accepted the program (`mimz sim`/`mimz eval`/`mimz test`, and
the WASM playground's single-source path). Catalogued in
[`13-tooling.md`](13-tooling.md#s0xxx--runtime-diagnostic-codes-r2-docsauditreview-2026-07-17md),
not here — `ALL_SIM_CODES` lives in `crates/mimz-sim`, not `mimz-core`.

| Code  | Meaning                                                             |
| ----- | ------------------------------------------------------------------- |
| E1001 | unterminated block comment                                          |
| E1002 | unterminated string                                                 |
| E1003 | Tamil digits in a literal (ASCII digits are universal)              |
| E1004 | malformed number                                                    |
| E1005 | reserved word used as a name                                        |
| E1006 | division `/` does not exist (teaches the hardware cost)             |
| E1007 | modulo `%` does not exist (teaches `+%`/slicing)                    |
| E1008 | unexpected character                                                |
| E1101 | expected-X-found-Y family (incl. terminators, missing `}`)          |
| E1102 | bad top-level item                                                  |
| E1103 | enum needs at least one variant                                     |
| E1104 | register has no reset value, or memory has no init value            |
| E1105 | `<-` outside an `on` block                                          |
| E1106 | `=` inside an `on` block                                            |
| E1107 | `test` block syntax (name, body statements)                         |
| E1108 | value-driving `if` without `else` (the latch lesson)                |
| E1109 | chained comparison                                                  |
| E1110 | call errors (not a builtin, wrong arity)                            |
| E1111 | parameter/const type is not `int`/`bool`                            |
| E1112 | unknown `syntax` profile (only `thamizh` is valid)                  |
| E1113 | nested too deeply to parse safely (the anti-stack-overflow guard)   |
| E1114 | `sim` block syntax (`speed`/`bind` clause is malformed)             |
| E1115 | `??` applied to an already-optional type (`bits[8]??`)              |
| E1116 | unknown `sync.*` method (only `double_flop`/`pulse` exist)          |
| E1201 | imported file does not exist                                        |
| E1202 | bad standard-library import (`std.<module>` shape / unknown module) |

Grouping rule: E1101 deliberately covers the whole expected/found
family — those messages share one translation shape; the codes that
stand alone are the TEACHING errors whose catalogs differ.

`E1113` is a **safety guard, not a language rule**: the recursive-descent
parser counts nesting depth (`parser::MAX_DEPTH`) and bails with one
clean diagnostic rather than letting adversarial input abort the process
with a stack overflow. It is latched, so a 2000-deep expression produces
exactly one E1113, not 2000.

Not every code has a long-form `mimz explain` entry yet: `E1112`,
`E1113` and `W0001` are message-only today. `E0904` is a parser code
that this page's numbering scheme would put in the checker block — see
the note in [`11-checker.md`](11-checker.md#error-code-catalog).

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

Current warnings — two sources, one severity:

| Code  | Raised by                               | Fires when                                                                                                                                                                    |
| ----- | --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| W0001 | `morph::flavor_mix_warning` (always on) | a file mixes **Tamil** keywords with English/Tanglish ones — English+Tanglish share code order (SVO) and mix freely, but Tamil reads differently; run `mimz fmt` to normalize |
| W0002 | `lint` (`mimz lint`)                    | a signal name is not `snake_case`                                                                                                                                             |
| W0003 | `lint` (`mimz lint`)                    | a module name is not `PascalCase`                                                                                                                                             |
| W0004 | `lint` (`mimz lint`)                    | a signal is declared but never read — prefix with `_` to suppress                                                                                                             |

Only `W0001` rides the normal `check`/`compile` path; `W0002`–`W0004`
live in the separate `lint.rs` pass and surface only through `mimz lint`
(never fail a build, additive by design — a new lint can never break an
existing program).

`W0001` IS a member of `ALL_CHECKER_CODES` (it ships with an error
fixture like every checker code); `W0002`–`W0004` are not, since that
list is the fixture-backed checker contract.

## Known limitations / planned evolution

- **Native-authored Tamil + Tanglish catalogs shipped** (2026-06-15,
  decision C3 ratified). The localized messages live in `lang/messages.toml`,
  keyed off the codes above; `morph::localized_msg` looks one up per code and
  flavor and interpolates the offending identifier (Tamil case-inflected) plus
  structured args (`{expected}/{found}/{op}/{lhs}/{rhs}/{first}/{second}/{type}`).
  **33 of 74 checker codes** (`diag::ALL_CHECKER_CODES`) are localized — E0403/E0404/E0405
  stay English-only (each emits many distinct shapes; the Tamil drafts are preserved as
  comments in `lang/messages.toml`). Any code with no template renders the English `msg` verbatim,
  so uncovered codes are byte-identical across flavors. JSON diagnostics stay
  English (the machine contract is unchanged). Details in `13-tooling.md`.
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
(`crates/mimz-core/src/emit_verilog/mod.rs`). The checker-side regression test for this
contract (`duplicate_module_across_files_is_e0001_in_the_right_file`)
was retired when packages/namespacing (spec/02 §1.5b) made cross-file
module/enum/bundle name collisions legal — a new multi-file checker
test (e.g. an E0110/E0111 ambiguity case) should replace it; tracked
for the packages/namespacing plan's fixture/catalog task. If you write
a new project-wide pass: stamp the file index on every diagnostic, and
render through `render_diags`.
