# Min-Mozhi — Syntax & Grammar

> **Spec v0.2.2.** English flavor shown; see `03-keywords-trilingual.md` for
> Tanglish/Tamil keyword equivalents. The grammar is identical across all
> three flavors. File extension: **`.mimz`** · CLI: **`mimz`**.

---

## 1. Syntax Tour

### 1.1 Hello, hardware — a combinational adder

```mimz
// adder.mimz
module Adder(WIDTH: int = 8) {
  in  a: bits[WIDTH]
  in  b: bits[WIDTH]
  out sum: bits[WIDTH + 1]

  sum = a + b        // `+` grows by one bit: bits[W] + bits[W] -> bits[W+1]
}
```

Rules on display:

- `module Name(params) { ... }` — parameters are compile-time (`int`, `bool`).
- Ports: `in name: type` / `out name: type`. Type after name, TS-style.
- `=` drives a wire/output **combinationally**. Each wire/output is driven
  exactly once.
- `+` is **lossless**: the result is one bit wider than the widest operand, so
  the carry is never silently dropped. `sum` must therefore be `WIDTH + 1`
  bits — the type system catches the classic dropped-carry bug.

### 1.2 Sequential logic — a counter

```mimz
// counter.mimz
module Counter(WIDTH: int = 8) {
  clock clk
  reset rst

  out count: bits[WIDTH]

  reg value: bits[WIDTH] = 0      // `= 0` is the reset value (mandatory)

  on rise(clk) {
    value <- value +% 1           // `+%` = wrapping add, same width
  }

  count = value
}
```

Rules on display:

- `clock` / `reset` are dedicated declarations, not ordinary `bit` inputs.
  Reset behavior is generated automatically from each register's reset value.
  A module that declares any `reg` **must** declare a `reset`.
- `reg name: type = resetValue` — the reset value is **mandatory**. No
  uninitialized state.
- `on rise(clk) { ... }` is the only place registers update, and `<-` is the
  only assignment allowed inside it. Using `=` on a reg, or `<-` on a wire, is
  a compile error with a teaching message. (v0.2: rising-edge only; `fall` is
  a reserved word for the future.)
- `value + 1` would be `bits[WIDTH+1]` and fail to assign. `+%` is the
  explicit wrapping (modulo) operator — counters wrap **on purpose**, visibly.
  The error message for `+` suggests `+%` so beginners learn the distinction
  immediately.
- An `if` without `else` inside an `on` block is fine: a register that is not
  assigned simply **holds its value**. (Only _wires_ must be fully driven —
  that is where latches come from.)
- A module may declare **multiple clocks**, each with its own `on` blocks.
  Every reg is owned by exactly one clock; reading a signal across clock
  domains is a compile error until the explicit `sync` construct ships
  (Phase 2).

### 1.3 Choosing — `if` and `match` are expressions

```mimz
module Alu(WIDTH: int = 8) {
  in  a:  bits[WIDTH]
  in  b:  bits[WIDTH]
  in  op: bits[2]
  out y:  bits[WIDTH]

  y = match op {
    0b00 => a +% b
    0b01 => a -% b
    0b10 => a & b
    0b11 => a | b
  }                       // exhaustive over all 4 values — no latch possible

  wire bigger: bits[WIDTH] = if a > b { a } else { b }   // else is mandatory
}
```

- `match` driving a wire must be **exhaustive** (cover every value or have a
  `_` arm). Arms may list several patterns: `0b00, 0b01 => a`. `if` driving a
  wire must have `else`. Latches are impossible to express by accident.
- Exhaustiveness rulings (v0.2.3): a `match` that names **every enum
  variant** (or every value of `bits[N]`) is exhaustive **without** `_`; a
  `_` arm AFTER full coverage is also legal — it documents the recovery
  path for invalid encodings (e.g. after a bit flip), and the emitted
  Verilog makes the last arm the default either way. An arm placed after
  `_`, or a pattern value already covered, is an error (unreachable).
