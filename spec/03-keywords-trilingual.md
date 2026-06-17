# Min-Mozhi — Trilingual Keyword Design

> **Spec v0.2.8.**
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
- **Exactly one canonical spelling per keyword.** No spelling-variant
  aliases — variants breed dialects. Near-miss spellings get a
  _did-you-mean_ compiler suggestion instead (`etram` → "did you mean
  `yetram`?"). This is distinct from deliberate **synonym aliases** (see
  "Aliases" below), which are separate words, not alternate spellings.

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
  mixed flavors (mixing stays legal — it is the learning path). Implemented
  2026-06-14: in-place, lossless (the `translate` token reskin); default target
  is the file's predominant flavor, `--to` overrides; `--strict` warns + exits
  non-zero on a mixed file (still normalizing it).
- Error messages are emitted in the flavor the file predominantly uses
  (`--lang` flag overrides). Implemented 2026-06-14 (`src/morph.rs`,
  `check`/`compile`/`eval`); the localized catalog itself is panel-gated — see
  `04-grammar-engine.md` §5.

### Identifiers

Unicode identifiers are legal in every flavor — `reg எண்ணி: bits[8] = 0`
is valid even in an otherwise-English file.

---

## 2. Keyword Table — v1 (FINALIZED)

> ✅ **Status: keyword set v1, FINALIZED** by native-speaker review (2026-06-15,
> Phase 0 closed). English column frozen. Spellings may change in a future v2;
> `keywords.toml` carries `version = 1`. This table mirrors `keywords.toml`
> exactly (`tests/grammar_sync.rs` enforces it).

| Token         | English   | Tanglish     | Tamil         | Notes                                                                                                      |
| ------------- | --------- | ------------ | ------------- | ---------------------------------------------------------------------------------------------------------- |
| KW_MODULE     | `module`  | `thoguthi`   | `தொகுதி`      | standard CS-textbook word for "module"                                                                     |
| KW_IN         | `in`      | `ulleedu`    | `உள்ளீடு`     | "input" — precise port direction (v1: was `உள்`)                                                           |
| KW_OUT        | `out`     | `veliyeedu`  | `வெளியீடு`    | "output" — exact counterpart to உள்ளீடு (v1: was `வெளி`)                                                   |
| KW_WIRE       | `wire`    | `kambi`      | `கம்பி`       | literal "wire"                                                                                             |
| KW_REG        | `reg`     | `pathivedu`  | `பதிவேடு`     | "register" — exact CS term (v1: was `நிலை`/"state")                                                        |
| KW_CLOCK      | `clock`   | `thudippu`   | `துடிப்பு`    | "pulse/beat" — a clock is a pulse (v1: was `கடிகாரம்`)                                                     |
| KW_RESET      | `reset`   | `meettamai`  | `மீட்டமை`     | "restore/reset" (standard UI/CS term)                                                                      |
| KW_ON         | `on`      | `pothu`      | `போது`        | "when/at the time of" (trails in thamizh order)                                                            |
| KW_RISE       | `rise`    | `yetram`     | `ஏற்றம்`      | "ascent/rise" — `on rise(clk)` (posedge)                                                                   |
| KW_FALL       | `fall`    | `irakkam`    | `இறக்கம்`     | "descent/fall" — `on fall(clk)` (negedge); Tanglish/Tamil PROVISIONAL, pending native review (R9/R11)      |
| KW_IF         | `if`      | `enil`       | `எனில்`       | conditional particle — natural trailing "if" in thamizh order (v1: was `என்றால்`)                          |
| KW_ELSE       | `else`    | `illaiyenil` | `இல்லையெனில்` | "otherwise" — mirrors எனில் (v1: was `இல்லையேல்`)                                                          |
| KW_MATCH      | `match`   | `thernthedu` | `தேர்ந்தெடு`  | "select/choose" (verb) — reads as a clause in thamizh order (v1: was `பொருத்து`)                           |
| KW_ENUM       | `enum`    | `vagai`      | `வகை`         | "kind/category"                                                                                            |
| KW_LET        | `let`     | `amai`       | `அமை`         | "set up" — instantiates a module (v1: was `வை`). EN `let` binds an instance, not a variable (spec/02 §1.5) |
| KW_CONST      | `const`   | `maarili`    | `மாறிலி`      | "constant" — exact math/science term (v1: was `மாறா`)                                                      |
| KW_REPEAT     | `repeat`  | `meendum`    | `மீண்டும்`    | "again" — compile-time generation (the unroll loop)                                                        |
| KW_IMPORT     | `import`  | `serkka`     | `சேர்க்க`     | en alias: `include`; "to add/include"                                                                      |
| KW_TRUE       | `true`    | `mei`        | `மெய்`        | boolean true — standard CS/math term (v1: was `உண்மை`)                                                     |
| KW_FALSE      | `false`   | `poi`        | `பொய்`        | boolean false                                                                                              |
| KW_TEST       | `test`    | `sodhanai`   | `சோதனை`       | "test/experiment"                                                                                          |
| KW_FOR (test) | `for`     | `kaaga`      | `க்காக`       | "for the sake of" — **binds** a module in a test (NOT a loop; `repeat` is the loop)                        |
| KW_TICK       | `tick`    | `kanam`      | `கணம்`        | "moment/instant" — a discrete time step (v1: was `தட்டு`)                                                  |
| KW_EXPECT     | `expect`  | `uruthisei`  | `உறுதிசெய்`   | "ensure/assert" — hardware assertion (v1: was `எதிர்பார்`)                                                 |
| KW_AND        | `and`     | `mattrum`    | `மற்றும்`     | alias of universal `&&` (G1-x)                                                                             |
| KW_OR         | `or`      | `alladhu`    | `அல்லது`      | alias of universal `\|\|`                                                                                  |
| KW_NOT        | `not`     | `alla`       | `அல்ல`        | alias of universal `!` (v1: was `இல்லா`)                                                                   |
| KW_SYNTAX     | `syntax`  | `ilakkanam`  | `இலக்கணம்`    | grammar-engine directive (Layer 2): `syntax thamizh` (section 04)                                          |
| KW_THAMIZH    | `thamizh` | `thamizh`    | `தமிழ்`       | the `thamizh-order` profile name; en==tanglish, Tamil script `தமிழ்`                                       |

