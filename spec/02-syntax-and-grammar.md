# Min-Mozhi — Syntax & Grammar

> **Spec v0.2.17.** English flavor shown; see `03-keywords-trilingual.md` for
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
- Reset is **synchronous, active-high** by default (`reset rst`): registers take
  their reset value on the clock edge while `rst` is high. Prefix `async`
  (`async reset rst`) for an **asynchronous** reset — the register clears the
  instant `rst` is asserted, lowering to `always @(posedge clk or posedge rst)`.
  (`async`'s Tanglish/Tamil spellings are provisional, pending native review.)
- `reg name: type = resetValue` — the reset value is **mandatory**. No
  uninitialized state.
- `on rise(clk) { ... }` (and `on fall(clk) { ... }`) is the only place
  registers update, and `<-` is the only assignment allowed inside it. Using `=`
  on a reg, or `<-` on a wire, is a compile error with a teaching message.
  `rise` lowers to Verilog `posedge`, `fall` to `negedge`; a register samples on
  the edge of its block. (`fall`'s Tanglish/Tamil spellings are provisional,
  pending native review.)
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
- **Don't-care patterns**: a binary pattern may use `?` for a don't-care bit —
  `0b1??` matches any value whose high bit is 1 (the Verilog `casez` idiom). A
  don't-care pattern must be **exactly as wide** as the scrutinee, and on its own
  it does **not** prove exhaustiveness — keep a `_` arm (or exact literal
  coverage). Binary only for now.
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
- All modules and enums of the imported file come into scope. Module,
  enum, and bundle names must be **unique within the file that declares
  them**; two different files MAY declare the same name (v0.2.19+). A bare
  reference to a name declared in 2+ visible files is ambiguous (`E0110`)
  and must be qualified with the import path it came in through:
  `a.b.Name` (see §1.5b). Function names remain project-wide unique
  (unaffected by this — functions are called in general expression
  position, where `.` already means field access, not a namespace path).
- Imports are not transitive and cycles are a compile error.

**Standard library — `import std.<module>`:**

- `import std.<module>` resolves to the **embedded** standard library,
  independent of the importing file's directory — no install path, offline,
  WASM-safe. The namespace is trilingual: `std` (English) / `nuulagam`
  (Tanglish) / `நூலகம்` (Tamil). The module segment is the English stem
  (`fifo` → `Fifo`) or the pure-Tamil twin name (`வரிசை` / `varisai` → `வரிசை`);
  the written alias selects which — the stem binds the canonical English module,
  the twin name binds the Tamil module.
- `mimz.toml [lib] std = "<dir>"` overrides the embedded library with a local
  copy (`import std.<m>` then loads `<dir>/<m>.mimz`, resolved relative to that
  `mimz.toml`). Populate the directory with `mimz eject std`.
- This is one reserved namespace, not general per-import aliasing (still no
  aliasing of arbitrary modules in this edition). A malformed std import (wrong
  segment count, or an unknown module) is **E1202**.

**Instantiation:**

- `let name = Module(params) { port: signal, ... }` connects **inputs** by
  name; outputs are read by dot access (`add.sum`). All inputs must be
  connected; missing or extra connections are compile errors.
- **`let` binds a hardware instance, not a variable.** Despite the
  JS-flavored keyword, there is no mutation and no re-binding: each `let`
  places one physical copy of the module, permanently. Named combinational
  values use `wire name: type = expr`; registers use `reg`. Known JS-instinct
  hazard — flagged for beginner testing.
- A child's `clock`/`reset` with the same name as the parent's is connected
  implicitly; different clocks must be wired explicitly.

### 1.5b — Packages / namespacing (v0.2.19)

Two files may declare the same module/enum/bundle name. Qualify a reference
with the exact import path you wrote, dot-joined, to pick one:

```mimz
import mine.fifo    // declares module Fifo
import std.fifo      // ALSO declares module Fifo

module Top {
  let a = mine.fifo.Fifo(DEPTH: 4) { ... }
  let b = std.fifo.Fifo(DEPTH: 4) { ... }
}
```

- The qualifier is exactly the import path as written in THIS file's own
  `import` statement — not a separate declared package name.
- Qualification is available at 4 positions: module instantiation
  (`let x = a.b.Name(...)`), test header (`test "..." for a.b.Name`), an
  enum type (`reg s: a.b.Name = ...`), and a bundle type
  (`wire w: a.b.Name(...)`).
- A bare (unqualified) reference still works exactly as before, as long as
  it is unambiguous — this is fully additive; no existing file needs to
  change.
- `E0110` — the bare name resolves to 2+ declarations across different
  files; qualify it.
- `E0111` — the qualifier doesn't match any `import` this file wrote.
- Function names (`fn`) are NOT covered — they stay project-wide unique
  (`E0801`), called in plain expression position.

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
wire quad: bits[16] = {4{hi}}         // replication — {hi, hi, hi, hi}
wire wide: bits[16] = extend(data, 16)  // explicit zero-extension

wire k1: bits[8] = 0b1010_0001        // binary, `_` separators allowed
wire k2: bits[8] = 0xA1               // hex
wire k3: bits[8] = 161                // decimal — must fit the target width
```

- The slice syntax is `x[hi:lo]` (inclusive, msb:lsb) and concatenation is
  `{a, b}` (msb-first) — these are the **canonical, final forms** (ratified
  2026-06-13).
- Rust-style range slicing (`x[lo..hi]`) is deliberately **not** adopted:
  `[hi:lo]` is the universal hardware convention (Verilog/VHDL/every textbook),
  and matching it keeps a student fluent across tools — the cross-tool familiarity
  outweighs the cosmetic gain.
- **Replication** is `{N{x}}` — the inner concatenation group repeated `N`
  times, msb-first, where `N` is a compile-time constant: `{4{hi}}` is
  `{hi, hi, hi, hi}`. Like Verilog's `{N{...}}`; the result width is `N *` the
  inner width (E0410 if that is not a valid width, E0201 if `N` is not constant).
- There is **no implicit** widening or truncation anywhere. `extend(x, N)`
  widens; slicing narrows. Both are visible at the call site.
- `extend(x, N)` requires `N >=` the current width; `trunc(x, N)` requires
  `N <=` it and keeps the **low** N bits. The same-width call is a no-op and
  legal — parameterized code like `extend(din, WIDTH)` must survive the
  `WIDTH = 1` instantiation.
- An unsized literal adapts to the context width if it fits; otherwise it is a
  compile error (never a silent wrap).
- **Arithmetic / reduction built-ins** (added v0.2.7):

  - `min(a, b)` / `max(a, b)` take two operands of the same width (a literal
    adapts to the sized side, like a comparison) and return that type.
  - `abs(x)` takes a `signed[N]` and returns `signed[N+1]` (room for `abs(MIN)`).
  - `nand(x)` / `nor(x)` / `xnor(x)` are the negated bit-reductions (one bit out),
    the dictionary spellings of `~&x` / `~|x` / `~^x`.

  They lower to plain Verilog-2005 (a `?:` for min/max/abs, the negated reduction
  operators for nand/nor/xnor) — no SystemVerilog. Like `extend`/`trunc`, they are
  runtime built-ins, not compile-time constant folders.

- **Compile-time built-in** `clog2(n)` (added v0.2.13) — the one **compile-time**
  built-in, the inverse of the runtime ones above. It takes a single constant
  argument and folds to the number of bits needed to address `n` items
  (`⌈log₂(n)⌉`, floored at 1). Because it produces a _constant_, it is valid
  exactly where a constant is — a width `bits[clog2(DEPTH)]`, a `const`, a
  `repeat` bound — and is a compile error (E0407) in a runtime value position
  (assign it to a `const` first).

  - `clog2(1)` = `clog2(2)` = 1, `clog2(3)` = `clog2(4)` = 2, `clog2(8)` = 3,
    `clog2(9)` = 4. The argument must const-evaluate to `>= 1` (E0202 otherwise).
  - Min-Mozhi has no zero-width signal (`bits[0]` does not exist), so `clog2`
    floors at 1 — deliberately one bit more than Verilog `$clog2(1) = 0` at
    `n = 1`, so `bits[clog2(N)]` is **always** a legal width. It is the SAME
    function the compiler uses internally to size enum signals
    (`clog2(variant count)`).
  - Of a literal or `const` it lowers to nothing — by emit time it has folded to
    a literal: `const DEPTH = 16` then `reg ptr: bits[clog2(DEPTH)]` derives its
    own pointer width.
  - Of an overridable module **parameter** it stays symbolic, so a **body**
    width (`reg`/`wire`/`mem`) lowers to a call of an emitted Verilog-2005
    `clog2` constant function — the width then tracks an instantiation-time
    parameter override (`reg [(clog2(DEPTH))-1:0] ptr`). The function matches
    this floor-at-1 definition.
  - **One limit:** a `clog2(PARAM)` in a **port** width is a compile error — the
    constant function lives in the module body and cannot reach the header port
    list. Size a body signal with it, or pass the width as its own parameter.

- Digits are **ASCII only** (`0-9`, `a-f`); Tamil digits (௦–௯) are not
  accepted in literals.

### 1.8b — `default` assignments

A `default` assignment sets a priority-lowest non-blocking assignment for a
register. It prevents forgetting to deassert signals on paths that do not
explicitly assign them.

**Syntax:**

```text
default NAME <- EXPR
```

- `NAME` must refer to a `reg` declared in the same module (E0809 if not).
- `EXPR` is evaluated and assigned non-blocking at the start of the
  always-block, before any conditional assignments.
- Each register may have at most one `default` in a given `on` block (E0810).
- `default` statements are legal only at the **top level** of an `on` block
  body. They are not allowed inside nested `if`/`match` arms.

**Example:**

```mimz
on rise(clk) {
    default done <- 0
    if start {
        done <- 1
    }
}
```

Emits as Verilog:

```verilog
always @(posedge clk) begin
    done <= 0;        // default emitted first
    if (start)
        done <= 1;    // conditional overrides
end
```

**Errors:** E0809 (target not a reg), E0810 (duplicate `default` for the
same reg in the same `on` block).

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

### 1.9b — Item-level `const if`

`const if` conditionally includes or excludes module-body items at
elaboration time. The condition must evaluate to a compile-time constant.

**Syntax:**

```text
const if (COND) {
    ITEM*
} [else {
    ITEM*
}]
```

- `COND` is a compile-time expression: may use module parameters,
  module-level `const`s, literals, and arithmetic/comparison operators.
- Items in the winning branch are elaborated normally. Items in the losing
  branch are completely discarded — they are not type-checked,
  name-resolved, or emitted.
- Ports, clocks, and resets may appear inside a `const if` branch; they are
  only registered if their branch wins.
- `const if` blocks may be nested.
- `COND` that cannot be resolved at compile time produces E0811.
- `const if` is **module-body only** — it may not appear at file level
  (`TopItem`). File-level conditional items are out of scope for v0.2.

**Example:**

```mimz
module Adder(WIDTH: int = 8) {
    in a: bits[WIDTH]
    in b: bits[WIDTH]
    out sum: bits[WIDTH]

    const if (WIDTH > 8) {
        wire carry: bit = (a[WIDTH-1] & b[WIDTH-1])
        out overflow: bit
        overflow = carry
    }

    sum = a + b
}
```

**Errors:** E0811 (condition not compile-time constant).

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
hardware. `tick` and `expect` are keywords valid only inside `test`. Execution
semantics (and the equivalent `await clk.cycles(n)` timing form) are specified in
[`spec/05-simulator.md`](05-simulator.md).

### 1.11 Memories — `mem`

```mimz
module RegFile {
  clock clk
  in  we: bit
  in  waddr: bits[2]
  in  wdata: bits[8]
  in  raddr: bits[2]
  out rdata: bits[8]

  mem m: bits[8][4] = 0       // 4 cells of bits[8], every cell seeded to 0

  on rise(clk) {
    if we {
      m[waddr] <- wdata       // clocked, indexed write
    }
  }

  rdata = m[raddr]            // combinational, indexed read
}
```

A `mem` is an **addressable memory** — `DEPTH` cells, each of an element type
(`bit` / `bits[W]` / `signed[W]`), declared `mem name: <element>[DEPTH] = init`.
It lowers to a Verilog packed-element memory `reg [W-1:0] name [0:DEPTH-1]`.

- **`DEPTH`** is a compile-time constant (`1..` cells; E0410 otherwise, E0201 if
  not constant).
- **Init.** The init value is **mandatory** and seeds **every** cell at power-on
  (Verilog `initial`) — the "no uninitialized state" safety rule, without an
  unsynthesizable whole-memory clear. A `reset` line clears registers only, not
  memory; so a memory-only module needs no `reset`.
- **Read** `m[addr]` is **combinational** and yields the element type; the
  address may be a runtime signal. A compile-time address outside `0..DEPTH-1` is
  E0406; a runtime out-of-range read yields the init value.
- **Write** `m[addr] <- v` is **sequential** — only inside an `on` block, where
  it binds to that block's clock/edge. `=` cannot write a memory (E0505), and a
  memory cannot be sliced or assigned as a whole (E0108). A memory is written by
  at most one `on` block (E0503).
- A memory is internal: its cells are not dumped to VCD (only the signals that
  read it are). Enum-element memories and 2-D memories are deferred (section 7).

### 1.12 Bundles

A `bundle` is a named group of signals that flattens to individual Verilog-2005
wires at compile time. Bundles have no runtime overhead — they are a
compile-time grouping construct only.

```mimz
bundle MemBus(WIDTH: int = 32) {
  valid: bit
  data:  bits[WIDTH]
}

module Passthrough {
  in  req: MemBus(WIDTH: 32)
  out rsp: MemBus(WIDTH: 32)

  // field access
  wire v: bit     = req.valid
  wire d: bits[32] = req.data

  // bundle literal (all fields required)
  rsp = { valid: v, data: d }

  // destructure (partial ok; module-body only, not in `on` blocks)
  let { valid } = req
}
```

Rules:

- `bundle Name(params) { field: type, ... }` — file scope only. Params are
  `int`/`bool` with optional defaults (same grammar as module params).
- Field types must be concrete bit-vectors (`bit`, `bits[N]`, `signed[N]`) or
  enum types. `clock`, `reset`, and nested bundles are disallowed.
- At use sites, params are named: `MemBus(WIDTH: 32)`. Positional is a parse error.
- A bundle literal `{ field: expr, ... }` must name every field (E0901/E0902).
- `let { f1, f2 } = expr` — partial destructure is allowed. Field rename
  syntax `{ f: alias }` is a parse error (E0904); use dot access instead.
- Bundle types are nominally typed: `A` and `B` are different types even if
  their fields match (structural subtyping is deferred to feature 2.9).

**Verilog emission:** a bundle-typed port `in req: MemBus(WIDTH: 32)` lowers to
`input wire req_valid; input wire [31:0] req_data;` — one signal per field,
prefixed `portname_fieldname`. Wires and regs flatten the same way.

### Bundle checker rules

| Code  | Triggered when                                                                       |
| ----- | ------------------------------------------------------------------------------------ |
| E0901 | Bundle literal missing a required field                                              |
| E0902 | Bundle literal references an unknown field name                                      |
| E0903 | Duplicate binding name in `let { }` destructure                                      |
| E0904 | Field rename `{ f: alias }` in `let { }` destructure is not supported (parser error) |
| E0905 | Bundle field type is `clock` or `reset` (deferred — Phase 2)                         |
| E0906 | Bundle type reference: unknown bundle name or wrong param count                      |
| E0907 | Bundle type mismatch (nominal — expected `A`, got `B`)                               |
| E0908 | Duplicate field name in `bundle` declaration (deferred — Phase 2)                    |
| E0909 | Bundle declared more than once (project-wide name collision)                         |

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

| Category               | Operators                   | Width rule                                                                                                                                                |
| ---------------------- | --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Lossless arithmetic    | `+` `-` `*`                 | result grows (`+`/`-`: max+1, `*`: sum of widths)                                                                                                         |
| Wrapping arithmetic    | `+%` `-%` `*%`              | result = width of operands (must match)                                                                                                                   |
| Bitwise                | `&` `\|` `^` `~`            | operand widths must match                                                                                                                                 |
| Shifts                 | `<<` `>>`                   | width preserved; amount is a constant or unsigned signal (never `signed`); shifted-out bits dropped _explicitly by definition_                            |
| Comparison             | `==` `!=` `<` `<=` `>` `>=` | result is `bit`; a monotonic one-direction chain (`0 <= x < 100`) desugars to `&&`; mixed-direction (`a < b > c`) and `==`/`!=` chains are errors (E1109) |
| Logical (on `bit`)     | `&&` `\|\|` `!`             | `bit` only — see keyword aliases below                                                                                                                    |
| Reduction              | `&x` `\|x` `^x` (prefix)    | any `bits[N]` → `bit`                                                                                                                                     |
| Concat / slice / index | `{a, b}` `x[hi:lo]` `x[i]`  | as written                                                                                                                                                |

**Logical-operator aliases (the one G1 exception):** the keyword forms
`and` / `or` / `not` are exact aliases of `&&` / `||` / `!` and, unlike the
symbols, are **translated** in the Tanglish/Tamil flavors
(`mattrum`/`alladhu`/`alla`). Both forms are always accepted;
`mimz fmt --strict` normalizes a file to one style.

**Precedence (Rust-style — bitwise binds tighter than comparison):**

```
unary  →  * *%  →  + - +% -%  →  << >>  →  &  →  ^  →  |
       →  comparison (chainable, one direction)  →  && / and  →  || / or
```

So `x & 1 == 0` parses as `(x & 1) == 0` — the C trap is defused.

**Comparison chaining:** a chain of comparisons that all point the **same
direction** — all `<`/`<=` (ascending) or all `>`/`>=` (descending) — is
allowed and desugars to the `&&` of its adjacent pairs:
`0 <= x < 100` becomes `(0 <= x) && (x < 100)`.

The shared middle operand is a combinational value, so reading it twice is
identical — there is none of software's evaluation-order subtlety.

The genuinely confusing forms stay **errors (E1109)**:

- mixed-direction chains (`a < b > c`)
- any chain that mixes in `==`/`!=`

A lone comparison is unaffected. _The original C trap — `a < b < c` meaning
`(a<b)<c` — was never legal here, so allowing the safe monotonic form only widens
what compiles; it breaks no existing program._

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
topItem     = importDecl | constDecl | moduleDecl | enumDecl | testDecl | fnDecl
            | bundleDecl ;

importDecl  = ( "import" | "include" ) IDENT { "." IDENT } NEWLINE ;
constDecl   = "const" IDENT ":" ( "int" | "bool" ) "=" constExpr NEWLINE ;

bundleDecl  = "bundle" IDENT [ "(" [ paramList ] ")" ] "{" { fieldDecl } "}" ;
fieldDecl   = IDENT ":" type NEWLINE ;

moduleDecl  = "module" IDENT [ "(" [ paramList ] ")" ] "{" { moduleItem } "}" ;
paramList   = param { "," param } ;
param       = IDENT ":" ( "int" | "bool" ) [ "=" constExpr ] ;

moduleItem  = portDecl | clockDecl | resetDecl | wireDecl | regDecl | memDecl
            | constDecl | enumDecl | instDecl | onBlock | driveStmt
            | repeatBlock | bundleDestructure ;

bundleDestructure = "let" "{" IDENT { "," IDENT } "}" "=" expr NEWLINE ;

portDecl    = ( "in" | "out" ) IDENT ":" type NEWLINE ;
clockDecl   = "clock" IDENT NEWLINE ;
resetDecl   = [ "async" ] "reset" IDENT NEWLINE ;
wireDecl    = "wire" IDENT ":" type "=" expr NEWLINE ;
regDecl     = "reg"  IDENT ":" type "=" constExpr NEWLINE ;
memDecl     = "mem"  IDENT ":" type "[" constExpr "]" "=" constExpr NEWLINE ;
enumDecl    = "enum" IDENT "{" enumVariant { "," enumVariant } [ "," ] "}" ;
enumVariant = IDENT [ "(" payloadField { "," payloadField } ")" ] ;
payloadField = IDENT ":" type ;                (* name is documentation-only; bindings are positional *)
instDecl    = "let" instName "=" IDENT "(" [ argList ] ")"
              [ "{" [ connList ] "}" ] NEWLINE ;
instName    = IDENT [ "[" constExpr "]" ] ;        (* indexed inside repeat *)
argList     = namedArg { "," namedArg } ;
namedArg    = IDENT ":" constExpr ;
connList    = conn { "," conn } [ "," ] ;
conn        = IDENT ":" expr ;

repeatBlock = "repeat" IDENT ":" constExpr ".." constExpr
              "{" { moduleItem } "}" ;             (* compile-time unrolled *)

onBlock     = "on" ( "rise" | "fall" ) "(" IDENT ")" seqBlock ;
seqBlock    = "{" { seqStmt } "}" ;
seqStmt     = regAssign | seqIf ;
regAssign   = lvalue "<-" expr NEWLINE ;
seqIf       = "if" expr seqBlock [ "else" ( seqIf | seqBlock ) ] ;

driveStmt   = lvalue "=" expr NEWLINE ;
lvalue      = IDENT [ "[" constExpr [ ":" constExpr ] "]" ] ;

type        = "bit"
            | "bits"   "[" constExpr "]"
            | "signed" "[" constExpr "]"
            | IDENT
            | IDENT "(" namedArg { "," namedArg } ")" ;

expr        = ifExpr | matchExpr | binExpr ;
ifExpr      = "if" expr "{" expr "}" "else" ( "{" expr "}" | ifExpr ) ;
matchExpr   = "match" expr "{" { matchArm } "}" ;
matchArm    = ( pattern { "," pattern } | "_" ) "=>" expr NEWLINE ;
pattern     = literal | maskLiteral
            | IDENT "." IDENT [ "(" IDENT { "," IDENT } ")" ] ;
                                               (* Enum.Variant or Enum.Variant(b1, b2, …) — positional bindings *)
maskLiteral = "0b" binMaskDigit { binMaskDigit } ;      (* `0b1??` — `?` is don't-care *)
binMaskDigit = "0" | "1" | "?" | "_" ;

binExpr     = unary { binOp unary } ;           (* precedence table, section 3 *)
binOp       = "+" | "-" | "*" | "+%" | "-%" | "*%" | "<<" | ">>"
            | "&" | "^" | "|" | "==" | "!=" | "<" | "<=" | ">" | ">="
            | "&&" | "||" | "and" | "or" ;
unary       = [ "~" | "-" | "!" | "not" | "&" | "|" | "^" ] postfix ;
postfix     = primary { "[" expr [ ":" expr ] "]" | "." IDENT } ;
primary     = literal | IDENT | "(" expr ")" | concat | replication | bundleLiteral | callExpr | fnCall ;
concat      = "{" expr { "," expr } "}" ;
bundleLiteral = "{" fieldInit { "," fieldInit } [ "," ] "}" ;
fieldInit     = IDENT ":" expr ;
replication = "{" expr "{" expr { "," expr } "}" "}" ;
callExpr    = ( "extend" | "trunc" ) "(" expr "," constExpr ")"
            | ( "signed" | "unsigned" | "abs"
              | "nand" | "nor" | "xnor" ) "(" expr ")"
            | ( "min" | "max" ) "(" expr "," expr ")" ;

literal     = [ "-" ] INT | BIN | HEX | "true" | "false" ;
constExpr   = expr ;   (* must fold to a constant at compile time *)

testDecl    = "test" STRING "for" IDENT "(" [ argList ] ")" testBlock ;
testBlock   = "{" { testStmt } "}" ;
testStmt    = tickStmt | expectStmt | testDrive | testIf ;
tickStmt    = "tick" "(" IDENT [ "," constExpr ] ")" NEWLINE ;
expectStmt  = "expect" expr NEWLINE ;
testDrive   = IDENT "=" expr NEWLINE ;          (* drive a module input *)
testIf      = "if" expr testBlock [ "else" ( testIf | testBlock ) ] ;

fnDecl      = "fn" IDENT "(" [ fnParamList ] ")" "->" type
              "{" { localLet } expr "}" ;       (* combinational; no clocks, no regs *)
fnParamList = fnParam { "," fnParam } ;
fnParam     = IDENT ":" type ;
localLet    = "let" IDENT "=" expr NEWLINE ;    (* named intermediate value *)
fnCall      = IDENT "(" [ expr { "," expr } ] ")" ;  (* user-defined fn call *)
```

Keywords in this grammar are flavor-mapped per `03-keywords-trilingual.md`;
all punctuation, operators, and built-in type/function names are universal.

**Disambiguation note:** `bundleLiteral` vs `concat` — if the first element
after `{` is `IDENT ":"`, it is a bundle literal; otherwise it is a
concat/replicate.

## 5a. Tagged-Union Enums — Physical Layout and Match Semantics

An `enum` whose variants carry payload fields is a **tagged union**. The
compiler packs it into a single bit-vector using the layout below (D3).

### Wire layout

```
[ tag_w bits | max_payload_w bits ]
  MSBs         LSBs
```

- **`tag_w`** = `clog2(variant_count)` — the fewest bits to distinguish all variants.
- **`max_payload_w`** = width of the widest variant's payload (zero-padded for
  narrower variants).
