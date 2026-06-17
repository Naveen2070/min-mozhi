# Min-Mozhi demo — write a design, simulate it, see the waveform

A self-contained sandbox for writing a `.mimz` design, compiling it to Verilog,
running the built-in simulator, and viewing the resulting waveform — in VS Code
or a browser.

The example here, [`cpu.mimz`](cpu.mimz) + [`alu.mimz`](alu.mimz), is the most
feature-dense design Min-Mozhi can build today: a single-clock **accumulator
CPU** that exercises every structural feature the simulator supports —

| Feature                               | Where in the design               |
| ------------------------------------- | --------------------------------- |
| module instance + cross-file `import` | the ALU (`alu.mimz`)              |
| `repeat` + bit-indexed drives         | the LED bus (`leds[i] = accr[i]`) |
| `enum` + state register               | the `Run`/`Halt` phase            |
| `match`-as-ROM                        | the 16-word program memory        |
| registers, sync reset, `on rise(clk)` | `acc` / `pc` / `phase`            |
| slices + wrapping arithmetic          | instruction decode + the ALU      |

It runs a fixed program; each clock fetches `ROM[pc]`, feeds `(acc, imm)` to the
ALU under the decoded opcode, latches the result, and advances `pc` until the FSM
halts.

---

## 0. Prerequisites

- **Build the compiler once** (from the repo root):

  ```
  cargo build --release
  ```

  This produces `target/release/mimz` (`mimz.exe` on Windows). Below, `mimz`
  means that binary — or just use `cargo run -q --` in place of `mimz`.

- **A waveform viewer** (pick one): the **Surfer** VS Code extension (or
  <https://app.surfer-project.org> in a browser), **GTKWave**, **VaporView**, or
  **WaveTrace**.

- _(Optional)_ **Icarus Verilog** (`iverilog`) only if you want to cross-check the
  emitted Verilog against another simulator.

> Run every command below from the **repo root**.

---

## 1. Check it (front end + safety checker)

```
mimz check demo/cpu.mimz
```

Expected:

```
OK: demo/cpu.mimz — 2 module(s), 1 test(s), 2 file(s)
```

A clean `OK:` means it lexed, parsed, and passed all six checker passes (widths,
drivers, exhaustive `match`, reset rules, clock domains, …).

## 2. Run the self-checking test

```
mimz test demo/cpu.mimz
```

Expected:

```
ok   program computes 0x86 (1 check)

1 passed, 0 failed
```

The `test` block resets the CPU, runs five instructions, and asserts the
accumulator ended at `0x86`. Exit code is non-zero if any test fails.

## 3. Watch it run in the console (quick sanity)

```
mimz sim demo/cpu.mimz --cycles 8 --trace
```

Expected — `acc` walks `0 → 5 → 8 → 6 → 6 → 134` (`0x86`), then the FSM halts
(`phase` flips `0 → 1`) and `pc` freezes:

```
cycle | acc | pc | leds | phase | pcr | accr
------+-----+----+------+-------+-----+-----
    0 |   0 |  0 |    0 |     0 |   0 |    0
    1 |   5 |  1 |    5 |     0 |   1 |    5
    2 |   8 |  2 |    8 |     0 |   2 |    8
    3 |   6 |  3 |    6 |     0 |   3 |    6
    4 |   6 |  4 |    6 |     0 |   4 |    6
    5 | 134 |  5 |    6 |     0 |   5 | 134
    6 | 134 |  6 |    6 |     1 |   6 | 134
    7 | 134 |  6 |    6 |     1 |   6 | 134
```

## 4. Generate the waveform (VCD)

```
mimz sim demo/cpu.mimz --cycles 8 -o demo/cpu.vcd
```

Writes `demo/cpu.vcd` — a standard IEEE-1364 value-change dump.

## 5. View the waveform as a graph

`cpu.vcd` is a standard VCD — any of these open it. In every viewer, add the
signals **`clk`, `rst`, `pc`, `acc`, `leds`, `phase`** to watch the accumulator
climb to `0x86` and `phase` flip `0 → 1` (Run → Halt).

### A. Web — zero install (fastest)

- **Surfer** — open <https://app.surfer-project.org>, then drag `demo/cpu.vcd`
  onto the page (or **File → Open file**). Runs in the browser via WASM; the file
  never leaves your machine.
- **VCDROM** — open <https://vc.drom.io> and drag `demo/cpu.vcd` in. Same idea,
  also fully client-side.

### B. VS Code extension — view it without leaving the editor

Install one (Quick Open `Ctrl/Cmd+P`, then paste the line), then just click
`demo/cpu.vcd` in the Explorer:

| Extension     | Install                             |
| ------------- | ----------------------------------- |
| **Surfer**    | `ext install surfer-project.surfer` |
| **VaporView** | `ext install lramseyer.vaporview`   |
| **WaveTrace** | `ext install wavetrace.wavetrace`   |

(Or open the Extensions panel and search "Surfer" / "VaporView" / "WaveTrace".)

### C. Desktop GUI app

- **GTKWave** (classic):
  - Windows: `winget install GTKWave` or `scoop install gtkwave`
  - macOS: `brew install --cask gtkwave`
  - Linux: `sudo apt install gtkwave`
  - then: `gtkwave demo/cpu.vcd`
- **Surfer desktop** (modern): `cargo install surfer`, then
  `surfer demo/cpu.vcd` — or grab a build from
  <https://gitlab.com/surfer-project/surfer/-/releases>.

## 6. Compile to Verilog (optional)

```
mimz compile demo/cpu.mimz -o demo/cpu.v
```

`demo/cpu.v` is plain synthesizable Verilog-2005 (Tamil identifiers, if any, are
transliterated to ASCII). Cross-check it with Icarus if installed:

```
iverilog -t null demo/cpu.v        # must elaborate cleanly
```

---

## Edit → re-run loop

1. Edit `demo/cpu.mimz` (or add your own `.mimz` file in this folder).
2. `mimz check demo/cpu.mimz` — fix any `E`-coded errors (each carries a teaching
   message; `mimz explain E0402` expands one).
3. `mimz test demo/cpu.mimz` — confirm behavior.
4. `mimz sim demo/cpu.mimz --cycles N -o demo/cpu.vcd` — regenerate the waveform
   and refresh it in your viewer.

### Ideas to try

- Change the program ROM in `cpu.mimz` (the `match pcr { … }`) and watch `acc`
  change in the trace/waveform.
- Add an opcode to `alu.mimz` (e.g. XOR `0b…`) — the checker will tell you if a
  width or exhaustiveness rule breaks.
- Write the whole thing in Tamil or Tanglish keywords — same circuit, same
  Verilog (`mimz translate demo/cpu.mimz --to tamil`).

## What it does NOT do (today's ceiling)

Single clock domain only; no RAM / register file (no array-of-registers yet), no
division. See the language docs (`docs/guide/`) for the full feature set.

---

_Generated `*.v` / `*.vcd` are git-ignored here — they rebuild from the `.mimz`
sources with the commands above._
