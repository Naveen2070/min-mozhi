# 4 — The Parser: Building the Tree (7 Files)

The parser takes the token stream and builds an **Abstract Syntax Tree (AST)** — a structured representation of your program's grammar. It's a recursive-descent parser with multi-error recovery.

---

## `parser/mod.rs` — The Parser Core

**`parse(toks)`** is the entry point. Creates a `Parser`, calls `file()`, and returns either the AST or all collected errors.

**`Profile` enum** — `CodeOrder` (default, English-like word order) or `Thamizh` (SOV/postpositional). Set by the `syntax thamizh` directive. Both produce the same AST — the profile only steers which clause-head syntax the parser accepts.

**Stack overflow protection** — `MAX_DEPTH = 64` levels. Each recursive call adds about 12 Rust stack frames, and the default thread stack is 1 MB. 64 levels is way more than any human-written file needs. A deeper file gets E1113 instead of a crash.

The `enter()`/`leave()` pair wraps every recursive function. `enter()` returns `None` once the depth limit is hit, and the `?` operator propagates it up.

**Token plumbing:**

- `peek()` / `peek_kind()` — look at the current token without consuming
- `at(kind)` / `at_kw(kw)` — boolean check
- `eat(kind)` / `eat_kw(kw)` — consume if it matches, return boolean
- `bump()` — unconditionally consume and return
- `expect(kind, what)` — consume or record E1101: "expected {what}, found {actual}"

**Error recovery:**

- `sync_to_newline()` — skip tokens until the next newline, `}`, or EOF. This lets the parser continue checking later statements in the same block.
- `terminator()` — expects a newline or an implicit terminator before `}`/EOF.

---

## `parser/expr.rs` — Parsing Expressions

Expression parsing uses **precedence climbing**. There's a table of operator precedences:

```
Level 0: or / ||
Level 1: and / &&
Level 2: ==, !=, <, <=, >, >=
Level 3: |, ^
Level 4: &
Level 5: <<, >>
Level 6: +, -, +%, -%
Level 7: *, *%
```

**`binary(min_prec)`** is the core. It parses a unary expression first, then loops: if the next operator has precedence ≥ `min_prec`, it consumes it and recursively parses the right-hand side at the next higher level. This naturally makes `+` and `-` left-associative while still binding tighter than `==`.

**`unary()`** handles prefix operators: `-`, `~`, `!`, `not`, `&`, `|`, `^` (reductions).

**`postfix()`** handles primary expressions followed by postfixes:

- `ident` — a bare name
- `ident(args)` — builtin call
- `N'literal` — width-annotated literal like `8'd42`
- `true` / `false`
- Number literal
- `( expr )` — grouping
- `{ exprs }` — concatenation
- `{ count{ exprs } }` — replication
- Then optionally: `.ident` (field access), `[i]` (index), `[hi:lo]` (slice)

**`if_expr()`** parses `if cond { then } else { else }`. In expression position, `else` is mandatory — no inferred latches.

**`match_expr()`** parses `match scrutinee { arms }`. Each arm: `patterns => value`. Multiple patterns in one arm OR together.

---

## `parser/items/file.rs` — Top-Level Items

**`syntax_directive()`** — checks for an optional leading `syntax thamizh`. Sets `self.profile = Profile::Thamizh`. The directive never enters the AST, so a thamizh-order file and its code-order twin parse into the same tree.

**`file()`** — the whole-file entry. Loops over file-level items:

- `import lib.adder` → `import_decl()`
- `const NAME: int = expr` → `const_decl()`
- `module Name(...)` → `module()`
- `enum Name { ... }` → `enum_decl()`
- `test "..." for M(...) { }` → `test_decl()`
- In thamizh profile, a bare identifier starts `test_decl_thamizh()`

This function never fails — a bad item records an error, skips to the next line, and the parser keeps going.

---

## `parser/items/mod.rs` — Shared Helpers

**`lvalue()`** — parses an assignment target: `ident` optionally followed by `[i]` or `[hi:lo]`.

**`expr_to_lvalue(expr)`** — thamizh-order seq statements parse their head as an expression before knowing if it's a condition or an lvalue. This function recovers the `LValue` from the expression.

**`ty()`** — parses a type: `bit`, `bits[N]`, `signed[N]`, or an enum name.

**`repeat_block()`** — parses `repeat i: lo..hi { body }`.

---

## `parser/items/module.rs` — Module Body Items

**`module()`** — parses the whole module: name, optional parameter list with defaults, brace-delimited body.

**`module_item()`** — dispatches on the leading keyword:

- `in`/`out` → port declaration
- `clock` → clock declaration
- `reset` → reset (synchronous, active-high)
- `async reset` → asynchronous reset
- `wire name: type = expr` — MUST have a drive value
- `reg name: type = value` — MUST have a reset value (E1104 if missing)
- `mem name: type[depth] = value` — MUST have an init value
- `let` → child module instantiation
- `on` → sequential block (code order)
- `rise`/`fall` → sequential block (thamizh order)
- `repeat` → compile-time generation
- Bare identifier → combinational drive `lhs = rhs`. If you write `<-` here, you get E1105 — a teaching message pointing you to `on` blocks.

---

## `parser/items/inst.rs` — Instantiations

**`inst()`** — parses `let name = Module(params) { connections }`. Supports:

- Optional `[index]` for instance arrays inside `repeat`
- Parameter overrides `(P: val, ...)`
- Port connections `{ port: signal, ... }`

---

## `parser/items/seq.rs` — Sequential (`on`) Blocks

**`on_block()`** — code order: `on rise(clk) { body }` / `on fall(clk) { body }`.

**`on_block_thamizh()`** — thamizh order: `rise(clk) on { body }` / `fall(clk) on { body }`. Same AST.

**`seq_stmt()`** — one statement inside a sequential block. Dispatches on profile:

- Code order: `if cond { }` or `lhs <- rhs`. Using `=` here gets E1106 (teaching message about `<-`).
- Thamizh order: parses the head as an expression, then checks for `enil` (conditional) or `<-` (assignment).

**`seq_if()`** — statement-level `if`. `else` is optional here (registers hold their value when not assigned — no latch risk, unlike wires).

---

## `parser/items/test.rs` — Test Blocks

**`test_decl()`** — code order: `test "name" for Module(args) { body }`.

**`test_decl_thamizh()`** — thamizh order: `Module(args) kaaga "name" sodhanai { body }`. Same AST.

**`test_block()`** — dispatches:

- `tick(clk [, N])` — advance N cycles
- `expect expr` — assert a condition
- `ident = expr` — drive an input
- `if cond { }` — test-time conditional
