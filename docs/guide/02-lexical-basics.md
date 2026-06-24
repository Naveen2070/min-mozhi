# 2 — Lexical Basics

This chapter covers the smallest pieces: comments, names, keywords, and the one
feature unique to Min-Mozhi — three interchangeable keyword flavors.

## Comments

```mimz
// line comment — to the end of the line
/* block comment
   spanning lines */
```

There is no documentation-comment syntax yet; `//` and `/* */` are it.

## Identifiers

Identifiers name your modules, ports, wires, registers, parameters, and enum
variants. They follow the usual rule — a letter or `_` then letters, digits, or
`_`:

```mimz
count   data_in   fa0   WIDTH   _tmp
```

Tamil-script identifiers are allowed too, and the emitter transliterates them to
readable ASCII in the generated Verilog (`விளக்கு` → `vilakku`):

```mimz
module விளக்கு {
  out ஒளி: bit
  ஒளி = true
}
```

Identifiers are **not** translated between flavors — only keywords are. Your
names stay exactly as written. (Tamil _digits_ inside number literals are
rejected — use ASCII digits.)

## Numbers

```mimz
42          // decimal
0b1010      // binary
0xFF        // hexadecimal
1_000       // _ is a digit separator, ignored
```

The written base is preserved into the emitted Verilog, so `0xFF` stays hex in
the output. Division `/` and modulo `%` do **not** exist as operators (they have
no cheap hardware meaning); writing one is a teaching error.

## Keywords and the three flavors

A keyword is a word the grammar reserves — `module`, `in`, `out`, `wire`, `reg`,
`if`, `match`, and so on. Min-Mozhi has the unusual property that every keyword
has **three spellings**, called flavors:

| Flavor       | `module`   | `in`      | `out`       | `wire`  | `reg`       |
| ------------ | ---------- | --------- | ----------- | ------- | ----------- |
| English      | `module`   | `in`      | `out`       | `wire`  | `reg`       |
| Tanglish     | `thoguthi` | `ulleedu` | `veliyeedu` | `kambi` | `pathivedu` |
| Tamil script | `தொகுதி`   | `உள்ளீடு` | `வெளியீடு`  | `கம்பி` | `பதிவேடு`   |

All three lex to the same token, so these two modules are identical to the
compiler:

```mimz
module M { in a: bit  out y: bit  y = a }
```

```mimz
தொகுதி M { உள்ளீடு a: bit  வெளியீடு y: bit  y = a }
```

The full table of all 28 keywords in all three flavors is in the
[cheat sheet](12-cheatsheet.md). The single source of truth is
[`../../lang/keywords.toml`](../../lang/keywords.toml).

> Only **keywords** change across flavors. Types (`bit`, `bits`, `signed`),
> built-in function names (`extend`, `min`, …), operators, and your own
> identifiers are universal — they are spelled the same in every flavor.

### Mixing flavors

You may mix flavors freely, even within one line. This is deliberate: it is the
migration path for a team moving between English and Tamil.

```mimz
module Mux {
  ulleedu a: bit    // tanglish `in`
  உள்ளீடு b: bit    // tamil `in`
  in  s: bit        // english `in`
  veliyeedu y: bit  // tanglish `out`
  y = if s { b } else { a }
}
```

Mixing is legal but a tidy file usually picks one flavor. `mimz fmt` normalizes a
file to a single flavor for you, and `mimz fmt --strict` warns when a file mixes
(see [the toolchain](11-toolchain.md)).

## Layout and newlines

Min-Mozhi is brace-delimited (`{ }`) and mostly whitespace-insensitive, with one
Go-style convenience: a statement can continue onto the next line after an
operator. Both of these are the same expression:

```mimz
y = a +
    b
```

```mimz
y = a + b
```

You do not write semicolons.

Next: [the type system](03-types-and-values.md).
