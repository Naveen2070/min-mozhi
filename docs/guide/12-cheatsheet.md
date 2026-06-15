# 12 — Cheat Sheet

One-page reference. The authoritative sources are
[`../../keywords.toml`](../../keywords.toml) (words) and
[`../../spec/`](../../spec/) (semantics).

## Keywords — all three flavors

| English   | Tanglish    | Tamil       | Used for                        |
| --------- | ----------- | ----------- | ------------------------------- |
| `module`  | `thoguthi`  | `தொகுதி`    | declare a module                |
| `in`      | `ulle`      | `உள்`       | input port                      |
| `out`     | `veli`      | `வெளி`      | output port                     |
| `wire`    | `kambi`     | `கம்பி`     | combinational signal            |
| `reg`     | `nilai`     | `நிலை`      | register (memory)               |
| `clock`   | `kadigaram` | `கடிகாரம்`  | clock signal                    |
| `reset`   | `meetamai`  | `மீட்டமை`   | reset signal                    |
| `on`      | `pothu`     | `போது`      | clocked block                   |
| `rise`    | `yetram`    | `ஏற்றம்`    | rising-edge selector            |
| `if`      | `endral`    | `என்றால்`   | conditional                     |
| `else`    | `illaiyel`  | `இல்லையேல்` | else branch                     |
| `match`   | `poruthu`   | `பொருத்து`  | pattern match                   |
| `enum`    | `vagai`     | `வகை`       | enumeration                     |
| `let`     | `vai`       | `வை`        | instantiate a module            |
| `const`   | `maara`     | `மாறா`      | compile-time constant           |
| `repeat`  | `meendum`   | `மீண்டும்`  | compile-time unroll             |
| `import`  | `serkka`    | `சேர்க்க`   | import a file (`include` alias) |
| `true`    | `unmai`     | `உண்மை`     | boolean literal                 |
| `false`   | `poi`       | `பொய்`      | boolean literal                 |
| `test`    | `sodhanai`  | `சோதனை`     | test block                      |
| `for`     | `kaaga`     | `க்காக`     | test instantiation              |
| `tick`    | `thattu`    | `தட்டு`     | advance a clock in a test       |
| `expect`  | `ethirpaar` | `எதிர்பார்` | assert in a test                |
| `and`     | `mattrum`   | `மற்றும்`   | logical and (`&&`)              |
| `or`      | `alladhu`   | `அல்லது`    | logical or (`\|\|`)             |
| `not`     | `illa`      | `இல்லா`     | logical not (`!`)               |
| `syntax`  | `ilakkanam` | `இலக்கணம்`  | grammar directive               |
| `thamizh` | `thamizh`   | `தமிழ்`     | thamizh word-order profile      |

Reserved for future features (using one is an error): `fall`, `mem`, `sync`,
`inout`, `struct`, `secret`, `declassify`, `default`, `pipeline`, `interface`,
`chan`, `prove`, `await`, `fixed`, `requires`, `ensures`.

## Types

| Type        | Meaning                             |
| ----------- | ----------------------------------- |
| `bit`       | one bit (boolean)                   |
| `bits[N]`   | `N`-bit unsigned                    |
| `signed[N]` | `N`-bit two's-complement            |
| `int`       | compile-time integer (params/const) |
| `bool`      | compile-time boolean (params/const) |

## Operators

| Group        | Operators                                                |
| ------------ | -------------------------------------------------------- |
| arithmetic   | `+` `-` `*` (lossless, grow) · `+%` `-%` `*%` (wrapping) |
| shift        | `<<` `>>`                                                |
| bitwise      | `&` `\|` `^` `~`                                         |
| reduction    | `&x` `\|x` `^x` (collapse a bus to one bit)              |
| comparison   | `==` `!=` `<` `<=` `>` `>=` · chained: `lo <= x <= hi`   |
| logical      | `&&`/`and` `\|\|`/`or` `!`/`not` (on `bit` only)         |
| build/select | `{a, b}` concat · `x[i]` index · `x[hi:lo]` slice        |

