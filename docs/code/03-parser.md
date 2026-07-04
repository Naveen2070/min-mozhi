# 03 — The Parser (`src/parser/`)

`Vec<Token>` → `ast::File`. Plain recursive descent — no parser
generator, no combinators. Boring on purpose: a contributor should be
able to map "the spec says X" to "the function that parses X" in seconds.

## File layout

| File       | Owns                                                                                  |
| ---------- | ------------------------------------------------------------------------------------- |
| `mod.rs`   | `parse()` / `parse_recover()` entries, `Parser` state, token plumbing, error recovery |
| `items/`   | File level, modules, `on`-blocks, `repeat`, `test` blocks, `fn` declarations          |
| `expr.rs`  | Expressions: precedence climbing, `if`/`match`, patterns, builtins, `FnCall`          |
| `tests.rs` | Unit tests (see [`10-test-map.md`](10-test-map.md))                                   |

The `items/` submodule splits item parsing across files by grammar
section: `items/mod.rs` (shared `ty`/`lvalue`/`repeat_block` helpers +
the `pub(in crate::parser) file()` entry), `items/file.rs` (imports,
consts, enums), `items/module.rs` (modules and ports), `items/inst.rs`
(instance declarations), `items/seq.rs` (`on`-blocks + thamizh
variants), `items/test.rs` (`test` blocks), `items/func.rs` (`fn`
declarations — `fnDecl` from spec/02 section 5, including the `fnStmt`
body: `let` / statement-level `if` / `return`, terminated by the
mandatory tail expression).

`mod.rs` owns the struct and all private plumbing; the `items/` files and
`expr.rs` are `impl Parser` blocks reached through `pub(super)` entry
points (`file()`, `expr()`, `lvalue()`). Rust privacy makes this work:
items in `mod.rs` are visible to descendant modules without being public
anywhere else.

**Every parse routine carries its EBNF production as a doc comment**
(e.g. `/// inst = "let" ident [ "[" expr "]" ] "=" ident ...`), mirroring
spec/02 section 5. Change the grammar → change the spec, the doc comment,
and the code together.

## The `Option` contract

Parse routines return `Option<T>`:

- `Some(node)` — parsed.
- `None` — failed, **and the error is already recorded** in `self.diags`.
  `None` never means "not present"; presence checks are done by peeking
  (`at`, `at_kw`) before committing.

So `?` propagates failure upward without losing the message, and the
caller decides where to resynchronize.

## Plumbing conventions (`mod.rs`)

| Family    | Behavior                                                                |
| --------- | ----------------------------------------------------------------------- |
| `at*`     | look, don't consume                                                     |
| `eat*`    | consume **if** it matches; returns whether it did                       |
| `expect*` | consume or record an error; the `what` argument is human text           |
| `bump`    | consume one token — never advances past `Eof`, so `peek` is always safe |

