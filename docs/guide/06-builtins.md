# 6 — Built-in Functions

Built-ins (and since v0.2.14, user-defined combinational functions) are the
call syntax in the language. Their names are **universal**: spelled the same
in every flavor. There are ten built-ins.

## Width casts and conversions

| Call           | Does                                                             |
| -------------- | ---------------------------------------------------------------- |
| `extend(x, N)` | widen `x` to `N` bits (zero-extend `bits`, sign-extend `signed`) |
| `trunc(x, N)`  | keep the low `N` bits of `x`                                     |
| `signed(x)`    | reinterpret the bits of `x` as `signed` (pattern unchanged)      |
| `unsigned(x)`  | reinterpret the bits of `x` as unsigned                          |

`extend` makes a resize **visible** — widths never change implicitly, so when a
1-bit value has to join an 8-bit bus you say so:

```mimz
in  din: bit
reg sr:  bits[8] = 0

on rise(clk) {
  sr <- (sr << 1) | extend(din, 8)
}
```

`extend` only widens; trying to "extend" to a narrower width is an error
(`E0407`) — use `trunc` to narrow. `signed`/`unsigned` are how you cross the
signed/unsigned boundary on purpose:

```mimz
in a: bits[4]
in b: signed[4]
out y: signed[6]
y = signed(extend(a, 5)) + extend(b, 5)
```

## Arithmetic built-ins

| Call        | Does                                 | Result        |
| ----------- | ------------------------------------ | ------------- |
| `min(a, b)` | the smaller of two same-width values | same width    |
| `max(a, b)` | the larger of two same-width values  | same width    |
| `abs(x)`    | absolute value of a `signed` value   | `signed[N+1]` |

`abs` grows by one bit so that the magnitude of the most-negative value fits:
`abs` of `signed[4]`'s −8 is +8, which needs `signed[5]`. The compiler picks the
wider type for you:

```mimz
in  s:   signed[4]
out mag: signed[5]
mag = abs(s)
```

## Negated reductions

These are the negations of the `&`/`|`/`^` reduction operators, each returning a
single `bit`:

| Call      | Equivalent to | Meaning                      |
| --------- | ------------- | ---------------------------- |
| `nand(x)` | `~(&x)`       | not (all bits set)           |
| `nor(x)`  | `~(\|x)`      | not (any bit set)            |
| `xnor(x)` | `~(^x)`       | even parity (not odd parity) |

```mimz
in  bus:  bits[4]
out allz: bit
allz = nor(bus)      // 1 when bus is all zeros
```

A negated reduction on a `signed` value is rejected (`E0403`) — reductions are a
`bits` operation.

## Worked example

The `datapath` example in [`../../examples/`](../../examples/) exercises the
multiply/shift/concat/slice/`trunc` family; `bitops` exercises
`min`/`max`/`abs`/`nand`/`nor`/`xnor`. Both have self-checking testbenches, so
they double as runnable specs for these built-ins.

Next: [expressions and control flow](07-expressions-and-control.md).