- `wire name: type = expr` introduces a named combinational signal.

### 1.4 State machines — `enum` + `match`

```mimz
module TrafficLight {
  clock clk
  reset rst

  out red:    bit
  out yellow: bit
  out green:  bit

  enum State { Red, Green, Yellow }

  reg state: State   = State.Red
  reg timer: bits[8] = 0

  on rise(clk) {
    if timer == 0 {
      state <- match state {
        State.Red    => State.Green
        State.Green  => State.Yellow
        State.Yellow => State.Red
      }
      timer <- match state {
        State.Red    => 50
        State.Green  => 40
        State.Yellow => 10
      }
    } else {
      timer <- timer -% 1
    }
  }

  red    = state == State.Red
  yellow = state == State.Yellow
  green  = state == State.Green
}
```

- `enum` defines a state type; the compiler picks the encoding (and can later
  expose `one-hot` etc. as attributes). `match` over an enum must cover every
  variant.

### 1.5 Composition — `import` and instantiation

```mimz
// top.mimz
import adder            // brings modules from adder.mimz into scope

module Top {
  clock clk
  reset rst

  in  x:     bits[8]
  in  y:     bits[8]
  out total: bits[9]

  let add = Adder(WIDTH: 8) { a: x, b: y }
  total = add.sum
}
```

**Import semantics (v0.2):**

- `import name` loads `name.mimz`, resolved **relative to the importing
  file's directory** (sub-paths via `import lib.adder` → `lib/adder.mimz`).
- `include` is an accepted English alias of `import` — both lex to the same
  token, identical semantics. Tooling (`mimz translate`, `mimz fmt`)
  normalizes it to the canonical `import`.
- All modules and enums of the imported file come into scope. Module names
  must be **unique across the whole project** — a duplicate is a compile
  error (no shadowing, no aliasing in v0.2).
- Imports are not transitive and cycles are a compile error.

**Instantiation:**

- `let name = Module(params) { port: signal, ... }` connects **inputs** by
  name; outputs are read by dot access (`add.sum`). All inputs must be
  connected; missing or extra connections are compile errors.
- **`let` binds a hardware instance, not a variable.** Despite the
  JS-flavored keyword, there is no mutation and no re-binding: each `let`
  places one physical copy of the module, permanently. (Named
  combinational values use `wire name: type = expr`; registers use
  `reg`.) Known JS-instinct hazard — flagged for beginner testing.
- A child's `clock`/`reset` with the same name as the parent's is connected
  implicitly; different clocks must be wired explicitly.

### 1.6 Repeated hardware — `repeat`

```mimz
module Chaser(N: int = 8) {
  clock clk
  reset rst
  in  enable: bits[8]
  out led:    bits[8]

  repeat i: 0..8 {
    led[i] = enable[i] & blink[i].out      // i is compile-time
    let blink[i] = Blinker(RATE: i + 1) { }
  }
}
```

- `repeat i: lo..hi { ... }` is **compile-time unrolling** — it generates
  hardware, it is not a runtime loop. `lo..hi` is half-open (`0..8` = 0–7)
  and must be constant.
- The loop variable `i` is a compile-time `int`, usable in indices, slices,
  and parameters.
- Instances declared inside a `repeat` are arrays: declare `let name[i] = …`,
  reference `name[i].port`. Outside the loop they are addressable only with
  constant indices.
- A `repeat` body may only **generate** hardware — drives, instances, and
  nested `repeat`s. It may **not declare** anything (a port, `wire`, `reg`,
  `clock`, `reset`, `const`, `enum`, or `on` block): N copies of one name is
  not a thing. Declare the signal once outside the loop and drive bit `i`
  inside. A declaration inside `repeat` is **E0303**.
- The bounds and any index/condition over the loop variable fold at compile
  time, so an `if` on `i` selects a branch per iteration rather than emitting
  a run-time mux (this is what lets `if i == 0 { … } else { name[i-1] … }`
  chain cleanly without ever referencing `name[-1]`).

