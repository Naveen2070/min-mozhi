---
title: Debug recipes
description: The errors you will actually hit, each with its cause and the fix — width growth, latches, drivers, assignment kind, exhaustiveness and clock domains.
order: 8
---

# Debug recipes

Common diagnostics, what actually caused them, and what to change. For the full
teaching text on any code, run `mimz explain <CODE>`.

## `E0401` — width mismatch

**Usually:** you assigned a grown arithmetic result back into its own width.

```mimz
count <- count + 1     // bits[8] + literal -> bits[9], into bits[8]
```

**Fix:** say which you meant.

```mimz
count <- count +% 1          // wrapping
sum   =  trunc(a + b, 8)     // truncating
```

## `E0402` — operand widths differ

**Usually:** two signals of different widths in one binary operator.

**Fix:** widen the narrow side — `a + extend(b, 16)`. `extend` zero-extends
`bits` and sign-extends `signed`.

## `E0403` — kind mixing

**Usually:** a `bits[N]` and a `signed[N]` in the same expression.

**Fix:** cross over explicitly with `signed(x)` or `unsigned(x)`. Both are
reinterpretations; no bits move.

## `E0404` — condition is not a `bit`

**Usually:** a C habit — treating a bus as "true if non-zero".

**Fix:** reduce it. `if |bus` for "any bit set", `if &bus` for "all bits set".

## `E1108` — value-driving `if` without `else`

**Usually:** you drove a value in one branch only. In hardware that means "hold
the old value", which is a latch.

**Fix:** add the `else`. If you genuinely want to hold state, use a `reg` in an
`on` block, and `default` for the fallback assignment.

## `E0301` — module has regs but no `reset`

**Fix:** add a `reset` port (or `async reset`). Registers must have a defined
power-on state.

## `E1104` — register has no reset value

**Fix:** give it one in the declaration — `reg count = 0`. Same for `mem`, which
needs an init value.

## `E0505` / `E1105` / `E1106` — wrong assignment kind

| Code    | Situation                    | Fix                       |
| ------- | ---------------------------- | ------------------------- |
| `E1105` | `<-` outside an `on` block   | use `=`, or move it in    |
| `E1106` | `=` inside an `on` block     | use `<-`                  |
| `E0505` | right place, wrong target    | `=` drives wire/out, `<-` drives reg/mem |

## `E0501` — more than one driver

**Usually:** two `=` assignments to the same wire, or a wire also connected as
an instance output.

**Fix:** one driver per signal. Merge the two into a single `if`/`match`
expression that produces the value once.

## `E0502` — output never driven, or only partly

**Usually:** an output assigned inside one branch of an `if`, or forgotten.

**Fix:** drive it on every path — an `if` with a complete `else`, or a `match`
that is exhaustive.

## `E0503` — reg assigned from zero or several `on` blocks

**Fix:** exactly one `on` block owns each reg. Combine the blocks, or split the
reg.

## `E0504` — combinational cycle

**Usually:** `a = b` and `b = a`, sometimes indirectly through several wires.

**Fix:** break the loop with a register, or re-express so the dependency runs one
way.

## `E0601` — `match` not exhaustive

**Fix:** cover every variant, or add a catch-all arm. `E0602` is the mirror
image — an arm that can never be reached, usually because an earlier arm already
covers it.

## `E0302` — instance input unconnected or connected twice

**Fix:** every input of an instantiated module gets exactly one connection.

## `E0420` — `clog2` cannot size a port

**Fix:** size a body `wire`/`reg` with it instead, or pass the width in as its
own `int` parameter. See [quirks](/handbook/06-quirks) for why.

## `E0419` — array literal indexed directly

**Fix:** bind it to a `let` first (inside a `fn`), then index that name.

## `E0701` — cross-clock-domain read

**Usually:** a signal from one clock domain read in another.

**Fix:** cross it deliberately with a `sync.*` primitive — `sync.double_flop` or
`sync.pulse`. Those are the only two (`E1116` if you name another). They need two
different declared clocks (`E0702`), a 1-bit signal (`E0703`), the source domain
to be right (`E0704`), and one legal position (`E0705`).

## `E1005` — reserved word used as a name

**Fix:** rename. The message says which future feature holds the reservation. The
full list is on the [keywords page](/handbook/01-keywords).

## `E1006` / `E1007` — `/` and `%`

Division and modulo do not exist in the language.

**Fix:** for powers of two use a shift. Otherwise instantiate a divider module.

## `E1109` — bad comparison chain

**Usually:** `a < b > c` or `a == b == c`.

**Fix:** chains must be monotonic — every link the same direction. `lo <= x <=
hi` is fine; write anything else as explicit `&&`.

## `W0001` — mixed keyword flavors

Advisory only. `mimz fmt --to <flavor>` normalizes the file;
`mimz fmt --strict` turns mixing into a warning you cannot miss.

## When the code has no explanation

`mimz explain` covers 100 E-codes and 3 W-codes. One emitted code is missing an
entry — `E1113` — covering expressions nested too deeply to parse and empty
parens on a tag-only enum variant. See [quirks](/handbook/06-quirks).
