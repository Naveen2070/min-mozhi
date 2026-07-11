# 11 — Hardware Emulation: `src/emulate/` + `crates/mimz-sim/src/sim/host.rs`

> **Beta.** This whole feature is only days old (2026-07-07 → 2026-07-09) —
> expect the trait shapes and peripheral configs described below to still
> move.

Everything else in this tour is about turning source text into Verilog or
running it in the simulator at full speed. This chapter is different: it's
about a `test`'s `sim` block turning a simulated design into something you
can actually watch and listen to — a blinking terminal LED, a real audio
tone, a real TCP serial connection — throttled to an approximate
real-world clock rate.

## `crates/mimz-sim/src/sim/host.rs` — The Abstract Seam

`mimz-sim` has zero optional dependencies on purpose (the workspace
split) — no ratatui, no crossterm, no cpal. But peripherals need exactly
those. The fix: `mimz-sim` defines a trait, `EmulationHost`, and never
imports anything ratatui/cpal-flavored itself. The harness
(`sim/harness.rs`) holds `Option<Box<dyn EmulationHost>>` — `None` means
"no host, run headless like always."

**`Direction`** — `Input` (the peripheral drives the sim, like `uart_rx`)
or `Output` (the sim drives the peripheral, like `led`/`speaker`/`uart_tx`).

**The seven trait methods** each answer one question the harness needs to
ask without knowing what a peripheral actually is: `bind` ("can you
handle this config?"), `direction_of` ("input or output?"), `on_change`
("the value just changed"), `on_tick` ("a cycle just happened"), `drive`
("what should I read from you this cycle?"), `frame` ("redraw, and did
the user quit?"), `finish` ("wrap up").

## `src/emulate/mod.rs` — The Concrete Registry

This is where the shell crate picks up the seam. **`Peripheral`** is the
object-safe trait every concrete peripheral implements (`on_change`,
`on_tick`, `drive`, `finish`, `render`) — `EmulateHost` (below) holds a
`Box<dyn Peripheral>` per bound port, so it never needs to know which
concrete peripheral it's talking to. **`registry()`** maps a name
(`"led"`, `"speaker"`, `"uart_tx"`, `"uart_rx"`) to an `Entry { direction,
construct }` — `construct` is a plain function pointer that validates a
bind's config and builds the peripheral or returns a teaching-quality
error string.

## `src/emulate/host.rs` — `EmulateHost`

The shell crate's actual `EmulationHost` implementation. Holds the
registry, every bound `(port, Box<dyn Peripheral>)` pair, an optional
`Dashboard` (only when `live`), and the `--step` state. Constructed once
per `test` block, **unconditionally** — even a headless `mimz test` builds
one, just with `live: false`, so a `sim{}` block's bind validation always
runs and a broken block can't accidentally pass CI just because nobody
ran `--emulate`.

## `src/emulate/led.rs` — the simplest peripheral

Read this one first if you're adding a new peripheral — it's the smallest
complete example. `construct` checks the bound width is `1..=64` bits and
parses an optional `color:` config; `on_change` just remembers the latest
value for `render` to draw as a lit/unlit colored block.

## `src/emulate/speaker.rs` — offline rendering, not live pacing

The peripheral with the most interesting history: an earlier version
tried to play audio **live**, pacing an `mpsc::sync_channel` against the
declared clock rate. That assumed the interpreter could keep up with a
real audio-rate clock in real time — measured throughput (~1M cycles/sec
even in release) is roughly 50x too slow for that to work, so live pacing
produced garbled or silent playback. The fix: render **offline**.
`on_tick` just buffers downsampled bits into a `Vec`; `finish` hands the
whole buffer to `cpal` and plays it back in one shot, on a dedicated
thread (calling `cpal` from the sim thread, which also drives the
dashboard, hangs indefinitely — audio I/O and terminal redraws don't mix
on one thread).

## `src/emulate/uart_tx.rs` / `uart_rx.rs` — the serial pair

`uart_tx` is an output peripheral: it watches the bound bit, decodes 8-N-1
serial framing at a `baud:`-derived bit rate (independent of the `sim`
block's `speed` — exactly like a real UART), and logs decoded bytes to
the dashboard and/or a local TCP socket (`port:`). `uart_rx` is the input
counterpart: it drives the bound bit from either a TCP socket or a
literal `source:` byte string (for a self-checking test with no real
client on the other end).

The one genuinely subtle bug either of these files fixed: `uart_rx` holds
the start bit for `cycles_per_bit + 1` cycles, not `cycles_per_bit` — see
the comment at `uart_rx.rs`'s `Phase::Start` arm. A receiver's `Idle`
state detects the start bit on a cycle that doesn't count toward its own
bit timer, so the naive `cycles_per_bit` shortfall corrupted every
received byte by one bit position. The `+ 1` is deliberate, not a rounding
artifact — don't "simplify" it away.

## `src/emulate/dashboard.rs` — the live view

Crate-private (`pub(crate)`) — nothing outside `src/emulate/` needs to
know it exists. A `ratatui` terminal UI, redrawn in ~30fps batches (not
every cycle — that would thrash a fast `speed`) by `EmulateHost::frame`.
Draws one row per bound peripheral via `Peripheral::render`, plus the test
name and running cycle count in the title. Waits for Enter/`q` at the end
of a live run instead of closing immediately (an early version closed the
instant the test finished, which meant the very Enter keypress that
launched `mimz` itself could dismiss the screen before you'd seen
anything).

## Wired together: `src/commands/test.rs`

This is where `--emulate`/`--step` become an actual `EmulateHost`:
`live = (emulate || step) && stdout.is_tty()`, computed once per file, not
per test. Piped or redirected output — CI, `| tee log`, anything non-tty —
silently takes the `live: false` path no matter what flags you passed, so
`mimz test --emulate` in a script behaves exactly like plain `mimz test`
rather than hanging on a terminal that was never there.

---

Next: back to [09 — Tooling and Entry Points](09-tooling-and-entry.md) for
the rest of the CLI, or [10 — Ecosystem](10-ecosystem.md) for the wider
project (site, fuzzing, benchmarks).
