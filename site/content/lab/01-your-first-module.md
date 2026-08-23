---
title: Your first module
description: Build an AND gate from an empty file, break it on purpose, compile it to Verilog, then grow it — graded by the compiler itself.
order: 1
chapter: /learn/04-your-first-module
module: Gate
---

A module is a box with ports and the logic driving its outputs. You will build
the smallest useful one — an AND gate — break it, compile it, and grow it,
while the compiler grades every step.

This lesson pairs with [Your first module](/learn/04-your-first-module) in
Learn. That chapter explains; this one makes you type it. Every exercise here
expects a module named `Gate` — keep that name.

## Step 1 - Wire the output

`y` is declared but never driven. Drive it with the AND of `a` and `b` — in
Min-Mozhi, `&` is bitwise AND. One line, where the comment is.

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

> hint: `&` is bitwise AND. A `bit` is one bit wide, so `y` can only ever be
> `0` or `1`. Assignment to a wire is plain `=`, not `<-`.

## Step 2 - Break it on purpose

Delete the `y = a & b` line and press Run (`check`). Read the error: an output
nothing drives is not a circuit, so it is an **error**, not a warning —
E0502 in the handbook. Put the line back before you verify.

```mimz starter
module Gate {
  in a: bit
  in b: bit
  out y: bit
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
  a = 1
  b = 1
  expect y == 1
}
```

> hint: the checker's message names the port and the error code. Every code
> has an entry in the [diagnostics reference](/learn/09-tests-and-tooling).

## Step 3 - Compile it to Verilog

Someone left this gate wired as OR. Fix it to AND, press Run, then type
`compile` in the command line below the editor. The output is real Verilog —
the same gate, in the language of every FPGA toolchain since 1995. Find the
`assign` statement; notice how one Min-Mozhi line became one Verilog line,
minus the traps [Learn 03](/learn/03-verilog-in-a-nutshell) warns about.

```mimz starter
module Gate {
  in a: bit
  in b: bit
  out y: bit

  y = a | b
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
test "still an and gate" for Gate {
  a = 0
  b = 1
  expect y == 0

  a = 1
  b = 1
  expect y == 1
}
```

> hint: `compile` runs the same safety checks as `check` first — you cannot
> compile something the checker would reject.

## Step 4 - Grow it

Make it a three-input gate: add input `c`, drive `y` with `(a & b) | c`.
Parentheses are free — use them even where precedence would save them. Update
nothing else; the check below was written against the grown gate.

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
  in c: bit
  out y: bit

  y = (a & b) | c
}
```

```mimz verify
test "and-or gate" for Gate {
  a = 0
  b = 0
  c = 0
  expect y == 0

  a = 1
  b = 1
  c = 0
  expect y == 1

  a = 0
  b = 0
  c = 1
  expect y == 1

  a = 1
  b = 0
  c = 0
  expect y == 0
}
```

> hint: `|` is bitwise OR. When both branches are one-bit, the result is
> one-bit too — no width surprises yet; that is the next lesson's story.

When you want open-ended play instead of rails, the
[Playground](/playground) is the sandbox. Go break things.
