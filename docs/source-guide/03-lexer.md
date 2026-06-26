# 3 — The Lexer: Tokenizing Your Code (4 Files)

The lexer is where your source text gets chopped into **tokens** — the smallest meaningful chunks like `module`, `42`, `+`, `<-`, `{`, and so on.

---

## `lexer/token.rs` — What a Token Looks Like

**`Kw` enum** — This lists every keyword: `Module`, `In`, `Out`, `Wire`, `Reg`, `Mem`, `Clock`, `Reset`, `Async`, `On`, `Rise`, `Fall`, `If`, `Else`, `Match`, `Enum`, `Let`, `Const`, `Repeat`, `Import`, `True`, `False`, `Test`, `For`, `Tick`, `Expect`, `And`, `Or`, `Not`, `Syntax`, `Thamizh`.

The important thing: `தொகுதி` and `thoguthi` and `module` all become `Kw::Module`. The flavor is recorded separately.

**`Flavor` enum** — `English | Tanglish | Tamil`. Only meaningful for keyword tokens — it records which language spelled the keyword. This is used by `mimz fmt`, `translate`, and error-language detection.

**`TokKind` enum** — Every possible kind of token: identifiers, numbers, all the operators (`+`, `+%`, `<-`, `==`, etc.), punctuation, newlines, and EOF.

Numbers carry their `raw` spelling (like `"0xFF"` not just `255`) so the Verilog emitter can preserve the author's chosen base.

**`Token` struct** — A token has a `kind`, a `span` (where in the source), and an optional `flavor` (only set for keywords).

---

## `lexer/mod.rs` — The Scanner Itself

**`lex(src)`** is the main entry. It:

1. Builds a list of all character positions upfront (for O(1) two-character lookahead)
2. Runs the main loop: for each character, decide what token it starts
3. Post-processes newlines (Go-style statement termination)
4. Returns the token stream or all lex errors

**The main loop (`run()`)** dispatches on the first character:

- Space/tab/carriage return → skip
- `\n` → emit a `Newline` token
- `//` → skip to end of line
- `/*` → read a block comment (may contain newlines; if so, emits a synthetic Newline)
- `"` → read a string literal (for test names)
- Digit → read a number (decimal, `0b` binary, `0x` hex)
- Unicode letter or `_` → read an identifier (Tamil script works because of XID rules)
- Everything else → read punctuation or operator

**Block comments (`block_comment`)** handle `/* ... */` properly, including nested-line tracking. An unclosed `/*` gets E1001.

**Strings (`string`)** are for test names only. An unclosed string gets E1002.

**Numbers (`number`)** are interesting:

- Supports decimal (`42`), binary (`0b1010`), and hex (`0xFF`)
- Underscores are allowed as separators: `0b_1010_1111`
- In binary, `?` is a don't-care digit for match patterns: `0b1??`
- Tamil digits (`௦`..`௯`) are explicitly rejected with a teaching message (E1003) pointing to ASCII digits
- Overflow protection prevents >128-bit don't-care patterns

**Identifiers (`ident`)** use Unicode XID rules, so Tamil-script names work naturally. The text is looked up in the keyword table:

- Match → keyword token with flavor recorded
- Reserved word → E1005 error ("this name is reserved for a future feature")
- Otherwise → identifier token

**Punctuation (`punct`)** uses longest-match-first:

- `+%` beats `+`
- `<-` beats `<`
- `==` beats `=`
- `/` and `%` get explicit teaching errors (E1006, E1007) — division and modulo don't exist in the language because they synthesize to slow, large hardware

### The Newline Policy (`postprocess_newlines`)

Min-Mozhi uses Go-style newlines as statement terminators. But a line that ends with an operator or open bracket clearly continues:

```
sum = a +    # ← this newline is eaten because the line ends with +
      b
```

So `postprocess_newlines` drops newlines when the previous token can't end a statement (operators, comma, open brackets, `=`, `<-`, `=>`, `:`).

---

## `lexer/keywords.rs` — The Keyword Table

This loads `lang/keywords.toml` (embedded at build time) and builds two lookup tables:

1. **spelling → (Kw, Flavor)** — for the lexer to recognize keywords
2. **Kw → [en, tanglish, tamil]** — for the translator to reskin keywords

**`TABLE`** is the global singleton, loaded once on first use via `LazyLock`. It panics at startup if:

- The TOML is malformed
- A required keyword key is missing (enforced by `REQUIRED_KEYS`)
- A spelling appears in two different keywords

**`REQUIRED_KEYS`** is a list of 31 keys that MUST be in the TOML. Without this guard, accidentally deleting `[keywords.module]` would silently turn `module` into a plain identifier.

**`kw_for_key(key)`** maps TOML key strings to `Kw` enum variants. Adding a new keyword means adding it here AND in the TOML.

Reserved words (like `fn`, `function`, `struct`, `sync`, `inout`) are not keywords yet but can't be used as identifiers. They're set aside for future features.
