# Min-Mozhi — Grammar Engine (இலக்கண இயந்திரம்)

> **Spec v0.1 DRAFT — design sketch for Phase 1.8.**
> Goal: let Tamil and Tanglish code read with **natural Tamil word order**
> (SOV, postpositional), not just Tamil words in English order.

---

## 1. The Problem It Solves

Layer 1 (the keyword skins in `03-keywords-trilingual.md`) translates _words_
but keeps English-derived _word order_:

```mimz
pothu yetram(clk) { ... }      // "when rise(clk)" — words Tamil, order English
endral timer == 0 { ... }      // "if timer == 0" — same issue
```

Real Tamil is **SOV and postpositional** — the condition comes first, the
conditional word comes after:

> \*timer == 0 **என்றால்\*** — "timer == 0, if-so"
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
- `mimz translate` gains `--order code|thamizh` and converts losslessly in
  both directions (parse to AST → pretty-print with the target profile).

## 3. What Flips in `thamizh-order`

Only **clause-level** constructs flip — the places where English order fights
Tamil grammar. Declarations, expressions, operators, and types stay identical
(they are not sentences; flipping them buys nothing and costs familiarity).

| Construct     | code-order                      | thamizh-order                                        | Reading                |
| ------------- | ------------------------------- | ---------------------------------------------------- | ---------------------- |
| conditional   | `endral <cond> { }`             | `<cond> endral { }`                                  | _timer == 0 என்றால் …_ |
| alternative   | `illaiyel { }`                  | `illaiyel { }` (unchanged — already leads naturally) | _இல்லையேல் …_          |
| clocked block | `pothu yetram(clk) { }`         | `yetram(clk) pothu { }`                              | _clk ஏற்றம் போது …_    |
| match         | `poruthu <expr> { }`            | `<expr> poruthu { }`                                 | _state-ஐப் பொருத்து …_ |
| if-expression | `endral c { a } illaiyel { b }` | `c endral { a } illaiyel { b }`                      |                        |
| test          | `sodhanai "…" kaaga M() { }`    | `M() kaaga "…" sodhanai { }`                         | _M-க்காக "…" சோதனை_    |

Unchanged in both profiles: `module/thoguthi`, port/wire/reg declarations,
`let` instantiation, `enum`, assignments (`=`, `<-`), all operators, all types.

### The counter, thamizh-order Tanglish

```mimz
syntax thamizh

thoguthi Counter(WIDTH: int = 8) {
  kadigaram clk
  meetamai rst
  veli count: bits[WIDTH]

  nilai value: bits[WIDTH] = 0

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
  கடிகாரம் clk
  மீட்டமை rst
  வெளி red: bit

  வகை State { Red, Green, Yellow }
  நிலை state: State = State.Red
  நிலை timer: bits[8] = 0

  ஏற்றம்(clk) போது {
    timer == 0 என்றால் {
      state <- state பொருத்து {
        State.Red    => State.Green
        State.Green  => State.Yellow
        State.Yellow => State.Red
      }
    } இல்லையேல் {
      timer <- timer -% 1
    }
  }

  red = state == State.Red
}
```

Read that `on`-block aloud: _"ஏற்றம் clk போது — timer பூஜ்ஜியம் என்றால் …"_ —
it parses as a Tamil sentence. That is the whole point of the engine.

## 4. Parsing Feasibility (why this is cheap, not research)

Postfix conditionals look scary but are routine for a recursive-descent parser:

- In statement position, parse an **expression first**, then look at the next
  token: `endral` → it was a condition; `poruthu` → it is a match scrutinee;
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

## 6. Scope Fence (v1 of the engine)

- ✅ Clause-level word order (table in §3)
- ✅ Lossless `translate --order` both directions
- ✅ Morphology-correct error interpolation
- ❌ Free word order / full Tamil grammar parsing — Min-Mozhi stays a formal
  language with two fixed orders, not a natural-language parser
- ❌ Flipped declarations (`count: bits[8] veli`) — declarations are not
  sentences; revisit only if users ask
- ❌ Verb conjugation in keywords (ஏறும்போது as one inflected word) — keywords
  stay as fixed dictionary forms so the lexer stays a table lookup

---

_Status: design draft. Build after Phase 1 parser exists; validate the §3
table with the same native-speaker review panel as the keyword table._
