# 12 - Cheat Sheet

One-page reference. The authoritative sources are
[`../../lang/keywords.toml`](../../lang/keywords.toml) (words) and
[`../../spec/`](../../spec/) (semantics).

## Keywords - all three flavors

| English   | Tanglish        | Tamil         | Used for                                                  |
| --------- | --------------- | ------------- | --------------------------------------------------------- |
| `module`  | `thoguthi`      | `தொகுதி`      | declare a module                                          |
| `in`      | `ulleedu`       | `உள்ளீடு`     | input port                                                |
| `out`     | `veliyeedu`     | `வெளியீடு`    | output port                                               |
| `wire`    | `kambi`         | `கம்பி`       | combinational signal                                      |
| `reg`     | `pathivedu`     | `பதிவேடு`     | register (memory)                                         |
| `mem`     | `ninaivagam`    | `நினைவகம்`    | memory / register array                                   |
| `clock`   | `thudippu`      | `துடிப்பு`    | clock signal                                              |
| `reset`   | `meettamai`     | `மீட்டமை`     | reset signal                                              |
| `async`   | `otthisaivatra` | `ஒத்திசைவற்ற` | asynchronous-reset modifier                               |
| `on`      | `pothu`         | `போது`        | clocked block                                             |
| `rise`    | `yetram`        | `ஏற்றம்`      | rising-edge selector                                      |
| `fall`    | `irakkam`       | `இறக்கம்`     | falling-edge selector                                     |
| `if`      | `enil`          | `எனில்`       | conditional                                               |
| `else`    | `illaiyenil`    | `இல்லையெனில்` | else branch                                               |
| `match`   | `thernthedu`    | `தேர்ந்தெடு`  | pattern match                                             |
| `enum`    | `vagai`         | `வகை`         | enumeration                                               |
| `let`     | `amai`          | `அமை`         | instantiate a module                                      |
| `const`   | `maarili`       | `மாறிலி`      | compile-time constant                                     |
| `repeat`  | `meendum`       | `மீண்டும்`    | compile-time unroll                                       |
| `import`  | `serkka`        | `சேர்க்க`     | import a file (`include` alias)                           |
| `true`    | `mei`           | `மெய்`        | boolean literal                                           |
| `false`   | `poi`           | `பொய்`        | boolean literal                                           |
| `test`    | `sodhanai`      | `சோதனை`       | test block                                                |
| `for`     | `kaaga`         | `க்காக`       | test instantiation                                        |
| `tick`    | `kanam`         | `கணம்`        | advance a clock in a test                                 |
| `expect`  | `uruthisei`     | `உறுதிசெய்`   | assert in a test                                          |
| `and`     | `mattrum`       | `மற்றும்`     | logical and (`&&`)                                        |
| `or`      | `alladhu`       | `அல்லது`      | logical or (`\|\|`)                                       |
| `not`     | `alla`          | `அல்ல`        | logical not (`!`)                                         |
| `fn`      | `saarbu`        | `சார்பு`      | combinational function (`function` alias)                 |
| `return`  | `thirumbu`      | `திரும்பு`    | function return statement                                 |
| `default` | `iyalbu`        | `இயல்பு`      | fallback register assignment                              |
| `bundle`  | `kattai`        | `கட்டை`       | named group of signals                                    |
| `loop`    | `suzhal`        | `சுழல்`       | combinational or `on` loop                                |
| `foreach` | `ovvondraga`    | `ஒவ்வொன்றாக`  | loop over a range or an array                             |
| `sync`    | `othisai`       | `ஒத்திசை`     | modifier for `sync loop`, and the `sync.*` CDC primitives |
| `extern`  | `anniya`        | `அன்னிய`      | declare a Verilog module we do not compile                |
| `assert`  | `valiyuruthu`   | `வலியுறுத்து` | hard runtime invariant                                    |
| `cover`   | `alavidu`       | `அளவிடு`      | functional-coverage counter                               |
| `sim`     | `paavnai`       | `பாவனை`       | hardware-emulation block                                  |
| `bind`    | `inai`          | `இணை`         | connect a port to a peripheral inside `sim`               |
| `speed`   | `vegam`         | `வேகம்`       | set the emulated clock rate inside `sim`                  |
| `syntax`  | `ilakkanam`     | `இலக்கணம்`    | grammar directive                                         |
| `thamizh` | `thamizh`       | `தமிழ்`       | thamizh word-order profile                                |

