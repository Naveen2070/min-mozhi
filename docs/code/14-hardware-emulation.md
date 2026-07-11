# 14 — Hardware Emulation (`sim` blocks, `--emulate`)

> **Beta.** Landed 2026-07-07 through 2026-07-09 — young enough that the
> `EmulationHost` trait shape, `Peripheral` trait shape, and `sim{}` config
> keys should all be treated as still settling, not a stable contract.
> Change any of them freely if a new peripheral needs it; just note the
> change in the same session's log (RULES R3/R4) since it's exactly the
> kind of churn this note exists to track.

The `test`-block feature that binds a design's ports to real, watchable
peripherals (an LED in the terminal, actual audio, a real TCP serial port)
and throttles execution to an approximate real-world clock rate. Landed
2026-07-07/09 (LED, then UART, then speaker); relocated from `mimz-sim` to
the root shell crate during the 2026-07-10 workspace split (see
[`07-decisions-and-evolution.md`](07-decisions-and-evolution.md)'s
`EmulationHost` Decision block, backfilled in
[`../log/2026-07-11.md`](../log/2026-07-11.md)).

## Why this is a shell-crate feature, not a `mimz-sim` one

`mimz-sim` is dependency-optional-free by design (the whole point of the
workspace split) — no ratatui, no crossterm, no cpal. But the dashboard,
LED colors, and audio playback are exactly those dependencies. The fix is
dependency inversion, not a feature-gated `cfg` sprinkled through the
simulator: `mimz-sim` defines a narrow trait, `EmulationHost`
(`crates/mimz-sim/src/sim/host.rs`), and the harness (`sim/harness.rs`)
knows only `Option<Box<dyn EmulationHost>>` — `None` is today's headless
degrade path, unchanged. The shell crate implements the trait
(`src/emulate/host.rs::EmulateHost`) and owns every ratatui/cpal type. No
type in `EmulationHost`'s signature comes from ratatui or cpal.

```text
crates/mimz-sim/src/sim/host.rs   EmulationHost trait + Direction enum (abstract seam)
src/emulate/host.rs               EmulateHost — the concrete implementation
src/emulate/mod.rs                Peripheral trait, registry, Entry
src/emulate/{led,speaker,uart_tx,uart_rx}.rs   one Peripheral impl each
src/emulate/dashboard.rs          ratatui live view (crate-private)
```

## `EmulationHost` (the abstract seam)

Seven methods, each mirroring one thing the harness needs from a peripheral
without knowing what a peripheral is:

| Method                                          | Called                                  | Purpose                                                                                     |
| ----------------------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------- |
| `bind(port, peripheral, width, args, speed_hz)` | once per `sim{}` `bind` line            | validate + construct; errors are the teaching-quality strings a `mimz test` diagnostic uses |
| `direction_of(name)`                            | during harness bind resolution          | input vs output, so the harness can validate the port side matches                          |
| `on_change(name, val)`                          | on every batched (~30fps) value change  | coarse — fine for `led`, too coarse for bit-exact serial                                    |
| `on_tick(name, val)`                            | after every individual simulated cycle  | bit-exact timing (`uart_tx`) or anything that can fail to open (`speaker`)                  |
| `drive(name)`                                   | before every individual simulated cycle | input peripherals (`uart_rx`) push a value onto the bound port                              |
| `frame(cycle)`                                  | batched ~30fps redraw                   | dashboard redraw; returns `true` if the user quit (aborts the test)                         |
| `finish()`                                      | once, end of test                       | flush/cleanup (speaker playback) + the live dismiss screen                                  |

`EmulateHost` is constructed **unconditionally** on every `mimz test` run —
see `src/commands/test.rs`'s test loop — even without `--emulate`. That's
deliberate: bind validation (unknown peripheral, wrong direction, a config
value out of range) must fire on every run, not just a live one, so a
broken `sim` block can't silently pass CI just because nobody happened to
run it with `--emulate`. A non-live `EmulateHost` (`live: false`) simply
no-ops every draw/pause call; `live = (emulate || step) && stdout.is_tty()`
is computed once per file in `commands/test.rs` and never overridden
per-test.

## `Peripheral` (the concrete side)

```rust
pub trait Peripheral: Send {
    fn on_change(&mut self, val: &Val);
    fn on_tick(&mut self, _val: &Val) -> Result<(), String> { Ok(()) }
    fn drive(&mut self) -> Option<u64> { None }
    fn finish(&mut self) -> Result<(), String> { Ok(()) }
    fn render(&self, area: Rect, buf: &mut Buffer);
}
```