### 1.7 Signed numbers

```mimz
wire t:  signed[8]  = -25                  // negative literals: signed only
wire u:  bits[8]    = 0xF0
wire s:  signed[8]  = signed(u)            // explicit reinterpret cast
wire b:  bits[8]    = unsigned(t)          // and back — same width, free
wire w:  signed[16] = extend(s, 16)        // extend is type-directed:
                                           //   bits -> zero-extend
                                           //   signed -> sign-extend
wire n:  signed[9]  = -s                   // unary minus: signed only,
                                           // lossless (result is N+1 bits)
wire eq: bit        = t < s                // signed comparison
```

- `signed[N]` is two's complement. **`signed` and `bits` never mix** in any
  operator — conversion is always the visible `signed()` / `unsigned()` cast
  (a free reinterpretation, same width).
- Negative literals are legal only in `signed` contexts and must fit.
- Unary `-` works only on `signed` and grows one bit (lossless — negating the
  most-negative value is otherwise a classic bug). Wrapping negate: `0 -% x`.
- Comparisons between `signed` operands compare as signed.
- `match` does not accept a `signed` scrutinee (patterns cannot express
  negative numbers yet) — match on `unsigned(x)` and handle the sign
  separately. Slicing a `signed` value yields raw `bits`.

### 1.8 Slicing, concatenation, literals

```mimz
wire lo:   bits[4] = data[3:0]        // slice (inclusive, msb:lsb)
wire hi:   bits[4] = data[7:4]
wire both: bits[8] = { hi, lo }       // concatenation, msb-first
wire wide: bits[16] = extend(data, 16)  // explicit zero-extension

wire k1: bits[8] = 0b1010_0001        // binary, `_` separators allowed
wire k2: bits[8] = 0xA1               // hex
wire k3: bits[8] = 161                // decimal — must fit the target width
```

- There is **no implicit** widening or truncation anywhere. `extend(x, N)`
  widens; slicing narrows. Both are visible at the call site.
- `extend(x, N)` requires `N >=` the current width; `trunc(x, N)` requires
  `N <=` it and keeps the **low** N bits. The same-width call is a no-op and
  legal — parameterized code like `extend(din, WIDTH)` must survive the
  `WIDTH = 1` instantiation.
- An unsized literal adapts to the context width if it fits; otherwise it is a
  compile error (never a silent wrap).
- Digits are **ASCII only** (`0-9`, `a-f`); Tamil digits (௦–௯) are not
  accepted in literals.

### 1.9 Constants

```mimz
const BAUD: int = 9600                // file or module scope
const FAST: bool = true

module Uart {
  const DIVISOR: int = 5208           // = 50 MHz / 9600, precomputed —
  ...                                 //   there is no `/`, even at compile time
}
```

`const` declares a named compile-time value (`int` or `bool`) at file or
module scope — the SystemVerilog `parameter/localparam` role, one keyword.

### 1.10 Tests

```mimz
test "counter counts" for Counter(WIDTH: 4) {
  a = 3                  // drive module inputs by assignment
  tick(clk)              // advance one rising edge
  expect count == 1
  tick(clk, 3)           // advance 3 edges
  expect count == 4
}
```

`test` blocks are simulation-only (Phase 1.5 executes them); they never emit
hardware. `tick` and `expect` are keywords valid only inside `test`.

---

## 2. Lexical Rules

- **Files:** `.mimz`, UTF-8. Identifiers may contain Unicode letters —
  Tamil-script identifiers (e.g. `எண்ணி`) are valid in every flavor.
- **Keywords:** the union of the English, Tanglish, and Tamil keyword tables
  is recognized everywhere; flavors may be mixed in one file. The three
  tables are disjoint by construction.
- **Comments:** `//` line, `/* ... */` block.
- **Numbers:** `123`, `0b1010_1100`, `0xFF`, `_` separators; ASCII digits only.
- **Newlines end statements** (Go-style); no semicolons. A line ending in an
  operator or open bracket continues to the next line.