`expect`'s `what` strings are part of the error UX: they describe the
expectation in learner terms ("a module name", "`:` then the wire's
type"), not grammar terms.

## Error recovery — multi-error by design

When a statement fails, `sync_to_newline()` skips to the next newline or
`}` and parsing continues. One typo therefore reports one error, and the
rest of the file still gets checked (spec/01 G1: a learner shouldn't fix
errors one compile at a time). Recovery points are statement boundaries —
fine-grained enough in practice, simple enough to reason about.

`terminator()` enforces the statement-ends-at-newline rule, accepting an
implicit terminator before `}` or `Eof`.

### Two entry points: `parse` (strict) vs `parse_recover` (best-effort)

Both run the same recursive descent (a shared `run()`); they differ only
in what they return:

- **`parse(toks) -> Result<File, Vec<Diag>>`** — the **strict** contract:
  ANY diagnostic discards the tree (`Err`). The compile/sim/emit pipeline
  depends on this — no codegen from a broken parse.
- **`parse_recover(toks) -> (File, Vec<Diag>)`** — never discards the tree.
  At each recovery boundary it pushes an **`Error` placeholder node**
  (`TopItem::Error`, `ModuleItem::Error`, `SeqStmt::Error`,
  `TestStmt::Error`, each carrying the skipped `Span`) instead of dropping
  the broken construct, so the surrounding good nodes survive. This is the
  prerequisite for LSP semantics on half-typed files (hover, completion).

`parse_recover` is the **only** source of `Error` nodes — `parse` returns
`Err` on the same input, so codegen never sees one. `Parser::span_since`
sizes each placeholder; every consumer (`checker/`, `emit_verilog/`,
`sim/`, `pretty.rs`) handles the variants (the checker skips them with no
cascade; the codegen stages treat them as documented unreachable no-ops).
Expression-level recovery (`ExprKind::Error`) is deferred — an error-expr
has no width/type, so it would need an "unknown" path through width/type
inference.

## Types and array literals — `ty()` (`items/mod.rs`), array-lit (`expr.rs`)

`ty()` is `arrayType | scalarType`: it first parses one `scalarType`
(`bit`, `bits[N]`, `signed[N]`, or an enum/bundle name — unchanged since
before arrays existed, now split out as `scalar_ty()`), then loops on a
trailing `[expr]` suffix, wrapping the type so far in `Type::Array { elem,
len }` on each iteration. This makes `bits[8][4]` parse as `Array { elem:
Bits(8), len: 4 }` and even `bits[8][4][2]` parse cleanly to a _nested_
`Array` (the checker, not the grammar, rejects nested-array elements,
`E0411` — this project's usual "lenient parser, narrowing checker" split,
the same one `mem`'s element-type restriction uses).

Array **literals** (`[e1, ..., eN]`) are parsed in `expr.rs`'s primary
dispatch, not as a postfix suffix: a `[` at the **start** of a primary
expression is unconditionally an array literal (there is no other
`[`-led primary), while `arr[idx]` indexing is recognized separately by
`postfix()`, which only matches `[` **after** an already-parsed base
expression. This is a simpler disambiguation than bundle-literal vs.
concat/replicate (both spelled with a leading `{` and split by
lookahead) because array literals have no ambiguous sibling.

## Expression parsing (`expr.rs`)

Precedence climbing in `binary(min_prec)`, with the table in `bin_op`:

```text
unary(9) → * (8) → + - (7) → << >> (6) → & (5) → ^ (4) → | (3)
        → comparisons (2, NON-associative) → && (1) → || (0)
```

Two deliberate deviations from C, both Rust-inspired (decision in the
2026-06-10 log):

- **Bitwise binds tighter than comparison** — `x & 1 == 0` means
  `(x & 1) == 0`, killing C's classic footgun.
- **Comparisons don't chain** — `a < b < c` is a hard error with a help
  message, not a silently-boolean comparison.

Other notable spots:

- `if` **expressions** require `else` (latches!) — enforced here in the
  parser, with the teaching help text.
- `match` parses its scrutinee with `binary(0)`, not `expr()`, to avoid
  ambiguity with a `{`-starting `if`/`match` head; parenthesize if needed.
- Word operators `and`/`or`/`not` are handled in the same precedence
  table / unary dispatch as `&&`/`||`/`!` — they are aliases (G1-x), not
  separate features.
- `else` may follow a newline: `seq_if`/`test_if` save the cursor, skip
  newlines, and **backtrack** if no `else` is found — the parser's only
  backtracking.
- One-token lookahead everywhere. This matters for Phase 1.8: the
  `thamizh-order` profile was explicitly designed (spec/04) to also need
  only one token of lookahead, so it can reuse this machinery with
  flipped clause heads.

## What the parser deliberately does NOT do

No name resolution, no width checking, no const evaluation — those are
the checker's passes (`docs/code/11-checker.md`). The parser only
enforces what is syntactically decidable, which includes several safety
rules: `=` vs `<-` placement, mandatory reg reset values, mandatory
`else` on if-expressions. User-defined function calls (`fnCall`) are
parsed in `expr.rs` alongside built-in calls — the distinction
(user fn vs. built-in) is resolved at the expression level by name
recognition; width/arity checking and recursion detection are
checker passes (E0801–E0805). Every parse
error carries a stable code (**E1101–E1111** — `self.error(span, code,
msg)` makes the code mandatory; catalog and the E1101 grouping rule in
[`06-diagnostics.md`](06-diagnostics.md)).

A `fn` body (`items/func.rs`) is `{ fnStmt } expr` — zero or more
statements (`let`, statement-level `if`, `return`) followed by exactly
one mandatory tail expression. Statement-level `if` (`fn_if`) is parsed
with `fn_stmt_block`, which unlike the top-level `fn_body` accepts no
tail expression and makes `else` optional — mirroring `seqIf`'s
optional-`else` shape rather than the expression-level `ifExpr`'s
mandatory one. The parser does not decide reachability past a `return`;
unreachable code after an unconditional `return` in the same block is a
checker diagnostic (E0812), not a parse error.