One `Box<dyn Peripheral>` per `bind`, held by `EmulateHost` for the
lifetime of one `test` block. `registry()` (`src/emulate/mod.rs`) maps a
peripheral name (`"led"`, `"speaker"`, `"uart_tx"`, `"uart_rx"`) to an
`Entry { direction, construct }` — `construct` validates `width`/`args`
against that peripheral's rules and either returns a boxed instance or a
teaching-quality error string. Adding a fifth peripheral means: one file
implementing `Peripheral`, one `registry()` entry, no changes to
`EmulateHost` or the harness.

**Peripherals key their internal state by bound `port` name, not
peripheral type name** — `EmulateHost::bind` stores each instance in a
`Vec<(String, Box<dyn Peripheral>)>` keyed by the port. This matters
because `bind rx -> uart_rx(...)` gives one hardware instance two
different names (the port `rx`, the peripheral type `uart_rx`); every
later dispatch (`on_change`, `on_tick`, `drive`) identifies the peripheral
by its port, so storing by the wrong one makes every later call silently
find nothing (a real bug this design specifically guards against — see
`crates/mimz-sim/src/sim/host.rs`'s doc comment on `bind`).

## The four peripherals

- **`led`** (`src/emulate/led.rs`) — output, `bit`/`bits[N]` (N ≤ 64),
  `color:` config. Simplest peripheral; the reference implementation to
  copy when adding a new one.
- **`speaker`** (`src/emulate/speaker.rs`) — output, `bit`, no config.
  Renders **offline**: `on_tick` just buffers downsampled bits, `finish`
  plays the whole clip back in one shot. This is a correction from an
  earlier live-pacing design (`mpsc::sync_channel`) that assumed the
  interpreter could out-run a declared clock rate in real time — measured
  throughput (~1M cycles/sec even in release) is ~50x short of that for a
  typical audio-rate design, so live pacing produced garbled or silent
  audio. All `cpal` calls run on a dedicated thread; calling them from the
  sim thread (which also drives the dashboard) hangs indefinitely.
- **`uart_tx`** (`src/emulate/uart_tx.rs`) — output, `bit`, `baud:`/`port:`
  config. `baud` derives `cycles_per_bit` against the `sim` block's
  `speed`, independent of the design's actual clock rate. Decodes 8-N-1
  serial live to the dashboard log and a local TCP socket.
- **`uart_rx`** (`src/emulate/uart_rx.rs`) — input, `bit`, `baud:` plus
  either `port:` (a TCP socket) or `source:` (a literal hex byte string,
  for a self-checking test with no real client). Holds the start bit for
  `cycles_per_bit + 1` cycles, not `cycles_per_bit` — a receiver's `Idle`
  state detects the start bit on a cycle that doesn't count toward its own
  bit timer, so the naive `cycles_per_bit` shortfall corrupted every
  received byte by one bit position (a real bug this timing choice fixes).

## Wiring into `mimz test`

`src/commands/test.rs`: `--emulate` and `--step` are independent flags;
either one (with a real terminal) sets `live = true`. `--step` additionally
sets `stepping = true` on `EmulateHost`, which pauses the dashboard after
every single-cycle `frame()` for a keypress (Enter to advance, `q` to
quit) — `harness.rs` checks `frame`'s return value and aborts the test loop
early if the user quit. Piped/non-tty output degrades silently to the
non-live path regardless of the flags (no hang waiting for a terminal that
isn't there); `test --emulate | tee log` behaves exactly like plain
`mimz test`.

`MAX_SIM_CYCLES` (`crates/mimz-sim/src/sim/run.rs`, 500M) bounds a
**headless** `tick(clk, N)` — a runaway-loop guard against an untrusted
test hanging the tool. A **live** (`--emulate`/`--step`) run lifts that cap
entirely (`harness.rs` uses `u64::MAX` when `self.live`), since a real
multi-hundred-million-cycle emulated tick (a melody at a real audio clock
rate) is throttled by `speed` anyway and is expected to take real wall-clock
time. VCD capture is separately capped at 1M frames to bound memory on a
long run regardless; peripheral reads use `Sim::peek` for live signal state
rather than the (possibly stale, past the capture cap) captured frame
buffer.

## Testing

Peripheral unit tests live alongside each implementation
(`src/emulate/{led,speaker,uart_tx,uart_rx}.rs`'s own `#[cfg(test)] mod
tests`) — bind validation (good/bad config, wrong width, wrong direction)
and, for `uart_tx`/`uart_rx`, an actual socket round-trip on a free OS-
assigned port. End-to-end coverage rides the `showcase/` self-checking
Icarus testbenches (`tests/icarus/sc_melody_player_tb.v`,
`tests/icarus/sc_uart_echo_tb.v`) and `tests/showcase.rs`'s four-flavor +
pure-Tamil equivalence checks — the emulated `test` blocks themselves are
exercised by plain `cargo test` (headless, `live: false`, so bind
validation and interpreter logic run without a real terminal or audio
device in CI).
