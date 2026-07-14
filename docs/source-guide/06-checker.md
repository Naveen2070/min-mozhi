# 6 — The Checker: Seven Safety Passes (13 Files)

The checker runs seven passes over the AST to catch hardware bugs **before** they get to silicon. Every error has a stable E-code and a teaching help message.

## `crates/mimz-core/src/checker/mod.rs` — The Entry

**`Checker` struct** holds all the state for all seven passes: the diagnostics list, module and enum maps, and the constant-evaluation environment.

**`check(files)`** runs all seven passes in order:

1. `collect_symbols()` — build module/enum maps
2. `collect_consts()` — evaluate file-level constants
3. `check_all()` — per-module name resolution
4. `widths::check_module()` — type and width checking
5. `drivers::check_module()` — single-driver and cycle rules
6. `funcs::check_functions()` — combinational function checking (E0801–E0808)
7. `clocks::check_module()` — clock domain ownership

### Pass 1: `symbols.rs` — Who's Who?

**`collect_symbols()`** scans every file for module and enum declarations and builds project-wide maps by name. Checks for duplicates: E0001 (two modules with the same name), E0002 (two enums with the same name).

### Pass 2: `consteval.rs` — What's the Value?

**`collect_consts()`** evaluates file-level `const` declarations top-to-bottom. It uses checked arithmetic — an overflow is E0202, never a silent wrap. Some operators don't work at compile time (`+%` needs a bit width) → E0201.

The results are available as `self.const_eval()` for later passes and the Verilog emitter.

### Pass 3: `names.rs` — Does Everything Refer to Something Real?

**`check_all()`** goes module by module and checks:

- **E0003** — duplicate names within a module
- **E0301** — every register must have a reset value
- **E0101, E0102, E0103** — every name used in an expression must refer to a real declaration
- **E0302** — every input of an instantiated module is connected exactly once
- **E0104** — reading `inst.port` where `port` is an output (not an input)
- **E0109** — `on rise(x)` — `x` must be a clock
- **E0303** — `repeat` bodies contain only hardware generation (drives, instances), not declarations

### Pass 4: `widths/` — Are the Bits Right? (5 Files)

This is the most complex pass, split across `mod.rs` (the `Ty`/`Wcx` types and
top-level dispatch), `expr.rs`, `patterns.rs`, `ops.rs` (operator/concat/
builtin typing: the lossless `+`/`-`/`*` growth rules, the width-matching
family `+%`/bitwise/comparisons, shifts, `{...}` concat, and the four
builtins), and `insts.rs` (instantiation resolution: binds a child's
parameters per call site and width-checks every connection against the
child's port types under that binding). It checks:

- **E0401** — expression width matches context (can't assign 8 bits to a 4-bit signal)
- **E0402** — type mismatch (mixing `bits` and `signed` without a cast)
- **E0403** — signed vs unsigned mismatch
- **E0408** — `if`/`match` arms must all produce the same width
- **E0601** — match must be exhaustive (all cases covered)
- **E0602** — unreachable pattern (a case that can never match)
- **E0409** — pattern type mismatch
- **E0406** — index or slice out of bounds

**`Ty<'a>`** (`widths/mod.rs`) is this pass's own internal type
representation — richer than the AST's `Type` (see
[`05-ast.md`](05-ast.md)) because it needs runtime-resolved facts the AST
doesn't carry: folded widths, and (since the `Ty::Bundle` consolidation) a
bundle's name plus its on-demand-resolved field types
(`resolve_bundle_fields`), replacing an earlier separate `Wcx::bundle_sigs`
side-table. A bundle-typed `fn` parameter or return value type-checks
against this same `Ty::Bundle`, so passing/returning bundles through `fn`s
is shape-checked identically to a plain module port.

### Pass 5: `drivers.rs` — One Driver Per Signal

In hardware, if two things try to drive the same wire, you get a short circuit — one pulls high, one pulls low, and they fight. So:

- **E0501** — every wire/output is driven exactly once (disjoint bit-ranges are okay though: driving `bus[3:0]` in one place and `bus[7:4]` in another is fine)
- **E0502** — every output is fully driven (no undriven bits)
- **E0503** — every register is assigned in exactly one `on` block
- **E0505** — `=` (for wires) vs `<-` (for registers) usage

**Combinational cycle detection (E0504)** — this is the interesting one. It detects signals that feed back through pure logic with no register breaking the loop. This would oscillate in hardware.

The checker uses a three-color DFS (white/gray/black) over the combinational dependency graph. It also builds combinational summaries for instantiated modules — which outputs depend on which inputs — so it can detect cycles through child instances.

### Pass 6: `funcs.rs` — Function Sanity (1 File)

**`check_functions()`** validates all `fn` declarations before synthesis:

- **E0801** — duplicate function names
- **E0802** — arity mismatch at call site
- **E0803** — return width doesn't match declaration
- **E0804** — recursion or mutual recursion (no combinational loops)
- **E0805** — constant folding of function calls
- **E0806** — payload binding count doesn't match variant
- **E0807** — payload field must be a concrete type (`bit`/`bits`/`signed`)
- **E0808** — OR-arm bindings have incompatible types across alternatives

### Pass 7: `clocks.rs` — Whose Clock Is It?

- **E0701** — every register is owned by exactly one clock
- Every combinational signal is "colored" with the clock domain(s) it derives from
- Reading a signal from one clock domain inside another clock's `on` block is rejected (metastability hazard)
