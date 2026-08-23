---
title: Write your own test
description: Test blocks are part of the language — drive inputs, tick clocks, read a failing report and fix it.
order: 6
chapter: /learn/09-tests-and-tooling
module: And2
---

Everything you have pressed Verify on so far was a `test` block — the
language's own answer to "is this right", checked by the same compiler as
everything else. This lesson flips the chair: now you write them, and read
one that fails.

## Step 1 - Read a failing report

This suite has one assertion that is simply wrong. Press Run (`test`) and
read the report top to bottom: which test failed, which check inside it, what
was expected versus actual. Then fix the expectation — the gate below checks
the corrected suite.

```mimz starter
module And2 {
  in a: bit
  in b: bit
  out y: bit

  y = a & b
}

test "and gate truth table" for And2 {
  a = 0
  b = 0
  expect y == 0

  a = 0
  b = 1
  expect y == 1

  a = 1
  b = 0
  expect y == 0

  a = 1
  b = 1
  expect y == 1
}
```

```mimz solution
module And2 {
  in a: bit
  in b: bit
  out y: bit

  y = a & b
}

test "and gate truth table" for And2 {
  a = 0
  b = 0
  expect y == 0

  a = 0
  b = 1
  expect y == 0

  a = 1
  b = 0
  expect y == 0

  a = 1
  b = 1
  expect y == 1
}
```

```mimz verify
test "reference suite" for And2 {
  a = 0
  b = 1
  expect y == 0
}
```

> hint: a FAIL report names the test, prints the assertion, and shows both
> sides of the comparison. One character was wrong; the circuit was right.

## Step 2 - The input nobody drove

This counter only counts while its enable is high — and this suite measures
three ticks of nothing. Run it, read the report, and find why: an input the
test never drives sits at `0`. Drive `en` so the counter actually counts.

```mimz starter
module Counter {
  clock clk
  reset rst
  in en: bit

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    if en == 1 {
      value <- value +% 1
    }
  }

  count = value
}

test "counts to three" for Counter {
  rst = 1
  tick(clk)
  rst = 0

  tick(clk)
  tick(clk)
  tick(clk)
  expect count == 3
}
```

```mimz solution
module Counter {
  clock clk
  reset rst
  in en: bit

  out count: bits[8]

  reg value: bits[8] = 0

  on rise(clk) {
    if en == 1 {
      value <- value +% 1
    }
  }

  count = value
}

test "counts to three" for Counter {
  rst = 1
  tick(clk)
  rst = 0

  en = 1

  tick(clk)
  expect count == 1
  tick(clk)
  expect count == 2
  tick(clk)
  expect count == 3
}
```

```mimz verify
test "known-state reference" for Counter {
  rst = 1
  tick(clk)
  rst = 0

  en = 1
  tick(clk)
  expect count == 1
}
```

> hint: the FAIL shows `expected 3` and what actually arrived — `0`. Every
> tick advanced the clock; the enable just never said "count". One line,
> `en = 1`, placed before the ticks, fixes all three assertions.

## Step 3 - A suite of your own

No grading rails for this one — Verify here only checks that the file still
compiles. Write a full test suite for this three-input OR gate yourself:
drive all eight combinations and assert each result. When the console shows
your suite passing under Run, you are done.

```mimz starter
module Or3 {
  in a: bit
  in b: bit
  in c: bit
  out y: bit

  y = a | b | c
}
```

```mimz solution
module Or3 {
  in a: bit
  in b: bit
  in c: bit
  out y: bit

  y = a | b | c
}

test "or3 full table" for Or3 {
  a = 0
  b = 0
  c = 0
  expect y == 0

  a = 1
  expect y == 1

  a = 0
  b = 1
  expect y == 1

  c = 1
  expect y == 1

  a = 0
  b = 0
  expect y == 1
}
```

That is the arc — you can now describe hardware, prove it behaves, and read
the compiler when it disagrees. Take a design idea to the
[Playground](/playground) and go break things.

