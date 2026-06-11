# 02 — The Lexer (`src/lexer/`)

Source text (NFC-normalized by the caller) → `Vec<Token>`.

## File layout

| File          | Owns                                                           |
| ------------- | -------------------------------------------------------------- |
| `mod.rs`      | The `Lexer` state machine and the newline post-pass            |
| `token.rs`    | `TokKind`, `Kw`, `Flavor`, `Token`, error-message name helpers |
| `keywords.rs` | Loading `keywords.toml` into the runtime table                 |
| `tests.rs`    | Unit tests                                                     |

## The trilingual keyword table — the heart of the language

`keywords.toml` (repo root) holds one row per keyword with three
canonical spellings — `en`, `tanglish`, `tamil` — plus optional
per-column **alias lists** (`en_aliases` etc.) for deliberate synonyms,
e.g. `include` as an English alias of `import`. At build time the file is
embedded with `include_str!`; at first use a `LazyLock` parses it into
one `HashMap<spelling → (Kw, Flavor)>` — aliases land in the same map, so
an alias is indistinguishable from its canonical word from the parser
onward.

Consequences, all deliberate:

- The lexer recognizes the **union of all three columns at all times** —
  `module`, `thoguthi`, and `தொகுதி` all become `Kw::Module`. Mixing
  flavors in one file is legal; that IS the migration path.
- Changing a word (native-speaker review!) is a **data change** — edit
  the TOML, touch no Rust.
- The table **panics at startup** if the TOML is malformed, names an
  unknown key, or any spelling (canonical or alias) appears twice. Table
  bugs must be impossible to ship, and a startup panic in CI is how
  that's enforced.
- The token records **which flavor** spelled the keyword. Nothing uses it
  yet — it is recorded from day one so `mimz translate`/`fmt` and
  error-language detection (Phase 1.8) won't need a token-shape change.

TOML gotcha (it bit us once): the root-level `reserved` list **must sit
above the first `[keywords.*]` table** — in TOML, root keys cannot follow
a table header; placed below, `reserved` silently becomes a key inside
the last keyword's table.

## Scanning

The `Lexer` pre-collects `(byte offset, char)` pairs and walks them with
O(1) two-character lookahead — all this grammar needs. Dispatch is on the
first character:

- **Whitespace** (space/tab/CR) vanishes. **`\n` becomes a token** —
  newlines terminate statements.
- **Comments**: `//` to end of line vanishes; `/* ... */` vanishes but
  emits one `Newline` if it spanned lines (a multi-line comment still
  separates statements).
- **Numbers**: decimal, `0b`, `0x`, with `_` separators. The token keeps
  both the parsed `value: u128` and the `raw` spelling so the emitter can
  preserve the writer's base (`0xFF` stays hex in the Verilog). Tamil
  digits (௦–௯) get a dedicated teaching error (decision B14: ASCII digits
  are universal vocabulary).
- **Identifiers**: Unicode XID rules (`unicode-ident`), so Tamil-script
  identifiers work everywhere. Each identifier-shaped lexeme gets one
  table lookup: keyword (with flavor) / reserved word (error: "set aside
  for a future feature") / plain identifier.
- **Punctuation**: longest match first — `+%` before `+`, `<-` before
  `<`, `==` before `=`. Two characters maximum.
- **`/` and `%` do not exist** in Min-Mozhi. They are caught here, with
  teaching errors explaining the wrapping operators and slicing — a
  learner coming from C should hit a helpful wall, not a parse mystery.

The stream always ends with exactly one `Eof` token; the parser leans on
that to never run off the end.

## The newline policy (Go-style, `postprocess_newlines`)

Statements end at newlines — there are no semicolons. To keep multi-line
expressions natural, a post-pass drops newline tokens that cannot end a
statement:

- after any operator, comma, dot, `=`, `<-`, `=>`, `:`, or open bracket
  (`(`, `[`, `{`) — the line visibly continues;
- after the word forms `and` / `or` / `not` too;
- runs of newlines collapse to one; leading newlines are dropped.

So this is one statement:

```mimz
wire big: bits[16] =
  {a, b} +
  extend(c, 16)
```

The rule is "break **after** the operator", same as Go. Breaking before
the operator does not continue the line — by design, one canonical style.

## Error behavior

The lexer never stops at the first problem: it records a `Diag` and keeps
scanning, so a file with three bad characters reports all three. `lex`
returns `Err(diags)` if anything was recorded — the token stream is not
used in that case.
