# Min-Mozhi — Grammar Engine (இலக்கண இயந்திரம்)

> **Spec v0.2.6 — Phase 1.8 complete; the test-header flip landed in Phase 1.5.**
> (v0.2.6, 2026-06-16: the **test-header flip** (`M(args) kaaga "…" sodhanai { }`,
> section 3 row 6) is now implemented — Phase 1.5 B6 gave `test` blocks an
> execution oracle (`mimz test`), so B7 added the flip with running tests as the
> proof (`src/parser/items/test.rs::test_decl_thamizh`,
> `src/pretty.rs`). This completes all five clause flips of the word-order engine.)
> (v0.2.5, 2026-06-16: DRAFT → stable. Phase 1.8 closed — the same-AST/round-trip
> suites are green, the native-speaker panel ratified the word order + error
> rendering (C3, 2026-06-15), and 33/36 checker codes are localized. The
> `test`-header word-order flip was deferred to Phase 1.5 per the section 6
> scope fence — now closed, see above.)
> (v0.2.4, 2026-06-15: section 3 examples synced to the finalized v1 keyword set —
> `enil`/`thernthedu`/`illaiyenil` and the `thudippu`/`veliyeedu`/`pathivedu`
> declaration words. `keywords.toml` is the source of truth.)
> Goal: let Tamil and Tanglish code read with **natural Tamil word order**
> (SOV, postpositional), not just Tamil words in English order.
>
> **Implemented:** the `syntax thamizh` directive and **all five** clause flips
> in section 3 — the **clocked block** (`rise(clk) on { }`), the **conditional**
> (`<cond> enil { }`), the **if-expression** (`c enil { a } illaiyenil { b }`),
> **match** (`<expr> thernthedu { }`), and the **test header**
> (`M(args) kaaga "…" sodhanai { }`, Phase 1.5 B7) — and **`mimz translate --order
code|thamizh`**, which
> converts a file between the two orders via an AST pretty-printer (`src/pretty.rs`).
> All flips parse to the same AST as code-order and emit byte-identical Verilog
> (`tests/grammar.rs`, `tests/fixtures/grammar/`). The **error-language
> plumbing** of section 5 also landed (2026-06-14): error-language **selection**
> (file-flavor majority + `--lang` override) and the case-suffix **inflection
> mechanism** (`src/morph.rs`), wired into `check`/`compile`/`eval` as an
> **additive, English-fallback** layer. **The human-authored Tamil/Tanglish
> error catalog also landed (2026-06-15):** `messages.toml` localizes **33 of
> the 36 checker E-codes** (the panel-authored Tamil + Tanglish forms; decision
> C3 ratified, sandhi rule finalized in `case_suffixes.toml`). E0403/E0404/E0405
> are deferred — each emits many heterogeneous message shapes that one template
> cannot fit faithfully, so they keep their English text (Tamil preserved as
> comments). The **test** flip landed in Phase 1.5 (B7) — `mimz test` runs the
> blocks, so a passing thamizh-order test (re-parsing to the same `TestDecl`) is
> the oracle in place of a same-Verilog comparison.

---

## 1. The Problem It Solves

Layer 1 (the keyword skins in `03-keywords-trilingual.md`) translates _words_
but keeps English-derived _word order_:

```mimz
pothu yetram(clk) { ... }      // "when rise(clk)" — words Tamil, order English
enil timer == 0 { ... }        // "if timer == 0" — same issue
```

Real Tamil is **SOV and postpositional** — the condition comes first, the
conditional word comes after:

> \*timer == 0 **எனில்\*** — "timer == 0, if-so"
> \*clk **ஏறும்போது\*** — "when clk rises"

The Grammar Engine adds a second **syntax profile** to the parser so Tamil and
Tanglish users can write in that natural order — while producing the **exact
same AST** as English code.

## 2. Architecture — Two Layers, One AST

```
source text
   │
   ▼
LEXER ── one trilingual keyword table (Layer 1, ships in Phase 1)
   │
   ▼
PARSER ── syntax profile: code-order | thamizh-order (Layer 2, Phase 1.8)
   │
   ▼
ONE SHARED AST ──► type check ──► Verilog / IR / simulator
```

Everything after the parser is completely unaware of which profile the source
used. The grammar engine is **parser-level only** — no semantic differences,
ever. Two profiles:

| Profile                | Word order                                                 | Who uses it                                   |
| ---------------------- | ---------------------------------------------------------- | --------------------------------------------- |
| `code-order` (default) | English-derived; works with all three keyword sets         | Layer 1 behavior, unchanged                   |
| `thamizh-order`        | SOV/postpositional productions for clause-level constructs | Tamil/Tanglish users who want natural reading |

