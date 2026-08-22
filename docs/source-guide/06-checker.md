# 6 - The Checker: Nine Safety Passes

The checker runs nine passes over the AST to catch hardware bugs **before** they get to silicon. Every error has a stable E-code and a teaching help message.

> **"Pass" here means one call inside `check()`.** Nine calls, spread over
> eight files - `funcs.rs` contributes two of them. Two passes grew big
> enough to become folders of their own: `names/` (7 files) and `widths/`
> (11 files). The unit tests live in `checker/tests/` (12 files).

## `crates/mimz-core/src/checker/mod.rs` - The Entry

**`Checker` struct** holds all the state for all nine passes: the diagnostics list, the module / enum / bundle / extern / function maps, the per-file constant environment, and the per-module name scopes.

**`check(files)`** runs all nine in this fixed order:

1. `build_symbols()` (`symbols.rs`) - build the project tables
2. `check_extern_modules()` (`extern_module.rs`) - `extern module` port shapes
3. `check_func_cycles()` (`funcs.rs`) - no recursive `fn`
4. `check_func_unreachable()` (`funcs.rs`) - no dead code after `return`
5. `eval_consts()` (`consteval.rs`) - evaluate file-level constants
6. `resolve_names()` (`names/`) - per-module name resolution + structure rules
7. `check_widths()` (`widths/`) - type and width checking, match exhaustiveness
8. `check_drivers()` (`drivers.rs`) - single-driver and combinational-cycle rules
9. `check_clocks()` (`clocks.rs`) - clock-domain ownership

Order matters: later passes reuse what earlier ones built. Pass 6 stores each module's name scope on the `Checker`, and passes 7 and 8 resolve against those same tables instead of walking the AST again.

### Pass 1: `symbols.rs` - Who's Who?

**`build_symbols()`** scans every file for module, enum, bundle, extern-module, and function declarations and builds the lookup tables by name. Checks for duplicates: E0001 (two modules with the same name in one file), E0002 (enums), E0909 (bundles), E1301 (extern modules), E0801/E0802 (functions - project-wide, and no shadowing a builtin).

It also synthesizes the two hidden `__Valid`/`__ValidSigned` bundles that back the `T?` sugar, so the rest of the checker never has to special-case it.

### Pass 2: `extern_module.rs` - Black-Box Port Shapes

**`check_extern_modules()`** validates Verilog-FFI `extern module` declarations. Ports must stay scalar (`bit`/`bits[N]`/`signed[N]`) - no bundle- or array-typed extern port, since there is no Verilog-side shape to enforce against (E1302).

### Passes 3 and 4: `funcs.rs` - Function Sanity

**`check_func_cycles()`** walks the call graph and rejects direct or mutual recursion (E0805). Hardware has no call stack, so a `fn` that calls itself has no fixed size.

**`check_func_unreachable()`** rejects statements after a `return` in the same statement list (E0812) - always a mistake, never intentional.

### Pass 5: `consteval.rs` - What's the Value?

**`eval_consts()`** evaluates file-level `const` declarations top-to-bottom. It uses checked arithmetic - an overflow is E0202, never a silent wrap. Some operators don't work at compile time (`+%` needs a bit width) → E0201. A duplicate name in one file is E0004.

The results are available as `self.const_eval()` for later passes and the Verilog emitter. Values wider than 128 bits are held in the shared `ConstVal`/`Bits` representation, so a wide constant folds instead of overflowing.

### Pass 6: `names/` - Does Everything Refer to Something Real? (7 Files)

**`resolve_names()`** goes module by module and checks:

- **E0003** - duplicate names within a module
- **E0301** - every register must have a reset value
- **E0101, E0102, E0103** - every name used in an expression must refer to a real declaration
- **E0302** - every input of an instantiated module is connected exactly once
- **E0104** - reading `inst.port` where `port` is an output (not an input)
- **E0109** - `on rise(x)` - `x` must be a clock
- **E0303** - `repeat` bodies contain only hardware generation (drives, instances), not declarations
- **E0110, E0111** - a bare name that matches declarations in two different files is ambiguous; a qualified `a.b.Name` must match a real `import`
- **E0803, E0806, E0808** - `fn` call arity, enum payload binding count, and the rule that every alternative of an OR-pattern binds the same names at the same widths
- **E0809, E0810, E0811** - `default` must target a `reg`, at most one per `on` block; a `const if` condition must be compile-time constant

The seven files split the work: `mod.rs` (scope construction and the per-module walk), `items.rs` (module items and `on`-block statements), `exprs.rs` (expressions and lvalues), `insts.rs` (instantiations and `test` headers), `funcs.rs` (`fn`-body scoping and field-type validation), `resolve.rs` (bare-vs-qualified symbol lookup), and `tests.rs`.

### Pass 7: `widths/` - Are the Bits Right? (11 Files)

