# Examples

The same 17 examples, four times — one folder per keyword flavor:

| Folder      | Keywords                                                                       |
| ----------- | ------------------------------------------------------------------------------ |
| `english/`  | English                                                                        |
| `tanglish/` | Tanglish (romanized Tamil)                                                     |
| `tamil/`    | Tamil script                                                                   |
| `mixed/`    | all three mixed in one file — mixing freely is legal and is the migration path |

Filenames and identifiers are identical across folders; **only the
keywords differ**. CI asserts that each example compiles to
**byte-identical Verilog** from all four folders (`tests/examples.rs`).

| Example               | Shows                                                          |
| --------------------- | -------------------------------------------------------------- |
| `adder.mimz`          | combinational logic; lossless `+` keeps the carry              |
| `counter.mimz`        | clock/reset, registers, `on rise`, wrapping `+%`               |
| `alu.mimz`            | `match` as an expression; `import` + module instantiation      |
| `traffic_light.mimz`  | FSM with `enum` + exhaustive `match`                           |
| `shift_register.mimz` | `<<` and `\|`, parameterized width                             |
| `mux4.mimz`           | 4-way mux via `match` on a 2-bit select                        |
| `comparator.mimz`     | comparisons; `if`-expression with mandatory `else`             |
| `blinker.mimz`        | clock divider + toggle with `^`                                |
| `edge_detector.mimz`  | one-cycle pulse from a previous-value register                 |
| `chained.mimz`        | `include` (alias of `import`) + dotted path `lib.full_adder`   |
| `ripple_adder.mimz`   | `repeat` unrolling + instance array + `const`-driven width     |
| `signed_math.mimz`    | `signed[N]`: sign-extending `extend`, signed `<`, lossless `+` |
| `window.mimz`         | monotonic chained comparison `lo <= value <= hi`               |
| `bitops.mimz`         | `min`/`max`/`abs` + negated reductions `nand`/`nor`/`xnor`     |
| `datapath.mimz`       | `*`/`*%`, `>>`, concat `{a, b}`, slice `a[3:2]`, `trunc`       |
| `vilakku.mimz`        | Tamil IDENTIFIERS end to end — transliterated to ASCII Verilog |
| `lib/full_adder.mimz` | import target — one-bit full adder                             |

Adding an example? It goes into **all four folders** (keyword spellings
come from `lang/keywords.toml` — never invent words) plus the `BASE_EXAMPLES`
list in `tests/examples.rs`. See `docs/code/10-test-map.md`.

## `tamil-pure/` — the fully-Tamil showcase

A fifth folder holds programs written **entirely in Tamil** — both keywords AND
identifiers:

| Example        | Twin of      | Shows                                   |
| -------------- | ------------ | --------------------------------------- |
| `kanakki.mimz` | `counter`    | a counter, names and all, in Tamil      |
| `cimitti.mimz` | `blinker`    | a blinker in Tamil                      |
| `oppidi.mimz`  | `comparator` | a comparator (`if`-expression) in Tamil |
| `thervi.mimz`  | `mux4`       | a 4-way mux (`match`) in Tamil          |

Because the identifiers are localized, these do **not** compile to byte-identical
Verilog — the compiler transliterates the names (`கணக்கி` → `kannakki`,
`மதிப்பு` → `mathippu`). They are instead proven to be the **same circuit** as
their English twin (canonical identifier renaming) and locked by their own
goldens + Icarus testbenches. They are a showcase, not part of the four-flavor
set (see R9 in `docs/RULES.md`).

Convert one to readable Tanglish — keywords **and** names — with the opt-in flag.
With `-o`, a `<out>.names.json` sidecar is written so the romanization is
reversible:

```sh
# Tamil -> Tanglish with Latin names (writes k.mimz.names.json beside k.mimz)
mimz translate --to tanglish --romanize-names -o k.mimz tamil-pure/kanakki.mimz

# back to the exact Tamil names — the sidecar is found automatically
mimz translate --to tamil k.mimz
```

The reverse run auto-discovers `k.mimz.names.json` next to the file, so no
`--names-map` is needed (`--no-names-map` opts out). Without `--romanize-names`,
translate swaps only the keywords and keeps the Tamil names verbatim (the lossless
default). Romanization itself is one-way — the sidecar name-map is what makes the
round-trip reversible (byte-identical for normal whitespace-separated code). One
edge: a number directly abutting a Tamil name (e.g. `42கணக்கி`, no space between)
gains a separating space when reskinned to ASCII, since the script change was the
only token boundary — so it round-trips token-equivalent, not byte-identical.

Repeated flags can live in a project **`mimz.toml`** (CLI flags override it):

```toml
[translate]
to = "tanglish"
```