- **Total wire width** = `tag_w + max_payload_w`.
- Fields within one variant's payload are packed **MSB-first** in the payload
  region: the first declared field occupies the top bits, the last the bottom.

A tag-only enum (all `fields: []`) has `max_payload_w = 0`; the total wire
width is just `tag_w` and the layout is identical to the pre-tagged encoding.

### Match extraction semantics

When a `match` arm carries payload bindings — `Packet.Read(a)` — each binding
name is bound to the corresponding payload slice of the scrutinee at that arm:

```
Packet.Read(addr)  → addr = scrutinee[max_payload_w - 1 : max_payload_w - field_w(addr)]
```

Bindings are **positional** (design decision D2): the field _names_ in the
`enum` declaration are documentation only. Binding names in the `match` arm are
arbitrary identifiers scoped to that arm's value expression.

### Checker rules for tagged enums

| Code  | Triggered when                                                              |
| ----- | --------------------------------------------------------------------------- |
| E0806 | Number of bindings in a `match` pattern ≠ number of payload fields          |
| E0807 | A payload field type is not a concrete bit-vector (`bit`, `bits`, `signed`) |

Payload field types must be concrete bit-vectors so the compiler can compute
`max_payload_w` statically. Nested enums as payload types are deferred (E0807
rejects them for now). The scrutinee of a match over a tagged enum must be a
simple identifier so the emitter can slice it without duplicating evaluation.

