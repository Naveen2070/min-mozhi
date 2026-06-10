# Min-Mozhi — Trilingual Keyword Design

> **Spec v0.2.**
> One grammar, three keyword skins: English, Tanglish (romanized Tamil), Tamil script.
> Stage 1 ships English + Tanglish; Tamil script comes for free from the same table.

---

## 1. The Mechanism

The lexer holds **one keyword table with three columns**. Every keyword token
(e.g. `KW_MODULE`) has up to three spellings. The lexer recognizes the union
of all three columns at all times:

- **No file-level mode, no pragma, no special extension.** Any flavor — or a
  mix — compiles. Mixing is the _migration path_: a learner starts in English
  and swaps keywords one at a time without ever breaking the build.
- The three columns are kept **disjoint** (no spelling appears in two columns),
  so there is never ambiguity.
- Tamil-script keywords never collide with identifiers in practice; if a user
  identifier matches a keyword spelling, the compiler error names all three
  spellings so the rule is learnable.
- **Universal vocabulary** — identical in all flavors, never translated:
  - all operators and punctuation (`+%`, `<-`, `=>`, `&&`, `{}`, …)
  - type names: `bit`, `bits`, `signed`, `int`, `bool`
  - built-ins: `extend`, `trunc`, `signed()`, `unsigned()`
  - numbers and literals (ASCII digits only)
  - **exception (G1-x):** the logical keyword aliases `and/or/not` _are_
    translated — they alias the universal symbols `&&`/`||`/`!`
    This keeps the translation surface to ~25 words and means any Min-Mozhi
    programmer can read any flavor's _structure_ at a glance.

### Romanization policy (Tanglish column)

There is no standard Tamil romanization, so Min-Mozhi fixes one:

- **Simple phonetic, no diacritics** — spell as a TN student would type in a
  chat message.
- **Exactly one canonical spelling per keyword.** No variant aliases —
  aliases breed dialects. Near-miss spellings get a _did-you-mean_ compiler
  suggestion instead (`etram` → "did you mean `yetram`?").

### Word-selection criteria (in order)

1. A TN polytechnic student recognizes it in a technical context.
2. **Aligns with TN SCERT school-textbook technical vocabulary** where a
   textbook term exists.
3. Short enough to type comfortably.

### Review & governance

- Reviewers: native-speaker tech/coder friends of the founder (the initial
  panel), growing to a community panel post-release.
- Final say: panel majority wins — even over the founder's preference — once
  a panel exists; until then the founder + available native speakers decide.
- The table is a **data file** (`keywords.toml` in the compiler), so word
  changes are data changes, reviewable without touching code.

### Tooling

- `mimz translate file.mimz --to english|tanglish|tamil` — lossless,
  token-level keyword rename. Comments and identifiers untouched.
- `mimz fmt` — can normalize a file to one flavor; `--strict` also warns on
  mixed flavors (mixing stays legal — it is the learning path).
- Error messages are emitted in the flavor the file predominantly uses
  (`--lang` flag overrides).

### Identifiers

Unicode identifiers are legal in every flavor — `reg எண்ணி: bits[8] = 0`
is valid even in an otherwise-English file.

---

## 2. Keyword Table — v0.2 DRAFT

> ⚠️ **Status: DRAFT — needs native-speaker review** (panel: tech/coder
> friends). English column is frozen for Phase 1. Weakest picks flagged in
> notes; check against TN SCERT vocabulary before freezing.

