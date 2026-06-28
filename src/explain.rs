//! `mimz explain <CODE>` — the classroom version of every diagnostic.
//!
//! The one-line `help:` on a [`crate::diag::Diag`] says HOW to fix the error
//! in the moment (spec/01 G1). This module is the long form: WHAT the rule is,
//! WHY silicon needs it, the corrected shape, and — where it earns its place —
//! a small honest ASCII diagram. It is the incremental build-out of idea 8.1
//! (`docs/Ideas/language_plan.md` section 9): didactic, Elm-style errors.
//!
//! Lib-backed on purpose: the CLI (`mimz explain`), editors over the LSP, and
//! the future WASM playground all read the same text. Keyed off the stable
//! E-codes (catalogs: docs/code/11-checker.md for E0xxx, docs/code/06 for the
//! lexer E10xx / parser E11xx / loader E12xx codes) — codes are never
//! renumbered, so these entries are append-only too.

/// Every `(code, long-form text)` pair, ordered by code. The single source —
/// [`explain`] searches it and [`codes`] derives the list. A unit test pins
/// every [`crate::diag::ALL_CHECKER_CODES`] entry to a row here so a new
/// checker code cannot ship without its explanation.
const EXPLANATIONS: &[(&str, &str)] = &[
    // ----- E00xx: structure & duplicate names -----
    (
        "E0001",
        "E0001 — duplicate module name (project-wide)\n\n\
         Two modules share one name across all the files the compiler loaded.\n\
         Module names are the global vocabulary of a design — an instance like\n\
         `let u = Counter()` has to resolve to exactly one definition — so they\n\
         must be unique across the whole project, not just per file.\n\n\
         Fix: rename one of them. Names travel with `import`, so a name that\n\
         looks free in this file may already be taken by an imported one.",
    ),
    (
        "E0002",
        "E0002 — duplicate file-level enum name (project-wide)\n\n\
         Two `enum` types share a name across the loaded files. Enums travel\n\
         with `import` just like modules, so their names are also project-wide.\n\n\
         Fix: rename one enum.",
    ),
    (
        "E0003",
        "E0003 — name declared twice inside one module\n\n\
         A port, wire, reg, const, or instance reuses a name already taken in\n\
         the same module. Inside a module every declared name is one physical\n\
         thing (a net or a piece of state); two declarations would be two\n\
         things fighting over one name.\n\n\
         Fix: rename one. The message names what already holds the name.",
    ),
    (
        "E0004",
        "E0004 — duplicate file-level `const`\n\n\
         Two `const` declarations in one file share a name. A const is a single\n\
         compile-time value; defining it twice is ambiguous.\n\n\
         Fix: rename one, or delete the redundant declaration.",
    ),
    // ----- E01xx: name resolution -----
    (
        "E0101",
        "E0101 — unknown name in an expression\n\n\
         You used a name nothing declares in scope. There is no implicit\n\
         declaration in Min-Mozhi — every wire, reg, port, const, and parameter\n\
         is spelled out, so a name with no declaration is almost always a typo\n\
         or a missing one.\n\n\
         Fix: check the spelling, or declare it (`wire x: bits[8]`, `const N = 4`, …).",
    ),
    (
        "E0102",
        "E0102 — unknown module\n\n\
         An instantiation or a `test` header names a module the project does not\n\
         define or import.\n\n\
         Fix: check the spelling, or add the missing `import \"path.mimz\"` so the\n\
         module is in scope.",
    ),
    (
        "E0103",
        "E0103 — unknown enum, variant, or named type\n\n\
         You referenced an enum, one of its variants, or a type name that does\n\
         not exist.\n\n\
         Fix: use one of the real variants — the message lists them.",
    ),
    (
        "E0104",
        "E0104 — reading a non-output of an instance\n\n\
         `inst.x` reads port `x` of a sub-module, but `x` is not an OUTPUT of\n\
         that module. Only outputs flow out of an instance; inputs are driven\n\
         IN, at the `let` that creates the instance.\n\n\
         Fix: read one of the module's outputs (the message lists them), and\n\
         connect inputs at instantiation: `let u = Sub(in_port: signal)`.",
    ),
    (
        "E0105",
        "E0105 — `.field` on something with no fields\n\n\
         The `.` operator is only for `Enum.Variant` and `instance.output`. You\n\
         applied it to a plain signal or value, which has no fields.\n\n\
         Fix: drop the `.field`, or operate on the value directly.",
    ),
    (
        "E0106",
        "E0106 — unknown parameter in instantiation or test header\n\n\
         You passed a parameter name the module does not declare.\n\n\
         Fix: use one of the module's real parameters — the message lists them.",
    ),
    (
        "E0107",
        "E0107 — bad connection port (unknown, or an output)\n\n\
         A connection at instantiation names a port that does not exist, or names\n\
         an OUTPUT. Outputs are not connected in — they are read back out with\n\
         `.` after the instance exists.\n\n\
         Fix: connect only inputs; read outputs with `inst.out`.",
    ),
    (
        "E0108",
        "E0108 — assigning to a non-signal\n\n\
         You drove something that cannot be driven — an input port, a const, a\n\
         clock, or a reset. Only `out` ports, `wire`s, and `reg`s carry a value\n\
         your logic produces.\n\n\
         Fix: assign to an out/wire/reg. Inputs arrive from outside; consts are\n\
         fixed at compile time.",
    ),
    (
        "E0109",
        "E0109 — `on rise(x)` where `x` is not a clock\n\n\
         An `on rise(...)` block must be triggered by a declared `clock`. A\n\
         clock is special hardware (it has a clock tree, timing constraints);\n\
         an ordinary wire is not one.\n\n\
         Fix: declare `clock clk` and trigger on it: `on rise(clk) { ... }`.",
    ),
    // ----- E02xx: const evaluation -----
    (
        "E0201",
        "E0201 — expression is not a compile-time constant\n\n\
         A position that the compiler must know before any hardware exists — a\n\
         width `bits[N]`, a `repeat` bound, a parameter default — used something\n\
         that only has a value at runtime (a signal), or an operator with no\n\
         compile-time meaning (`+%` needs a bit width; `match`, concat, index,\n\
         slice, and builtins are runtime).\n\n\
         Allowed here: literals, `const`s, parameters, `repeat` variables, and\n\
         `+ - *`, shifts, comparisons, `&& || !`, `if/else` over those.\n\n\
         Fix: make the value a `const`/parameter, or move the runtime work into\n\
         the body where signals live.",
    ),
    (
        "E0202",
        "E0202 — constant evaluation overflow\n\n\
         A compile-time calculation exceeded the evaluator's i128 range. The\n\
         compiler refuses to wrap silently — that would be the exact dishonesty\n\
         the language forbids at runtime.\n\n\
         Fix: use smaller constants, or restructure the expression.",
    ),
    // ----- E03xx: module structure rules -----
    (
        "E0301",
        "E0301 — module has regs but no `reset`\n\n\
         A module declares state (`reg`) but no `reset`. Hardware powers on in\n\
         an unknown state; without a reset path that state is indeterminate at\n\
         time zero and in simulation. Min-Mozhi is 2-state by design, so a reset\n\
         is mandatory — there is no 'X' to fall back on.\n\n\
         Fix: add `reset rst` to the module; every reg's declared init value\n\
         becomes its reset value.",
    ),
    (
        "E0302",
        "E0302 — instance input unconnected, or connected twice\n\n\
         Every input of a sub-module is a physical wire that must be driven by\n\
         exactly one thing — no more (two drivers = a short), no less (an\n\
         undriven input floats).\n\n\
         Fix: connect each input exactly once at the `let`. Clock and reset\n\
         connect implicitly by name and need not be listed.",
    ),
    (
        "E0303",
        "E0303 — declaration inside `repeat`\n\n\
         A `repeat` block only STAMPS OUT hardware that already exists; it is not\n\
         a scope that can declare new ports, wires, regs, clocks, resets, consts,\n\
         enums, or `on` blocks. Declaring inside it would mean N copies of one\n\
         named thing.\n\n\
         Fix: declare the wire/reg once OUTSIDE the loop (size it with the loop\n\
         bound, e.g. `wire acc: bits[N]`), and drive its bits inside.",
    ),
    // ----- E04xx: width & type rules -----
    (
        "E0401",
        "E0401 — assignment / connection width mismatch\n\n\
         The two sides of an `=`, `<-`, init, or port connection are different\n\
         widths. Bits are physical wires; you cannot silently pour 9 bits into 8\n\
         (Verilog would truncate and you'd lose the top bit — a classic bug).\n\n\
         `a + b` of two 8-bit values is 9 bits wide (it can carry). If you MEANT\n\
         to keep 8 bits and wrap, that is `+%`. Otherwise widen the destination.\n\n\
         Fix: `extend(x, N)` / `trunc(x, N)` / slice `x[hi:lo]` to match, or use\n\
         `+%` for deliberate wrapping arithmetic.",
    ),
    (
        "E0402",
        "E0402 — operand width mismatch\n\n\
         A binary operator (`+%` family, `& | ^`, comparisons) got two operands\n\
         of different widths. These operate bit-by-bit (or compare equal widths),\n\
         so the sides must already be the same size — there is no implicit pad.\n\n\
         Fix: `extend` the narrow side to the wide side's width first.",
    ),
    (
        "E0403",
        "E0403 — kind mixing\n\n\
         You mixed incompatible kinds: a `signed` value with a `bits` value,\n\
         an enum used as a number, or a clock/reset used as data. Each kind has\n\
         different hardware meaning, so the compiler will not silently coerce.\n\n\
         Fix: convert explicitly — `signed(x)` / `unsigned(x)` — so the\n\
         reinterpretation is visible in the source.",
    ),
    (
        "E0404",
        "E0404 — logical op / condition on a non-`bit`\n\n\
         `&&`, `||`, `!`, and `if`/`match` conditions need a single `bit`, but\n\
         you gave a multi-bit value. A condition is one yes/no wire.\n\n\
         Fix: compare it (`x != 0`) or reduce it (`|x` is 1 if any bit is set)\n\
         to get a `bit`.",
    ),
    (
        "E0405",
        "E0405 — compile-time value does not fit\n\n\
         A literal/const is too large for the width it lands in, or it has no\n\
         sized context to adopt. Compile-time integers are polymorphic — `1`\n\
         takes the width of what it is added to — but only if they FIT.\n\n\
         Fix: widen the target, or use a value that fits. The message shows the\n\
         value, the width, and the maximum that fits.",
    ),
    (
        "E0406",
        "E0406 — index / slice out of range\n\n\
         An index is past the end, a slice's bounds are reversed, or the base is\n\
         not indexable. Slices are written msb-first: `bus[hi:lo]` with hi >= lo.\n\n\
         Fix: keep indices in `0..=N-1`; write slices high bit first, `bus[7:0]`.",
    ),
    (
        "E0407",
        "E0407 — builtin / unary misuse\n\n\
         A builtin or unary operator was used against its purpose — e.g.\n\
         `extend` to a NARROWER width (that loses bits — use `trunc`), or unary\n\
         `-` on unsigned `bits` (which have no sign).\n\n\
         Fix: use the right tool. To wrap-negate an unsigned value, `0 -% x`.",
    ),
    (
        "E0408",
        "E0408 — `if`/`match` arms disagree on type or width\n\n\
         Every arm of a value-producing `if`/`match` becomes the SAME output\n\
         wire, so they must all yield the same type and width — a multiplexer\n\
         has one output bus.\n\n\
         Fix: make every arm the same width/type (extend or convert as needed).",
    ),
    (
        "E0409",
        "E0409 — pattern error\n\n\
         A `match` pattern does not fit its scrutinee: matching on a signed\n\
         value, naming the wrong enum, or a value too wide for the type.\n\n\
         Fix: match patterns the scrutinee's type admits — the message says what\n\
         that is.",
    ),
    (
        "E0410",
        "E0410 — invalid width expression\n\n\
         A width evaluated to zero, negative, or otherwise absurd. Every wire is\n\
         at least one physical bit.\n\n\
         Fix: ensure the width expression is >= 1.",
    ),
    // ----- E05xx: drivers & cycles -----
    (
        "E0501",
        "E0501 — more than one driver\n\n\
         A signal is driven in two places, or by overlapping bit ranges. In\n\
         hardware two outputs wired together fight — one pulls high, one low —\n\
         and you get a short circuit, not a value:\n\n\
         \x20   driver A ──┐\n\
         \x20             ├── wire   ← who wins? neither. short.\n\
         \x20   driver B ──┘\n\n\
         A wire takes its value from exactly ONE source. To choose between\n\
         sources, multiplex: `y = if (sel) a else b` (one driver, a mux).\n\n\
         Fix: one `=` per signal. Driving different BIT RANGES of one bus is\n\
         fine as long as they are disjoint (`y[3:0] = ...`, `y[7:4] = ...`).",
    ),
    (
        "E0502",
        "E0502 — output never driven (or only partly)\n\n\
         An `out`/wire is read but some of its bits are never assigned. An\n\
         undriven wire is electrically floating — the hardware equivalent of a\n\
         null pointer, with an undefined voltage.\n\n\
         Fix: drive every bit. The message names the first undriven bit; cover it\n\
         with an assignment or a default.",
    ),
    (
        "E0503",
        "E0503 — reg assigned from zero or several `on` blocks\n\n\
         A register's next-state must come from exactly one clocked process.\n\
         Two `on` blocks writing one reg are two flip-flops fighting over one\n\
         piece of state.\n\n\
         Fix: let exactly one `on` block own each reg; merge the logic if needed.",
    ),
    (
        "E0504",
        "E0504 — combinational cycle\n\n\
         A signal feeds back into itself through pure logic, with no register in\n\
         the loop:\n\n\
         \x20   a = b & c\n\
         \x20   b = a | d     ← a depends on b depends on a, instantly\n\n\
         With no flip-flop to break it, the loop has no settled value — it\n\
         oscillates or latches unpredictably.\n\n\
         Fix: route the feedback through a `reg` in an `on rise(clk)` block, so\n\
         each pass is one clock cycle apart. The message prints the cycle path.",
    ),
    (
        "E0505",
        "E0505 — wrong assignment kind\n\n\
         `=` and `<-` are different hardware. `=` is a combinational wire (its\n\
         value follows its inputs instantly). `<-` is a register update inside\n\
         an `on` block (the value lands on the next clock edge). You used the\n\
         wrong one.\n\n\
         Fix: `<-` for regs inside `on rise(clk)`; `=` for wires/outs in\n\
         combinational logic.",
    ),
    // ----- E06xx: exhaustiveness -----
    (
        "E0601",
        "E0601 — `match` not exhaustive\n\n\
         A `match` does not cover every possible value of what it matches. In\n\
         hardware an uncovered case has no defined output — and a bit flip from\n\
         radiation or noise can push a state machine into exactly those\n\
         uncovered states, where it can latch forever.\n\n\
         Fix: add the missing arms (the message names them), or end with a\n\
         catch-all `_ =>` — which for an FSM is also your safe recovery state.",
    ),
    (
        "E0602",
        "E0602 — unreachable `match` arm\n\n\
         An arm can never be selected — it sits after a `_` catch-all, or it\n\
         duplicates an earlier value. Dead arms hide bugs.\n\n\
         Fix: move `_ =>` to the end, or delete the duplicate arm.",
    ),
    // ----- E07xx: clock domains -----
    (
        "E0701",
        "E0701 — cross-clock-domain read\n\n\
         A signal clocked by one clock is read directly in another clock's\n\
         domain (or one wire mixes two domains). Sampling a signal that can\n\
         change at an unrelated clock edge causes metastability — the flip-flop\n\
         catches it mid-transition and its output is briefly undefined. This is\n\
         the hardware data race.\n\n\
         Fix: keep one clock domain per signal. A proper synchronizer for\n\
         crossing domains arrives with `sync` in Phase 2.",
    ),
    // ----- E08xx: user-defined functions -----
    (
        "E0801",
        "E0801 — duplicate function name (project-wide)\n\n\
         Two `fn` declarations share the same name across all the files the\n\
         compiler loaded. Function names are project-wide — any module can call\n\
         any function, so the name must resolve to exactly one definition.\n\n\
         Fix: rename one of them. Only the second declaration triggers this error;\n\
         the first is kept.",
    ),
    (
        "E0802",
        "E0802 — function name shadows a builtin\n\n\
         The chosen name is reserved for a language builtin (`extend`, `trunc`,\n\
         `signed`, `unsigned`, `min`, `max`, `abs`, `nand`, `nor`, `xnor`,\n\
         `clog2`). Builtins are wired into the parser and type-checker; a\n\
         user-defined function with the same name would create an ambiguity that\n\
         the compiler cannot resolve.\n\n\
         Fix: choose a different name for your function.",
    ),
    (
        "E0803",
        "E0803 — wrong number of arguments in function call\n\n\
         A call to a user-defined `fn` passes a different number of arguments\n\
         than the function declares parameters. Because `fn` bodies are\n\
         combinational and purely structural, every parameter must be bound at\n\
         the call site — the compiler cannot infer or default missing arguments.\n\n\
         Fix: pass exactly the number of arguments the function declares.\n\
         The error message names both the expected and received counts.",
    ),
    (
        "E0804",
        "E0804 — function body width does not match the declared return type\n\n\
         The final expression in a `fn` body has a different width or kind than\n\
         the function's `->` return type. A `fn` is purely combinational — its\n\
         body is a single expression whose bits flow directly to the caller.\n\
         There is no silent truncation or zero-extension: the widths must agree\n\
         exactly, the same rule that governs every wire and port in the language.\n\n\
         Fix: resize the body expression with `extend`, `trunc`, or a slice to\n\
         match the declared return type, or change the declared return type to\n\
         match the expression the body actually produces.\n\n\
         help: the body width and return type printed in the error are the two\n\
         values that must agree — pick one to change.",
    ),
    (
        "E0805",
        "E0805 — recursive function call\n\n\
         A `fn` calls itself, directly or through a chain of other `fn`s. Because\n\
         `fn` bodies are purely combinational and inlined at every call site, a\n\
         recursive call would require infinite hardware unrolling — the synthesis\n\
         tool has no way to stop.\n\n\
         Fix: replace the recursion with a fixed-size computation. Use a\n\
         parameterized module with `repeat` for structural replication, or\n\
         restructure the algorithm so every function produces its result in a\n\
         bounded number of steps without calling itself.\n\n\
         help: the reported function closes the recursive cycle — starting from it,\n\
         trace its call chain to find all members. Redesign so no function\n\
         appears in any other's call chain.",
    ),
    (
        "E0806",
        "E0806 — wrong number of bindings in a tagged enum pattern\n\n\
         A match arm for a tagged enum variant provides a different number of\n\
         binding names than the variant's declared payload fields. Because\n\
         bindings are positional (design decision D2), every payload field must\n\
         be matched exactly once — no more, no fewer.\n\n\
         Fix: provide exactly as many binding names as the variant has payload\n\
         fields. If a field's value is not needed in the arm body, use `_` as\n\
         the binding name to make the intent explicit.\n\n\
         Tag-only variants (no payload) never take a binding list — omit the\n\
         `(...)` entirely.",
    ),
    (
        "E0807",
        "E0807 — payload field type is not a concrete bit-vector\n\n\
         A tagged enum variant declares a payload field whose type is another\n\
         enum or a memory. Payload fields must be concrete bit-vector types:\n\
         `bit`, `bits[N]`, or `signed[N]`. Nested enums and memories have no\n\
         fixed bit-vector encoding that the compiler can place in the union's\n\
         payload slot.\n\n\
         Fix: convert the field to its bit-vector encoding manually. For an\n\
         enum `S`, declare the field as `bits[clog2(N)]` where `N` is the\n\
         number of variants and encode/decode with match expressions. For a\n\
         memory, store the address or a serialised snapshot instead.",
    ),
    // ----- E10xx: lexer -----
    (
        "E1001",
        "E1001 — unterminated block comment\n\n\
         A `/* ... */` comment opened but never closed before end of file.\n\n\
         Fix: add the closing `*/`.",
    ),
    (
        "E1002",
        "E1002 — unterminated string\n\n\
         A string literal opened but the line/file ended before its closing\n\
         quote. (Strings appear only in `test` names today.)\n\n\
         Fix: close the string with `\"`.",
    ),
    (
        "E1003",
        "E1003 — Tamil digits in a literal\n\n\
         Number literals use ASCII digits (`0-9`), which are universal across\n\
         every tool and textbook. Keyword and identifier SPELLINGS can be Tamil;\n\
         the digits inside a number stay ASCII.\n\n\
         Fix: write the number with ASCII digits.",
    ),
    (
        "E1004",
        "E1004 — malformed number\n\n\
         A numeric literal is not well formed — e.g. `0x` with no hex digits, or\n\
         a stray separator.\n\n\
         Fix: write a complete literal: `0b1010`, `0xFF`, `42`, `1_000`.",
    ),
    (
        "E1005",
        "E1005 — reserved word used as a name\n\n\
         You used a word the language reserves for a future feature (e.g.\n\
         `struct`, `sync`, `fixed`, `requires`). It is not a keyword yet, but it\n\
         is not free to use as an identifier either — reserving it now means\n\
         today's code will not break when the feature lands.\n\n\
         Fix: pick a different name.",
    ),
    (
        "E1006",
        "E1006 — division `/` does not exist\n\n\
         There is no `/` operator. A general divider is large, slow hardware\n\
         (many gates, many cycles) — hiding it behind a one-character operator\n\
         would teach the wrong instinct about cost.\n\n\
         Fix: divide/multiply by powers of two with shifts (`x >> 1`); for true\n\
         division, instantiate a divider module so its cost is visible.",
    ),
    (
        "E1007",
        "E1007 — modulo `%` does not exist\n\n\
         There is no `%` operator, for the same reason as `/`: it implies a\n\
         divider.\n\n\
         Fix: `x % 2^k` is the low k bits — `x[k-1:0]`. For other moduli, build\n\
         it explicitly.",
    ),
    (
        "E1008",
        "E1008 — unexpected character\n\n\
         A character that is not part of any token appeared in the source.\n\n\
         Fix: remove it. If you meant an operator, check spec/02 for the real\n\
         spelling.",
    ),
    // ----- E11xx: parser -----
    (
        "E1101",
        "E1101 — expected one thing, found another\n\n\
         The parser was partway through a construct and the next token was not\n\
         what the grammar allows there (this also covers missing statement\n\
         terminators and a missing `}`). The message says what was expected.\n\n\
         Fix: supply the expected token; check for a missing newline, comma, or\n\
         closing brace just before this point.",
    ),
    (
        "E1102",
        "E1102 — bad top-level item\n\n\
         Only `module`, `enum`, `const`, `import`, and `test` may appear at the\n\
         top level of a file.\n\n\
         Fix: move the construct inside a module, or correct the keyword.",
    ),
    (
        "E1103",
        "E1103 — enum needs at least one variant\n\n\
         An `enum` declared no variants. An empty enum encodes nothing.\n\n\
         Fix: add variants: `enum State { IDLE, RUN }`.",
    ),
    (
        "E1104",
        "E1104 — register has no reset value\n\n\
         A `reg` was declared without an initial value. State must power on\n\
         known (see E0301) — the init value IS the reset value.\n\n\
         Fix: give it one: `reg count: bits[8] = 0`.",
    ),
    (
        "E1105",
        "E1105 — `<-` outside an `on` block\n\n\
         `<-` is a register update and only has meaning inside `on rise(clk)`,\n\
         where 'next clock edge' is defined.\n\n\
         Fix: use `=` for combinational wires, or move the `<-` into an `on` block.",
    ),
    (
        "E1106",
        "E1106 — `=` inside an `on` block\n\n\
         Inside a clocked `on` block, state updates use `<-` (lands next edge).\n\
         A plain `=` there is the wrong kind.\n\n\
         Fix: use `<-` for the reg; pure combinational `=` goes outside the block.",
    ),
    (
        "E1107",
        "E1107 — `test` block syntax\n\n\
         A `test` block's header or body is malformed (it needs a string name\n\
         and statement body).\n\n\
         Fix: `test \"name\" { ... }`; see spec/02 section 1.10.",
    ),
    (
        "E1108",
        "E1108 — value-driving `if` without `else`\n\n\
         An `if` used as a VALUE (`y = if (c) a`) must have an `else`. Without\n\
         one, what drives `y` when the condition is false? Nothing — so the\n\
         tool would infer a LATCH to 'remember' the old value. Accidental\n\
         latches are a top hardware bug.\n\n\
         Fix: give every value-`if` an `else`: `y = if (c) a else b`.",
    ),
    (
        "E1109",
        "E1109 — chained comparison\n\n\
         Comparison chains are restricted. A monotonic one-direction chain\n\
         (`0 <= x <= 100`, all `<`/`<=` or all `>`/`>=`) is allowed and desugars\n\
         to `&&` of the pairs. A mixed-direction chain (`a < b > c`) or any chain\n\
         mixing in `==`/`!=` is rejected — it reads ambiguously.\n\n\
         Fix: keep the chain one-direction, or split it with explicit `&&`.",
    ),
    (
        "E1110",
        "E1110 — call error\n\n\
         A call names something that is not a builtin, or a builtin with the\n\
         wrong number of arguments. There are no user-defined functions yet —\n\
         only the builtins (`extend`, `trunc`, `signed`, `unsigned`).\n\n\
         Fix: use a builtin with its correct arity, or instantiate a module.",
    ),
    (
        "E1111",
        "E1111 — parameter/const type is not `int`/`bool`\n\n\
         Parameters and consts are compile-time numbers or booleans, not\n\
         hardware types.\n\n\
         Fix: give the parameter an `int`/`bool` type (or none and let it infer).",
    ),
    // ----- E12xx: loader -----
    (
        "E1201",
        "E1201 — imported file does not exist\n\n\
         An `import \"path\"` points at a file the loader cannot find. Paths are\n\
         resolved relative to the importing file.\n\n\
         Fix: correct the path, or create the file.",
    ),
    (
        "E1202",
        "E1202 — bad standard-library import\n\n\
         An `import std.<module>` names the embedded standard library but does\n\
         not resolve: either it has the wrong shape (a std import is exactly two\n\
         segments — one namespace, one module) or `<module>` is not a real std\n\
         module. The namespace is trilingual — `std` / `nuulagam` / `நூலகம்` —\n\
         and the module is the English stem (`fifo`) or its pure-Tamil twin name\n\
         (`வரிசை` / `varisai`).\n\n\
         Fix: write `import std.<module>` with one of the available modules — the\n\
         message lists them. To customize a module, `mimz eject std` and point\n\
         `mimz.toml [lib] std` at the directory (spec/02 section 1.5).",
    ),
    // ----- Wxxxx: lint warnings -----
    (
        "W0002",
        "W0002 — signal name should be snake_case\n\n\
         A port, wire, reg, instance, clock, or reset name is not snake_case.\n\
         The project convention is `snake_case` for signal-level names —\n\
         lowercase letters, digits, and underscores — which is the de-facto\n\
         standard in hardware designs and the style the emitter uses for\n\
         Verilog identifiers.\n\n\
         Fix: rename to snake_case: `my_signal`, `data_bus_0`, `clk_50mhz`.",
    ),
    (
        "W0003",
        "W0003 — module name should be PascalCase\n\n\
         A module name is not PascalCase. Modules are types (roughly), so they\n\
         follow the type naming convention: uppercase first letter, no underscores.\n\
         This matches Verilog module conventions and Rust's type naming rule.\n\n\
         Fix: rename to PascalCase: `MyModule`, `Adder8`, `RiscVCore`.",
    ),
    (
        "W0004",
        "W0004 — signal declared but never used\n\n\
         A wire, reg, instance, or constant is declared inside a module but is\n\
         never read (appears on the RHS of no assignment, is mentioned in no\n\
         expression). An unused declaration is dead code — it wastes hardware\n\
         (or is a bug: you meant to use it but forgot).\n\n\
         Fix: remove the unused declaration, or prefix it with `_`\n\
         (e.g. `_unused_wire`) to signal the intent explicitly.",
    ),
];

