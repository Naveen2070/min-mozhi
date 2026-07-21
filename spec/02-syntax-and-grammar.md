# Min-Mozhi — Syntax & Grammar

> **Spec v0.2.27.** English flavor shown; see `03-keywords-trilingual.md` for
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
  domains directly is a compile error — crossing requires the explicit
  `sync.double_flop`/`sync.pulse` synchronizer primitives (section 1.2b).

### 1.2b — Clock-domain-crossing synchronizers — `sync.double_flop`/`sync.pulse`

```mimz
module SyncDoubleFlop {
  clock clk_src
  clock clk_dst
  reset rst

  in  fast_bit: bit
  out slow_bit: bit

  reg fast_reg: bit = 0
  reg synced:   bit = 0

  on rise(clk_src) {
    fast_reg <- fast_bit
  }

  on rise(clk_dst) {
    synced <- sync.double_flop(fast_reg, clk_src, clk_dst)
  }

  slow_bit = synced
}
```

- `sync.double_flop(signal, src_clock, dst_clock)` — a classic 2-flop
  synchronizer for a level/control signal. `sync.pulse(signal, src_clock,
dst_clock)` — a toggle-based synchronizer for a single-cycle pulse. Both
  are ordinary builtin-namespace calls (`Builtin::SyncDoubleFlop`/
  `Builtin::SyncPulse`), not a new expression form — three positional
  arguments: the signal to cross, its source clock, its destination clock.
  There is no `@ clock` annotation anywhere in this grammar; `src_clock`/
  `dst_clock` are ordinary bare `Ident` expressions that type-check to the
  existing `Ty::Clock` a `clock` declaration already produces.
- **Width restriction — 1 bit only (E0703).** A bit-independent synchronizer
  applied to a multi-bit bus is a real-hardware metastability hazard a
  functional simulator can't observe, so both primitives reject anything
  wider than `bit`. Multi-bit data crossing (handshake protocols, async
  FIFOs) is explicitly out of scope — see the note below.
- **Domain rule (E0704), asymmetric on purpose:** `double_flop`'s `signal`
  may be domain-free (an external/async input) OR already owned by
  `src_clock`. `pulse`'s `signal` must ALREADY be owned by `src_clock` —
  domain-free is rejected, because the toggle encoding `pulse` lowers to
  only makes sense sampled synchronously on the source side first.
- **Placement (E0705), one legal position each.** `double_flop` must appear
  as the direct `<-` right-hand side of an assignment inside its own
  `dst_clock`'s `on rise`/`on fall` block (its hidden synchronizer register
  is spliced into that existing block). `pulse` must appear as a `wire`'s
  direct initializer (it lowers to its own dedicated `on` blocks on both
  clocks, never merged into a user-written block).
- **Clock-argument shape (E0702).** Both clock arguments must be declared
  `clock`s, and they must be two _different_ clocks — a same-clock call is
  rejected (there is nothing to synchronize).
- **Not yet provided:** handshake (req/ack) protocols and async FIFOs
  (gray-code pointers) — the actual multi-bit-safe data-bus crossing —
  remain future work, buildable as ordinary `.mimz` stdlib modules layered
  on top of these two primitives (no further compiler/checker/emitter/
  simulator changes needed for that layering). See
  `docs/superpowers/specs/2026-07-20-sync-cdc-design.local.md` for the full
  design rationale and Axis decisions.

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

### 1.5c — `extern module` (Verilog FFI)

`extern module` declares the **port list** of a hand-written Verilog
module without defining its body — a thin wrapper so a real, external
`.v` file (an IP core, a vendor PLL primitive, a hand-tuned block) can
be instantiated from mimz source exactly like a native module.

```mimz
// declared without an alias — the mimz name IS the Verilog module name
extern module Pll(MULT: int = 2) {
  doc: "50MHz input, 100MHz output, ~10us lock time"
  clock clk_in
  out clk_out: bit
  out locked: bit
}

module ExternDemo {
  clock sysclk
  out fast_clk: bit
  out pll_ok: bit
  let u = Pll(MULT: 4) { clk_in: sysclk }
  fast_clk = u.clk_out
  pll_ok = u.locked
}
```

```mimz
// declared WITH an alias — mimz name `Pll`, real Verilog module name
// "PLL_HARD_IP_v2" (the emitted instantiation uses the real name)
extern module Pll = "PLL_HARD_IP_v2" {
  clock clk_in
  out clk_out: bit
}

module AliasDemo {
  clock sysclk
  out fast_clk: bit
  let u = Pll() { clk_in: sysclk }
  fast_clk = u.clk_out
}
```