| Token         | English  | Tanglish    | Tamil       | Notes / alternatives                                                         |
| ------------- | -------- | ----------- | ----------- | ---------------------------------------------------------------------------- |
| KW_MODULE     | `module` | `thoguthi`  | `தொகுதி`    | standard CS-textbook word for "module"                                       |
| KW_IN         | `in`     | `ulle`      | `உள்`       | or `ulleedu` (உள்ளீடு, "input") — longer but more precise                    |
| KW_OUT        | `out`    | `veli`      | `வெளி`      | or `veliyeedu` (வெளியீடு, "output")                                          |
| KW_WIRE       | `wire`   | `kambi`     | `கம்பி`     | literal "wire" — strong pick                                                 |
| KW_REG        | `reg`    | `nilai`     | `நிலை`      | "state" — strong pick                                                        |
| KW_CLOCK      | `clock`  | `kadigaram` | `கடிகாரம்`  | literal "clock"; long — `gadi` is a casual option                            |
| KW_RESET      | `reset`  | `meetamai`  | `மீட்டமை`   | "restore/reset"                                                              |
| KW_ON         | `on`     | `pothu`     | `போது`      | "when/at the time of"                                                        |
| KW_RISE       | `rise`   | `yetram`    | `ஏற்றம்`    | "ascent/rise" (`fall` removed in v0.2 — reserved, untranslated until needed) |
| KW_IF         | `if`     | `endral`    | `என்றால்`   | classic conditional suffix                                                   |
| KW_ELSE       | `else`   | `illaiyel`  | `இல்லையேல்` | "otherwise"                                                                  |
| KW_MATCH      | `match`  | `poruthu`   | `பொருத்து`  | "fit/match"                                                                  |
| KW_ENUM       | `enum`   | `vagai`     | `வகை`       | "kind/category"                                                              |
| KW_LET        | `let`    | `vai`       | `வை`        | "place/put" — weakest pick, review                                           |
| KW_CONST      | `const`  | `maara`     | `மாறா`      | "unchanging"                                                                 |
| KW_REPEAT     | `repeat` | `meendum`   | `மீண்டும்`  | "again" — new in v0.2 (compile-time generation)                              |
| KW_IMPORT     | `import` | `serkka`    | `சேர்க்க`   | "to add/include"; `irakkumathi` is literal but trade-flavored                |
| KW_TRUE       | `true`   | `unmai`     | `உண்மை`     |                                                                              |
| KW_FALSE      | `false`  | `poi`       | `பொய்`      |                                                                              |
| KW_TEST       | `test`   | `sodhanai`  | `சோதனை`     | "test/experiment"                                                            |
| KW_FOR (test) | `for`    | `kaaga`     | `க்காக`     | "for the sake of"                                                            |
| KW_TICK       | `tick`   | `thattu`    | `தட்டு`     | "tap/knock" — new in v0.2 (test blocks only), review                         |
| KW_EXPECT     | `expect` | `ethirpaar` | `எதிர்பார்` | "expect" — new in v0.2 (test blocks only)                                    |
| KW_AND        | `and`    | `mattrum`   | `மற்றும்`   | alias of universal `&&` (G1-x)                                               |
| KW_OR         | `or`     | `alladhu`   | `அல்லது`    | alias of universal `\|\|`                                                    |
| KW_NOT        | `not`    | `illa`      | `இல்லா`     | alias of universal `!`; review vs KW_ELSE family for confusion               |

**Word-order caveat:** this layer keeps one fixed (English-derived) order, so
`on rise(clk)` becomes `pothu yetram(clk)` — understandable, but not idiomatic
Tamil syntax (Tamil is SOV: "clk ஏறும்போது"). Natural Tamil word order is
**Layer 2 — the Grammar Engine** (`04-grammar-engine.md`, Phase 1.8), which
adds a `thamizh-order` parser profile (`yetram(clk) pothu { }`,
`<cond> endral { }`) over the same AST. Layer 1 ships first; Layer 2 follows
once the Phase 1 parser exists.

---

## 3. The Same Counter, Three Ways

**English:**

```mimz
module Counter(WIDTH: int = 8) {
  clock clk
  reset rst
  out count: bits[WIDTH]

  reg value: bits[WIDTH] = 0

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

**Tanglish:**

```mimz
thoguthi Counter(WIDTH: int = 8) {
  kadigaram clk
  meetamai rst
  veli count: bits[WIDTH]

  nilai value: bits[WIDTH] = 0

  pothu yetram(clk) {
    value <- value +% 1
  }

  count = value
}
```

**Tamil:**

```mimz
தொகுதி Counter(WIDTH: int = 8) {
  கடிகாரம் clk
  மீட்டமை rst
  வெளி count: bits[WIDTH]

  நிலை value: bits[WIDTH] = 0

  போது ஏற்றம்(clk) {
    value <- value +% 1
  }

  count = value
}
```

**Mixed (legal — the migration path in action):**

```mimz
module Counter(WIDTH: int = 8) {
  clock clk
  reset rst
  veli count: bits[WIDTH]          // out → veli, rest still English

  nilai value: bits[WIDTH] = 0     // reg → nilai

  on rise(clk) {
    value <- value +% 1
  }

  count = value
}
```

---

## 4. Implementation Notes (for the Rust lexer)

- One `phf`/static map: `spelling → KeywordToken`, populated from all three
  columns of a single source-of-truth table (`keywords.toml` in the repo, so
  community review of word choices is a data change, not a code change).
- Tokenizer normalizes nothing — exact-match on the spelling, after standard
  Unicode NFC normalization of the source.
- Near-miss detection (edit distance ≤ 2 against the Tanglish column) powers
  _did-you-mean_ suggestions per the romanization policy.
- `mimz translate` = lex → re-emit tokens, swapping keyword spellings,
  preserving all trivia (comments, whitespace).
- Lexer records which flavor each keyword used → drives default error-message
  language, `mimz fmt` flavor detection, and the `--strict` mixed-flavor warning.

---

## Changelog

- **v0.2 (2026-06-10):** CLI/extension → `mimz`/`.mimz`. Romanization policy
  (one canonical phonetic spelling, did-you-mean), word-selection criteria
  incl. TN SCERT alignment, review/governance section (panel majority).
  Removed KW_FALL (reserved). Added KW_REPEAT, KW_TICK, KW_EXPECT.
  `and/or/not` reclassified as translated aliases of universal `&&`/`||`/`!`
  (resolves the v0.1 universality contradiction).
- **v0.1 (2026-06-10):** Initial draft.