- **Naming conventions** (Modules `CapitalCase`, signals `lower_case`) are
  enforced by the **linter as warnings**, never by the compiler.

## 3. Operators (universal across flavors)

| Category               | Operators                   | Width rule                                                                                                                     |
| ---------------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Lossless arithmetic    | `+` `-` `*`                 | result grows (`+`/`-`: max+1, `*`: sum of widths)                                                                              |
| Wrapping arithmetic    | `+%` `-%` `*%`              | result = width of operands (must match)                                                                                        |
| Bitwise                | `&` `\|` `^` `~`            | operand widths must match                                                                                                      |
| Shifts                 | `<<` `>>`                   | width preserved; amount is a constant or unsigned signal (never `signed`); shifted-out bits dropped _explicitly by definition_ |
| Comparison             | `==` `!=` `<` `<=` `>` `>=` | result is `bit`; non-associative (`a < b < c` is an error)                                                                     |
| Logical (on `bit`)     | `&&` `\|\|` `!`             | `bit` only — see keyword aliases below                                                                                         |
| Reduction              | `&x` `\|x` `^x` (prefix)    | any `bits[N]` → `bit`                                                                                                          |
| Concat / slice / index | `{a, b}` `x[hi:lo]` `x[i]`  | as written                                                                                                                     |

**Logical-operator aliases (the one G1 exception):** the keyword forms
`and` / `or` / `not` are exact aliases of `&&` / `||` / `!` and, unlike the
symbols, are **translated** in the Tanglish/Tamil flavors
(`mattrum`/`alladhu`/`illa`). Both forms are always accepted;
`mimz fmt --strict` normalizes a file to one style.

**Precedence (Rust-style — bitwise binds tighter than comparison):**

```
unary  →  * *%  →  + - +% -%  →  << >>  →  &  →  ^  →  |
       →  comparison (non-associative)  →  && / and  →  || / or
```

So `x & 1 == 0` parses as `(x & 1) == 0` — the C trap is defused.

**Deliberately absent:** division `/` and modulo `%` do not exist — they
synthesize to large, slow hardware and beginners reach for them by reflex.
Use shifts, or wait for an explicit divider module in the stdlib (Phase 4).

## 4. Types

| Type             | Meaning                                                                    |
| ---------------- | -------------------------------------------------------------------------- |
| `bit`            | single wire, values `0`/`1` (also `true`/`false`) — identical to `bits[1]` |
| `bits[N]`        | N-bit unsigned vector                                                      |
| `signed[N]`      | N-bit two's-complement vector (section 1.7 — never mixes with `bits`)      |
| `clock`, `reset` | dedicated domain types — never mix with data                               |
| `enum` types     | user-defined, compiler-encoded                                             |
| `int`, `bool`    | **compile-time only** (params, widths, `const`, `repeat`) — never hardware |

## 5. Formal Grammar (EBNF, v0.2)