## 5b. OR-Pattern Binding Intersection (v0.2.16)

When a match arm lists multiple patterns separated by `,` (OR-patterns), all
alternatives must expose an **identical binding interface**: the same names,
each with the same type.

**Rule:** For each binding name `n`, if alternative `i` declares `n: T_i`
then every other alternative must also declare `n` with the same type `T`.

**Correct:**

```mimz
Op.Add(a, b), Op.Sub(a, b) => a + b   // same names, same types
```

**Rejected (E0808) — name mismatch:**

```mimz
Op.Add(a, b), Op.Mul(x) => a          // `b` absent in Mul alternative
```

**Rejected (E0808) — width mismatch:**

```mimz
Op.Big(x), Op.Small(x) => x           // `x` is bits[16] vs bits[8]
```

`_` wildcards do not satisfy a binding requirement. `A(x), _ => x` is E0808
because the `_` alternative provides no binding for `x`.

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

| Feature                                         | Target                                                |
| ----------------------------------------------- | ----------------------------------------------------- |
| `inout`/tristate ports                          | Phase 2                                               |
| Enum-element and 2-D memories (`mem`)           | post-v1 (scalar `bit`/`bits`/`signed` cells ship now) |
| Clock-domain crossing (`sync`)                  | Phase 2                                               |
| Structs/bundles/buses                           | post-Phase 2 (stdlib time)                            |
| `match` ranges (e.g. `0..7`)                    | v0.3+                                                 |
| Division/modulo                                 | never as operators; stdlib divider module (Phase 4)   |
| Wrapping/instantiating external Verilog modules | per Constitution — design in Phase 2+                 |

