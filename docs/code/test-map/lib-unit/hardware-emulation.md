# Unit: hardware-emulation peripherals (`src/emulate/`, 42 tests)

> Back to [Test Map Index](../index.md) · [Overview](../../10-test-map.md)

The native LED / speaker / UART peripherals bound in `sim { bind … }`
blocks and driven through `mimz-sim`'s `EmulationHost` trait
(`mimz test --emulate`). Feature-gated behind `hw-emulation` and never
compiled for `wasm32`. Design notes in
[`13-hardware-emulation.md`](../../../guide/13-hardware-emulation.md).

| Module               | Tests | Covers                                                                                                                  |
| -------------------- | ----: | ----------------------------------------------------------------------------------------------------------------------- |
| `emulate/mod.rs`     |     5 | the registry: which peripheral names exist and in which direction (`led`/`speaker`/`uart_tx` out, `uart_rx` in)         |
| `emulate/host.rs`    |     1 | `drive` dispatches by PORT name, not peripheral name (two LEDs on different ports stay separate)                        |
| `emulate/led.rs`     |     7 | config validation (`color:`), single-bit-only signals, on/off change tracking                                           |
| `emulate/speaker.rs` |     6 | single-bit-only, no config args, sample recording, and silencing a held-high bit on drop                                |
| `emulate/uart_rx.rs` |    10 | 8N1 framing of a literal source, socket transport, port validation, idle-high when the queue drains                     |
| `emulate/uart_tx.rs` |    13 | baud/speed validation, byte decoding, framing-error logging, socket streaming, non-blocking writes when the peer stalls |

The UART tests are the fussiest on purpose: a socket peripheral that
blocks would hang the whole simulation, so
`socket_write_does_not_block_when_peer_stops_reading` and
`socket_target_with_no_client_falls_back_to_log_without_repeated_stalls`
exist specifically to keep that from regressing.
