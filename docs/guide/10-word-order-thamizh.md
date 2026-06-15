# 10 — Natural Word Order (thamizh)

Everything so far used **code order** — the clause head leads, like English and
most programming languages: `on rise(clk) { … }`, `if c { … }`, `match e { … }`.
Tamil, however, is an SOV / postpositional language: the operand comes first and
the clause word trails. Min-Mozhi's grammar engine lets Tamil and Tanglish code
read in that natural order — without changing the meaning at all.

> Word order and keyword flavor are **independent**. Flavor chooses the words;
> word order chooses where the clause word sits. You can have English keywords in
> thamizh order, or Tamil keywords in code order — any combination.

## Turning it on

Add the directive `syntax thamizh` at the top of the file (in any flavor —
`ilakkanam thamizh` in Tanglish/Tamil). Without the directive, code order is the
default; there is no `code` directive word.

## What flips

Only the clause words move. Three constructs are order-sensitive:

| Code order              | Thamizh order           |
| ----------------------- | ----------------------- |
| `on rise(clk) { … }`    | `rise(clk) on { … }`    |
| `if c { … } else { … }` | `c if { … } else { … }` |
| `match e { … }`         | `e match { … }`         |

The clause word (`on`, `if`, `match`) trails its operand. Everything else — ports,
declarations, assignments, expressions — is unchanged.

## The counter, in thamizh order

Compare with the code-order counter from chapter 8. This is Tanglish keywords in
thamizh word order:

```mimz
ilakkanam thamizh

thoguthi Counter(WIDTH: int = 8) {
  thudippu clk
  meettamai rst

  veliyeedu count: bits[WIDTH]

  pathivedu value: bits[WIDTH] = 0

  yetram(clk) pothu {
    value <- value +% 1
  }

  count = value
}
```

`yetram(clk) pothu { … }` is `rise(clk) on { … }` — the edge leads, `on` (`pothu`)
trails. A `match` flips the same way, scrutinee first:

```mimz
y = op thernthedu {
  0b00 => a +% b
  0b01 => a -% b
  0b10 => a & b
  0b11 => a | b
}
```

## The guarantee

Both orders parse to the **same AST**, so a thamizh-order file compiles to
_byte-identical_ Verilog as its code-order twin. Word order is purely a reading
experience; the hardware is the same.

You can convert between orders mechanically:

```text
mimz translate counter.mimz --order thamizh --to tamil
```

This re-emits the program in natural-order Tamil (it adds the `syntax thamizh`
directive for you). Note that `--order` re-emits from the AST, so it reformats and
drops comments; `--to` alone (flavor only) is lossless. If you always want this
pair, set `[translate] order = "thamizh"` / `to = "tamil"` in a `mimz.toml` so you
need not retype them. More in [the toolchain](11-toolchain.md).

Next: [the toolchain](11-toolchain.md).