---

## Changelog

- **v0.2.19 (2026-07-01):** **Packages / namespacing** (backlog:
  `docs/plan/phase-2-ir-synthesis.md`). Module/enum/bundle name uniqueness
  narrows from project-wide to per-file (§1.5). New qualified-reference
  syntax `a.b.Name` (§1.5b) at 4 reference points, reusing the `import`
  path already written — no new keyword. `E0110` (ambiguous bare
  reference), `E0111` (qualifier matches no import). Function names
  unaffected (stay project-wide unique, `E0801`) — see Decision
  D-PKG-1. Additive: no existing `.mimz` file changes behavior.
- **v0.2.18 (2026-07-01):** **Bundles** — `bundle Name(params) { fields }` at
  file scope (feature 2.4). Parametric; field types must be concrete bit-vectors
  or enums. Port/wire/reg usage; bundle literals `{ field: expr }` (E0901/E0902);
  dot access `bus.field` (deferred); `let { f }` destructure (E0903, rename syntax
  E0904 in parser). Nominal typing (E0906/E0907/E0909). Emitter flattens to
  `signalname_fieldname` prefixed Verilog-2005 wires. `bundle` promoted from
  reserved to active keyword (PROVISIONAL Tanglish/Tamil). Additive — no existing
  grammar breakage.
