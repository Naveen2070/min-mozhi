# Examples

The same 44 designs (plus 5 stdlib modules = 49 base examples), four times — one folder per keyword flavor:

| Folder      | Keywords                                                                       |
| ----------- | ------------------------------------------------------------------------------ |
| `english/`  | English                                                                        |
| `tanglish/` | Tanglish (romanized Tamil)                                                     |
| `tamil/`    | Tamil script                                                                   |
| `mixed/`    | all three mixed in one file — mixing freely is legal and is the migration path |

Filenames and identifiers are identical across folders; **only the
keywords differ**. CI asserts that each example compiles to
**byte-identical Verilog** from all four folders (`tests/examples.rs`).

| Example                   | Shows                                                               |
| ------------------------- | ------------------------------------------------------------------- |
| `adder.mimz`              | combinational logic; lossless `+` keeps the carry                   |
| `alu.mimz`                | `match` as an expression; `import` + module instantiation           |
| `assert_clocked.mimz`     | clocked `assert` — runtime invariant checked by simulator           |
| `assert_comb.mimz`        | combinational `assert` — runtime invariant checked by simulator     |
| `async_reset.mimz`        | `async reset` widens sensitivity list                               |
| `bitops.mimz`             | `min`/`max`/`abs` + negated reductions `nand`/`nor`/`xnor`          |
| `blinker.mimz`            | clock divider + toggle with `^`                                     |
| `bundle_passthrough.mimz` | bundle ports, flattening, field access                              |
| `chained.mimz`            | `include` (alias of `import`) + dotted path `lib.full_adder`        |
| `comparator.mimz`         | comparisons; `if`-expression with mandatory `else`                  |
| `counter.mimz`            | clock/reset, registers, `on rise`, wrapping `+%`                    |
| `cover_clocked.mimz`      | clocked `cover` — counts real edges under Icarus                    |
| `cover_comb.mimz`         | combinational `cover` — counts condition true from time zero        |
| `datapath.mimz`           | `*`/`*%`, `>>`, concat `{a, b}`, slice `a[3:2]`, `trunc`            |
| `debug_wrapper.mimz`      | `const if` conditional module items (WIDTH > 8 adds overflow port)  |
| `dual_edge.mimz`          | `on fall(clk)` + mixed-edge registers                               |
| `edge_detector.mimz`      | one-cycle pulse from a previous-value register                      |
| `enum_construct.mimz`     | `Enum.Variant(...)` construction syntax for tagged unions           |
| `enum_encoding.mimz`      | `encoding(e)` returns enum's on-wire bit pattern                    |
| `fn_array_search.mimz`    | array-typed `fn` param + `loop` + `return` first-match search       |
| `fn_const_local.mimz`     | `fn` with compile-time `const` local                                |
| `fn_mac.mimz`             | combinational function with `if`/`return` guard clauses             |
| `fn_mac_local.mimz`       | `fn` with local `const` used in body                                |
| `fn_return_guard.mimz`    | `return` in `fn` body (priority-selected, not silicon early-exit)   |
| `fn_with_const.mimz`      | `fn` using module-level `const`                                     |
| `foreach_fill.mimz`       | `foreach i in 0..4` range form sugar over `repeat`                  |
| `foreach_sum.mimz`        | `foreach v in array` elements form sugar over `loop`                |
| `mux4.mimz`               | 4-way mux via `match` on a 2-bit select                             |
| `priority.mimz`           | don't-care `match` patterns `0b1??`                                 |
| `pulse_gen.mimz`          | `default` assignment for registers                                  |
| `regfile.mimz`            | `mem` — register file with indexed read/write                       |
| `replicate.mimz`          | `{N{x}}` replication operator                                       |
| `ripple_adder.mimz`       | `repeat` unrolling + instance array + `const`-driven width          |
| `shift_register.mimz`     | `<<` and `\|`, parameterized width                                  |
| `shift.mimz`              | shift operations (used by sim shift tests)                          |
| `signed_math.mimz`        | `signed[N]`: sign-extending `extend`, signed `<`, lossless `+`      |
| `sync_double_flop.mimz`   | `sync.double_flop` — 2-flop CDC synchronizer for level signal       |
| `sync_loop_search.mimz`   | `sync loop` — cycle-iterating FSM over a range                      |
| `sync_pulse.mimz`         | `sync.pulse` — toggle-based CDC synchronizer for single-cycle pulse |
| `tagged_packet.mimz`      | tagged union `enum` with payloads + exhaustive `match`              |
| `tested_adder.mimz`       | inline `test` blocks with `tick`/`expect`                           |
| `traffic_light.mimz`      | FSM with `enum` + exhaustive `match`                                |
| `vilakku.mimz`            | Tamil IDENTIFIERS end to end — transliterated to ASCII Verilog      |
| `window.mimz`             | monotonic chained comparison `lo <= value <= hi`                    |
| `lib/full_adder.mimz`     | import target — one-bit full adder                                  |

Adding an example? It goes into **all four folders** (keyword spellings
come from `lang/keywords.toml` — never invent words) plus the `BASE_EXAMPLES`
list in `tests/examples.rs`. See `docs/code/10-test-map.md`.

## `tamil-pure/` — the fully-Tamil showcase

A fifth folder holds programs written **entirely in Tamil** — both keywords AND
identifiers:

| Example              | Twin of         | Shows                                    |
| -------------------- | --------------- | ---------------------------------------- |
| `kanakki.mimz`       | `counter`       | a counter, names and all, in Tamil       |
| `cimitti.mimz`       | `blinker`       | a blinker in Tamil                       |
| `oppidi.mimz`        | `comparator`    | a comparator in Tamil                    |
| `thervi.mimz`        | `mux4`          | a 4-way mux in Tamil                     |
| `kuutti.mimz`        | `adder`         | a full adder in Tamil                    |
| `saalaivilakku.mimz` | `traffic_light` | an FSM (traffic light) in Tamil          |
| `nakartthi.mimz`     | `shift`         | shift register in Tamil                  |
| `nilaippaduthi.mimz` | `debouncer`     | debouncer stdlib module in Tamil         |
| `ennkaatti.mimz`     | `seg7`          | 7-segment decoder stdlib in Tamil        |
| `minukki.mimz`       | `pwm`           | PWM stdlib module in Tamil               |
| `varisai.mimz`       | `fifo`          | FIFO stdlib module in Tamil              |
| `anuppi.mimz`        | `uart_tx`       | UART transmitter stdlib in Tamil         |
| `tested_kuutti.mimz` | `tested_adder`  | tested adder with inline `test` in Tamil |

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
