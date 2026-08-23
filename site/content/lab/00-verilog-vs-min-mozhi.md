---
title: Verilog vs Min-Mozhi
description: The same gate, then the same counter, side by side — and the trap Verilog lets through that cannot even be written here.
order: 0
chapter: /learn/03-verilog-in-a-nutshell
---

You know some Verilog — or you have read [chapter 03](/learn/03-verilog-in-a-nutshell).
This lesson is the translation table, done with your hands: the same circuits
you would write there, written here, and one trap that Verilog allows which
Min-Mozhi refuses to compile.

## Step 1 - The same gate

This Verilog AND gate:

```verilog
module gate(y, a, b);
  output y;
  input  a, b;
  assign y = a & b;
endmodule
```

Write its Min-Mozhi twin in the editor: a module named `Gate`, ports in the
body, one line of logic. Notice what disappeared — the direction list at the
top, the `assign` keyword, the semicolons.

```mimz starter
module Gate {
  in a: bit
  in b: bit
  out y: bit

  // your code here
}
```

```mimz solution
module Gate {
  in a: bit
  in b: bit
  out y: bit

  y = a & b
}
```

```mimz verify
test "and gate truth table" for Gate {
  a = 0
  b = 1
  expect y == 0

  a = 1
  b = 1
  expect y == 1
}
```

> hint: no `assign`, no semicolons, no port list in parentheses. `y = a & b`
> alone drives the output.

## Step 2 - The same counter

And this Verilog counter:

```verilog
module counter(clk, rst, count);
  parameter WIDTH = 8;
  output reg [WIDTH-1:0] count;
  input clk, rst;
  always @(posedge clk) begin
    if (rst)               count <= 0;
    else if (count == 199) count <= 0;
    else                   count <= count + 1;
  end
endmodule
```

The Min-Mozhi version is already mostly written below. Add the missing
lines inside `on rise(clk)`: when `value` reaches `LIMIT`, reset it to `0`;
otherwise increment it. Use `<-` to drive the register, and `+%` to add.
Notice there is no reset branch to write — declaring the `reset` port is
the whole contract.

```mimz starter
module Counter(LIMIT: int = 200) {
  clock clk
  reset rst

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    // your lines here: wrap at LIMIT, else count up
  }

  count = value
}
```

```mimz solution
module Counter(LIMIT: int = 200) {
  clock clk
  reset rst

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    if value == LIMIT - 1 {
      value <- 0
    } else {
      value <- value +% 1
    }
  }

  count = value
}
```

```mimz verify
test "counts and wraps" for Counter(LIMIT: 4) {
  rst = 1
  tick(clk)
  rst = 0

  tick(clk)
  expect count == 1
  tick(clk)
  expect count == 2

  tick(clk, 2)
  expect count == 0

  tick(clk, 4)
  expect count == 0
}
```

> hint: `<-` drives registers inside `on rise`; `+%` adds without growing.
> If you want "unless something else", `default` says it — but plain
> if/else works too.

## Step 3 - A trap that cannot happen here

Silent truncation: Verilog happily drops the top bits when an 8-bit result
lands in a narrower wire. Try it here — press Run on the starter and read
what happens instead:

```mimz starter fails E0401
module Narrow {
  in a: bits[8]
  in b: bits[8]
  out sum: bits[8]

  sum = a + b
}
```

```mimz solution
module Narrow {
  in a: bits[8]
  in b: bits[8]
  out sum: bits[8]

  sum = a +% b
}
```

```mimz verify
test "wraps like hardware intends" for Narrow {
  a = 200
  b = 100
  expect sum == 44
}
```

> hint: the compiler refuses, with a code — nothing is dropped quietly. When
> you DO want the low 8 bits, say so: `+%` wraps, `trunc(x, 8)` keeps them.
> See [types and widths](/learn/05-types-and-widths) next.