- **v0.2.17 (2026-06-30):** **`default` assignments + item-level `const if`.**
  Added section 1.8b: `default NAME <- EXPR` in `on` blocks — priority-lowest
  non-blocking assignment, emitted before conditional statements so
  conditional `<-` always overrides (E0809 target-not-reg, E0810
  duplicate-default). Added section 1.9b: `const if (COND) { items } [else { items
}]` in module bodies — compile-time conditional elaboration, winning branch
  only (E0811 condition-not-const). `default` promoted from reserved to
  active keyword (Tanglish `iyalbu` / Tamil `இயல்பு`, PROVISIONAL).
  Additive — no grammar breakage.
- **v0.2.16 (2026-06-29):** **OR-pattern binding intersection** — when a match
  arm lists multiple patterns separated by `,`, every alternative must expose the
  same binding interface: identical names with identical types. Violations are
  **E0808** (both name-mismatch and width-mismatch sub-cases). `_` wildcards do
  not satisfy a binding requirement. Added section 5b. Additive — no grammar
  change, no new keyword.
- **v0.2.15 (2026-06-28):** **Tagged-union enums** — `enumDecl` now uses
  `enumVariant` (new) which carries an optional `payloadField` list; `pattern`
  gains optional positional payload bindings `(b1, b2, …)`. Added section 5a
  (physical layout, match extraction, E0806/E0807). Additive. Covered by the
  `tagged_packet` four-flavor example and the `sirappu_pothi` pure-Tamil
  showcase.