```ebnf
file        = { topItem } ;
topItem     = importDecl | constDecl | moduleDecl | enumDecl | testDecl ;

importDecl  = ( "import" | "include" ) IDENT { "." IDENT } NEWLINE ;
constDecl   = "const" IDENT ":" ( "int" | "bool" ) "=" constExpr NEWLINE ;

moduleDecl  = "module" IDENT [ "(" [ paramList ] ")" ] "{" { moduleItem } "}" ;
paramList   = param { "," param } ;
param       = IDENT ":" ( "int" | "bool" ) [ "=" constExpr ] ;

moduleItem  = portDecl | clockDecl | resetDecl | wireDecl | regDecl
            | constDecl | enumDecl | instDecl | onBlock | driveStmt
            | repeatBlock ;

portDecl    = ( "in" | "out" ) IDENT ":" type NEWLINE ;
clockDecl   = "clock" IDENT NEWLINE ;
resetDecl   = "reset" IDENT NEWLINE ;
wireDecl    = "wire" IDENT ":" type "=" expr NEWLINE ;
regDecl     = "reg"  IDENT ":" type "=" constExpr NEWLINE ;
enumDecl    = "enum" IDENT "{" IDENT { "," IDENT } [ "," ] "}" ;
instDecl    = "let" instName "=" IDENT "(" [ argList ] ")"
              [ "{" [ connList ] "}" ] NEWLINE ;
instName    = IDENT [ "[" constExpr "]" ] ;        (* indexed inside repeat *)
argList     = namedArg { "," namedArg } ;
namedArg    = IDENT ":" constExpr ;
connList    = conn { "," conn } [ "," ] ;
conn        = IDENT ":" expr ;

repeatBlock = "repeat" IDENT ":" constExpr ".." constExpr
              "{" { moduleItem } "}" ;             (* compile-time unrolled *)

onBlock     = "on" "rise" "(" IDENT ")" seqBlock ; (* "fall" reserved *)
seqBlock    = "{" { seqStmt } "}" ;
seqStmt     = regAssign | seqIf ;
regAssign   = lvalue "<-" expr NEWLINE ;
seqIf       = "if" expr seqBlock [ "else" ( seqIf | seqBlock ) ] ;

driveStmt   = lvalue "=" expr NEWLINE ;
lvalue      = IDENT [ "[" constExpr [ ":" constExpr ] "]" ] ;

type        = "bit"
            | "bits"   "[" constExpr "]"
            | "signed" "[" constExpr "]"
            | IDENT ;                          (* enum type *)

expr        = ifExpr | matchExpr | binExpr ;
ifExpr      = "if" expr "{" expr "}" "else" ( "{" expr "}" | ifExpr ) ;
matchExpr   = "match" expr "{" { matchArm } "}" ;
matchArm    = ( pattern { "," pattern } | "_" ) "=>" expr NEWLINE ;
pattern     = literal | IDENT "." IDENT ;       (* value or Enum.Variant *)

binExpr     = unary { binOp unary } ;           (* precedence table, section 3 *)
binOp       = "+" | "-" | "*" | "+%" | "-%" | "*%" | "<<" | ">>"
            | "&" | "^" | "|" | "==" | "!=" | "<" | "<=" | ">" | ">="
            | "&&" | "||" | "and" | "or" ;
unary       = [ "~" | "-" | "!" | "not" | "&" | "|" | "^" ] postfix ;
postfix     = primary { "[" expr [ ":" expr ] "]" | "." IDENT } ;
primary     = literal | IDENT | "(" expr ")" | concat | callExpr ;
concat      = "{" expr { "," expr } "}" ;
callExpr    = ( "extend" | "trunc" ) "(" expr "," constExpr ")"
            | ( "signed" | "unsigned" ) "(" expr ")" ;

literal     = [ "-" ] INT | BIN | HEX | "true" | "false" ;
constExpr   = expr ;   (* must fold to a constant at compile time *)

testDecl    = "test" STRING "for" IDENT "(" [ argList ] ")" testBlock ;
testBlock   = "{" { testStmt } "}" ;
testStmt    = tickStmt | expectStmt | testDrive | testIf ;
tickStmt    = "tick" "(" IDENT [ "," constExpr ] ")" NEWLINE ;
expectStmt  = "expect" expr NEWLINE ;
testDrive   = IDENT "=" expr NEWLINE ;          (* drive a module input *)
testIf      = "if" expr testBlock [ "else" ( testIf | testBlock ) ] ;
```

Keywords in this grammar are flavor-mapped per `03-keywords-trilingual.md`;
all punctuation, operators, and built-in type/function names are universal.

## 6. Static Safety Rules (enforced after parse)

1. **Single driver:** every `out`, `wire` driven exactly once; every `reg`
   driven from exactly one `on` block.
2. **Exact widths:** assignment and operands require matching widths
   (after `+`/`-`/`*` growth rules).
3. **Exhaustiveness:** `match` total; wire-driving `if` has `else`.
4. **Reset completeness:** every `reg` has a reset value; a module containing
   any `reg` must declare a `reset`.