/// Long-form explanation for a diagnostic `code` (e.g. `"E0501"`), or `None`
/// if the code is unknown. Matching is case-insensitive and trims whitespace,
/// so `e0501` and ` E0501 ` both resolve.
pub fn explain(code: &str) -> Option<&'static str> {
    let code = code.trim().to_ascii_uppercase();
    EXPLANATIONS
        .iter()
        .find(|(k, _)| *k == code)
        .map(|(_, text)| *text)
}

/// Every code that has an explanation, in catalog order — drives the
/// "unknown code" CLI message and `mimz explain` with no argument.
pub fn codes() -> impl Iterator<Item = &'static str> {
    EXPLANATIONS.iter().map(|(k, _)| *k)
}

/// List every code with its one-line summary — drives `mimz explain --list`.
/// Each entry is `(code, first_line_of_explanation)`.
pub fn list_all() -> impl Iterator<Item = (&'static str, &'static str)> {
    EXPLANATIONS.iter().map(|(code, text)| {
        let first_line = text.split('\n').next().unwrap_or("");
        (*code, first_line)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::ALL_CHECKER_CODES;

    /// The 8.1 contract: every stable checker code has long-form text, so a new
    /// checker code cannot ship without its teaching explanation (the same
    /// docs-sync spirit as the error-fixture coverage guard in tests/errors.rs).
    #[test]
    fn every_checker_code_has_an_explanation() {
        let missing: Vec<&str> = ALL_CHECKER_CODES
            .iter()
            .copied()
            .filter(|c| explain(c).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "these checker codes have no `mimz explain` entry: {missing:?} — add a row to EXPLANATIONS"
        );
    }

    /// The table is ordered and free of duplicate codes (it is the source for
    /// `codes()`), and every entry's text starts with its own code so the
    /// printed explanation is self-identifying.
    #[test]
    fn table_is_sorted_unique_and_self_labelled() {
        let codes: Vec<&str> = codes().collect();
        for w in codes.windows(2) {
            assert!(w[0] < w[1], "EXPLANATIONS not sorted/unique at {:?}", w);
        }
        for (code, text) in EXPLANATIONS {
            assert!(
                text.starts_with(code),
                "explanation for {code} should open with the code, got: {:?}",
                &text[..text.len().min(12)]
            );
        }
    }

    #[test]
    fn lookup_is_case_insensitive_and_trims() {
        assert!(explain("e0501").is_some());
        assert!(explain("  E0501 ").is_some());
        assert!(explain("E9999").is_none());
    }
}