- **v0.2.14 (2026-06-28):** **Combinational functions** `fn` added (new section 5
  productions `fnDecl`, `fnParamList`, `fnParam`, `localLet`, `fnCall`). `fn` declared at
  file level: `fn f(p: T, …) -> R { [let x = e …] bodyExpr }` — zero or more named
  intermediates (`localLet`) followed by a single return expression. Called as `f(a, b)`,
  which parses as `fnCall` in `primary`. Functions are combinational only (no clocks, no
  registers, no module instantiation). Recursive calls are a compile error (E0805). The
  keyword `fn` (aliases `function` / `saarbu` / `சார்பு`) was promoted from reserved to
  active (spec/03 v0.2.12). Checked by E0801–E0805; lowers to Verilog-2005
  `function automatic`. Additive. Covered by the `fn_mac` four-flavor example
  (kernel == VCD == Icarus).
- **v0.2.13 (2026-06-27):** **Compile-time built-in `clog2`** added (section 1.8) —
  `clog2(n)` folds to the bits needed to address `n` items (`⌈log₂(n)⌉`, floored at 1).
  Valid only in constant positions (widths, `const`, `repeat` bounds); a runtime value
  position is E0407. Named `clog2` (a universal vocabulary built-in, untranslated).
  Parametric form (`clog2(<module param>)` in body widths) lowers to an injected
  Verilog-2005 constant function; a `clog2(<param>)` in a port width is an error. Additive.