That is the complete active keyword set - 44 words. The table itself lives
in [`lang/keywords.toml`](../../lang/keywords.toml); adding a word there is
a DATA change, not a compiler change. The Tanglish and Tamil columns were
ratified at keyword-set **v1** (2026-06-15); they are frozen **except the
entries marked PROVISIONAL in the table** - the spellings of `mem`,
`async`, `fall`, `fn`, `return`, `default`, `bundle`, `assert`, `cover`,
`loop`, `foreach`, `sync`, `extern`, `sim`, `bind`, and `speed` are
placeholders pending native-speaker review. A program you write today
keeps lexing the same way.

Reserved for future features (using one is an error): `inout`, `struct`,
`secret`, `declassify`, `pipeline`, `interface`, `chan`, `prove`,
`await`, `fixed`, `requires`, `ensures`.

## Types

| Type        | Meaning                                        |
| ----------- | ---------------------------------------------- |
| `bit`       | one bit (boolean)                              |
| `bits[N]`   | `N`-bit unsigned                               |
| `signed[N]` | `N`-bit two's-complement                       |
| `T?`        | valid-bundle sugar (`{ valid: bit, data: T }`) |
| `T[N]`      | fixed-size array of `N` elements               |
| `Bundle`    | named bundle of signals                        |
| `int`       | compile-time integer (params/const)            |
| `bool`      | compile-time boolean (params/const)            |

## Operators

| Group        | Operators                                                                                      |
| ------------ | ---------------------------------------------------------------------------------------------- |
| arithmetic   | `+` `-` `*` (lossless, grow) · `+%` `-%` `*%` (wrapping)                                       |
| shift        | `<<` `>>`                                                                                      |
| bitwise      | `&` `\|` `^` `~`                                                                               |
| reduction    | `&x` `\|x` `^x` (collapse a bus to one bit)                                                    |
| comparison   | `==` `!=` `<` `<=` `>` `>=` · chained: `lo <= x <= hi`                                         |
| logical      | `&&`/`and` `\|\|`/`or` `!`/`not` (on `bit` only)                                               |
| build/select | `{a, b}` concat · `{N{x}}` replicate · `x[i]` index · `x[hi:lo]` slice · `lhs ?? rhs` coalesce |

Precedence is Rust-style: `x & 1 == 0` is `(x & 1) == 0`.

## Built-in functions

| Call           | Result                                                                     |
| -------------- | -------------------------------------------------------------------------- |
| `extend(x, N)` | widen to `N` bits (zero/sign extend)                                       |
| `trunc(x, N)`  | keep the low `N` bits                                                      |
| `signed(x)`    | reinterpret as signed                                                      |
| `unsigned(x)`  | reinterpret as unsigned                                                    |
| `min(a, b)`    | smaller (same width)                                                       |
| `max(a, b)`    | larger (same width)                                                        |
| `abs(x)`       | magnitude of signed → `signed[N+1]`                                        |
| `nand(x)`      | `~(&x)` → one bit                                                          |
| `nor(x)`       | `~(\|x)` → one bit                                                         |
| `xnor(x)`      | `~(^x)` → one bit (even parity)                                            |
| `encoding(e)`  | read enum value's on-wire bit pattern as `bits[N]`                         |
| `clog2(n)`     | bits to address `n` items (compile-time; a body width may use a parameter) |

## Assignment

| Operator | For       | Where                        |
| -------- | --------- | ---------------------------- |
| `=`      | wire, out | combinational (outside `on`) |
| `<-`     | reg       | clocked (inside `on rise`)   |

## Error codes (selection)

Run `mimz explain <CODE>` for the full classroom version of any of these.
The letter tells you WHICH stage complained, before you even read the
message:

| Prefix           | Raised by | Means                                                              |
| ---------------- | --------- | ------------------------------------------------------------------ |
| `E10xx`          | lexer     | a typo at the character level - the text could not become words    |
| `E11xx`          | parser    | a typo at the grammar level - the words could not become a program |
| `E12xx`          | loader    | an `import` could not be found                                     |
| `E0xxx`, `E13xx` | checker   | the program parsed, but breaks a HARDWARE rule                     |
| `W000x`          | lint      | advice only - the build still succeeds                             |
| `S0xxx`          | simulator | it compiled, but something went wrong while running it             |