5. **Domain typing:** `clock`/`reset` never appear in data expressions; data
   never used as a clock; each reg owned by one clock; cross-clock reads are
   errors until `sync` (Phase 2).
6. **Combinational cycles:** rejected (wire graph must be a DAG).
7. **Const-ness:** widths, params, reset values, `const`, `repeat` bounds fold
   at compile time.
8. **No signed/unsigned mixing:** conversion only via `signed()`/`unsigned()`.

## 7. Deferred Features (explicitly out of v0.2)

| Feature                                              | Target                                              |
| ---------------------------------------------------- | --------------------------------------------------- |
| `on fall(...)` falling-edge blocks                   | reserved keyword, post-v1                           |
| `inout`/tristate ports                               | Phase 2                                             |
| Memories/arrays (`mem`)                              | Phase 2 spec bump                                   |
| Clock-domain crossing (`sync`)                       | Phase 2                                             |
| Structs/bundles/buses                                | post-Phase 2 (stdlib time)                          |
| `match` ranges and don't-care bit patterns (`0b1??`) | v0.3+                                               |
| Division/modulo                                      | never as operators; stdlib divider module (Phase 4) |
| Wrapping/instantiating external Verilog modules      | per Constitution — design in Phase 2+               |

---

## Changelog

- **v0.2.4 (2026-06-12):** `repeat` semantics nailed down while implementing
  emitter unrolling (section 1.6): a `repeat` body generates hardware only —
  declarations inside it are **E0303**; bounds, indices, and conditions over
  the loop variable fold at compile time (a compile-time `if i == …` selects
  its branch, never emitting the dead arm). The emitter now unrolls `repeat`
  (instance arrays `name[i]` flatten to `name__<i>` with outputs
  `name__<i>_<port>`); `const`s fold to literals in emitted Verilog (they are
  compile-time-only, never hardware — section 4). No grammar change.
- **v0.2.3 (2026-06-12):** exhaustiveness rulings, settled while
  implementing the checker's completion slice (E0302/E0601/E0602/E0701):
  full enum/value coverage is exhaustive WITHOUT `_`; a defensive `_`
  after full coverage is legal (bit-flip recovery), never unreachable;
  arms after `_` and duplicate pattern values are errors. Section 1.3
  updated. No grammar changes — rules 3 and 5 of section 6 and the
  section 1.5 connection rule are now compiler-enforced as written.
- **v0.2.2 (2026-06-11):** width-rule clarifications, settled while
  implementing the checker's width pass: `bit` is identical to `bits[1]`;
  lossless `+`/`-` accept unequal operand widths (result `max + 1`);
  `extend`/`trunc` allow the same-width no-op (`extend` never narrows,
  `trunc` never widens and keeps the LOW bits); shift amounts are
  constants or unsigned signals; `match` rejects `signed` scrutinees;
  slicing `signed` yields `bits`. Section 1.9 example corrected (`/` does
  not exist, even in `const` expressions).
- **v0.2.1 (2026-06-11):** `include` accepted as an English alias of
  `import` (same token, same semantics; normalized to `import` by tooling).
  `include` is therefore now a keyword, no longer a legal identifier.
- **v0.2 (2026-06-10):** `.mimz`/`mimz` naming. Rust-style precedence
  (bitwise > comparison; comparisons non-associative). Logical ops: `&&`/`||`/`!`
  - translated keyword aliases (G1-x). Added `const` declarations, `repeat`
    compile-time generation, `import` semantics, full `test` grammar
    (`tick`/`expect`/drives), signed-number semantics (`signed()`/`unsigned()`
    casts replace `signedval`; negative literals; type-directed `extend`; unary
    minus). Cut `on fall` (reserved). Division/modulo declared deliberately
    absent. Multi-clock ownership rule, reg-requires-reset rule, no-mixing rule
    added to section 6. Deferred-features table added.
- **v0.1 (2026-06-10):** Initial draft.
