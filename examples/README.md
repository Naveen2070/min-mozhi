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
come from `keywords.toml` — never invent words) plus the `BASE_EXAMPLES`
list in `tests/examples.rs`. See `docs/code/10-test-map.md`.