This is the most complex pass. `mod.rs` owns the `Ty`/`Wcx` types and the
top-level dispatch; the siblings hold one concern each - `sigs.rs`
(signal tables and `Type` → `Ty` resolution), `expr/` (the bidirectional
typing engine plus lvalue/index/slice range checks), `ops/`
(operator/concat/builtin typing: the lossless `+`/`-`/`*` growth rules,
the width-matching family `+%`/bitwise/comparisons, shifts, `{...}`
concat, and the builtin call table), `bundles.rs` (`resolve_bundle_fields`

- resolving a bundle type's fields to `(name, Ty)` pairs under a given set
  of params, the E0901-E0912 bundle-shape checks lean on), `stmts.rs`
  (module items, `on`-block statements, enum tag/payload widths),
  `patterns.rs` (match patterns and exhaustiveness), `funcs.rs` (`fn`
  bodies and returns), and `insts.rs` (instantiation resolution: binds a
  child's parameters per call site and width-checks every connection
  against the child's port types under that binding). It checks:

- **E0401** - expression width matches context (can't assign 8 bits to a 4-bit signal)
- **E0402** - type mismatch (mixing `bits` and `signed` without a cast)
- **E0403** - signed vs unsigned mismatch
- **E0408** - `if`/`match` arms must all produce the same width
- **E0601** - match must be exhaustive (all cases covered)
- **E0602** - unreachable pattern (a case that can never match)
- **E0409** - pattern type mismatch
- **E0406** - index or slice out of bounds
- **E0411–E0416** - array rules: element type, length, argument length, element agreement, constant index range, and "arrays are `fn` parameters only"
- **E0417** - `foreach x in y` where `y` is not an array or `mem`
- **E0804, E0813** - `fn` return-width agreement and `let`-shadowing at a different width
- **E0901–E0912** - bundle rules: missing/unknown fields, structural shape mismatch, and the `??` operand rules

**`Ty<'a>`** (`widths/mod.rs`) is this pass's own internal type
representation - richer than the AST's `Type` (see
[`05-ast.md`](05-ast.md)) because it needs runtime-resolved facts the AST
doesn't carry: folded widths, and (since the `Ty::Bundle` consolidation) a
bundle's name plus its on-demand-resolved field types
(`resolve_bundle_fields`), replacing an earlier separate `Wcx::bundle_sigs`
side-table. A bundle-typed `fn` parameter or return value type-checks
against this same `Ty::Bundle`, so passing/returning bundles through `fn`s
is shape-checked identically to a plain module port.

### Pass 8: `drivers.rs` - One Driver Per Signal

In hardware, if two things try to drive the same wire, you get a short circuit - one pulls high, one pulls low, and they fight. So:

- **E0501** - every wire/output is driven exactly once (disjoint bit-ranges are okay though: driving `bus[3:0]` in one place and `bus[7:4]` in another is fine)
- **E0502** - every output is fully driven (no undriven bits)
- **E0503** - every register is assigned in exactly one `on` block
- **E0505** - `=` (for wires) vs `<-` (for registers) usage

**Combinational cycle detection (E0504)** - this is the interesting one. It detects signals that feed back through pure logic with no register breaking the loop. This would oscillate in hardware.

The checker uses a three-color DFS (white/gray/black) over the combinational dependency graph. It also builds combinational summaries for instantiated modules - which outputs depend on which inputs - so it can detect cycles through child instances.

### Pass 9: `clocks.rs` - Whose Clock Is It?

- **E0701** - every register is owned by exactly one clock
- Every combinational signal is "colored" with the clock domain(s) it derives from
- Reading a signal from one clock domain inside another clock's `on` block is rejected (metastability hazard)

The one legal way across is a CDC primitive, and this pass polices those too:

- **E0702** - `sync.double_flop`/`sync.pulse` clock arguments must be two DIFFERENT declared clocks
- **E0703** - the signal being crossed must be exactly 1 bit
- **E0704** - that signal must actually belong to the source clock's domain
- **E0705** - the call must sit in its one legal position (`double_flop`: the direct `<-` right-hand side inside the destination clock's `on` block; `pulse`: a `wire`'s direct initializer)

## Where the `fn` error codes actually live

The `E08xx` block is spread across three passes, which is easy to get wrong when reading the code:

| Code                | Raised by                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------ |
| E0801, E0802        | pass 1, `symbols.rs` - duplicate name / collides with a builtin                            |
| E0805               | pass 3, `funcs.rs` - recursive call graph                                                  |
| E0812               | pass 4, `funcs.rs` - dead code after `return`                                              |
| E0803, E0806, E0808 | pass 6, `names/exprs.rs` - call arity, payload binding count, OR-arm bindings              |
| E0809, E0810, E0811 | pass 6, `names/items.rs` - `default` target, duplicate `default`, `const if` condition     |
| E0807               | pass 6 `names/funcs.rs` + pass 7 `widths/stmts.rs` - payload field must be a concrete type |
| E0804, E0813        | pass 7, `widths/funcs.rs` - return width, `let` shadowing at a different width             |

The full catalog, with the fix each help message teaches, is in
[`docs/code/11-checker.md`](../code/11-checker.md).