This is a selection, not the catalog. The full list is in
[`docs/code/11-checker.md`](../code/11-checker.md); the simulator's own
`S0xxx` list is in [`docs/code/13-tooling.md`](../code/13-tooling.md).

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
| `E0702` | `sync.*` clock arguments are not two different declared clocks   |
| `E0703` | `sync.*` can only cross a 1-bit signal                           |
| `E0704` | the signal handed to `sync.*` is in the wrong clock domain       |
| `E0705` | `sync.*` used somewhere other than its one legal position        |
| `E0417` | `foreach x in y` where `y` is not an array or `mem`              |
| `E1104` | register has no reset value, or memory has no init value         |
| `E1105` | `<-` used outside an `on` block                                  |
| `E1106` | `=` used inside an `on` block                                    |
| `E0806` | wrong number of payload bindings in a tagged-union match pattern |
| `E0807` | payload field type must be concrete `bit`/`bits`/`signed`        |
| `E0808` | OR-arm bindings have incompatible types across alternatives      |
| `E0809` | `default` used on something that is not a `reg`                  |
| `E0810` | two `default`s for the same reg in one `on` block                |
| `E0811` | `const if` condition is not a compile-time constant              |
| `E0812` | unreachable code after `return`                                  |
| `E0813` | a `fn`-body `let` shadows an earlier name at a different width   |
| `E0901` | bundle literal is missing a required field                       |
| `E0910` | a bundle passed somewhere is missing a field that site needs     |
| `E0911` | `??`'s left side is not an optional (`T?`) value                 |
| `E0912` | `??`'s right side does not match the left's `data` type          |
| `E1301` | `extern module` name reused in this file                         |
| `E1302` | `extern module` port is not a plain `bit`/`bits[N]`/`signed[N]`  |
| `E1108` | value-driving `if` without an `else`                             |
| `E1109` | bad chained comparison (mixed direction / `==`)                  |
| `E1110` | built-in called with the wrong arity                             |
| `E1111` | parameter/const type must be `int`/`bool`                        |
| `E1113` | expression nested too deeply to parse safely                     |
| `E1116` | unknown `sync.*` method (only `double_flop` and `pulse` exist)   |
| `W0001` | (warning) file mixes Tamil keywords with English/Tanglish        |
| `W0002` | (`mimz lint`) signal name is not `snake_case`                    |
| `W0003` | (`mimz lint`) module name is not `PascalCase`                    |
| `W0004` | (`mimz lint`) signal declared but never used                     |

## Command-line flags

`mimz <command> [file] [flags]`. Per-project defaults can live in a `mimz.toml`
(CLI flags override it); see [the toolchain](11-toolchain.md).

| Command       | Flags                                                                                                                                                                                               |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `init`        | _(takes a project name, creates `./<name>/`)_                                                                                                                                                       |
| `check`       | `--tokens` (dump tokens) · `--json` (machine-readable) · `--watch` (re-check on save) · `--lang <flavor>`                                                                                           |
| `compile`     | `-o <path>` · `--lang <flavor>` · `--emit-testbench` · `--json` · `--extern-src <f>` · `--extern-sim warn\|strict`                                                                                  |
| `eval`        | `--in a=1,b=2` · `--module <M>` · `--param W=8` · `--lang <flavor>`                                                                                                                                 |
| `sim`         | `-o <path.vcd>` · `--cycles N` · `--clock <c>` · `--in a=1,b=2` · `--param W=8` · `--sweep a=0\|1,b=2` · `--module <M>` · `--trace[=changes]` · `--verbose` · `--signals <a,b>` · `--lang <flavor>` |
| `test`        | `--filter <substr>` · `--trace[=changes]` · `--verbose` · `--signals <a,b>` · `--lang <flavor>` · `--extern-sim warn\|strict` · `--emulate` · `--step` (implies `--emulate`)                        |
| `lint`        | `--json` (machine-readable) · `--lang <flavor>`                                                                                                                                                     |
| `repl`        | `--param W=8` · `--module <M>` · `--lang <flavor>`                                                                                                                                                  |
| `explain`     | _(takes an `E`-code, case-insensitive)_                                                                                                                                                             |
| `translate`   | `--to <flavor>` · `--order code\|thamizh` · `--romanize-names` · `--names-map <f>` · `--no-names-map` · `-o <path>`                                                                                 |
| `fmt`         | `--to <flavor>` · `--strict` · `-o <path>`                                                                                                                                                          |
| `doctor`      | `--dev` (contributor toolchain check) · aliased as `env`                                                                                                                                            |
| `completions` | _(takes a shell name: bash \| zsh \| fish \| powershell \| elvish)_                                                                                                                                 |
| `eject`       | `--to <dir>` · `--flavor english\|tamil` · `--force`                                                                                                                                                |

Global: `-c`/`--config <path>` points at a specific `mimz.toml` · `-q`/`--quiet`
(suppress status banners) · `-d`/`--debug` (verbose progress) · `--color
always\|never\|auto`. Flavors are `english` / `tanglish` / `tamil` (or `en` / `tl`
/ `ta`).

## The safety rules, in one breath

No inferred latches · no silent truncation · no multiple drivers · no
combinational loops · no uninitialized registers · no `=`/`<-` confusion · no
signed/unsigned mixing · no C-style precedence traps. Every one is a compile
error with a teaching message.

← Back to the [guide index](README.md).
