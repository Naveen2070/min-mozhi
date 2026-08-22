---
title: Diagnostics
description: How to read a Min-Mozhi error code — the family map across all 100 E-codes, which stage raises each, and how to get the long-form explanation.
order: 5
---

# Diagnostics

## Read the code first

Every diagnostic carries a code, and the code tells you **which stage
complained** before you read a word of the message. That usually tells you what
kind of mistake it is.

| Prefix           | Raised by | Means                                                        |
| ---------------- | --------- | ------------------------------------------------------------ |
| `E10xx`          | lexer     | a typo at the character level — text could not become words  |
| `E11xx`          | parser    | a typo at the grammar level — words could not become a program |
| `E12xx`          | loader    | an `import` could not be resolved                            |
| `E0xxx`, `E13xx` | checker   | it parsed, but it breaks a **hardware** rule                 |
| `W000x`          | lint      | advice only — the build still succeeds                       |
| `S0xxx`          | simulator | it compiled, but something went wrong while running it       |

The largest group by far is the checker's. That is the point of the language:
most of what would be a silent bug in Verilog is a named compile error here.

## Get the long version

```console
$ mimz explain E0401       # the full teaching text for one code
$ mimz explain --list      # every code with a one-line summary
```

`mimz explain` currently carries **100 E-codes and 3 W-codes**. Codes are
case-insensitive.

> **Gap worth knowing.** Three codes the compiler really emits have **no
> `explain` entry** — `mimz explain` reports them as unknown:
>
> | Code    | What it is                                                                     |
> | ------- | ------------------------------------------------------------------------------ |
> | `E0904` | bundle destructure field rename (`let { y: alias }`) — use dot access instead   |
> | `E1112` | `syntax <name>` names an unknown grammar profile                                |
> | `E1113` | expression nested too deeply to parse safely, **or** empty parens on a tag-only enum variant |
>
> All three are real: each is covered by a test in the compiler's own suite.

## Checker families

| Family  | Count | Subject                                                       |
| ------- | ----- | ------------------------------------------------------------- |
| `E000x` | 4     | duplicate names — modules, enums, consts, in-module names     |
| `E01xx` | 11    | name resolution — unknown names, bad references, ambiguity    |
| `E02xx` | 2     | compile-time evaluation — not constant, or overflow           |
| `E03xx` | 3     | structural — missing `reset`, instance wiring, `repeat` scope |
| `E04xx` | 20    | types and widths — the biggest family                         |
| `E05xx` | 5     | drivers — multiple drivers, undriven outputs, comb cycles     |
| `E06xx` | 2     | `match` — exhaustiveness and reachability                     |
| `E07xx` | 5     | clock domains — CDC reads and the `sync.*` primitives         |
| `E08xx` | 13    | functions, patterns, `default`, `const if`                    |
| `E09xx` | 9     | bundles and valid-bundles                                     |
| `E13xx` | 2     | `extern` module declarations                                  |

## Lexer, parser, loader

| Family  | Count | Subject                                                          |
| ------- | ----- | ---------------------------------------------------------------- |
| `E10xx` | 8     | unterminated comments/strings, Tamil digits, `/` and `%`, stray characters |
| `E11xx` | 14    | grammar — including the "helpful refusal" codes below            |
| `E12xx` | 2     | imports — missing file, bad standard-library import              |

Several `E11xx` codes exist purely to give a better message than "syntax error"
for a mistake carried over from another language:

| Code    | Refusal                                              |
| ------- | ---------------------------------------------------- |
| `E1006` | `/` does not exist                                   |
| `E1007` | `%` does not exist                                   |
| `E1105` | `<-` outside an `on` block                           |
| `E1106` | `=` inside an `on` block                             |
| `E1108` | value-driving `if` with no `else` (a latch)          |
| `E1109` | a comparison chain that mixes direction              |

## Warnings

| Code    | Source       | Meaning                                        |
| ------- | ------------ | ---------------------------------------------- |
| `W0001` | compiler     | file mixes Tamil keywords with English/Tanglish |
| `W0002` | `mimz lint`  | signal name is not `snake_case`                |
| `W0003` | `mimz lint`  | module name is not `PascalCase`                |
| `W0004` | `mimz lint`  | signal declared but never used                 |

Warnings never fail a build. Run [`mimz lint`](/handbook/07-cli) to see the
`W000x` set for a file.

## When you are stuck

Start with [debug recipes](/handbook/08-debug-recipes) — the common codes with
cause and fix side by side.