### Reserved words

Set aside for future features — using one as an identifier is a compile
error (E1005) explaining why. They live in the `reserved` list in
`keywords.toml`, above the keyword tables:

| Reserved           | Held for                                                                 |
| ------------------ | ------------------------------------------------------------------------ |
| `mem`              | memories/arrays (Phase 2)                                                |
| `sync`             | clock-domain crossing (Phase 2)                                          |
| `inout`            | top-level bidirectional pads (Phase 2)                                   |
| `struct`           | bundles/interfaces (post-Phase 2)                                        |
| `secret`           | explicit information-flow types (v0.3 G5)                                |
| `declassify`       | the only `secret`→public escape (v0.3 G5)                                |
| `default`          | sticky-fault / default values (v0.3)                                     |
| `pipeline`         | pipeline-stage construct (v0.3 backlog)                                  |
| `interface`        | named port bundles (v0.3 backlog)                                        |
| `chan`             | handshake channels (v0.3 backlog)                                        |
| `prove`            | formal/temporal assertions (v0.3 backlog)                                |
| `async`            | async/await (v0.3 backlog)                                               |
| `await`            | handshake sequencing (v0.3 backlog)                                      |
| `fixed`            | fixed-point arithmetic type (section 8 triage)                           |
| `requires`         | caller-side precondition contract (section 8)                            |
| `ensures`          | module postcondition contract (section 8)                                |
| `fn` / `function`  | a future combinational-function construct (Phase 2; unblocks pipe `\|>`) |
| `suzhal` / `சுழல்` | a future controlled `for`-loop (v1 reserved; `repeat` stays the unroll)  |

Reserved words are untranslated until their feature lands (no Tamil
words before the native-speaker review — same rule as aliases).

### Aliases

A column may carry deliberate **synonym aliases** in addition to its
canonical word — listed per column in `keywords.toml` (e.g. `en_aliases`),
never invented by the compiler. An alias lexes to the exact same token as
the canonical word, so nothing after the lexer can tell them apart; tooling
(`mimz translate`, `mimz fmt`) normalizes aliases to the canonical
spelling. Aliases are keywords: their words stop being legal identifiers.

Current aliases (v0.2.1):

| Keyword   | Column | Alias     | Why                                              |
| --------- | ------ | --------- | ------------------------------------------------ |
| KW_IMPORT | en     | `include` | both verbs are common; either should "just work" |

The Tanglish/Tamil columns carry no aliases until the native-speaker
review (section "Review & governance") — no new Tamil words before that.