### Profile selection

The word-order profile is declared **explicitly at the top of the file**:

```mimz
syntax thamizh
```

- The directive word itself is trilingual like any keyword:
  `syntax thamizh` ≡ `ilakkanam thamizh` ≡ `இலக்கணம் தமிழ்`.
- No directive → `code-order`. No auto-detection — word order changes how the
  parser works, so it must be unambiguous before parsing starts.
- **Keyword flavors remain freely mixable** in both profiles (Layer 1 rule).
  Only the _order_ is fixed per file.
- `mimz translate` gains `--order code|thamizh`, which converts between the two
  orders by parsing to the AST and pretty-printing with the target profile
  (`src/pretty.rs`). **Decision (2026-06-14):** because the AST holds no comments
  or original layout, `--order` output is **canonically formatted and drops
  comments** — meaning-preserving (same Verilog, same AST), not byte-preserving.
  Trivia-preservation stays with the keyword-only `--to` path (the token
  reskin). Lossless round-tripping including comments would require carrying
  trivia in the AST — a separate, later change.
- `mimz translate` also gains **`--romanize-names`** (2026-06-15), an opt-in flag
  on the keyword-only `--to` path that rewrites non-ASCII (Tamil) IDENTIFIERS to
  readable Latin, reusing the Verilog emitter's `romanize` (`கணக்கி` →
  `kannakki`). It is **one-way** (transliteration cannot be inverted by rule), so
  it is OFF by default; the lossless round-trip holds only with it off. It exists
  so a fully-Tamil program (`examples/tamil-pure/`) can be converted to readable
  Tanglish/English. **Reversibility:** with `-o <out>`, romanizing also writes a
  per-file sidecar `<out>.names.json` (`romanized → original Tamil`); a reverse
  run with `--names-map <file>` restores the exact Tamil names. For idiomatic,
  whitespace-separated code (the whole `examples/tamil-pure/` corpus) the full
  `Tamil → Latin → Tamil` round-trip is byte-identical. One narrow exception: a
  numeric literal directly abutting a Tamil keyword/identifier (e.g. `42தொகுதி`)
  relies on the Latin/Tamil script change as its only separator; reskinning to
  ASCII would glue it (`42module`), so the reskin inserts a single separating
  space — the result stays lexable and token-equivalent, but the restored source
  gains that space. Full behavior: `docs/code/13-tooling.md`.

## 3. What Flips in `thamizh-order`

Only **clause-level** constructs flip — the places where English order fights
Tamil grammar. Declarations, expressions, operators, and types stay identical
(they are not sentences; flipping them buys nothing and costs familiarity).

| Construct     | code-order                      | thamizh-order                                          | Reading                  |
| ------------- | ------------------------------- | ------------------------------------------------------ | ------------------------ |
| conditional   | `enil <cond> { }`               | `<cond> enil { }`                                      | _timer == 0 எனில் …_     |
| alternative   | `illaiyenil { }`                | `illaiyenil { }` (unchanged — already leads naturally) | _இல்லையெனில் …_          |
| clocked block | `pothu yetram(clk) { }`         | `yetram(clk) pothu { }`                                | _clk ஏற்றம் போது …_      |
| match         | `thernthedu <expr> { }`         | `<expr> thernthedu { }`                                | _state-ஐத் தேர்ந்தெடு …_ |
| if-expression | `enil c { a } illaiyenil { b }` | `c enil { a } illaiyenil { b }`                        |                          |
| test          | `sodhanai "…" kaaga M() { }`    | `M() kaaga "…" sodhanai { }`                           | _M-க்காக "…" சோதனை_      |

Unchanged in both profiles: `module/thoguthi`, port/wire/reg declarations,
`let` instantiation, `enum`, assignments (`=`, `<-`), all operators, all types.

### The counter, thamizh-order Tanglish

```mimz
syntax thamizh

thoguthi Counter(WIDTH: int = 8) {
  thudippu clk
  meettamai rst
  veliyeedu count: bits[WIDTH]

  pathivedu value: bits[WIDTH] = 0

  yetram(clk) pothu {
    value <- value +% 1
  }

  count = value
}
```

### The traffic light, thamizh-order Tamil script

```mimz
syntax thamizh

தொகுதி TrafficLight {
  துடிப்பு clk
  மீட்டமை rst
  வெளியீடு red: bit

  வகை State { Red, Green, Yellow }
  பதிவேடு state: State = State.Red
  பதிவேடு timer: bits[8] = 0

  ஏற்றம்(clk) போது {
    timer == 0 எனில் {
      state <- state தேர்ந்தெடு {
        State.Red    => State.Green
        State.Green  => State.Yellow
        State.Yellow => State.Red
      }
    } இல்லையெனில் {
      timer <- timer -% 1
    }
  }

  red = state == State.Red
}
```