Grammar (mirrors `module`'s param-list and port-line grammar exactly):

```
externModule = "extern" "module" ident [ "=" string ]
  [ "(" paramList ")" ] "{" [ "doc" ":" string ] { port | clock | reset } "}"
```

- `ident` is the name used on the mimz side (instantiation, `.` field
  access). The optional `= "string"` names the **real** Verilog module
  emitted into the instantiation — omit it when the mimz name already
  matches the real module name exactly.
- The optional `doc: "..."` line (first thing in the body, before any
  port) is a human-readable note about the wrapped IP — carried through
  to nowhere mechanical yet, purely documentation for the reader.
- The body accepts **only** `in`/`out`/`clock`/`reset` declarations —
  there is no body for `wire`/`reg`/`on`/etc. to belong to, since
  `extern module` defines no logic, only a port list.
- **Ports are scalar-only**: `bit` / `bits[N]` / `signed[N]` (plus
  `clock`/`reset`). A real Verilog module's port list is always flat
  wires, so bundle- and array-typed ports are rejected (`E1302`) —
  flatten them to the fields the real module actually exposes.
- Instantiation, connection checking, and width checking all work
  exactly as they do for a native `module` (missing/extra/mismatched
  connections are the same errors); the only difference is the emitter
  never writes a `module ... endmodule` body for it — only the
  instantiation, referencing the real module name. The compiled Verilog
  is expected to be linked (Yosys/iverilog) against the real, separately
  supplied `.v` file (wired via `mimz.toml` or `--extern-src`).
- `extern module` names are unique **within one file**, same rule as
  `module` (`E1301`; a different file may reuse the name, qualify by
  import path if it becomes ambiguous, §1.5b).

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

#### 1.10b Hardware-emulation `sim{}` blocks

A `sim{}` block, nested inside a `test` block, binds test-block ports to
REAL emulated peripherals (LED, speaker, UART) instead of plain assertions —
only active under `mimz test --emulate`; ignored by a normal `mimz test` run:

```mimz
test "blink pattern" for Blinker(LIMIT: 3) {
  sim {
    speed hz(2)                     // real-world pacing: 2 Hz
    bind led -> led(color: "green")
    bind tx  -> uart_tx(baud: 9600)
  }
  tick(clk, 10)
}
```

- **`speed`** takes one of `hz(n)` / `khz(n)` / `mhz(n)` — desugars to a plain
  multiplication (`n * 1`/`1_000`/`1_000_000`), setting how many real
  clock-cycles-per-second the emulated run paces itself to.
- **`bind`** connects one port to one peripheral: `bind <port> -> <peripheral>(<config>)`.
  Config values are `key: value` pairs (string, identifier, or bare integer).
  The peripheral set and their config keys are documented in
  [`docs/guide/13-hardware-emulation.md`](../docs/guide/13-hardware-emulation.md).
- A `sim{}` block requires the `hw-emulation` cargo feature; without
  `--emulate`, it parses and checks normally but has no runtime effect.

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
- Bundle types are matched STRUCTURALLY (feature 2.9, shipped): a bundle
  satisfies any bundle-typed slot whose required fields it covers with
  exactly-matching types (widths never coerce), regardless of the two
  bundles' declared names. Extra fields on the provided side are allowed
  and ignored. Applies uniformly to `let` bindings, `Drive` assignments,
  module-instantiation port connections, and `fn` bundle-typed
  args/returns. Fully automatic — no conformance declaration needed.

**Verilog emission:** a bundle-typed port `in req: MemBus(WIDTH: 32)` lowers to
`input wire req_valid; input wire [31:0] req_data;` — one signal per field,
prefixed `portname_fieldname`. Wires and regs flatten the same way. A
bundle-typed `fn` PARAMETER flattens the same way too (one `input` per
field). A bundle-typed `fn` RETURN does not yet — a Verilog `function` can
only return one value, so flattening a return the way ports/wires/params do
isn't applicable; this is a real, open gap (`docs/audit/bugs.md` BUG-10),
not yet supported.

### 1.12a Valid-bundle sugar: `T?` and `??`

`T?` is shorthand for "maybe-present `T`": a trailing `?` on `bit`, `bits[N]`,
or `signed[N]` desugars at **parse time** to a reference to one of two
compiler-synthesized bundles — no new `Type` variant, no new runtime
representation, just sugar over the bundle machinery `§1.12` already
describes.

```mimz
module Mux {
  in  a: bits[8]?
  in  b: bits[8]?
  in  fallback: bits[8]
  out picked: bits[8]
  out either: bits[8]?

  // unwrap: T? ?? T -> T
  picked = a ?? fallback

  // OR-mux: T? ?? T? -> T?
  either = a ?? b
}
```

Rules:

- `bit?` and `bits[N]?` desugar to `{ valid: bit, data: bits[N] }` (`N`
  defaults to 1 for bare `bit?`); `signed[N]?` desugars to
  `{ valid: bit, data: signed[N] }`. `T?` is legal anywhere a scalar `Type`
  is legal — wire/reg/port/`fn` parameter/`fn` return — but **not** as a
  bundle field type; that hits the pre-existing nested-bundle rejection
  (E0807), same as any other bundle used as a field type.
- Construction is **bundle-literal only**: `{ valid: expr, data: expr }`,
  reusing the exact same E0901/E0902 rules as any other bundle literal.
  There is no implicit auto-wrap from a bare value — `wire w: bit? = 1`
  does not compile.
- Because `T?` desugars to an ordinary structurally-typed bundle, a
  user-declared bundle with the exact shape `{ valid: bit, data: T }`
  satisfies a `T?`-typed slot and vice versa — an accepted consequence of
  `§1.12`'s structural matching rule, not a special case carved out for
  this feature.
- `a ?? b` — new operator, lowest precedence (binds looser than `||`),
  left-associative (`a ?? b ?? c` parses as `(a ?? b) ?? c`). Which of two
  forms applies is decided by the right operand's shape, not a keyword or
  annotation:
  - **Unwrap** — `T? ?? T -> T`: the right side's type matches the left
    operand's `data` field exactly.
  - **OR-mux** — `T? ?? T? -> T?`: the right side is itself a valid-bundle
    whose `data` matches the left operand's exactly.
  - Neither form coerces width in either direction (E0912 if the right
    side's type/`data` type doesn't match exactly). The left operand must
    itself be valid-bundle-shaped (E0911 otherwise).
- Never tri-state: both forms always lower to an ordinary two-way
  mux (ternary / `IfExpr`) — never a Verilog `z`/`x` literal — matching
  the "no uninitialized state" guarantee the rest of the spec holds
  elsewhere (`mem` init, reset).
- A `T?` used as an `fn` **return** type inherits the pre-existing,
  unrelated bundle-return gap: the checker accepts it, but the emitter
  cannot flatten ANY bundle-typed `fn` return yet, `?`-sugar or not
  (`docs/audit/bugs.md` BUG-10). Not addressed by this feature.

**Verilog emission:** `T?` flattens exactly like any other bundle (`§1.12`)
— one wire per field (`req_valid`, `req_data`), nothing sugar-specific about
the emitted signals. `a ?? b` lowers to a ternary: the unwrap form emits
`a_valid ? a_data : b`; the OR-mux form emits one ternary per field
(`a_valid ? 1'b1 : b_valid` for `valid`, `a_valid ? a_data : b_data` for
`data`), evaluated at every field-consuming call site a bundle-typed value
can reach (wire/reg init, `Drive`, module-port connection, `fn`-call
argument) in the Verilog emitter. The simulator supports the same lowering
at wire/reg init and `Drive` only — bundle-typed instance ports and `fn`
arguments are unsupported in the simulator today, `??` or not
(`docs/audit/bugs.md` BUG-15). A chained `??` (`x ?? y ?? z`) recurses into
nested `??` operands rather than treating an already-bundle-typed
sub-expression as a plain signal — required for the chain to lower to
correct, not just plausible-looking, Verilog.

### Bundle checker rules

| Code  | Triggered when                                                                                                                                                                                                                                          |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| E0901 | Bundle literal (`{ field: expr, ... }`) missing a required field, at any site a literal can appear (Drive RHS, `fn` call argument, `fn` return/tail) — not the same as passing an already bundle-typed value; that structural case is E0910/E0907/E0804 |
| E0902 | Bundle literal references an unknown field name                                                                                                                                                                                                         |
| E0903 | Duplicate binding name in `let { }` destructure                                                                                                                                                                                                         |
| E0904 | Field rename `{ f: alias }` in `let { }` destructure is not supported (parser error)                                                                                                                                                                    |
| E0905 | Bundle field type is `clock` or `reset` (deferred — Phase 2)                                                                                                                                                                                            |
| E0906 | Bundle type reference: unknown bundle name or wrong param count                                                                                                                                                                                         |
| E0907 | Bundle field type mismatch (structural — a shared field's type differs)                                                                                                                                                                                 |
| E0908 | Duplicate field name in `bundle` declaration (deferred — Phase 2)                                                                                                                                                                                       |
| E0909 | Bundle declared more than once (project-wide name collision)                                                                                                                                                                                            |
| E0910 | Bundle is missing a required field (structural — extra fields are fine, missing ones are not)                                                                                                                                                           |
| E0911 | `??`'s left operand is not a valid-bundle (`T?`) — must be `bit?`/`bits[N]?`/`signed[N]?`-shaped, or a user bundle with the identical `{ valid: bit, data: T }` shape                                                                                   |
| E0912 | `??`'s right operand doesn't match the left operand's `data` type exactly — neither the unwrap form (`T`) nor the OR-mux form (`T?`) coerce width                                                                                                       |

---

### 1.13 `fn` bodies: statements and `return`

`fn` bodies are zero or more statements — `let`, statement-level `if`,
`return` — followed by exactly one tail expression, the guaranteed
fallthrough value if no `return` fires:

```mimz
fn find_first(a: bits[8]) -> int {
  if a[0] == 1 { return 0 }
  if a[1] == 1 { return 1 }
  -1
}
```

- `return expr` immediately yields `expr` as the function's result.
- A statement-level `if` (distinct from the expression-level `if`, which
  requires `else`) may omit `else` — a branch that doesn't return falls
  through to the next statement, or ultimately the tail.
- The tail expression is mandatory, exactly like a `fn` body always was
  before `return` existed — every function has a well-defined result on
  every path, so there is no "missing return" diagnostic.
- Unreachable code after an unconditional `return` in the same block is
  `E0812`.
- `return` saves no hardware and exits nothing in silicon — it is
  priority-selected assignment, not control flow. Every branch's logic is
  still fully instantiated and evaluates every time; `return` only changes
  which already-computed branch is selected first (the same synthesizable
  idiom SystemVerilog functions already use). No cycles, area, or work are
  skipped by returning "early".

### 1.14 Arrays — fixed-size `fn` parameters

`<scalarType>[N]` — one or more trailing `[N]` suffixes on a scalar type
declare a fixed-size, immutable array. Today this is supported for `fn`
**parameters** and `let` locals inside a `fn` body only (module-level
ports/wires/registers stay scalar — `E0416` if you try):

```mimz
fn find_index(vals: bits[8][4], target: bits[8]) -> signed[4] {
  if vals[0] == target { return 0 }
  if vals[1] == target { return 1 }
  if vals[2] == target { return 2 }
  if vals[3] == target { return 3 }
  -1
}

module FindIndex {
  in a: bits[8]
  in b: bits[8]
  in c: bits[8]
  in d: bits[8]
  in target: bits[8]
  out idx: signed[4]
  idx = find_index([a, b, c, d], target)
}
```

- **Element type:** `bit`, `bits[N]`, or `signed[N]` only — nested arrays
  and enum/bundle elements are rejected (`E0411`). **Length:** a
  compile-time constant, at least 1 (`E0412`).
- **Array literals** `[e1, ..., eN]` construct a value of this type at a
  call site (or as a `let` local): every element must share the first
  element's width and signedness (`E0414`), and an array-typed call
  argument's literal length must exactly match the callee's declared
  length (`E0413`).
- **Indexing** `arr[idx]` reuses the existing postfix-index syntax.
  - A **compile-time-constant** index (`arr[2]`) folds directly to that
    element — no hardware is generated for the index itself, exactly like
    a fixed slice bound.
  - A **runtime** index (`arr[i]` where `i` is a signal) is out of range at
    compile time only if it can be proven so (`E0415`); otherwise the
    emitter generates a right-associated priority-mux over every element —
    `(i == 0) ? vals_0 : (i == 1) ? vals_1 : ... : vals_{N-1}` — so an
    out-of-range runtime value reads the last element.
- **Never real Verilog hardware.** An array is elaborated away entirely: a
  `bits[8][4]` parameter becomes 4 independent scalar input ports/locals
  named `<param>_0` .. `<param>_3`, matching how `repeat` already
  elaborates to N copies of hardware rather than a real loop. The
  simulator lowers arrays to N independent `Val`s the same way, so both
  backends agree.

### 1.15 Bounded compile-time loop — `loop`

```mimz
module BitToggle {
  const WIDTH: int = 8

  clock clk
  reset rst
  in  enable: bits[WIDTH]
  reg flags: bits[WIDTH] = 0

  on rise(clk) {
    loop i: 0..WIDTH {
      if enable[i] { flags[i] <- ~flags[i] }
    }
  }
}
```

```mimz
fn find_index(vals: bits[8][4], target: bits[8]) -> signed[4] {
  loop i: 0..4 {
    if vals[i] == target { return i }
  }
  -1
}
```

- `loop i: lo..hi { ... }` is, like `repeat` (section 1.6), **compile-time
  unrolling** — `lo..hi` is half-open and must be constant — but unlike
  `repeat` (item-level only), `loop` is a _statement_: legal anywhere a
  statement is expected inside an `on` block or a `fn` body. It generates
  `hi-lo` copies of its body at elaboration time, one per value of `i`. It is
  **not** a runtime loop and creates no new kind of hardware.
- **In an `on` block:** each unrolled copy is an ordinary sequential
  statement in the same always-block. If two iterations happen to target the
  _same_ register, ordinary last-assignment-wins semantics decide which one
  sticks — there is no free accumulation. Index a different slice per
  iteration (`flags[i]`, above) to get N independent updates.
- **In a `fn` body:** each unrolled copy is a statement in the function's
  statement-based body (section 1.13), so `return` inside a `loop` gives
  first-match-wins search — the compiler generates the same priority chain a
  hand-written `if vals[0] == target { return 0 } if vals[1] == target { ... }`
  would, and the lowest matching index always wins on a duplicate match
  (proven end to end against a real Verilog toolchain by the `fn_array_search`
  example's Icarus differential).
- **The bound must fold to a compile-time constant** — the same requirement
  as `repeat`'s `hi`.
- **Honesty rule — bare `loop`/`suzhal`/`சுழல்` costs area, not time.**
  Despite reading like a software `for` loop, this is not "iterate over
  clock cycles": it unrolls at compile time, in full, before any hardware
  exists. N iterations means **N× the hardware**, evaluated in parallel, with
  **zero** extra clock cycles. A large bound does not make the circuit
  slower — it makes the circuit bigger. Reach for `loop` the way you reach
  for `repeat`: to avoid retyping N copies of a pattern by hand, not to
  spread work across time.
- **`sync loop`/`sync suzhal`** — a distinct keyword pair (section 1.15b) —
  is the opposite trade: a cycle-iterating FSM-plus-counter form that spans
  multiple clock edges and costs cycles instead of area. Bare
  `loop`/`suzhal`/`சுழல்` never grows that behavior; if what you want is
  "iterate over time," reach for `sync loop` instead.

### 1.15b — Cycle-iterating loop — `sync loop`

```mimz
module FindFirst {
  clock clk
  reset rst

  in  key: bits[4]
  mem m:   bits[4][8] = 0

  sync loop find_first on rise(clk) (i: 0..8) -> result: bits[3] = 0 {
    if m[i] == key {
      result <- i
    }
  }
}
```

- `sync loop NAME on (rise|fall)(CLOCK) (VAR: LO..HI) -> RESULT: TYPE = INIT { BODY }`
  is the cycle-iterating sibling of `loop` (section 1.15): the same `lo..hi`
  range and body shape, but instead of unrolling to `hi-lo` copies of
  hardware, it elaborates to **one** small FSM that walks the range one
  value of `VAR` per clock edge. `LO..HI` must still fold to a compile-time
  constant, the same rule `loop`/`repeat` already follow.
- `sync loop` is a **module-body item** (like an `on` block), not a
  statement — unlike bare `loop`, it cannot nest inside an `on` block or a
  `fn` body.
- **Generated interface.** Declaring `sync loop NAME ...` implicitly adds
  four signals, named off `NAME`:
  - `in NAME_start: bit` — pulse high for one cycle to begin a run.
  - `out NAME_done: bit` — pulses high for exactly one cycle when the run
    finishes.
  - `out NAME_result: TYPE` — the accumulator's value; holds the final
    result from the cycle `NAME_done` pulses until the next run overwrites
    it.
  - `out NAME_running: bit` — high for the duration of the run; it rises
    the cycle after `NAME_start` is sampled and drops on the same edge
    `NAME_done` pulses (`NAME_running` and `NAME_done` are never both
    high at once).
    Inside `BODY`, `VAR` reads the live loop index and `RESULT <- expr`
    writes the accumulator, exactly like ordinary `<-` assignments inside an
    `on` block.
- **Timing — costs `hi - lo + 1` clock cycles, not zero.** One edge to
  leave idle and load `VAR = lo`; then one edge per value of `VAR` in the
  range (`hi - lo` of them), each running `BODY` once, the last of which
  also raises `NAME_done`. A `NAME_start` pulse sampled on one edge
  therefore produces `NAME_done` exactly `hi - lo + 1` edges later. Holding
  `NAME_start` high through a run does not re-trigger it — the FSM only
  samples `NAME_start` while idle.

**Honesty rule — `loop` and `sync loop` are opposite trades, on purpose:**

> `loop`/`suzhal`/`சுழல்` costs **area, not time** — N iterations means N×
> the hardware, evaluated in parallel, zero extra clock cycles.
>
> `sync loop`/`sync suzhal`/`sync சுழல்` costs **time, not area** — hardware
> is small and constant regardless of N; the cost is `hi - lo + 1` clock
> cycles per run.

**A genuine semantic difference, not a bug.** `loop` combined with `return`
(section 1.15) gets first-match-wins for free from its if/else priority-mux
structure — each candidate value is chosen combinationally by priority,
never overwritten in time. `sync loop` has no priority structure and no
early-exit primitive: an unguarded body like `if m[i] == key { result <- i }`
overwrites the accumulator on every matching cycle, so `NAME_result` reflects
the LAST matching value of `VAR`, not the first — exactly like an unguarded
imperative `for i in 0..8 { if a[i] == k { result = i } }` would in software,
since the writes happen sequentially in time rather than through priority
selection. To get first-match-wins inside a `sync loop`, guard the write
yourself with an "already found" latch, e.g.
`if m[i] == key && !found { result <- i; found <- 1 }`.

### 1.16 `foreach` — array/range sugar over `repeat`/`loop`

```mimz
module ForeachFill {
  out lamps: bits[32]

  foreach i in 0..4 {
    lamps[i * 8 + 7 : i * 8] = i * 2
  }
}
```

```mimz
fn sum8(values: bits[8][8], acc: bits[11]) -> bits[11] {
  foreach v in values {
    let acc = acc +% extend(v, 11)
  }
  acc
}
```

- `foreach VAR in SOURCE { BODY }` is pure syntax sugar with two source
  forms:
  - **Range form** — `foreach i in lo..hi { ... }` — `VAR` walks the
    half-open range exactly like `repeat`/`loop`'s own `lo..hi`. This is
    nothing more than `in` spelled where `repeat`/`loop` spell `:`.
  - **Elements form** — `foreach v in ARR { ... }` — `ARR` must be an
    array or `mem`-typed name already in scope (a `fn`'s own array
    parameter, or — at module-item level — a sibling `mem`); `VAR` binds
    each element **by value**, one iteration per element, front to back.
    The iteration count is `ARR`'s own declared length — never
    hand-written, so it can never drift out of sync with the array.
- **Placement mirrors `repeat`/`loop` exactly.** At module-item level,
  `foreach` desugars toward `repeat` (section 1.6): compile-time-only,
  item-level. Inside an `on` block or a `fn` body, `foreach` desugars
  toward bare `loop` (section 1.15): a statement, legal wherever a
  statement is expected. There is no third placement — `foreach` never
  appears anywhere `repeat`/`loop` couldn't.
- **Honesty rule — `foreach` costs exactly what the hand-written
  `repeat`/`loop` it desugars to would cost, nothing more.** Same
  area-not-time trade as sections 1.6 and 1.15: N iterations means N×
  the hardware, evaluated in parallel, zero extra clock cycles. `foreach`
  is never a hidden `sync loop` in disguise — see the next bullet.
- **Always elaboration-time — never lowers to `sync loop`.** However long
  `SOURCE` is, `foreach` unrolls fully at compile time before any
  hardware exists, exactly like `repeat`/`loop`. If what's wanted is
  "iterate over clock cycles," reach for `sync loop` (section 1.15b)
  directly — `foreach` does not and will not grow that behavior.
- **The Elements form's array/`mem` source must resolve to an
  array/`mem` type** — E0417 if it doesn't (an undeclared name, a scalar
  signal, or a `fn` parameter that isn't array-typed).

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

| Type              | Meaning                                                                                           |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| `bit`             | single wire, values `0`/`1` (also `true`/`false`) — identical to `bits[1]`                        |
| `bits[N]`         | N-bit unsigned vector                                                                             |
| `signed[N]`       | N-bit two's-complement vector (section 1.7 — never mixes with `bits`)                             |
| `clock`, `reset`  | dedicated domain types — never mix with data                                                      |
| `enum` types      | user-defined, compiler-encoded                                                                    |
| `int`, `bool`     | **compile-time only** (params, widths, `const`, `repeat`) — never hardware                        |
| `<scalarType>[N]` | fixed-size, immutable array (section 1.14) — `fn` params/locals only, never real Verilog hardware |

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
            | repeatBlock | bundleDestructure | syncLoopBlock | foreachBlock ;

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

foreachSource = ( constExpr ".." constExpr ) | IDENT ;
              (* range form (lo..hi) or elements form (an array/mem name) *)
foreachBlock = "foreach" IDENT "in" foreachSource "{" { moduleItem } "}" ;
              (* compile-time unrolled sugar over repeatBlock, section 1.16 *)

syncLoopBlock = "sync" "loop" IDENT "on" ( "rise" | "fall" ) "(" IDENT ")"
                "(" IDENT ":" constExpr ".." constExpr ")"
                "->" IDENT ":" type "=" constExpr seqBlock ;
                (* cycle-iterating FSM form, section 1.15b — NOT unrolled *)

onBlock     = "on" ( "rise" | "fall" ) "(" IDENT ")" seqBlock ;
seqBlock    = "{" { seqStmt } "}" ;
seqStmt     = regAssign | seqIf | seqLoop | seqForeach ;
regAssign   = lvalue "<-" expr NEWLINE ;
seqIf       = "if" expr seqBlock [ "else" ( seqIf | seqBlock ) ] ;
seqLoop     = "loop" IDENT ":" constExpr ".." constExpr seqBlock ;
              (* compile-time unrolled; usable inside on blocks, unlike item-level repeat *)
seqForeach  = "foreach" IDENT "in" foreachSource seqBlock ;
              (* sugar over seqLoop, section 1.16 *)

driveStmt   = lvalue "=" expr NEWLINE ;
lvalue      = IDENT [ "[" constExpr [ ":" constExpr ] "]" ] ;

type        = scalarType { "[" constExpr "]" } ;  (* trailing "[N]" wraps in an array type — fn params/locals only *)
scalarType  = "bit"
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
postfix     = primary { "[" expr [ ":" expr ] "]"
                       | "." IDENT [ "(" [ expr { "," expr } ] ")" ] } ;
                                               (* trailing "(args)" after "." IDENT constructs a
                                                  payload-carrying (or tag-only, zero-arg) enum
                                                  value — Enum.Variant(arg1, arg2, …), positional,
                                                  in the variant's declared field order. Requires a
                                                  bare identifier before "." (the enum name); without
                                                  it, "." IDENT is the ordinary field-access form
                                                  (instance output / bundle field). See the
                                                  Enum.Variant read-side in `pattern` above. *)
primary     = literal | IDENT | "(" expr ")" | concat | replication | bundleLiteral
            | arrayLiteral | callExpr | fnCall ;
concat      = "{" expr { "," expr } "}" ;
bundleLiteral = "{" fieldInit { "," fieldInit } [ "," ] "}" ;
fieldInit     = IDENT ":" expr ;
arrayLiteral  = "[" [ expr { "," expr } ] "]" ;   (* fn param/local array value *)
replication = "{" expr "{" expr { "," expr } "}" "}" ;
callExpr    = ( "extend" | "trunc" ) "(" expr "," constExpr ")"
            | ( "signed" | "unsigned" | "abs"
              | "nand" | "nor" | "xnor" ) "(" expr ")"
            | ( "min" | "max" ) "(" expr "," expr ")" ;

literal     = [ "-" ] INT | BIN | HEX | "true" | "false" ;
constExpr   = expr ;   (* must fold to a constant at compile time *)

testDecl    = "test" STRING "for" IDENT "(" [ argList ] ")" testBlock ;
testBlock   = "{" { testStmt } "}" ;
testStmt    = tickStmt | expectStmt | testDrive | testIf | simBlock ;
tickStmt    = "tick" "(" IDENT [ "," constExpr ] ")" NEWLINE ;
expectStmt  = "expect" expr NEWLINE ;
testDrive   = IDENT "=" expr NEWLINE ;          (* drive a module input *)
testIf      = "if" expr testBlock [ "else" ( testIf | testBlock ) ] ;

simBlock    = "sim" "{" [ speedStmt ] { bindStmt } "}" ;
             (* hardware-emulation only — binds test-block ports to emulated
                peripherals for `mimz test --emulate`; see spec/03 for the
                `sim`/`bind`/`speed` keyword rows (provisional Tanglish/Tamil
                spellings) *)
speedStmt   = "speed" ( "hz" | "khz" | "mhz" ) "(" expr ")" NEWLINE ;
             (* desugars to `expr * <multiplier>` (1 / 1_000 / 1_000_000) *)
bindStmt    = "bind" IDENT "->" IDENT "(" [ bindArg { "," bindArg } ] ")" NEWLINE ;
             (* port -> peripheral(config...); peripheral is a name like `led` *)
bindArg     = IDENT ":" ( STRING | IDENT | INT ) ;

fnDecl      = "fn" IDENT "(" [ fnParamList ] ")" "->" type
              "{" { fnStmt } expr "}" ;         (* combinational; no clocks, no regs *)
fnParamList = fnParam { "," fnParam } ;
fnParam     = IDENT ":" type ;
fnStmt      = localLet | fnIf | returnStmt | fnLoop | fnForeach ;
localLet    = "let" IDENT "=" expr NEWLINE ;    (* named intermediate value *)
returnStmt  = "return" expr NEWLINE ;           (* priority-selected result, not a silicon exit *)
fnIf        = "if" expr "{" { fnStmt } "}"
              [ "else" ( fnIf | "{" { fnStmt } "}" ) ] ; (* else OPTIONAL, unlike ifExpr *)
fnLoop      = "loop" IDENT ":" constExpr ".." constExpr "{" { fnStmt } "}" ;
              (* compile-time unrolled; combine with return for first-match search *)
fnForeach   = "foreach" IDENT "in" foreachSource "{" { fnStmt } "}" ;
              (* sugar over fnLoop, section 1.16; elements form resolves against
                 the fn's own array-typed params, no enclosing module *)
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

### Construction semantics: `Enum.Variant(args)`

`Enum.Variant(arg1, arg2, …)` is an expression that builds a tagged-union
value — the write-side counterpart to `match`'s extraction above. Arguments
are **positional**, one per `payloadField`, in the variant's declared
field order (same D2 convention as match bindings); a tag-only variant
(zero payload fields) is constructed `Enum.Variant()`, with an empty
argument list. The emitter and simulator both lower it to the exact
tag+payload concatenation the wire layout above describes: `tag_w` bits
holding the variant's index, then each argument packed MSB-first into the
payload region, then zero-padding for any unused low bits.

```
enum Packet { Ctrl(k: bits[4]), Data(v: bits[8]) }

module M {
  in k: bits[4]
  out y: Packet
  y = Packet.Ctrl(k)          // payload-carrying construction
}

enum State { Idle, Running }

module N {
  out y: State
  y = State.Idle()            // tag-only construction — the "()" is required
}
```

A bare `Enum.Variant` (no trailing `(...)`) without payload fields is still
the pre-existing `Field`-expression form; `Enum.Variant(...)` is a distinct
AST node (`EnumConstruct`) that only exists when a `(...)` argument list
follows.

### Checker rules for tagged enums

| Code  | Triggered when                                                                                                |
| ----- | ------------------------------------------------------------------------------------------------------------- |
| E0806 | Number of bindings in a `match` pattern, or arguments to `Enum.Variant(...)`, ≠ variant's payload field count |
| E0807 | A payload field type is not a concrete bit-vector (`bit`, `bits`, `signed`)                                   |
| E0401 | An `Enum.Variant(...)` argument's width doesn't fit the corresponding payload field's declared width          |
| E0103 | `Enum.Variant(...)` (construction) or a match pattern names a variant the enum doesn't have                   |

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
   errors except through the explicit `sync.double_flop`/`sync.pulse`
   synchronizer primitives (§1.2b).
6. **Combinational cycles:** rejected (wire graph must be a DAG).
7. **Const-ness:** widths, params, reset values, `const`, `repeat` bounds fold
   at compile time.
8. **No signed/unsigned mixing:** conversion only via `signed()`/`unsigned()`.

## 7. Deferred Features (explicitly out of v0.2)

| Feature                                         | Target                                                |
| ----------------------------------------------- | ----------------------------------------------------- |
| `inout`/tristate ports                          | Phase 2                                               |
| Enum-element and 2-D memories (`mem`)           | post-v1 (scalar `bit`/`bits`/`signed` cells ship now) |
| Handshake (req/ack) protocols, async FIFOs      | future work (stdlib, on top of §1.2b)                 |
| Structs/bundles/buses                           | post-Phase 2 (stdlib time)                            |
| `match` ranges (e.g. `0..7`)                    | v0.3+                                                 |
| Division/modulo                                 | never as operators; stdlib divider module (Phase 4)   |
| Wrapping/instantiating external Verilog modules | per Constitution — design in Phase 2+                 |

---

## Changelog

- **v0.2.27 (2026-07-21):** **Clock-domain-crossing synchronizer primitives
  `sync.double_flop`/`sync.pulse`** (new section 1.2b, placed directly after
  `1.2`'s counter example). Reuses the existing `sync` token — disambiguated
  from `sync loop` by the `.` immediately after `sync`, per the dual-purpose
  decision recorded in `spec/03-keywords-trilingual.md`'s v0.2.22 entry — and
  the existing `ExprKind::Call` shape via two new `Builtin` variants, no new
  top-level expression kind. `double_flop(signal, src_clock, dst_clock)` is a
  2-flop synchronizer for a level/control signal; `pulse(signal, src_clock,
dst_clock)` is a toggle-based synchronizer for a single-cycle pulse; both
  reject anything wider than `bit` (E0703). New diagnostics E0702
  (clock-argument shape), E0703 (width), E0704 (domain rule — asymmetric:
  `double_flop` accepts a domain-free or `src_clock`-owned signal, `pulse`
  requires `src_clock`-owned only), E0705 (illegal placement), plus parser
  diagnostic E1116 (unknown `sync.` method name). §6 rule 5 and the §7
  deferred-features table updated accordingly — cross-clock reads are no
  longer blanket-deferred to "Phase 2," only handshake/FIFO multi-bit
  crossing remains so. Full design rationale:
  `docs/superpowers/specs/2026-07-20-sync-cdc-design.local.md`; execution
  plan: `docs/superpowers/plans/2026-07-20-sync-cdc.local.md` (9 tasks,
  `phase-2-correctness-consolidation-part2` branch). `docs/log/2026-07-21.md`.
- **v0.2.25 (2026-07-12):** **`foreach` — array/range sugar over
  `repeat`/`loop`** (new section 1.16, placed directly after `sync loop`'s
  section 1.15b). New section 5 productions `foreachBlock` (mirrors
  `repeatBlock`, `moduleItem` gains it), `seqForeach` (mirrors `seqLoop`,
  `seqStmt` gains it), `fnForeach` (mirrors `fnLoop`, `fnStmt` gains it),
  and the shared `foreachSource` production (a `lo..hi` range, or an
  array/`mem` identifier already in scope). Two source forms: **range**
  (`foreach i in lo..hi`, `in` spelled where `repeat`/`loop` spell `:`)
  and **elements** (`foreach v in arr`, binds each element of `arr` by
  value — the iteration count comes from `arr`'s own declared length,
  never hand-written). Placement mirrors `repeat`/`loop` exactly:
  module-item level desugars toward `repeat`, `on`-block/`fn`-body
  desugars toward bare `loop` — always elaboration-time, never a hidden
  `sync loop`. Elements form on a source that doesn't resolve to an
  array/`mem` type is E0417. `docs/log/2026-07-12.md`.
- **v0.2.24 (2026-07-11):** **Bundle-typed `fn` argument/return shape
  checking.** E0901 (section "Bundle checker rules") widened from
  bundle-literal-only to also cover a bundle-typed function call argument or
  `return` value whose shape doesn't match the parameter/return type —
  closing the gap where a bundle passed through a `fn` boundary skipped
  shape checking entirely. Backed by a new `Ty::Bundle` variant in the
  checker's width pass (consolidating the prior `Wcx::bundle_sigs`
  side-table). No grammar change — checker-only. `docs/log/2026-07-11.md`.
- **v0.2.23 (2026-07-06):** **Cycle-iterating loop `sync loop`/`sync suzhal`**
  (new section 1.15b, placed directly after `loop`'s section 1.15). New
  section 5 production `syncLoopBlock` (`moduleItem` gains it): a module-body
  item, distinct from statement-level `loop`, that elaborates to one small
  FSM instead of unrolled hardware. Generates four signals off its name
  (`<name>_start` in; `<name>_done`/`<name>_result`/`<name>_running` out); a
  `<name>_start` pulse produces `<name>_done` exactly `hi - lo + 1` clock
  edges later, and a held-high `<name>_start` does not re-trigger mid-run.
  Costs **time, not area** — the direct opposite trade of bare `loop`'s
  **area, not time** (both honesty callouts now stated side by side).
  Documented a genuine semantic difference from `loop`+`return`: an
  unguarded `sync loop` body is LAST-match-wins, not first-match-wins,
  since it has no early-exit primitive and simply overwrites the
  accumulator on every matching cycle — a user-written "already found"
  latch is required for first-match-wins. Removed the stale "future
  `sync loop`" forward-reference in section 1.15 and the corresponding
  Deferred Features row (section 7) now that the construct has shipped.
  Header version note corrected from a stale v0.2.17 to track the
  changelog (last corrected at v0.2.6; drifted again since). Covered by the
  `sync_loop_search` five-flavor example and its Icarus differential
  (`tests/icarus.rs`).
- **v0.2.22 (2026-07-05):** **Bounded compile-time loop `loop`/`suzhal`/`சுழல்`**
  (new section 1.15). `loop i: lo..hi { ... }` is `repeat`'s compile-time
  unrolling made available as a _statement_ — legal inside an `on` block
  (`seqStmt` gains `seqLoop`) or a `fn` body (`fnStmt` gains `fnLoop`), unlike
  `repeat`, which stays item-level only. Unrolls into `hi-lo` copies of its
  body at elaboration time; combined with `return` (section 1.13) gives
  first-match-wins linear search over an array/`mem`. The bound must fold to
  a compile-time constant, same rule as `repeat`'s `hi`. **Costs area, not
  time**: N iterations is N× the hardware, fully unrolled, zero extra clock
  cycles — a future, differently-spelled `sync loop`/`sync suzhal` is
  reserved for the cycle-iterating form (Deferred Features table, section 7).
  Additive — no grammar breakage. Covered by the `fn_array_search` five-flavor
  example (kernel == VCD == Icarus, including a duplicate-match case proving
  first-match priority).
- **v0.2.21 (2026-07-04):** **Array-typed `fn` parameters** (section 1.14).
  `<scalarType>[N]` trailing suffix (grammar: `type = scalarType { "[" N
"]" }`) declares a fixed-size, immutable array — `fn` parameters and
  `let` locals only (module-level ports/wires/registers reject it,
  `E0416`). Array literals `[e1, ..., eN]` (`arrayLiteral` in section 5)
  construct values at call sites. Element type restricted to
  `bit`/`bits[N]`/`signed[N]` (`E0411`); length must be a positive
  compile-time constant (`E0412`); array-literal elements must share one
  width/signedness (`E0414`) and a call argument's literal length must
  match the parameter's declared length (`E0413`). Indexing reuses the
  existing postfix `[expr]` syntax: a compile-time-constant index folds
  directly to that element (no hardware for the index itself); a runtime
  index generates a priority-mux over every element (`E0415` for a
  provably-out-of-range constant index). An array is never real Verilog
  hardware — it always elaborates to N independent scalar signals
  (`<name>_0` .. `<name>_{N-1}`), matching how `repeat` already elaborates
  to N copies of hardware. Additive — no existing `.mimz` file changes
  behavior.
- **v0.2.20 (2026-07-04):** **`return` + statement-based `fn` bodies**
  (section 1.13). Added section 5 productions `fnStmt`, `returnStmt`,
  `fnIf` — `fnDecl` bodies are now `{ fnStmt } expr`, replacing the old
  `{ localLet } expr`. `fnStmt` is `let` / statement-level `if` (`else`
  OPTIONAL, unlike expression-level `if`) / `return`. `return expr`
  immediately yields the function's result; the tail expression stays
  mandatory (no "missing return" diagnostic). New E0812 (unreachable code
  after an unconditional `return` in the same block). `return` is
  priority-selected assignment, not control flow — every branch is still
  fully instantiated and evaluated every time; no cycles, area, or work
  are skipped. New keyword `return` (spec/03 v0.2.20). Additive — every
  pre-existing `fn` (locals + tail expr only) parses unchanged.
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