- **v0.2.12 (2026-06-17):** **Asynchronous reset** added (section 1.2) — prefix a
  reset declaration with `async` (`async reset rst`) to widen every always-block
  that uses it to `@(posedge clk or posedge rst)`; a plain `reset` stays
  synchronous (the default). Active-high only for this cut (active-low polarity is
  deferred — no polarity keyword is reserved yet). `async` was promoted from
  reserved to an active keyword (KW_ASYNC; Tanglish/Tamil provisional). Additive.
  Grammar `resetDecl` gained the optional `async`. The cycle-based kernel models
  async and sync reset identically at its per-cycle sample points (sub-cycle
  timing is out of scope); the distinction lives in the emitted Verilog. Covered
  by the `async_reset` four-flavor example (kernel == VCD == Icarus).
- **v0.2.11 (2026-06-17):** **Memories `mem`** added (new section 1.11) — an
  addressable array `mem name: <element>[DEPTH] = init`, with a combinational
  indexed read (`m[addr]`) and a clocked indexed write (`m[addr] <- v`); lowers
  to a Verilog packed-element `reg [W-1:0] m [0:DEPTH-1]` with an `initial`
  power-on seed. `mem` was promoted from reserved to an active keyword (KW_MEM;
  Tanglish/Tamil provisional). Additive. Grammar gained `memDecl`. Covered by the
  `regfile` four-flavor example (kernel == VCD == Icarus). Also corrected the
  Deferred table: `on fall` (shipped v0.2.10) and don't-care patterns (shipped
  v0.2.9) were stale entries; enum-element / 2-D memories remain deferred.