Precedence is Rust-style: `x & 1 == 0` is `(x & 1) == 0`.

## Built-in functions

| Call           | Result                               |
| -------------- | ------------------------------------ |
| `extend(x, N)` | widen to `N` bits (zero/sign extend) |
| `trunc(x, N)`  | keep the low `N` bits                |
| `signed(x)`    | reinterpret as signed                |
| `unsigned(x)`  | reinterpret as unsigned              |
| `min(a, b)`    | smaller (same width)                 |
| `max(a, b)`    | larger (same width)                  |
| `abs(x)`       | magnitude of signed → `signed[N+1]`  |
| `nand(x)`      | `~(&x)` → one bit                    |
| `nor(x)`       | `~(\|x)` → one bit                   |
| `xnor(x)`      | `~(^x)` → one bit (even parity)      |

## Assignment

| Operator | For       | Where                        |
| -------- | --------- | ---------------------------- |
| `=`      | wire, out | combinational (outside `on`) |
| `<-`     | reg       | clocked (inside `on rise`)   |

## Error codes (selection)

Run `mimz explain <CODE>` for the full classroom version of any of these.

| Code    | Meaning                                                          |
| ------- | ---------------------------------------------------------------- |
| `E0301` | a `reg` (or module) has no reset value                           |
| `E0401` | assignment/connection width mismatch (e.g. lossless into narrow) |
| `E0403` | mixing `bits` and `signed` without a cast                        |
| `E0404` | logical op / condition on a non-`bit`                            |
| `E0405` | literal does not fit its type                                    |
| `E0406` | index or slice out of range / reversed                           |
| `E0407` | built-in misuse (e.g. `abs` of unsigned, `extend` narrowing)     |
| `E0408` | `if`/`match` arms disagree on type or width                      |
| `E0501` | more than one driver on a signal                                 |
| `E0502` | output never (or only partly) driven                             |
| `E0504` | combinational cycle                                              |
| `E0505` | wrong assignment kind (`=` on reg, `<-` on wire)                 |
| `E0601` | `match` not exhaustive                                           |
| `E0701` | cross-clock-domain read                                          |
| `E1104` | register declared with no reset value                            |
| `E1105` | `<-` used outside an `on` block                                  |
| `E1106` | `=` used inside an `on` block                                    |
| `E1108` | value-driving `if` without an `else`                             |
| `E1109` | bad chained comparison (mixed direction / `==`)                  |
| `E1110` | built-in called with the wrong arity                             |
| `W0001` | (warning) file mixes Tamil keywords with English/Tanglish        |

## Command-line flags

`mimz <command> [file] [flags]`. Per-project defaults can live in a `mimz.toml`
(CLI flags override it); see [the toolchain](11-toolchain.md).

| Command     | Flags                                                                                                               |
| ----------- | ------------------------------------------------------------------------------------------------------------------- |
| `check`     | `--tokens` (dump tokens) · `--json` (machine-readable) · `--lang <flavor>`                                          |
| `compile`   | `-o <path>` · `--lang <flavor>`                                                                                     |
| `eval`      | `--in a=1,b=2` · `--module <M>` · `--param W=8` · `--lang <flavor>`                                                 |
| `translate` | `--to <flavor>` · `--order code\|thamizh` · `--romanize-names` · `--names-map <f>` · `--no-names-map` · `-o <path>` |
| `fmt`       | `--to <flavor>` · `--strict` · `-o <path>`                                                                          |
| `explain`   | _(takes an `E`-code, case-insensitive)_                                                                             |

Global: `--config <path>` points at a specific `mimz.toml`. Flavors are
`english` / `tanglish` / `tamil` (or `en` / `tl` / `ta`).

## The safety rules, in one breath

No inferred latches · no silent truncation · no multiple drivers · no
combinational loops · no uninitialized registers · no `=`/`<-` confusion · no
signed/unsigned mixing · no C-style precedence traps. Every one is a compile
error with a teaching message.

← Back to the [guide index](README.md).
