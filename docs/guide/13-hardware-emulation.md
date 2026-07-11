# 13 — Hardware Emulation: `sim` Blocks

> **Beta.** Hardware emulation shipped 2026-07-07 through 2026-07-09 and is
> still young — four peripherals, native platforms only (behind the
> `hw-emulation` Cargo feature, not available in the WASM playground), and
> the API surface (`sim{}` grammar, peripheral config keys) may still
> change without a full edition bump. Safe to use in `test` blocks today;
> just don't treat it as a frozen contract yet.

Everything so far runs at simulator speed — thousands of cycles a second,
with no sense of real time. A `sim` block inside a `test` block gives up
that speed on purpose, in exchange for something a plain `mimz test` can't
give you: a real, watchable peripheral — a blinking LED in your terminal, an
actual audio tone, a real TCP serial port — driven by your design at (an
approximation of) real-world timing.

This is **simulation-only**. `mimz compile` never sees a `sim` block; it
exists purely inside `test`, alongside `tick`/`expect`.

## Turning it on: `--emulate`

A `sim` block is inert by default — `mimz test` runs it as a normal,
full-speed test with no throttling and no dashboard. To actually emulate:

```sh
mimz test blink.mimz --emulate
```

`--emulate` only does something in a real terminal. Piped or redirected
output (CI, a script, `mimz test | tee log`) auto-degrades to the same
full-speed run as leaving the flag off, with a logged note explaining why —
it never hangs a CI job waiting for a terminal that isn't there.

Add `--step` for single-cycle control: the run pauses after every cycle,
waiting for Enter to advance or `q` to quit. Useful for watching an LED
pattern or a UART byte one bit at a time instead of at full throttle.

## Writing a `sim` block

```mimz
test "quick beep (emulated)" for MelodyPlayer(TICK: 500000) {
  sim {
    speed mhz(50)
    bind audio -> speaker()
    bind playing -> led(color: "green")
  }
  start = 1
  tick(clk)
  start = 0
  tick(clk, 6000000)
}
```

- `speed mhz(N)` — the real-world clock rate the design's declared clock
  should be throttled to. Everything inside the block paces against this;
  without a `sim` block, `tick` runs as fast as the interpreter can go.
- `bind <port> -> <peripheral>(<config>)` — connects one of the module's
  ports to a virtual peripheral. One `bind` per port; a port can only be
  bound once.

## The peripherals

| Peripheral | Direction | Binds to                 | Config                                                                                                |
| ---------- | --------- | ------------------------ | ----------------------------------------------------------------------------------------------------- |
| `led`      | output    | `bit`/`bits[N]` (N ≤ 64) | `color: red\|green\|blue\|...` — which color the dashboard draws it                                   |
| `speaker`  | output    | `bit`                    | none — plays the bound bit as a square-wave tone on the host's default audio output                   |
| `uart_tx`  | output    | `bit`                    | `baud: N`, `port: N` — encodes 8-N-1 serial, decoded live to the dashboard log and a local TCP socket |
| `uart_rx`  | input     | `bit`                    | `baud: N`, plus either `port: N` (a TCP socket) or `source: "<hex bytes>"` (a literal byte string)    |

`uart_tx`/`uart_rx`'s `baud` is independent of the `sim` block's `speed` —
the peripheral derives its own bit timing (`cycles_per_bit`) from the two
together, the same way a real UART's baud rate is independent of the
system clock it's driven from.

A binding error (wrong port direction, a config value out of range, an
unknown peripheral name) is a teaching-quality message at `mimz test` time,
the same tier as any other diagnostic — it fires even without `--emulate`,
so a broken `sim` block never silently passes CI.

## What you see

In a real terminal with `--emulate`, each bound peripheral gets a row in a
live dashboard: an LED shows its color when lit, a UART row shows decoded
bytes as they arrive, a speaker row shows it's playing. The dashboard
redraws in ~30fps batches, not every single cycle, so it stays responsive
even at a fast `speed`. At the end of a live run (pass, fail, or `--step`
quit), the dashboard waits for Enter (or `q`) before closing, so you don't
miss the final state.

Outside a real terminal — or without `--emulate` at all — none of this
renders; the test just runs and reports pass/fail like any other.

## Worked example

`showcase/*/melody_player.mimz` binds a `speaker` (playing an actual tune)
and a `led` (lit while playing); `showcase/*/uart_echo.mimz` binds `uart_tx`
to a real local TCP socket, so you can listen in with `nc localhost 8081`
while its self-checking test drives `rx` directly. Both exist in all four
code-order flavor folders (`english`, `tanglish`, `tamil`, `mixed`) plus a
pure-Tamil twin (`isai.mimz`, `edhiroli.mimz`) — run
`mimz test showcase/english/melody_player.mimz --emulate` in a real
terminal to hear it.

---

_Deeper internals: [`docs/code/14-hardware-emulation.md`](../code/14-hardware-emulation.md)
(maintainer docs) or [`docs/source-guide/11-hardware-emulation.md`](../source-guide/11-hardware-emulation.md)
(friendly code tour)._