- **v0.2.10 (2026-06-17):** **Falling-edge `on fall(clk)`** added (section 1.2) —
  the negedge sibling of `on rise(clk)`; lowers to Verilog `always @(negedge clk)`.
  `fall` was promoted from reserved to an active keyword (KW_FALL; see
  `03-keywords-trilingual.md`, Tanglish/Tamil provisional). Additive. The
  simulator gained an edge-aware kernel (posedge updates before negedge within a
  period), so mixed-edge designs match Icarus bit-for-bit — covered by the
  `dual_edge` four-flavor example.
- **v0.2.9 (2026-06-17):** **Don't-care `match` patterns** added (section 1.3) —
  a binary pattern may use `?` for a don't-care bit (`0b1??`, the `casez` idiom).
  It must match the scrutinee width exactly (E0409 otherwise) and earns no
  exhaustiveness credit (a `_` arm or exact literal coverage is still required —
  E0601). Additive (no new keyword); binary only. Lowers to a masked equality
  `(s & MASK) == VALUE`; covered by the `priority` four-flavor example and the
  Icarus differential.
- **v0.2.8 (2026-06-17):** **Replication `{N{x}}`** added (section 1.8) — repeats
  an inner concatenation group `N` times, `N` a compile-time constant; the result
  width is `N *` the inner width (E0410 if that is not a valid width, E0201 if `N`
  is not constant). Additive (no new keyword) — the first of the pre-v0.1.0
  RTL-parity batch. Lowers to Verilog `{N{...}}`; covered by the `replicate`
  four-flavor example and the Icarus differential.
- **v0.2.6 (2026-06-13):** two pre-v0.1.0-freeze syntax rulings (idea triage
  section 8, `docs/Ideas/language_plan.md` section 9). (1) **Comparison chaining allowed**:
  a monotonic one-direction chain (`0 <= x < 100`) desugars to `&&` of its
  adjacent pairs; mixed-direction and `==`/`!=` chains stay E1109. This only
  widens what compiles — `a < b < c` was already rejected, so no program
  breaks (section 3). (2) **Slice/concat ratified final**: `x[hi:lo]` and
  `{a, b}` are the canonical forms; Rust-style range slicing is not adopted
  (universal hardware convention wins — section 1.8). (Header version note was
  stale at v0.2.2; corrected to track the changelog.)
- **v0.2.5 (2026-06-12):** emission rulings, settled while finishing the
  Phase 1 emitter. (1) **Transliteration**: Tamil-script identifiers emit
  as readable ASCII Verilog names via a pragmatic ISO-15919-flavored
  table (விளக்கு → `villakku`); other scripts fall back to `_uXXXX` hex;
  collisions take deterministic `_2`, `_3`, … suffixes. Source-level
  names are untouched — this is an emission detail, errors still show
  the Tamil spelling. (2) **Signed emission**: `signed[N]` signals are
  declared `signed` in Verilog, so `extend` sign-extends and comparisons
  are signed exactly as section 1.7 promises — now verified exhaustively
  under Icarus (`signed_math` example). No grammar changes.
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