Read that `on`-block aloud: _"ஏற்றம் clk போது — timer பூஜ்ஜியம் எனில் …"_ —
it parses as a Tamil sentence. That is the whole point of the engine.

## 4. Parsing Feasibility (why this is cheap, not research)

Postfix conditionals look scary but are routine for a recursive-descent parser:

- In statement position, parse an **expression first**, then look at the next
  token: `enil` → it was a condition; `thernthedu` → it is a match scrutinee;
  `<-` → it was a register assignment target; otherwise → error.
- One token of lookahead after an expression resolves every flipped
  production. No backtracking, no GLR, no ambiguity — because we flipped only
  a small closed set of clause heads.
- The pretty-printer (used by `mimz translate` and `mimz fmt`) is the same
  AST walker with per-profile output templates.

Estimated effort: **1–2 months** on top of the working Phase 1 parser. It can
proceed in parallel with the simulator (Phase 1.5) since both sit on the same
AST.

## 5. Grammar-Aware Error Messages (part of the engine)

Translating error _templates_ word-by-word produces broken Tamil. The engine
includes a small **morphology helper** — table-driven Tamil case-suffix rules
(வேற்றுமை உருபுகள்: -ஐ, -க்கு, -இல், -ஆல்) applied to signal names when
composing sentences:

> English: `'sum' is 8 bits but 'a + b' produces 9 bits — use '+%' for wrapping math, or widen 'sum'.`
> Tamil: `'sum' 8 பிட்கள்தான், ஆனால் 'a + b' 9 பிட்கள் தரும் — மடக்கு கணிதத்திற்கு '+%' பயன்படுத்தவும், அல்லது 'sum'-ஐ அகலமாக்கவும்.`

This is a suffix lookup table plus sandhi-joining rules for the ~10 message
shapes the compiler emits — **not** NLP, not machine translation. Error texts
are authored once per language by humans; the helper only inflects the
interpolated identifiers correctly.

### Status (2026-06-14): mechanism implemented, content panel-gated

The **engineering half** is in `src/morph.rs` and wired into `check`/`compile`/
`eval`:

- **Selection** — `majority_flavor` counts a file's keyword flavors;
  `effective_lang` lets `--lang en|tanglish|tamil` override it (the spec/03 rule).
- **Inflection** — the four case suffixes are DATA in `case_suffixes.toml` (the
  keywords.toml doctrine); `inflect(name, case, flavor)` attaches them.
- **Additive, English-fallback** — diagnostics render in the chosen flavor only
  for E-codes the localized catalog covers; every other message keeps its
  English text verbatim, byte-for-byte.

> **Decision (R3, 2026-06-14): build the mechanism now, gate the content on C3.**
> The full Tamil + Tanglish catalog and the real **sandhi-joining rules** require
> the native-speaker panel (decision C3) — machine-guessed Tamil is exactly the
> "broken Tamil" this section warns against.
>
> **Resolved (2026-06-15, C3 ratified):** the panel authored the catalog. It now
> ships in `messages.toml` covering **33 of 36 checker E-codes** in Tamil and
> Tanglish; the sandhi rule in `case_suffixes.toml` is finalized (no longer
> PROVISIONAL). **E0403/E0404/E0405 are deferred** — each emits many
> heterogeneous message shapes that a single template cannot render faithfully,
> so they keep their English text with the Tamil preserved as comments. JSON
> diagnostic output stays English (the machine contract in `06-diagnostics.md`
> is unchanged). The ~36 inline English messages were not refactored into the
> catalog — they remain the byte-for-byte fallback for any uncovered code.

## 6. Scope Fence (v1 of the engine)

- ✅ Clause-level word order (table in section 3)
- ✅ Lossless `translate --order` both directions
- ✅ Morphology-correct error interpolation
- ❌ Free word order / full Tamil grammar parsing — Min-Mozhi stays a formal
  language with two fixed orders, not a natural-language parser
- ❌ Flipped declarations (`count: bits[8] veliyeedu`) — declarations are not
  sentences; revisit only if users ask
- ❌ Verb conjugation in keywords (ஏறும்போது as one inflected word) — keywords
  stay as fixed dictionary forms so the lexer stays a table lookup

---

_Status: stable (Phase 1.8 closed 2026-06-16). The section 3 word-order table has been validated by the
same native-speaker review that finalized the v1 keyword table (Phase 0 closed);
the keyword spellings here track `keywords.toml`._