**Word-order caveat:** this layer keeps one fixed (English-derived) order, so
`on rise(clk)` becomes `pothu yetram(clk)` — understandable, but not idiomatic
Tamil syntax (Tamil is SOV: "clk ஏறும்போது"). Natural Tamil word order is
**Layer 2 — the Grammar Engine** (`04-grammar-engine.md`, Phase 1.8), which
adds a `thamizh-order` parser profile (`yetram(clk) pothu { }`,
`<cond> enil { }`) over the same AST. Layer 1 ships first; Layer 2 follows
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
  thudippu clk
  meettamai rst
  veliyeedu count: bits[WIDTH]

  pathivedu value: bits[WIDTH] = 0

  pothu yetram(clk) {
    value <- value +% 1
  }

  count = value
}
```

**Tamil:**

```mimz
தொகுதி Counter(WIDTH: int = 8) {
  துடிப்பு clk
  மீட்டமை rst
  வெளியீடு count: bits[WIDTH]

  பதிவேடு value: bits[WIDTH] = 0

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
  veliyeedu count: bits[WIDTH]     // out → veliyeedu, rest still English

  pathivedu value: bits[WIDTH] = 0 // reg → pathivedu

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

- **v0.2.8 (2026-06-17):** Promoted `fall` from **reserved** to an active keyword
  KW_FALL for falling-edge `on fall(clk)` blocks (A3, Verilog `negedge`). Its
  Tanglish/Tamil spellings — `irakkam` / `இறக்கம்` ("descent", the antonym of
  `yetram`/`ஏற்றம்` = "ascent") — are **PROVISIONAL** dev/testing placeholders
  pending native-speaker review (R9/R11), founder-authorized so the four-flavor
  tooling works before the v0.1.0 freeze. Removed `fall` from the reserved table;
  added the grammar keyword rule + lexer test (the R11 pipeline, reversed).
- **v0.2.7 (2026-06-16):** Reserved `async` to pair with the already-reserved
  `await` (async/await, v0.3 backlog). Reserved pre-v0.1.0 freeze so no program
  can claim it (E1005); English-only until the feature lands and native review
  supplies Tamil (R11). This keeps open the Phase 1.5 sub-decision of whether the
  `await clk.cycles(n)` test-timing form needs an `async` test-block marker.
- **v0.2.6 (2026-06-16):** Reserved `fn` and `function` for a future
  combinational-function construct (Phase 2 RTL parity; also unblocks the pipe
  `|>` operator). Both spellings reserved pre-v0.1.0 freeze so no program can
  claim either (E1005); English-only until the feature lands and native review
  supplies Tamil (R11). Closes the last keyword-namespace gap before the freeze.
- **v0.2.5 (2026-06-13):** Promoted `syntax` / `ilakkanam` / `இலக்கணம்` from
  reserved to active keyword KW_SYNTAX, and added KW_THAMIZH
  (`thamizh` / `thamizh` / `தமிழ்`) — the `syntax thamizh` grammar-engine
  directive (Layer 2, section 04) lands its first slice in Phase 1.8 (the
  `rise(clk) on { }` clocked-block flip). The profile name `thamizh` is
  identical in the English and Tanglish columns (no distinct English word); the
  loader now permits one spelling to repeat across columns of the SAME keyword.
- **v0.2.4 (2026-06-13):** Reserved three section 8 deep-triage words — `fixed`
  (fixed-point arithmetic), `requires` / `ensures` (boundary contracts) —
  so v0.1 programs cannot claim them (E1005). Namespace protection ahead of
  the v0.1.0 freeze; English-only until each feature lands (rationale in
  `docs/Ideas/language_plan.md` section 9).
- **v0.2.3 (2026-06-12):** Reserved the eight v0.3 backlog words
  (`secret`, `declassify`, `default`, `pipeline`, `interface`, `chan`,
  `prove`, `await`) so v0.1 programs cannot claim them as identifiers
  (E1005). English-only — untranslated until each feature lands, per the
  reserved-words rule above.
- **v0.2.2 (2026-06-12):** Reserved-words table added (the eight words
  the `reserved` list in `keywords.toml` holds, each with the feature it
  waits for) — completeness audit; no word changes. The loader now also
  panics at startup if a required `[keywords.*]` entry is MISSING (the
  unknown-key panic only guarded the other direction).
- **v0.2.1 (2026-06-11):** Synonym-alias mechanism (per-column
  `*_aliases` lists in `keywords.toml`); first alias: en `include` for
  KW_IMPORT. Clarified that the one-canonical-spelling rule bans spelling
  variants, not deliberate synonyms.
- **v0.2 (2026-06-10):** CLI/extension → `mimz`/`.mimz`. Romanization policy
  (one canonical phonetic spelling, did-you-mean), word-selection criteria
  incl. TN SCERT alignment, review/governance section (panel majority).
  Removed KW_FALL (reserved). Added KW_REPEAT, KW_TICK, KW_EXPECT.
  `and/or/not` reclassified as translated aliases of universal `&&`/`||`/`!`
  (resolves the v0.1 universality contradiction).
- **v0.1 (2026-06-10):** Initial draft.
